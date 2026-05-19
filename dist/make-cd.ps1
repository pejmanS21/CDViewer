# Stage a portable DICOM-viewer CD/DVD payload at dist\cd-staging\ (Windows).
#
# Usage:
#   dist\make-cd.ps1
#   dist\make-cd.ps1 -DicomSource C:\path\to\study
#
# After running, drop your DICOM files into dist\cd-staging\DICOM\ and burn
# the staging directory with Windows Disc Image Burner or any CD writer.

[CmdletBinding()]
param(
    [string]$DicomSource = ""
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path "$PSScriptRoot\..").Path
$Stage    = Join-Path $RepoRoot "dist\cd-staging"
$Dist     = Join-Path $RepoRoot "dist"

Write-Host "▸ Building release binary…" -ForegroundColor Cyan
Push-Location $RepoRoot
try { cargo build --release } finally { Pop-Location }

$BinPath = Join-Path $RepoRoot "target\release\dicom-viewer.exe"
if (-not (Test-Path $BinPath)) { throw "Release binary missing: $BinPath" }

Write-Host "▸ Cleaning staging dir…" -ForegroundColor Cyan
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Path "$Stage\DICOM" | Out-Null

Write-Host "▸ Copying viewer + manifests…" -ForegroundColor Cyan
Copy-Item $BinPath                          $Stage
Copy-Item (Join-Path $Dist "autorun.inf")   $Stage
Copy-Item (Join-Path $Dist "README.txt")    $Stage
Copy-Item (Join-Path $Dist "HELP.txt")      $Stage

if ($DicomSource -ne "") {
    if (-not (Test-Path $DicomSource)) { throw "Not a directory: $DicomSource" }
    Write-Host "▸ Copying study data from $DicomSource …" -ForegroundColor Cyan
    Copy-Item -Path (Join-Path $DicomSource "*") -Destination "$Stage\DICOM\" -Recurse
}

Write-Host ""
Write-Host "✔  Staging ready at: $Stage" -ForegroundColor Green
Write-Host ""
Write-Host "Next:"
Write-Host "  • Drop your DICOM files into  $Stage\DICOM\"
Write-Host "  • In File Explorer, right-click the staging folder ▸ Burn to disc."
Write-Host "  • Or build an ISO using e.g. oscdimg:"
Write-Host "      oscdimg -j1 -lDICOM_VIEWER `"$Stage`" dicom-viewer.iso"
