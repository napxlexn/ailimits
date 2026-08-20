# check-manifests.ps1 — pre-submission gate for the winget manifests.
#
# Run this before every submission to microsoft/winget-pkgs. It exists because
# two separate mistakes have already cost real time on PR #407754:
#
#   * the word "explicitly" in the Description tripped the Policy-Test-2.7
#     adult-content label — winget-pkgs runs substring matches, not a content
#     review, and arguing the false positive in comments achieved nothing;
#   * a placeholder SHA256 of all zeroes sits in the manifests between the
#     version bump and the release build, and nothing but attention stopped it
#     being submitted.
#
# Exit code 0 = safe to submit. Non-zero = do not submit.
#
#   pwsh installer\check-manifests.ps1

[CmdletBinding()]
param(
    [string]$ManifestDir = (Join-Path $PSScriptRoot 'winget'),
    [string]$CargoToml   = (Join-Path (Split-Path $PSScriptRoot -Parent) 'Cargo.toml')
)

$ErrorActionPreference = 'Stop'
$problems = @()
$checks   = 0

function Fail($msg) { $script:problems += $msg }
function Pass($msg) { Write-Host "  ok   $msg" -ForegroundColor DarkGreen }

Write-Host "checking manifests in $ManifestDir" -ForegroundColor Cyan

$files = Get-ChildItem -Path $ManifestDir -Filter '*.yaml' -ErrorAction SilentlyContinue
if (-not $files) { Write-Host "no manifests found in $ManifestDir" -ForegroundColor Red; exit 2 }

# --- 1. policy trigger substrings -------------------------------------------
# The matcher is a substring match, so "explicitly" hits on "explicit". Keep the
# meaning and reword: explicitly -> manually / directly, hack -> troubleshoot.
$triggers = 'explicit|adult|xxx|porn|nude|sex|erotic|gambl|casino|drug|weapon|hack|crack|keygen|torrent|pirat'
$checks++
$hits = Select-String -Path $files.FullName -Pattern $triggers -AllMatches
if ($hits) {
    foreach ($h in $hits) {
        Fail "policy trigger word in $($h.Filename):$($h.LineNumber) -> $($h.Line.Trim())"
    }
} else {
    Pass 'no policy trigger substrings'
}

# --- 2. the installer SHA256 must be real -----------------------------------
# A published installer's hash is 64 hex digits and is never all zeroes. The
# placeholder is deliberate between a version bump and the release build, which
# is exactly why it needs a gate rather than vigilance.
$installer = Join-Path $ManifestDir 'napxlexn.AILimits.installer.yaml'
$checks++
if (Test-Path $installer) {
    $shaLine = Select-String -Path $installer -Pattern '^\s*InstallerSha256:\s*([0-9A-Fa-f]+)\s*$'
    if (-not $shaLine) {
        Fail 'InstallerSha256 not found in the installer manifest'
    } else {
        $sha = $shaLine.Matches[0].Groups[1].Value
        if ($sha -match '^0+$') {
            Fail "InstallerSha256 is still the all-zero placeholder - run the release build and paste the real hash"
        } elseif ($sha.Length -ne 64) {
            Fail "InstallerSha256 is $($sha.Length) chars, expected 64"
        } else {
            Pass "InstallerSha256 looks real ($($sha.Substring(0,8))...)"
        }
    }
} else {
    Fail "installer manifest missing: $installer"
}

# --- 3. one version everywhere ----------------------------------------------
# PackageVersion is repeated in all three manifests, appears twice inside the
# installer URL, and must match the crate the release was built from.
$checks++
$versions = @{}
foreach ($f in $files) {
    $m = Select-String -Path $f.FullName -Pattern '^PackageVersion:\s*(\S+)\s*$'
    if ($m) { $versions[$f.Name] = $m.Matches[0].Groups[1].Value }
    else    { Fail "no PackageVersion in $($f.Name)" }
}
# @() matters: with one distinct value the pipeline yields a bare string, and
# indexing a string returns its first CHARACTER — "0.6.1"[0] is "0".
$distinct = @($versions.Values | Sort-Object -Unique)
if ($distinct.Count -gt 1) {
    Fail "PackageVersion differs between manifests: $($versions.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" } | Join-String -Separator ', ')"
} elseif ($distinct.Count -eq 1) {
    Pass "PackageVersion is $($distinct[0]) in all manifests"
}
$pkgVersion = if ($distinct.Count -eq 1) { $distinct[0] } else { $null }

# --- 4. the crate version must agree ----------------------------------------
$checks++
if ($pkgVersion -and (Test-Path $CargoToml)) {
    $cargoLine = Select-String -Path $CargoToml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
    if ($cargoLine) {
        $cargoVersion = $cargoLine.Matches[0].Groups[1].Value
        if ($cargoVersion -ne $pkgVersion) {
            Fail "Cargo.toml is $cargoVersion but the manifests say $pkgVersion - the manifest must describe the version actually released"
        } else {
            Pass "Cargo.toml agrees ($cargoVersion)"
        }
    }
}

# --- 5. the download URL must point at that version -------------------------
$checks++
if ($pkgVersion -and (Test-Path $installer)) {
    $urlLine = Select-String -Path $installer -Pattern 'InstallerUrl:\s*(\S+)'
    if ($urlLine) {
        $url = $urlLine.Matches[0].Groups[1].Value
        if ($url -notmatch [regex]::Escape("v$pkgVersion/") -or $url -notmatch [regex]::Escape($pkgVersion)) {
            Fail "InstallerUrl does not carry version ${pkgVersion}: $url"
        } else {
            Pass 'InstallerUrl points at the matching release tag'
        }
    } else {
        Fail 'InstallerUrl not found'
    }
}

# --- verdict ----------------------------------------------------------------
Write-Host ''
if ($problems.Count -gt 0) {
    Write-Host "DO NOT SUBMIT - $($problems.Count) problem(s):" -ForegroundColor Red
    foreach ($p in $problems) { Write-Host "  * $p" -ForegroundColor Red }
    exit 1
}
Write-Host "all $checks checks passed - safe to submit" -ForegroundColor Green
exit 0
