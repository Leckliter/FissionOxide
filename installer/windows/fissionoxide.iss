; The FissionOxide installer for Windows (Inno Setup 6.4 or later).
;
; Build it after `cargo build --release -p fissionoxide -p fissionoxide-cli`:
;
;   iscc /DAppVersion=0.2.0 installer\windows\fissionoxide.iss
;
; which writes dist\FissionOxide-0.2.0-x64-setup.exe. The release workflow
; (.github/workflows/release.yml) does this for every tag vX.Y.Z.
;
; Updating: every version has the same AppId, so a newer installer run over an installed copy
; replaces it in place (same folder, same Start menu entry, one line in Settings > Apps) and
; keeps the user's choices. The app's updater runs it with /SILENT /RELAUNCH=1 (a progress window,
; then FissionOxide starts again) or /VERYSILENT (as the app closes). Running FissionOxide is
; closed through the Restart Manager first.

#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif
; where the release executables are, relative to this file
#ifndef SourceDir
  #define SourceDir "..\..\target\release"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\dist"
#endif

#define AppName "FissionOxide"
#define AppExe "fissionoxide.exe"
#define CliExe "fissionoxide-cli.exe"
#define Repo "https://github.com/leckliter/FissionOxide"
#define Brand "..\..\brand\windows"
; file version resources take only numbers: 0.2.0-beta.1 -> 0.2.0
#if Pos("-", AppVersion) > 0
  #define NumericVersion Copy(AppVersion, 1, Pos("-", AppVersion) - 1)
#else
  #define NumericVersion AppVersion
#endif

