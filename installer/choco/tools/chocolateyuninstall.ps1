$ErrorActionPreference = 'Stop'

# Inno per-user uninstall entry written by the installer.
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{7A1B9C44-5E2D-4F8A-9C3B-AILIMITS0001}_is1'
$entry = Get-ItemProperty $key -ErrorAction SilentlyContinue
if ($entry -and $entry.QuietUninstallString) {
  # The uninstaller Inno wrote, run with no window at all; its exit code
  # is checked by Uninstall-ChocolateyPackage.
  $exe = ($entry.QuietUninstallString -split '(?<=")\s+', 2)[0].Trim('"')
  Uninstall-ChocolateyPackage -PackageName $env:ChocolateyPackageName -FileType 'exe' `
    -SilentArgs '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART' -File $exe
} else {
  Write-Warning 'AI Limits uninstall entry not found - it may already be removed.'
}
