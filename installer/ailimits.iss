; installer/ailimits.iss — Inno Setup installer script for the AI Limits widget.
;
; Build: ISCC.exe installer\ailimits.iss
; Binaries are taken from target\release-min (cargo build --profile release-min).

#define AppName "AI Limits"
#define AppVersion "0.7.0"
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
; The AppUserModelID is what toasts are attributed to; the app registers the
; same id per user, so this is the belt to that braces.
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"; AppUserModelID: "napxlexn.AILimits"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon; AppUserModelID: "napxlexn.AILimits"

[Registry]
; Autostart via HKCU Run — removed by the uninstaller.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; \
    ValueName: "AiLimits"; ValueData: """{app}\{#AppExe}"""; Tasks: autostart; \
    Flags: uninsdeletevalue
; The toast identity the app registers on first run (display name + icon);
; written here too so the uninstaller knows to remove the key.
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\napxlexn.AILimits"; ValueType: string; \
    ValueName: "DisplayName"; ValueData: "{#AppName}"; Flags: uninsdeletekey

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:RunApp}"; \
    Flags: nowait postinstall skipifsilent
; The app's own silent self-update (src/updater.rs) exits so its files can be
; replaced and asks for itself back with /RELAUNCH=1: start it again from the
; directory just installed. A plain silent install (winget, Chocolatey) does
; not pass the switch and starts nothing, as before.
Filename: "{app}\{#AppExe}"; Flags: nowait; Check: RelaunchAfterUpdate

[UninstallRun]
; Stop the widget before uninstalling.
Filename: "taskkill"; Parameters: "/im {#AppExe} /f"; Flags: runhidden; RunOnceId: "KillWidget"
; Remove the app-created secrets from Credential Manager (API keys, PATs,
; usage tokens, the app's own Cursor session) — without this they would
; outlive the uninstall. A missing entry is a no-op (exit 0); nothing here
; touches the network. Runs before files are deleted. Every label the app
; writes must be removed here: tests/uninstall_labels_test.rs checks it.
Filename: "{app}\ailimits-auth.exe"; Parameters: "remove claude"; Flags: runhidden; RunOnceId: "RmKeyClaude"
Filename: "{app}\ailimits-auth.exe"; Parameters: "remove copilot"; Flags: runhidden; RunOnceId: "RmKeyCopilot"
Filename: "{app}\ailimits-auth.exe"; Parameters: "remove-usage-token claude"; Flags: runhidden; RunOnceId: "RmUsageClaude"
Filename: "{app}\ailimits-auth.exe"; Parameters: "remove-usage-token codex"; Flags: runhidden; RunOnceId: "RmUsageCodex"
Filename: "{app}\ailimits-auth.exe"; Parameters: "logout cursor"; Flags: runhidden; RunOnceId: "RmCursorSession"

[UninstallDelete]
; Remove the user config and cache as well.
Type: filesandordirs; Name: "{userappdata}\AiLimits"

[Code]
// The [Run] relaunch after the app's own silent self-update: only in a silent
// run, and only when the updater passed /RELAUNCH=1 (src/updater.rs,
// RELAUNCH_SWITCH). An interactive install has its own "Launch" checkbox.
function RelaunchAfterUpdate: Boolean;
begin
  Result := WizardSilent and (ExpandConstant('{param:RELAUNCH|0}') = '1');
end;
