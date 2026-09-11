# Build the Microsoft Store package from a release-min build.
#
#   pwsh installer/msix/pack.ps1                      # version from Cargo.toml
#   pwsh installer/msix/pack.ps1 -Sign                # plus a self-signed test signature
#   pwsh installer/msix/pack.ps1 -IdentityName napxlexn.AILimits -Publisher "CN=..." -PublisherDisplayName napxlexn
#
# The package for the Store is uploaded UNSIGNED: Partner Center signs it,
# and the identity name and publisher must be the ones Partner Center shows
# once the app name is reserved (Product management > Product identity).
# -Sign is for a local sideload only: it makes a self-signed certificate
# with the manifest's publisher name, signs with it, and prints the two
# commands that install the certificate and the package for testing.
param(
    [string]$Version,
    [string]$IdentityName = 'napxlexn.AILimits',
    [string]$Publisher = 'CN=napxlexn',
    [string]$PublisherDisplayName = 'napxlexn',
    [switch]$Sign
)
$ErrorActionPreference = 'Stop'
$here = Split-Path $MyInvocation.MyCommand.Path
$root = Resolve-Path (Join-Path $here '..\..')

if (-not $Version) {
    $Version = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
}
# MSIX wants four parts; the Store reserves the fourth and requires it to be 0.
$version4 = "$Version.0"

$bin = Join-Path $root 'target\release-min'
foreach ($exe in 'ailimits.exe', 'ailimits-auth.exe') {
    if (-not (Test-Path (Join-Path $bin $exe))) {
        throw "missing $bin\$exe - run: cargo build --profile release-min"
    }
}

# The newest Windows SDK on the machine carries makeappx and signtool.
$kits = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Directory |
    Where-Object { Test-Path (Join-Path $_.FullName 'x64\makeappx.exe') } |
    Sort-Object { [version]$_.Name } | Select-Object -Last 1
if (-not $kits) { throw 'no Windows SDK with makeappx.exe found under Windows Kits\10\bin' }
$makeappx = Join-Path $kits.FullName 'x64\makeappx.exe'
$signtool = Join-Path $kits.FullName 'x64\signtool.exe'

# Stage exactly what ships, nothing from the working tree by accident.
$out = Join-Path $root 'target\msix'
$stage = Join-Path $out 'stage'
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Path (Join-Path $stage 'Assets') -Force | Out-Null
Copy-Item (Join-Path $bin 'ailimits.exe') $stage
Copy-Item (Join-Path $bin 'ailimits-auth.exe') $stage
foreach ($doc in 'README.md', 'LICENSE', 'TRADEMARKS.md') { Copy-Item (Join-Path $root $doc) $stage }
Copy-Item (Join-Path $here 'Assets\*.png') (Join-Path $stage 'Assets')

$manifest = Get-Content (Join-Path $here 'AppxManifest.xml') -Raw
$manifest = $manifest.Replace('{{IDENTITY_NAME}}', $IdentityName).
    Replace('{{PUBLISHER}}', $Publisher).
    Replace('{{VERSION4}}', $version4).
    Replace('{{PUBLISHER_DISPLAY}}', $PublisherDisplayName)
# UTF-8 without a BOM: makeappx rejects a manifest that starts with one.
[IO.File]::WriteAllText((Join-Path $stage 'AppxManifest.xml'), $manifest, [Text.UTF8Encoding]::new($false))

$msix = Join-Path $out "AiLimits-$Version.msix"
if (Test-Path $msix) { Remove-Item $msix -Force }
& $makeappx pack /o /d $stage /p $msix | Out-Null
if ($LASTEXITCODE -ne 0) { throw "makeappx failed ($LASTEXITCODE)" }
Write-Host "packed: $msix"

if ($Sign) {
    # One test certificate per publisher name, kept in the user's store so
    # repeated packs sign with the same one. Never used for anything shipped.
    $cert = Get-ChildItem Cert:\CurrentUser\My |
        Where-Object { $_.Subject -eq $Publisher -and $_.FriendlyName -eq 'AI Limits MSIX test' } |
        Select-Object -First 1
    if (-not $cert) {
        $cert = New-SelfSignedCertificate -Type Custom -Subject $Publisher `
            -KeyUsage DigitalSignature -FriendlyName 'AI Limits MSIX test' `
            -CertStoreLocation Cert:\CurrentUser\My `
            -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
    }
    $cer = Join-Path $out 'AiLimits-test.cer'
    Export-Certificate -Cert $cert -FilePath $cer -Force | Out-Null
    & $signtool sign /fd SHA256 /sha1 $cert.Thumbprint /t http://timestamp.digicert.com $msix | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "signtool failed ($LASTEXITCODE)" }
    Write-Host "signed with the test certificate $($cert.Thumbprint)"
    Write-Host ''
    Write-Host 'To sideload for testing (the first line needs an elevated shell, once):'
    Write-Host "  Import-Certificate -FilePath '$cer' -CertStoreLocation Cert:\LocalMachine\TrustedPeople"
    Write-Host "  Add-AppxPackage '$msix'"
}
