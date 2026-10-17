<#
.SYNOPSIS
  Agrega o quita la carpeta de instalacion en el Path del usuario (HKCU\Environment).

.DESCRIPTION
  Lo ejecuta el instalador NSIS (hooks.nsh) para que el comando `elision` funcione en
  cualquier terminal nueva. La carpeta llega en la variable de entorno ELISION_PATH_ENTRY:
  asi no hay que escapar comillas, apostrofos ni una barra final en la linea de comandos.

  - Lee el valor SIN expandir y lo reescribe con su tipo original (casi siempre
    REG_EXPAND_SZ). NO usa [Environment]::SetEnvironmentVariable: expandiria
    %USERPROFILE% y guardaria el valor como REG_SZ.
  - add: agrega la carpeta al final, solo si no esta ya. Reinstalar o actualizar no la
    duplica.
  - remove: quita solo las entradas que apuntan a la carpeta; el resto queda igual. Si
    el Path queda vacio, borra el valor.
  - Una entrada apunta a la carpeta si coincide tras expandir sus variables, sin
    distinguir mayusculas, comillas ni la barra final.
  - No hay limite de 1024 caracteres: NSIS nunca manipula el valor, solo este script.

  Codigos de salida: 0 ok (tambien si no habia nada que hacer), 1 error inesperado,
  2 falta ELISION_PATH_ENTRY, 3 el Path existe pero no es texto (no se toca).
  hooks.nsh difunde WM_SETTINGCHANGE al terminar.

  El archivo es ASCII a proposito (como scripts/*.ps1): Windows PowerShell 5.1 lee los
  .ps1 sin BOM con la pagina de codigos ANSI.

.PARAMETER Mode
  add al instalar, remove al desinstalar.

.PARAMETER Key
  Subclave de HKCU que guarda el Path. Solo la cambian las pruebas
  (elision-path.tests.ps1), para no tocar el Path real.
#>
param(
  [Parameter(Mandatory = $true, Position = 0)]
  [ValidateSet('add', 'remove')]
  [string]$Mode,

  [string]$Key = 'Environment'
)

$ErrorActionPreference = 'Stop'

$entry = $env:ELISION_PATH_ENTRY
if ([string]::IsNullOrWhiteSpace($entry)) { exit 2 }

# Forma comparable de una entrada: expandida y sin espacios, comillas ni barra final.
# El -eq de PowerShell ya ignora mayusculas.
function ConvertTo-Comparable([string]$Path) {
  [Environment]::ExpandEnvironmentVariables($Path).Trim().Trim('"').TrimEnd('\')
}

$target = ConvertTo-Comparable $entry

function Test-OwnEntry([string]$Part) {
  $Part.Trim() -ne '' -and (ConvertTo-Comparable $Part) -eq $target
}

$reg = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($Key)
try {
  # Se reescribe con el nombre que ya tenga el valor (Path, PATH...).
  $name = @($reg.GetValueNames() | Where-Object { $_ -eq 'Path' })[0]
  if ($null -eq $name) {
    $name = 'Path'
    $raw = ''
    $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
  } else {
    $kind = $reg.GetValueKind($name)
    if ($kind -ne [Microsoft.Win32.RegistryValueKind]::ExpandString -and
        $kind -ne [Microsoft.Win32.RegistryValueKind]::String) {
      exit 3
    }
    $options = [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
    $raw = [string]$reg.GetValue($name, '', $options)
  }

  $parts = $raw -split ';'
  $own = @($parts | Where-Object { Test-OwnEntry $_ })

  if ($Mode -eq 'add') {
    if ($own.Count -gt 0) { exit 0 }
    # Si el Path terminaba en ';', sigue terminando asi: remove lo deja byte a byte.
    if ($raw -eq '') { $new = $entry }
    elseif ($raw.EndsWith(';')) { $new = "$raw$entry;" }
    else { $new = "$raw;$entry" }
  } else {
    if ($own.Count -eq 0) { exit 0 }
    $new = @($parts | Where-Object { -not (Test-OwnEntry $_) }) -join ';'
  }

  if ($new -eq '') { $reg.DeleteValue($name, $false) }
  else { $reg.SetValue($name, $new, $kind) }
} finally {
  $reg.Close()
}
exit 0