[Setup]
; Never change the AppId: it is how Windows and later installers recognise the installed app.
AppId={{6F0B8E52-3C1D-4A7E-9B25-F0C5D1A8E7B3}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppName}
AppPublisherURL={#Repo}
AppSupportURL={#Repo}/issues
AppUpdatesURL={#Repo}/releases
AppCopyright=FissionOxide contributors. MIT or Apache-2.0.
AppComments=Parametric 3D CAD
VersionInfoVersion={#NumericVersion}
VersionInfoProductName={#AppName}
VersionInfoDescription={#AppName} Setup
; Settings > Apps: name, icon, publisher, version and size come from here
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExe}
; per-user by default (no administrator rights, and silent updates need no prompt); the wizard
; offers "Install for all users" too
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableWelcomePage=no
; on an update the folder page is skipped and the old folder kept
DisableDirPage=auto
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
WizardStyle=modern
WizardSizePercent=110
WizardImageFile={#Brand}\wizard-100.bmp,{#Brand}\wizard-150.bmp,{#Brand}\wizard-200.bmp
WizardSmallImageFile={#Brand}\wizard-small-100.bmp,{#Brand}\wizard-small-150.bmp,{#Brand}\wizard-small-200.bmp
SetupIconFile={#Brand}\fissionoxide.ico
OutputDir={#OutputDir}
OutputBaseFilename={#AppName}-{#AppVersion}-x64-setup
Compression=lzma2/ultra64
SolidCompression=yes
; compress in a separate process: the 32-bit compiler runs out of memory doing ultra64 itself
LZMAUseSeparateProcess=yes
ChangesAssociations=yes
; close a running FissionOxide before replacing it; the updater restarts it (see [Run])
CloseApplications=yes
CloseApplicationsFilter=*.exe
RestartApplications=no
SetupLogging=yes
ShowLanguageDialog=no
; code signing: the release workflow passes /DSignTool and /Ssigntool=... when it has a certificate
#ifdef SignTool
SignTool=signtool
SignedUninstaller=yes
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
AssocGroup=File types:
AssocFox=Open FissionOxide designs (.fox, .foxz) with FissionOxide
UpdatesCaption=Updates
UpdatesTitle=How should FissionOxide stay up to date?
UpdatesText=FissionOxide can look for new versions when it starts (about once a day). You can change this at any time in Help > Check for Updates.
UpdateAuto=Download and install updates automatically (recommended)
UpdateNotify=Tell me when an update is available
UpdateOff=Don't check for updates
UpdatesMemo=Updates:
OlderVersion=FissionOxide %1 is installed, which is newer than this installer (%2).%n%nInstall %2 anyway?
RemoveSettings=Also remove FissionOxide's settings and downloaded updates from this computer?%n%nYour designs and projects (in %1) are kept either way.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "assoc"; Description: "{cm:AssocFox}"; GroupDescription: "{cm:AssocGroup}"

[Files]
Source: "{#SourceDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\{#CliExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; DestName: "README.md"; Flags: ignoreversion
; the fox in 3D (brand/model.py); the app also adds it to the data hub the first time it starts
Source: "..\..\brand\model\FissionOxide.fox"; DestDir: "{app}\Samples"; Flags: ignoreversion
; the ripple loader's frames for the progress page (made by brand/build.py; see CreateLoader)
Source: "{#Brand}\loader\*.bmp"; Flags: dontcopy

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; AppUserModelID: "FissionOxide.FissionOxide"; Comment: "Parametric 3D CAD"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; AppUserModelID: "FissionOxide.FissionOxide"; Comment: "Parametric 3D CAD"; Tasks: desktopicon

[Registry]
; Win+R "fissionoxide"
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\{#AppExe}"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExe}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\{#AppExe}"; ValueType: string; ValueName: "Path"; ValueData: "{app}"
; "Open with" for designs and the formats FissionOxide imports
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".fox"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".foxz"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".step"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".stp"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".iges"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".igs"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".stl"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".3mf"; ValueData: ""
; .fox and .foxz open in FissionOxide (the assoc task)
Root: HKA; Subkey: "Software\Classes\.fox\OpenWithProgids"; ValueType: string; ValueName: "FissionOxide.Design"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.foxz\OpenWithProgids"; ValueType: string; ValueName: "FissionOxide.Design"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\FissionOxide.Design"; ValueType: string; ValueName: ""; ValueData: "FissionOxide Design"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\FissionOxide.Design\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExe},0"; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\FissionOxide.Design\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: assoc

[INI]
; read by the app (fox-desktop's settings.rs); the choice from the Updates page
Filename: "{localappdata}\FissionOxide\settings.ini"; Section: "Updates"; Key: "Mode"; String: "{code:UpdateMode}"

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent runasoriginaluser
; after an update the app asked for (/RELAUNCH=1)
Filename: "{app}\{#AppExe}"; Flags: nowait runasoriginaluser; Check: Relaunch

[UninstallDelete]
Type: filesandordirs; Name: "{localappdata}\FissionOxide\updates"

[Code]
const
  LoaderFrames = 20;
  { ms a frame: the loop takes 1.6 s, like brand/logo/motion/loader-ripple.svg }
  LoaderTick = 80;

var
  UpdatesPage: TInputOptionWizardPage;
  Loader: TBitmapImage;
  LoaderBitmaps: array of TBitmap;
  LoaderFrame: Integer;
  LoaderTimer: LongWord;

function SetTimer(hWnd: LongWord; nIDEvent, uElapse: LongWord; lpTimerFunc: LongWord): LongWord;
  external 'SetTimer@user32.dll stdcall';
function KillTimer(hWnd: LongWord; uIDEvent: LongWord): BOOL;
  external 'KillTimer@user32.dll stdcall';

function SettingsFile: String;
begin
  Result := ExpandConstant('{localappdata}\FissionOxide\settings.ini');
end;

{ The installed version, from the uninstall entry an earlier install left, or '' }
function InstalledVersion: String;
var
  Key: String;
begin
  Result := '';
  Key := 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{6F0B8E52-3C1D-4A7E-9B25-F0C5D1A8E7B3}_is1';
  if not RegQueryStringValue(HKCU, Key, 'DisplayVersion', Result) then
    if not RegQueryStringValue(HKLM64, Key, 'DisplayVersion', Result) then
      Result := '';
end;

{ The numeric part of a version (before any -pre-release), packed for comparing }
function Packed(V: String; var P: Int64): Boolean;
var
  I: Integer;
begin
  I := Pos('-', V);
  if I > 0 then
    V := Copy(V, 1, I - 1);
  Result := StrToVersion(V, P);
end;

function InitializeSetup: Boolean;
var
  Old: String;
  A, B: Int64;
begin
  Result := True;
  Old := InstalledVersion;
  if (Old <> '') and Packed(Old, A) and Packed('{#AppVersion}', B) and (ComparePackedVersion(A, B) > 0) then
    Result := SuppressibleMsgBox(FmtMessage(CustomMessage('OlderVersion'), [Old, '{#AppVersion}']),
      mbConfirmation, MB_YESNO or MB_DEFBUTTON2, IDNO) = IDYES;
end;

{ The FissionOxide ripple under the progress bar while files are copied: the brand's loading
  indicator, as still frames because the wizard can only show bitmaps }
procedure CreateLoader;
var
  Scale, I: Integer;
  Name: String;
begin
  { the frames drawn for the screen's scaling: 100, 150 or 200 % }
  Scale := ScaleX(100);
  if Scale >= 175 then
    Scale := 200
  else if Scale >= 125 then
    Scale := 150
  else
    Scale := 100;
  SetArrayLength(LoaderBitmaps, LoaderFrames);
  for I := 0 to LoaderFrames - 1 do
  begin
    Name := Format('loader-%d-%.2d.bmp', [Scale, I]);
    ExtractTemporaryFile(Name);
    LoaderBitmaps[I] := TBitmap.Create;
    LoaderBitmaps[I].LoadFromFile(ExpandConstant('{tmp}\') + Name);
  end;
  Loader := TBitmapImage.Create(WizardForm);
  Loader.Parent := WizardForm.InstallingPage;
  Loader.Width := LoaderBitmaps[0].Width;
  Loader.Height := LoaderBitmaps[0].Height;
  Loader.Left := (WizardForm.InstallingPage.ClientWidth - Loader.Width) div 2;
  Loader.Top := WizardForm.ProgressGauge.Top + WizardForm.ProgressGauge.Height + ScaleY(32);
  Loader.Bitmap := LoaderBitmaps[0];
end;

procedure LoaderStep(Wnd, Msg, IdEvent, Ticks: LongWord);
begin
  LoaderFrame := (LoaderFrame + 1) mod LoaderFrames;
  Loader.Bitmap := LoaderBitmaps[LoaderFrame];
end;

procedure StopLoader;
begin
  if LoaderTimer <> 0 then
  begin
    KillTimer(0, LoaderTimer);
    LoaderTimer := 0;
  end;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if (CurPageID = wpInstalling) and (LoaderTimer = 0) and (Loader <> nil) then
    LoaderTimer := SetTimer(0, 0, LoaderTick, CreateCallback(@LoaderStep));
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    StopLoader;
end;

procedure DeinitializeSetup;
begin
  StopLoader;
end;

procedure InitializeWizard;
var
  Mode: String;
begin
  CreateLoader;
  UpdatesPage := CreateInputOptionPage(wpSelectTasks, CustomMessage('UpdatesCaption'),
    CustomMessage('UpdatesTitle'), CustomMessage('UpdatesText'), True, False);
  UpdatesPage.Add(CustomMessage('UpdateAuto'));
  UpdatesPage.Add(CustomMessage('UpdateNotify'));
  UpdatesPage.Add(CustomMessage('UpdateOff'));
  { an update keeps the choice made before (also when changed in the app) }
  Mode := Lowercase(GetIniString('Updates', 'Mode', 'auto', SettingsFile));
  if Mode = 'notify' then
    UpdatesPage.SelectedValueIndex := 1
  else if Mode = 'off' then
    UpdatesPage.SelectedValueIndex := 2
  else
    UpdatesPage.SelectedValueIndex := 0;
end;

function UpdateMode(Param: String): String;
begin
  case UpdatesPage.SelectedValueIndex of
    1: Result := 'notify';
    2: Result := 'off';
  else
    Result := 'auto';
  end;
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo,
  MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
begin
  Result := MemoDirInfo;
  if MemoTasksInfo <> '' then
    Result := Result + NewLine + NewLine + MemoTasksInfo;
  Result := Result + NewLine + NewLine + CustomMessage('UpdatesMemo') + NewLine + Space
    + UpdatesPage.CheckListBox.ItemCaption[UpdatesPage.SelectedValueIndex];
end;

function Relaunch: Boolean;
begin
  Result := ExpandConstant('{param:RELAUNCH|0}') = '1';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    if SuppressibleMsgBox(FmtMessage(CustomMessage('RemoveSettings'), [ExpandConstant('{userappdata}\FissionOxide')]),
      mbConfirmation, MB_YESNO or MB_DEFBUTTON2, IDNO) = IDYES then
      DelTree(ExpandConstant('{localappdata}\FissionOxide'), True, True, True);
end;
