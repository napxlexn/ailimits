# AI Limits terminal installer.
#
#   irm https://raw.githubusercontent.com/napxlexn/ailimits/master/install.ps1 | iex
#
# Downloads the latest release installer, verifies its SHA-256 against the
# digest GitHub publishes for the asset, and runs a silent per-user install
# (no admin rights). No digest, or a mismatch, means no install - the same
# digest rule the app's own auto-updater enforces (which also checks the
# release's Ed25519 signature, the .sig asset beside the installer).
$ErrorActionPreference = 'Stop'

$release = Invoke-RestMethod 'https://api.github.com/repos/napxlexn/ailimits/releases/latest'
$asset = $release.assets | Where-Object { $_.name -like 'AiLimits-Setup-*.exe' } | Select-Object -First 1
if (-not $asset) { throw 'no installer asset found on the latest release' }

$expected = $asset.digest -replace '^sha256:', ''
if (-not $expected) { throw "the release publishes no digest for $($asset.name) - refusing to install unverified" }

$path = Join-Path $env:TEMP $asset.name
Write-Host "downloading $($asset.name) ($([math]::Round($asset.size / 1MB, 1)) MB)..."
Invoke-WebRequest $asset.browser_download_url -OutFile $path

$actual = (Get-FileHash $path -Algorithm SHA256).Hash
if ($actual -ne $expected.ToUpper()) {
    Remove-Item $path -Force
    throw "SHA-256 mismatch: expected $expected, got $actual - download discarded"
}
Write-Host 'checksum verified'

Start-Process -FilePath $path -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait
Remove-Item $path -Force
Write-Host "AI Limits $($release.tag_name) installed. Start it from the Start menu; on a first start it asks which of the tools found on this PC to show."
