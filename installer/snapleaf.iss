#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

[Setup]
AppId={{694F8F31-7809-479B-B5AB-982152DAFE67}
AppName=Snapleaf
AppVersion={#AppVersion}
AppPublisher=Sunghwan Yoo
AppPublisherURL=https://github.com/shyoo/snapleaf
DefaultDirName={localappdata}\Programs\Snapleaf
DefaultGroupName=Snapleaf
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
UninstallDisplayIcon={app}\snapleaf.exe
SetupIconFile=..\assets\snapleaf.ico
LicenseFile=..\LICENSE
OutputDir=..\dist
OutputBaseFilename=snapleaf-{#AppVersion}-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=no
RestartApplications=no

[Files]
Source: "..\dist\snapleaf.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Icons]
Name: "{group}\Snapleaf"; Filename: "{app}\snapleaf.exe"
Name: "{autodesktop}\Snapleaf"; Filename: "{app}\snapleaf.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\snapleaf.exe"; Description: "Open Snapleaf"; Flags: nowait postinstall skipifsilent

[Code]
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if CheckForMutexes('Local\Snapleaf.SingleInstance') then
    Result := 'Exit Snapleaf from its tray menu before continuing. Save any annotated images first.';
end;

function InitializeUninstall(): Boolean;
begin
  Result := not CheckForMutexes('Local\Snapleaf.SingleInstance');
  if not Result then
    MsgBox('Save your images and exit Snapleaf from its tray menu before uninstalling.', mbInformation, MB_OK);
end;
