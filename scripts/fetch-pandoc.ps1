#Requires -Version 5.1
<#
.SYNOPSIS
  Provisiona el sidecar de pandoc para el build de Tauri (M2).

.DESCRIPTION
  Descarga el release OFICIAL pineado de pandoc y lo deja como
  src-tauri/binaries/pandoc-<triple>.exe, que es lo que espera
  `bundle.externalBin` en tauri.conf.json. El mismo script sirve para dev local
  y para CI (GitHub Actions windows-latest). El binario va gitignored: no se
  commitea (~150 MB) y se regenera con este script.

.EXAMPLE
  pwsh scripts/fetch-pandoc.ps1
#>
[CmdletBinding()]
param(
    # Version pineada para builds reproducibles. Bumpear a mano cuando se quiera.
    [string]$Version = "3.9.0.2",
    # Target triple del sidecar (coincide con el host MSVC de Windows x64).
    [string]$Triple = "x86_64-pc-windows-msvc"
)
$ErrorActionPreference = "Stop"

$binDir = Join-Path $PSScriptRoot "..\src-tauri\binaries"
$target = Join-Path $binDir "pandoc-$Triple.exe"

if (Test-Path $target) {
    Write-Host "Sidecar ya presente, nada que hacer: $target"
    exit 0
}

New-Item -ItemType Directory -Force -Path $binDir | Out-Null

$url = "https://github.com/jgm/pandoc/releases/download/$Version/pandoc-$Version-windows-x86_64.zip"
$zip = Join-Path $env:TEMP "pandoc-$Version-windows-x86_64.zip"
$extract = Join-Path $env:TEMP "pandoc-$Version-extract"

Write-Host "Descargando pandoc $Version..."
Invoke-WebRequest -Uri $url -OutFile $zip

if (Test-Path $extract) { Remove-Item -Recurse -Force $extract }
Expand-Archive -Path $zip -DestinationPath $extract -Force

$exe = Get-ChildItem -Path $extract -Recurse -Filter "pandoc.exe" | Select-Object -First 1
if (-not $exe) { throw "no se encontro pandoc.exe dentro del zip descargado" }

Copy-Item $exe.FullName $target -Force
Write-Host "Sidecar listo: $target"
