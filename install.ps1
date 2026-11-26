# AI Limits terminal installer.
#
#   irm https://raw.githubusercontent.com/napxlexn/ailimits/master/install.ps1 | iex
#
# Downloads the latest release installer, verifies its SHA-256 against the
# digest GitHub publishes for the asset, and runs a silent per-user install
# (no admin rights). No digest, or a mismatch, means no install - the same
# digest rule the app's own auto-updater enforces. This script does NOT check
# the release's Ed25519 signature (the .sig asset beside the installer):
# PowerShell has no Ed25519 of its own. Only the app's updater checks it.
$ErrorActionPreference = 'Stop'

$release = Invoke-RestMethod 'https://api.github.com/repos/napxlexn/ailimits/releases/latest'
$asset = $release.assets | Where-Object { $_.name -like 'AiLimits-Setup-*.exe' } | Select-Object -First 1
if (-not $asset) { throw 'no installer asset found on the latest release' }

$expected = $asset.digest -replace '^sha256:', ''
if (-not $expected) { throw "the release publishes no digest for $($asset.name) - refusing to install unverified" }

# A folder of its own with a random name, so nothing else can put a file
# at the path between the check and the start.
$dir = Join-Path $env:TEMP ("ailimits-install-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $dir | Out-Null
$path = Join-Path $dir $asset.name
Write-Host "downloading $($asset.name) ($([math]::Round($asset.size / 1MB, 1)) MB)..."
Invoke-WebRequest $asset.browser_download_url -OutFile $path

$actual = (Get-FileHash $path -Algorithm SHA256).Hash
if ($actual -ne $expected.ToUpper()) {
    Remove-Item $dir -Recurse -Force
    throw "SHA-256 mismatch: expected $expected, got $actual - download discarded"
}
Write-Host 'checksum verified'

Start-Process -FilePath $path -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait
Remove-Item $dir -Recurse -Force
Write-Host "AI Limits $($release.tag_name) installed. Start it from the Start menu; on a first start it asks which of the tools found on this PC to show."
