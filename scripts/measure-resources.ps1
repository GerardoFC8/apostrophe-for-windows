#Requires -Version 5.1
<#
.SYNOPSIS
  Mide la RAM y la CPU que consume Elision mientras la usas.

.DESCRIPTION
  Suma el proceso principal (apostrophe.exe) y todos sus descendientes: los procesos de
  WebView2 (navegador, renderizadores, GPU, utilidades) y los sidecars que estén corriendo
  (pandoc, typst). Contar solo apostrophe.exe engaña: casi toda la memoria vive en WebView2.

  Toma una muestra cada -Interval segundos durante -Duration segundos y al final muestra el
  promedio y el máximo, en total y por tipo de proceso. Con -CsvPath guarda cada muestra.

  Memoria:
  - Working set: lo que el Administrador de tareas llama "memoria" (incluye lo compartido).
  - Privada: lo que es solo de la app. Es la cifra para comparar antes y después de un cambio.
  CPU: porcentaje del equipo completo (100 % = todos los núcleos al máximo), como el
  Administrador de tareas.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-resources.ps1
  Mide 30 s la instancia abierta. Desplázate, escribe o cambia de pestaña mientras tanto.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-resources.ps1 -Duration 60 -Interval 0.5 -CsvPath demo/medicion.csv
#>
[CmdletBinding()]
param(
    # Nombre del proceso principal, sin .exe.
    [string]$ProcessName = "apostrophe",
    # PID concreto, si hay más de una instancia.
    [int]$ProcessId = 0,
    # Segundos de medición.
    [double]$Duration = 30,
    # Segundos entre muestras.
    [double]$Interval = 1,
    # Archivo CSV con una fila por muestra y tipo de proceso (opcional).
    [string]$CsvPath = ""
)
$ErrorActionPreference = "Stop"

if ($ProcessId -gt 0) {
    $root = Get-Process -Id $ProcessId
} else {
    $found = @(Get-Process -Name $ProcessName -ErrorAction SilentlyContinue)
    if ($found.Count -eq 0) { throw "No hay ningún proceso '$ProcessName' abierto. Abre Elision primero." }
    if ($found.Count -gt 1) {
        throw "Hay $($found.Count) procesos '$ProcessName' ($($found.Id -join ', ')). Elige uno con -ProcessId."
    }
    $root = $found[0]
}
$cores = [Environment]::ProcessorCount

# Tipo de cada proceso: WebView2 lo dice en `--type=` de su línea de comandos.
function Get-Kind($proc) {
    if ($proc.ProcessId -eq $root.Id) { return "app" }
    $name = [IO.Path]::GetFileNameWithoutExtension($proc.Name)
    if ($name -ne "msedgewebview2") { return $name }
    if ($proc.CommandLine -match '--type=([\w-]+)') {
        $type = $Matches[1]
        if ($type -eq "utility" -and $proc.CommandLine -match '--utility-sub-type=[\w.]*?(\w+)\.mojom') {
            return "webview-utility-$($Matches[1])"
        }
        return "webview-$type"
    }
    return "webview-browser"
}

# El árbol se vuelve a leer en cada muestra: los renderizadores y los sidecars van y vienen.
function Get-Tree {
    $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, CommandLine
    $byParent = $all | Group-Object ParentProcessId -AsHashTable -AsString
    $result = @($all | Where-Object ProcessId -eq $root.Id)
    $queue = [Collections.Generic.Queue[int]]::new()
    $queue.Enqueue($root.Id)
    while ($queue.Count -gt 0) {
        $children = $byParent["$($queue.Dequeue())"]
        foreach ($child in @($children)) {
            if (-not $child) { continue }
            $result += $child
            $queue.Enqueue([int]$child.ProcessId)
        }
    }
    $result
}

$prevCpu = @{}
$prevTime = [Diagnostics.Stopwatch]::StartNew()
$samples = [Collections.Generic.List[object]]::new()
$kinds = @{}

