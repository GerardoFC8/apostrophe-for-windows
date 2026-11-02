<#
.SYNOPSIS
  Pruebas de elision-path.ps1 sobre una clave de registro temporal.

.DESCRIPTION
  Ejecuta el script igual que el instalador NSIS (powershell.exe -File ... add|remove,
  con la carpeta en ELISION_PATH_ENTRY), pero contra HKCU\Software\ElisionPathTest-<guid>
  en vez de HKCU\Environment: el Path real del usuario no se toca. La clave temporal se
  borra al terminar.

  Lo corre la CI (windows-latest). A mano, desde la raiz del repo:
    powershell -NoProfile -ExecutionPolicy Bypass -File src-tauri/windows/elision-path.tests.ps1

  El archivo es ASCII a proposito (como scripts/*.ps1): Windows PowerShell 5.1 lee los
  .ps1 sin BOM con la pagina de codigos ANSI. Los caracteres no ASCII se arman con [char].
#>
$ErrorActionPreference = 'Stop'

$script = Join-Path $PSScriptRoot 'elision-path.ps1'
$powershell = Join-Path $PSHOME 'powershell.exe'
$subKey = "Software\ElisionPathTest-$([guid]::NewGuid())"
$failures = 0

$expand = [Microsoft.Win32.RegistryValueKind]::ExpandString
$plain = [Microsoft.Win32.RegistryValueKind]::String
$dword = [Microsoft.Win32.RegistryValueKind]::DWord

# Una carpeta de instalacion incomoda: espacios, apostrofo y un caracter no ASCII.
$dir = "C:\Users\Jos$([char]0xE9) O'Brien\App Data\Local\Apostrophe"

# Ejecuta el script en otro proceso, como lo hace hooks.nsh. Devuelve el codigo de salida.
function Invoke-PathScript([string]$Mode, [string]$Entry = $dir) {
  $env:ELISION_PATH_ENTRY = $Entry
  try {
    & $powershell -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass `
      -File $script $Mode -Key $subKey | Out-Host
    $LASTEXITCODE
  } finally {
    Remove-Item Env:ELISION_PATH_ENTRY -ErrorAction SilentlyContinue
  }
}

# Deja el valor Path de la clave temporal como se indica ($null = sin valor).
function Set-TestPath($Value, $Kind = $expand) {
  $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($subKey)
  try {
    if ($null -eq $Value) { $key.DeleteValue('Path', $false) }
    else { $key.SetValue('Path', $Value, $Kind) }
  } finally {
    $key.Close()
  }
}

# El valor Path SIN expandir y su tipo, o $null si no existe.
function Get-TestPath {
  $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($subKey)
  try {
    if ($key.GetValueNames() -notcontains 'Path') { return $null }
    $options = [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
    [pscustomobject]@{
      Value = $key.GetValue('Path', $null, $options)
      Kind  = "$($key.GetValueKind('Path'))"
    }
  } finally {
    $key.Close()
  }
}

# Compara exacto (con mayusculas) y anota el fallo sin detener las demas pruebas.
function Assert-Equal([string]$Name, $Expected, $Actual) {
  if ($Expected -ceq $Actual) {
    Write-Host "ok    $Name"
  } else {
    $script:failures++
    Write-Host "FALLA $Name"
    Write-Host "      esperado: [$Expected]"
    Write-Host "      obtenido: [$Actual]"
  }
}

# Comprueba codigo de salida, valor y tipo de una sola vez.
function Assert-State([string]$Name, $Code, $ExpectedCode, $Value, $Kind) {
  Assert-Equal "$Name (codigo)" $ExpectedCode $Code
  $state = Get-TestPath
  if ($null -eq $Value) {
    Assert-Equal "$Name (sin valor)" $true ($null -eq $state)
    return
  }
  Assert-Equal "$Name (valor)" $Value $state.Value
  Assert-Equal "$Name (tipo)" "$Kind" $state.Kind
}

try {
  # Un Path de usuario largo (> 1024 caracteres, el limite de las cadenas de NSIS) y con
  # una variable sin expandir: debe quedar igual, con la carpeta al final.
  $long = '%USERPROFILE%\bin;' + ((1..60 | ForEach-Object { "C:\Herramientas\carpeta-$_" }) -join ';')
  Assert-Equal 'el Path de la prueba supera 1024 caracteres' $true ($long.Length -gt 1024)

  Set-TestPath $long $expand
  $code = Invoke-PathScript add
  Assert-State 'agrega al final sin expandir ni cambiar el tipo' $code 0 "$long;$dir" $expand

  $code = Invoke-PathScript add
  Assert-State 'reinstalar no duplica la entrada' $code 0 "$long;$dir" $expand

  $code = Invoke-PathScript remove
  Assert-State 'desinstalar deja el Path como estaba' $code 0 $long $expand

  $code = Invoke-PathScript remove
  Assert-State 'quitar sin la entrada no cambia nada' $code 0 $long $expand

  Set-TestPath 'C:\a;C:\b' $plain
  $code = Invoke-PathScript add
  Assert-State 'conserva el tipo REG_SZ' $code 0 "C:\a;C:\b;$dir" $plain
  $code = Invoke-PathScript remove
  Assert-State 'y lo restaura con REG_SZ' $code 0 'C:\a;C:\b' $plain

  Set-TestPath 'C:\a;' $expand
  $code = Invoke-PathScript add
  Assert-State 'respeta el punto y coma final' $code 0 "C:\a;$dir;" $expand
  $code = Invoke-PathScript remove
  Assert-State 'y lo deja al desinstalar' $code 0 'C:\a;' $expand

  Set-TestPath $null
  $code = Invoke-PathScript add
  Assert-State 'sin Path previo crea un REG_EXPAND_SZ' $code 0 $dir $expand
  $code = Invoke-PathScript remove
  Assert-State 'y al desinstalar borra el valor vacio' $code 0 $null $null

  # Mayusculas, comillas y barra final no cuentan: es la misma carpeta.
  $variant = "C:\a;`"$($dir.ToUpperInvariant())\`";C:\b"
  Set-TestPath $variant $expand
  $code = Invoke-PathScript add
  Assert-State 'una variante de la carpeta cuenta como propia' $code 0 $variant $expand
  $code = Invoke-PathScript remove
  Assert-State 'y se quita al desinstalar' $code 0 'C:\a;C:\b' $expand

  # Una entrada con variables que, expandida, es la carpeta tambien cuenta como propia.
  $env:ELISION_TEST_ROOT = Split-Path $dir -Parent
  try {
    Set-TestPath 'C:\a;%ELISION_TEST_ROOT%\Apostrophe' $expand
    $code = Invoke-PathScript add
    Assert-State 'una entrada con variables que apunta a la carpeta' $code 0 'C:\a;%ELISION_TEST_ROOT%\Apostrophe' $expand
  } finally {
    Remove-Item Env:ELISION_TEST_ROOT -ErrorAction SilentlyContinue
  }

  # Entradas repetidas (de un instalador anterior con fallos): se quitan todas.
  Set-TestPath "$dir;C:\a;$dir" $expand
  $code = Invoke-PathScript remove
  Assert-State 'quita todas las copias de la entrada' $code 0 'C:\a' $expand

  # Un Path que no es texto no se toca.
  Set-TestPath 5 $dword
  $code = Invoke-PathScript add
  Assert-Equal 'un Path que no es texto termina con 3' 3 $code
  $state = Get-TestPath
  Assert-Equal 'y no se toca' "$dword" $state.Kind

  # Sin carpeta no hay nada que hacer.
  Set-TestPath 'C:\a' $expand
  $code = Invoke-PathScript add ''
  Assert-State 'sin ELISION_PATH_ENTRY termina con 2' $code 2 'C:\a' $expand
} finally {
  [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($subKey, $false)
}

if ($failures -gt 0) {
  Write-Host "$failures comprobaciones fallaron"
  exit 1
}
Write-Host 'todas las comprobaciones pasaron'
exit 0
