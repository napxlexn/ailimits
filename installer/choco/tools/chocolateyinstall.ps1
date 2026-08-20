$ErrorActionPreference = 'Stop'

$packageArgs = @{
  packageName    = 'ailimits'
  fileType       = 'exe'
  url64bit       = 'https://github.com/napxlexn/ailimits/releases/download/v{{VERSION}}/AiLimits-Setup-{{VERSION}}.exe'
  checksum64     = '{{CHECKSUM}}'
  checksumType64 = 'sha256'
  # Inno Setup, per-user install, no admin rights.
  silentArgs     = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART'
  validExitCodes = @(0)
}
Install-ChocolateyPackage @packageArgs
