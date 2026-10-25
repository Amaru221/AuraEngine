; Inno Setup script for Aura Engine.
; Build with: .\scripts\build-installer.ps1
;
; The version can be overridden from the command line:
;   ISCC.exe /DMyAppVersion=1.0.17 installer\aura.iss

#ifndef MyAppName
  #define MyAppName "Aura Engine"
#endif

#ifndef MyAppVersion
  #define MyAppVersion "1.0.17"
#endif

#define MyAppPublisher "Amaru221"
#define MyAppURL "https://github.com/Amaru221/AuraEngine"
#define MyAppExeName "aura.exe"

[Setup]
AppId={{7A3E2C41-9B6D-4F0A-9C2E-A1B2C3D4E5F6}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
AppCopyright=Copyright (C) 2026 Amaru221 and contributors
VersionInfoVersion={#MyAppVersion}.0
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription={#MyAppName} installer
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersion}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
AllowNoIcons=yes
LicenseFile=..\LICENSE
InfoBeforeFile=..\NOTICE
OutputDir=..\dist
OutputBaseFilename=aura-v{#MyAppVersion}-windows-x64-setup
SetupIconFile=..\assets\aura.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
CloseApplications=yes
RestartApplications=no
; No AppMutex here: PrepareToInstall quits a running instance explicitly,
; which also works in silent/upgrade installs.
DisableWelcomePage=no
DisableReadyPage=no
DisableFinishedPage=no
SetupLogging=yes
UsePreviousAppDir=yes
UsePreviousGroup=yes
UsePreviousTasks=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[CustomMessages]
english.StartWithWindows=Start Aura Engine with Windows
spanish.StartWithWindows=Iniciar Aura Engine con Windows
english.TrustCertificate=Trust the Aura self-signed certificate (recommended for this build)
spanish.TrustCertificate=Confiar en el certificado autofirmado de Aura (recomendado para esta compilacion)
english.StartupGroup=Startup
spanish.StartupGroup=Inicio
english.SecurityGroup=Security
spanish.SecurityGroup=Seguridad
english.RemoveUserData=Remove Aura Engine settings, cache and downloaded wallpapers?
spanish.RemoveUserData=Eliminar la configuracion, cache y fondos descargados de Aura Engine?

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "autostart"; Description: "{cm:StartWithWindows}"; GroupDescription: "{cm:StartupGroup}"
Name: "trustcert"; Description: "{cm:TrustCertificate}"; GroupDescription: "{cm:SecurityGroup}"; Flags: unchecked

[Files]
Source: "..\target\release\aura.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\screenshots\*"; DestDir: "{app}\assets\screenshots"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\NOTICE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_LICENSES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_LICENSES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\SOURCE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\certs\aura-selfsigned.cer"; DestDir: "{app}\certs"; Flags: ignoreversion skipifsourcedoesntexist
Source: "mpv.exe"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

[InstallDelete]
; Remove the shortcut and executable created by the older install.ps1 flow.
Type: files; Name: "{userprograms}\Aura Engine.lnk"
Type: files; Name: "{userdesktop}\Aura Engine.lnk"
Type: files; Name: "{localappdata}\Aura\bin\aura.exe"

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Comment: "Aura Engine - Live Wallpaper Manager"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\aura.exe"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppExeName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\aura.exe"; ValueType: string; ValueName: "Path"; ValueData: "{app}"; Flags: uninsdeletekey

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent
Filename: "{cmd}"; Parameters: "/c certutil -addstore -f -user Root ""{app}\certs\aura-selfsigned.cer"" & certutil -addstore -f -user TrustedPublisher ""{app}\certs\aura-selfsigned.cer"""; Flags: runhidden; Tasks: trustcert; Check: CertExists

[UninstallRun]
Filename: "{app}\{#MyAppExeName}"; Parameters: "quit"; Flags: runhidden; RunOnceId: "QuitAura"
Filename: "{cmd}"; Parameters: "/c ping 127.0.0.1 -n 3 >nul & taskkill /IM aura.exe /F >nul 2>&1"; Flags: runhidden; RunOnceId: "KillAura"

[UninstallDelete]
Type: dirifempty; Name: "{app}\certs"

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  RunValue = 'AuraEngine';

function CertExists: Boolean;
begin
  Result := FileExists(ExpandConstant('{app}\certs\aura-selfsigned.cer'));
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Exe: String;
begin
  if CurStep = ssInstall then
  begin
    // Manage autostart explicitly: remove the value left by the legacy
    // PowerShell install or by a previous version, then recreate it only
    // when the user selected the task.
    RegDeleteValue(HKEY_CURRENT_USER, RunKey, RunValue);
    if WizardIsTaskSelected('autostart') then
    begin
      Exe := ExpandConstant('{app}\aura.exe');
      RegWriteStringValue(HKEY_CURRENT_USER, RunKey, RunValue,
        '"' + Exe + '" --autostart');
    end;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
  I: Integer;
begin
  // Ask a running instance to quit so the executable can be replaced.
  if FileExists(ExpandConstant('{app}\aura.exe')) then
  begin
    Exec(ExpandConstant('{app}\aura.exe'), 'quit', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    Sleep(1500);
  end;
  if FileExists(ExpandConstant('{localappdata}\Aura\bin\aura.exe')) then
  begin
    Exec(ExpandConstant('{localappdata}\Aura\bin\aura.exe'), 'quit', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    Sleep(1500);
  end;

  // Give the application a moment to release the mutex and the executable.
  for I := 1 to 10 do
  begin
    if not CheckForMutexes('AuraEngine.SingleInstance') then
      Break;
    Sleep(500);
  end;

  // Last resort: terminate a stuck instance.
  if CheckForMutexes('AuraEngine.SingleInstance') then
    Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM aura.exe /F', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);

  Result := '';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ResultCode: Integer;
begin
  if CurUninstallStep = usUninstall then
  begin
    RegDeleteValue(HKEY_CURRENT_USER, RunKey, RunValue);
    if FileExists(ExpandConstant('{app}\aura.exe')) then
    begin
      Exec(ExpandConstant('{app}\aura.exe'), 'quit', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
      Sleep(1500);
    end;
    Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM aura.exe /F', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  end;

  if (CurUninstallStep = usPostUninstall) and (not UninstallSilent) then
  begin
    if MsgBox(ExpandConstant('{cm:RemoveUserData}') + #13#10 + #13#10 +
              '%APPDATA%\Aura' + #13#10 + '%LOCALAPPDATA%\Aura',
              mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
    begin
      DelTree(ExpandConstant('{userappdata}\Aura'), True, True, True);
      DelTree(ExpandConstant('{localappdata}\Aura'), True, True, True);
    end;
  end;
end;
