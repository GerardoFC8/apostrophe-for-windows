#Requires -Version 5.1
<#
.SYNOPSIS
  Provisiona el sidecar de typst, el motor de PDF (M13).

.DESCRIPTION
  Pandoc NO sabe generar PDF por su cuenta: delega en un motor externo. Se usa typst
  porque es un unico binario (~21 MB comprimido); la alternativa, xelatex, exigiria
  TeXLive (gigabytes). Upstream (Apostrophe/GNOME) tomo la misma decision.

  Descarga el release OFICIAL pineado y lo deja como
  src-tauri/binaries/typst-<triple>.exe, que es lo que espera `bundle.externalBin`
  en tauri.conf.json. El binario va gitignored: no se commitea y se regenera con
  este script.

  IMPORTANTE — el nombre del binario no es cosmetico. Pandoc valida el nombre del
  motor que le pasas por `--pdf-engine`: acepta una ruta absoluta, pero SOLO si su
  ultimo componente es un motor conocido (`typst`, `xelatex`, ...). Al empaquetar,
  Tauri quita el target triple y deja `typst.exe` junto al ejecutable, que es
  justamente lo que pandoc acepta.

  IMPORTANTE — requiere pandoc 3.9 o superior. El template typst de pandoc 3.7
  emitia `font: ()` (lista vacia) y typst 0.15 la rechaza con "font fallback list
  must not be empty". 3.9 lo corrigio usando `font: none`. Si bajas el pin de
  fetch-pandoc.ps1 por debajo de 3.9, la exportacion a PDF deja de funcionar.

.EXAMPLE
  pwsh scripts/fetch-typst.ps1
#>
[CmdletBinding()]
param(
    # Version pineada para builds reproducibles. Bumpear a mano cuando se quiera.
    [string]$Version = "0.15.1",
    # SHA-256 del zip oficial de esa version. Sale del `digest` que GitHub publica para cada
    # archivo de una release (api.github.com/repos/typst/typst/releases/tags/v<version>).
    # Al cambiar -Version hay que cambiar tambien este valor: si no coinciden, el script falla
    # en vez de meter en el instalador un binario que nadie verifico. typst.exe no viene
    # firmado, asi que este hash es la unica verificacion.
    [string]$Sha256 = "19ce3551153c2fe7ee9fa2f95208310c8f4d3209fedb699e0333faf8913f6736",
    # Target triple del sidecar (coincide con el host MSVC de Windows x64).
    [string]$Triple = "x86_64-pc-windows-msvc"
)
$ErrorActionPreference = "Stop"

$binDir = Join-Path $PSScriptRoot "..\src-tauri\binaries"
$target = Join-Path $binDir "typst-$Triple.exe"

if (Test-Path $target) {
    Write-Host "Sidecar ya presente, nada que hacer: $target"
    exit 0
}

New-Item -ItemType Directory -Force -Path $binDir | Out-Null

$url = "https://github.com/typst/typst/releases/download/v$Version/typst-x86_64-pc-windows-msvc.zip"
$zip = Join-Path $env:TEMP "typst-$Version-windows-x86_64.zip"
$extract = Join-Path $env:TEMP "typst-$Version-extract"

Write-Host "Descargando typst $Version..."
Invoke-WebRequest -Uri $url -OutFile $zip

# Se verifica ANTES de extraer: un zip corrupto o manipulado no llega a tocar el disco del
# proyecto ni, por lo tanto, el instalador.
$actual = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLowerInvariant()
if ($actual -ne $Sha256.ToLowerInvariant()) {
    Remove-Item -Force $zip
    throw "El SHA-256 del zip de typst $Version no coincide.`n  esperado: $Sha256`n  obtenido: $actual"
}
Write-Host "SHA-256 verificado."

if (Test-Path $extract) { Remove-Item -Recurse -Force $extract }
Expand-Archive -Path $zip -DestinationPath $extract -Force

$exe = Get-ChildItem -Path $extract -Recurse -Filter "typst.exe" | Select-Object -First 1
if (-not $exe) { throw "no se encontro typst.exe dentro del zip descargado" }

Copy-Item $exe.FullName $target -Force
Write-Host "Sidecar listo: $target"
