$ErrorActionPreference = 'Stop'

# Inno per-user uninstall entry written by the installer.
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{7A1B9C44-5E2D-4F8A-9C3B-AILIMITS0001}_is1'
$entry = Get-ItemProperty $key -ErrorAction SilentlyContinue
if ($entry -and $entry.QuietUninstallString) {
  # QuietUninstallString is `"unins000.exe" /SILENT`; run it as-is.
  $exe, $args = $entry.QuietUninstallString -split '(?<=")\s+', 2
  Start-Process -FilePath ($exe.Trim('"')) -ArgumentList $args -Wait
} else {
  Write-Warning 'AI Limits uninstall entry not found - it may already be removed.'
}
