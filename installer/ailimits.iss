; installer/ailimits.iss — Inno Setup installer script for the AI Limits widget.
;
; Build: ISCC.exe installer\ailimits.iss
; Binaries are taken from target\release-min (cargo build --profile release-min).

#define AppName "AI Limits"
#define AppVersion "0.6.0"
#define AppExe "ailimits.exe"

[Setup]
AppId={{7A1B9C44-5E2D-4F8A-9C3B-AILIMITS0001}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=napxlexn
AppPublisherURL=https://github.com/napxlexn/ailimits
LicenseFile=..\LICENSE
DefaultDirName={localappdata}\AiLimits
DefaultGroupName={#AppName}
; Per-user install, no admin rights.
PrivilegesRequired=lowest
OutputDir=..\target\installer
OutputBaseFilename=AiLimits-Setup-{#AppVersion}
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
Compression=lzma2/max
SolidCompression=yes
; Close the running widget before updating.
CloseApplications=yes
WizardStyle=modern
DisableProgramGroupPage=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "ukrainian"; MessagesFile: "compiler:Languages\Ukrainian.isl"

[CustomMessages]
english.Autostart=Start with Windows
ukrainian.Autostart=Запускати при старті Windows
english.RunApp=Launch {#AppName}
ukrainian.RunApp=Запустити {#AppName}

[Tasks]
Name: "autostart"; Description: "{cm:Autostart}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked

[Files]
Source: "..\target\release-min\ailimits.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release-min\ailimits-auth.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\TRADEMARKS.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; Autostart via HKCU Run — removed by the uninstaller.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; \
    ValueName: "AiLimits"; ValueData: """{app}\{#AppExe}"""; Tasks: autostart; \
    Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:RunApp}"; \
    Flags: nowait postinstall skipifsilent

[UninstallRun]
; Stop the widget before uninstalling.
Filename: "taskkill"; Parameters: "/im {#AppExe} /f"; Flags: runhidden; RunOnceId: "KillWidget"
; Remove the app-created secrets from Credential Manager (API keys, PATs,
; usage tokens) — without this they would outlive the uninstall. A missing
; entry is a no-op (exit 0). Runs before files are deleted.
Filename: "{app}\ailimits-auth.exe"; Parameters: "remove claude"; Flags: runhidden; RunOnceId: "RmKeyClaude"
Filename: "{app}\ailimits-auth.exe"; Parameters: "remove copilot"; Flags: runhidden; RunOnceId: "RmKeyCopilot"
Filename: "{app}\ailimits-auth.exe"; Parameters: "remove-usage-token claude"; Flags: runhidden; RunOnceId: "RmUsageClaude"
Filename: "{app}\ailimits-auth.exe"; Parameters: "remove-usage-token codex"; Flags: runhidden; RunOnceId: "RmUsageCodex"

[UninstallDelete]
; Remove the user config and cache as well.
Type: filesandordirs; Name: "{userappdata}\AiLimits"
