; Inno Setup script for the Veil privacy browser.
; Built by scripts\build-installer.ps1 — do not run ISCC on it before `cargo build --release`.

#define AppName "Veil Browser"
#define AppVersion "0.2.0"
#define AppExe "veil.exe"

[Setup]
AppId={{6B0E3F7A-3C1D-4D8E-9B57-6E2F1A9C4D21}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Veil By Claude
SetupIconFile=..\target\release\veil.ico
DefaultDirName={autopf}\Veil
DefaultGroupName=Veil
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=VeilSetup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "..\target\release\veil.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\WebView2Loader.dll"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Veil"; Filename: "{app}\{#AppExe}"
Name: "{group}\Uninstall Veil"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Veil"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,Veil}"; Flags: nowait postinstall skipifsilent

[Code]
// Veil renders pages with Microsoft Edge WebView2, which ships with Windows 10/11.
function WebView2Installed(): Boolean;
var
  Version: String;
begin
  Result :=
    RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version) or
    RegQueryStringValue(HKCU, 'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version);
  Result := Result and (Version <> '') and (Version <> '0.0.0.0');
end;

function InitializeSetup(): Boolean;
begin
  Result := True;
  if not WebView2Installed() then
    MsgBox('Veil needs the Microsoft Edge WebView2 Runtime, which was not found.' + #13#10 +
           'Install it from https://go.microsoft.com/fwlink/p/?LinkId=2124703 and Veil will work.',
           mbInformation, MB_OK);
end;
