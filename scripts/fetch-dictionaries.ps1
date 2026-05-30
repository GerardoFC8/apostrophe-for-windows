#Requires -Version 5.1
<#
.SYNOPSIS
  Provisiona los diccionarios Hunspell para el spellcheck (M6).

.DESCRIPTION
  Descarga los .aff/.dic de wooorm/dictionaries a src-tauri/dictionaries/. Mismo
  patron que fetch-pandoc.ps1: idempotente, gitignored, se regenera con este script.
  - es = idioma ACTIVO del MVP (se bundlea via tauri.conf.json bundle.resources).
  - en = preparado para el futuro (se descarga pero NO se bundlea todavia).
  Licencia de los diccionarios: triple MPL 1.1 / GPLv3 / LGPLv3 (rla-es via wooorm).
  El proyecto los usa bajo MPL 1.1; se incluye el archivo de licencia (es.license.txt).

.EXAMPLE
  pwsh scripts/fetch-dictionaries.ps1
#>
[CmdletBinding()]
param(
    # Ref de wooorm/dictionaries. Pinear a un commit/tag para builds reproducibles.
    [string]$Ref = "main"
)
$ErrorActionPreference = "Stop"

$dir = Join-Path $PSScriptRoot "..\src-tauri\dictionaries"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$base = "https://raw.githubusercontent.com/wooorm/dictionaries/$Ref/dictionaries"

foreach ($lang in @("es", "en")) {
    Write-Host "Descargando diccionario '$lang'..."
    Invoke-WebRequest "$base/$lang/index.aff" -OutFile (Join-Path $dir "$lang.aff")
    Invoke-WebRequest "$base/$lang/index.dic" -OutFile (Join-Path $dir "$lang.dic")
    Invoke-WebRequest "$base/$lang/license" -OutFile (Join-Path $dir "$lang.license.txt")
}
Write-Host "Diccionarios listos en $dir"
