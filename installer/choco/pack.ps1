# Build the Chocolatey package for a published release.
#
#   pwsh installer/choco/pack.ps1 -Version 0.6.2
#
# Fills the version and the installer's SHA-256 (taken from the digest GitHub
# publishes for the release asset, the same source check-manifests.ps1 and the
# app's own updater verify against), then runs `choco pack`. Push it with:
#
#   choco push installer/choco/ailimits.<version>.nupkg --source https://push.chocolatey.org/ --api-key <key>
param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = 'Stop'
$here = Split-Path $MyInvocation.MyCommand.Path

$release = Invoke-RestMethod "https://api.github.com/repos/napxlexn/ailimits/releases/tags/v$Version"
$asset = $release.assets | Where-Object name -eq "AiLimits-Setup-$Version.exe"
if (-not $asset) { throw "release v$Version has no AiLimits-Setup-$Version.exe" }
$checksum = ($asset.digest -replace '^sha256:', '').ToUpper()
if ($checksum.Length -ne 64) { throw "no usable digest on the asset" }

$build = Join-Path $here 'build'
Remove-Item $build -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory "$build\tools" | Out-Null
(Get-Content "$here\ailimits.nuspec" -Raw) -replace '\{\{VERSION\}\}', $Version |
  Set-Content "$build\ailimits.nuspec" -Encoding utf8
foreach ($f in Get-ChildItem "$here\tools") {
  (Get-Content $f.FullName -Raw) -replace '\{\{VERSION\}\}', $Version -replace '\{\{CHECKSUM\}\}', $checksum |
    Set-Content "$build\tools\$($f.Name)" -Encoding utf8
}
choco pack "$build\ailimits.nuspec" --outputdirectory $here
Write-Host "packed: $here\ailimits.$Version.nupkg"