Write-Host ("Midiendo {0} (PID {1}) durante {2} s, una muestra cada {3} s…" -f $root.ProcessName, $root.Id, $Duration, $Interval)
$deadline = (Get-Date).AddSeconds($Duration)
$first = $true
while ((Get-Date) -lt $deadline -and -not $root.HasExited) {
    $elapsed = $prevTime.Elapsed.TotalSeconds
    $prevTime.Restart()
    $tree = Get-Tree
    $row = @{}
    foreach ($p in $tree) {
        $proc = Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue
        if (-not $proc) { continue }
        $kind = Get-Kind $p
        if (-not $kinds.ContainsKey([int]$p.ProcessId)) { $kinds[[int]$p.ProcessId] = $kind }
        $cpu = $proc.TotalProcessorTime.TotalSeconds
        $delta = if ($prevCpu.ContainsKey($proc.Id)) { $cpu - $prevCpu[$proc.Id] } else { 0 }
        $prevCpu[$proc.Id] = $cpu
        if (-not $row.ContainsKey($kind)) { $row[$kind] = [pscustomobject]@{ Ws = 0.0; Priv = 0.0; Cpu = 0.0; Count = 0 } }
        $row[$kind].Ws += $proc.WorkingSet64 / 1MB
        $row[$kind].Priv += $proc.PrivateMemorySize64 / 1MB
        if (-not $first -and $elapsed -gt 0) { $row[$kind].Cpu += 100 * $delta / ($elapsed * $cores) }
        $row[$kind].Count++
    }
    if (-not $first) {
        $time = Get-Date
        foreach ($k in $row.Keys) {
            $samples.Add([pscustomobject]@{
                Time = $time.ToString("HH:mm:ss.fff"); Kind = $k; Processes = $row[$k].Count
                WorkingSetMB = [math]::Round($row[$k].Ws, 1); PrivateMB = [math]::Round($row[$k].Priv, 1)
                CpuPercent = [math]::Round($row[$k].Cpu, 2)
            })
        }
    }
    $first = $false
    Start-Sleep -Milliseconds ([int]($Interval * 1000))
}

if ($samples.Count -eq 0) { throw "No se tomó ninguna muestra (¿la app se cerró?)." }
if ($CsvPath) {
    $samples | Export-Csv -Path $CsvPath -NoTypeInformation -Encoding UTF8
    Write-Host "Muestras guardadas en $CsvPath"
}

function Summarize($group, $label) {
    $ws = $group | Measure-Object WorkingSetMB -Average -Maximum
    $pv = $group | Measure-Object PrivateMB -Average -Maximum
    $cp = $group | Measure-Object CpuPercent -Average -Maximum
    [pscustomobject]@{
        Tipo = $label
        Procesos = ($group | Measure-Object Processes -Maximum).Maximum
        "WS prom (MB)" = [math]::Round($ws.Average, 1); "WS máx (MB)" = [math]::Round($ws.Maximum, 1)
        "Privada prom (MB)" = [math]::Round($pv.Average, 1); "Privada máx (MB)" = [math]::Round($pv.Maximum, 1)
        "CPU prom (%)" = [math]::Round($cp.Average, 2); "CPU máx (%)" = [math]::Round($cp.Maximum, 2)
    }
}

$perKind = $samples | Group-Object Kind | Sort-Object Name | ForEach-Object { Summarize $_.Group $_.Name }
$totals = $samples | Group-Object Time | ForEach-Object {
    [pscustomobject]@{
        Processes = ($_.Group | Measure-Object Processes -Sum).Sum
        WorkingSetMB = ($_.Group | Measure-Object WorkingSetMB -Sum).Sum
        PrivateMB = ($_.Group | Measure-Object PrivateMB -Sum).Sum
        CpuPercent = ($_.Group | Measure-Object CpuPercent -Sum).Sum
    }
}
@($perKind) + (Summarize $totals "TOTAL") | Format-Table -AutoSize | Out-String -Width 200 | Write-Host
Write-Host ("{0} muestras, {1} núcleos lógicos." -f $totals.Count, $cores)
