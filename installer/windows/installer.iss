; The project is migrating to the official AdeshLang organization; keep this URL
; as the one easily changed publisher URL for the installer.
#define MyAppURL "https://github.com/adeshlang/adeshlang"
#define MyAppName "AdeshLang"
#define MyAppVersion "0.3.0"
#define MyAppPublisher "AdeshLang"
#define MyAppExeName "adesh.exe"

[Setup]
AppId={{6E48268F-7B2B-450F-A8C5-18159E4712F2}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
AppCopyright=Copyright (C) 2026 AdeshLang
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription=AdeshLang Programming Language Setup
VersionInfoVersion={#MyAppVersion}
VersionInfoCopyright=Copyright (C) 2026 AdeshLang
DefaultDirName={autopf}\AdeshLang
DefaultGroupName=AdeshLang Programming Language
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE
OutputBaseFilename=AdeshLang-0.3.0-x86_64-windows-setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
ChangesEnvironment=yes
SetupIconFile=resources\installer.ico
UninstallDisplayName={#MyAppName} {#MyAppVersion}
UninstallDisplayIcon={app}\bin\{#MyAppExeName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "downloadtoolchain"; Description: "Install pinned LLVM 18.1.8 Toolchain (official LLVM/Clang compiler, lld linker, llvm-ar; configured automatically)"; Flags: checkedonce
Name: "vsbuildtools"; Description: "Install Visual Studio Build Tools + Windows SDK (required for MSVC-target AOT native linking, ~2 GB)"; Flags: unchecked; Check: ShouldShowVSBuildTools
Name: "python"; Description: "Install Python 3.12 (required for full AI features: `adesh ai train/evaluate/generate` and MLIR source builds)"; Flags: unchecked; Check: ShouldShowPython
Name: "buildmlir"; Description: "Build MLIR GPU tools from source (adds 30–90 minutes; requires Visual Studio C++ Build Tools + CMake + Python 3)"; Flags: unchecked
Name: "fileassoc"; Description: "Associate .adl and .adesh files with AdeshLang"; Flags: unchecked
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; The distribution staging script supplies all files below. The toolchain is
; deliberately downloaded by adesh at install time rather than bundled here.
; The default AI model (ai\models) ships with the distribution so `adesh ai`
; works offline right after install; heavier quantizations stay opt-in via
; `adesh ai setup`.
;
; The first two entries are unpacked to {tmp} before the wizard starts so the
; toolchain can be installed right after the destination page (before the
; core files are copied); they are removed when setup exits.
Source: "..\..\dist\windows-x86_64\bin\adesh.exe"; DestDir: "{tmp}"; Flags: ignoreversion deleteafterinstall
Source: "..\manifests\toolchain-manifest.json"; DestDir: "{tmp}"; Flags: ignoreversion deleteafterinstall
Source: "..\..\dist\windows-x86_64\bin\*"; DestDir: "{app}\bin"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\..\dist\windows-x86_64\lib\*"; DestDir: "{app}\lib"; Flags: ignoreversion recursesubdirs createallsubdirs skipifsourcedoesntexist
Source: "..\..\dist\windows-x86_64\std\*"; DestDir: "{app}\std"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\..\dist\windows-x86_64\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\..\dist\windows-x86_64\ai\*"; DestDir: "{app}\ai"; Flags: ignoreversion recursesubdirs createallsubdirs skipifsourcedoesntexist
Source: "..\manifests\toolchain-manifest.json"; DestDir: "{app}\config"; Flags: ignoreversion

[Icons]
Name: "{group}\AdeshLang Doctor"; Filename: "{app}\bin\{#MyAppExeName}"; Parameters: "doctor"
Name: "{group}\AdeshLang TUI Editor"; Filename: "{app}\bin\adesh-editor.exe"; Check: EditorExists
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\AdeshLang"; Filename: "{app}\bin\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
; ADESH_HOME is intentionally machine-wide because this installer is elevated.
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: string; ValueName: "ADESH_HOME"; ValueData: "{app}"; Flags: preservestringtype
; The code constant reads and preserves the existing machine PATH. The entry
; is REG_EXPAND_SZ, and uninstall removes only the exact entries we added.
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: expandsz; ValueName: "PATH"; ValueData: "{code:GetSystemPathWithBin}"; Flags: preservestringtype

; Registry file associations for .adl and .adesh. .adl remains associated with
; the language source command even though "adl" is also the package manager.
Root: HKA; Subkey: "Software\Classes\.adl"; ValueType: string; ValueName: ""; ValueData: "AdeshLangSourceFile"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\.adesh"; ValueType: string; ValueName: ""; ValueData: "AdeshLangSourceFile"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\AdeshLangSourceFile"; ValueType: string; ValueName: ""; ValueData: "AdeshLang Source Code File"; Flags: uninsdeletekey; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\AdeshLangSourceFile\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\bin\{#MyAppExeName},0"; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\AdeshLangSourceFile\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\bin\{#MyAppExeName}"" run ""%1"""; Tasks: fileassoc

[UninstallDelete]
Type: files; Name: "{app}\toolchain-install.log"
; The toolchain is downloaded by `adesh toolchain install` at install time, so
; Inno did not track these files; remove them explicitly on uninstall.
Type: filesandordirs; Name: "{app}\toolchain"

[Code]
const
  SystemEnvironmentKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';
  VSBuildToolsURL = 'https://aka.ms/vs/17/release/vs_buildtools.exe';

function EditorExists: Boolean;
begin
  Result := FileExists(ExpandConstant('{app}\bin\adesh-editor.exe'));
end;

function PathHasEntry(PathValue, Entry: String): Boolean;
var
  Item: String;
  Separator: Integer;
begin
  Result := False;
  while Length(PathValue) > 0 do
  begin
    Separator := Pos(';', PathValue);
    if Separator = 0 then
    begin
      Item := Trim(PathValue);
      PathValue := '';
    end
    else
    begin
      Item := Trim(Copy(PathValue, 1, Separator - 1));
      Delete(PathValue, 1, Separator);
    end;
    if CompareText(Item, Entry) = 0 then
    begin
      Result := True;
      Exit;
    end;
  end;
end;

function RemovePathEntry(PathValue, Entry: String): String;
var
  Item: String;
  Separator: Integer;
  NewValue: String;
begin
  NewValue := '';
  while Length(PathValue) > 0 do
  begin
    Separator := Pos(';', PathValue);
    if Separator = 0 then
    begin
      Item := Trim(PathValue);
      PathValue := '';
    end
    else
    begin
      Item := Trim(Copy(PathValue, 1, Separator - 1));
      Delete(PathValue, 1, Separator);
    end;
    if (Item <> '') and (CompareText(Item, Entry) <> 0) then
    begin
      if NewValue <> '' then
        NewValue := NewValue + ';';
      NewValue := NewValue + Item;
    end;
  end;
  Result := NewValue;
end;

function GetSystemPathWithBin(Param: String): String;
var
  PathValue: String;
  BinDir: String;
begin
  BinDir := ExpandConstant('{app}\bin');
  if not RegQueryStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'PATH', PathValue) then
    PathValue := '';
  if not PathHasEntry(PathValue, BinDir) then
  begin
    if PathValue = '' then
      PathValue := BinDir
    else
      PathValue := PathValue + ';' + BinDir;
  end;
  Result := PathValue;
end;

function HasVSBuildTools: Boolean;
var
  Root: String;
begin
  Result := False;
  Root := 'C:\Program Files (x86)\Microsoft Visual Studio\2019';
  if DirExists(Root + '\BuildTools\VC\Tools\MSVC') or
     DirExists(Root + '\Community\VC\Tools\MSVC') or
     DirExists(Root + '\Enterprise\VC\Tools\MSVC') then
    Result := True;
  Root := 'C:\Program Files (x86)\Microsoft Visual Studio\2022';
  if DirExists(Root + '\BuildTools\VC\Tools\MSVC') or
     DirExists(Root + '\Community\VC\Tools\MSVC') or
     DirExists(Root + '\Enterprise\VC\Tools\MSVC') then
    Result := True;
  if RegKeyExists(HKEY_LOCAL_MACHINE, 'SOFTWARE\Microsoft\VisualStudio\Setup\Instances\BuildTools') or
     RegKeyExists(HKEY_LOCAL_MACHINE, 'SOFTWARE\WOW6432Node\Microsoft\VisualStudio\Setup\Instances\BuildTools') then
    Result := True;
end;

function ShouldShowVSBuildTools: Boolean;
begin
  Result := not HasVSBuildTools;
end;

function HasPython: Boolean;
var
  ResultCode: Integer;
begin
  Result := False;
  if RegKeyExists(HKEY_LOCAL_MACHINE, 'SOFTWARE\Python\PythonCore\3.13') or
     RegKeyExists(HKEY_LOCAL_MACHINE, 'SOFTWARE\Python\PythonCore\3.12') or
     RegKeyExists(HKEY_LOCAL_MACHINE, 'SOFTWARE\Python\PythonCore\3.11') or
     RegKeyExists(HKEY_LOCAL_MACHINE, 'SOFTWARE\Python\PythonCore\3.10') or
     RegKeyExists(HKEY_CURRENT_USER, 'SOFTWARE\Python\PythonCore\3.13') or
     RegKeyExists(HKEY_CURRENT_USER, 'SOFTWARE\Python\PythonCore\3.12') or
     RegKeyExists(HKEY_CURRENT_USER, 'SOFTWARE\Python\PythonCore\3.11') or
     RegKeyExists(HKEY_CURRENT_USER, 'SOFTWARE\Python\PythonCore\3.10') then
  begin
    Result := True;
    Exit;
  end;
  if FileExists('C:\Program Files\Python312\python.exe') or
     FileExists('C:\Program Files\Python311\python.exe') or
     FileExists('C:\Python312\python.exe') or
     FileExists('C:\Python311\python.exe') then
  begin
    Result := True;
    Exit;
  end;
  if Exec('py.exe', '-3 --version', '', SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0) then
  begin
    Result := True;
    Exit;
  end;
end;

function ShouldShowPython: Boolean;
begin
  Result := not HasPython;
end;

function InstallPython: Boolean;
var
  InstallerPath: String;
  DownloadCommand: String;
  ResultCode: Integer;
  PowerShellPath: String;
  PythonURL: String;
begin
  PythonURL := 'https://www.python.org/ftp/python/3.12.8/python-3.12.8-amd64.exe';
  InstallerPath := ExpandConstant('{tmp}\python_installer.exe');
  PowerShellPath := ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe');

  // Try winget first if available (silent, hidden)
  if Exec('winget.exe', 'install --id Python.Python.3.12 --source winget --silent --accept-package-agreements --accept-source-agreements',
      '', SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0) then
  begin
    Result := True;
    Exit;
  end;

  // Fallback to web download
  DownloadCommand := '$ProgressPreference=''SilentlyContinue''; Invoke-WebRequest -UseBasicParsing -Uri ''' +
    PythonURL + ''' -OutFile ''' + InstallerPath + '''';
  if not Exec(PowerShellPath,
      '-NoLogo -NoProfile -ExecutionPolicy Bypass -Command "' + DownloadCommand + '"',
      '', SW_HIDE, ewWaitUntilTerminated, ResultCode) or (ResultCode <> 0) then
  begin
    Result := False;
    Exit;
  end;

  Result := Exec(InstallerPath,
    '/quiet InstallAllUsers=1 PrependPath=1 Include_pip=1',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0);
end;

function InstallVSBuildTools: Boolean;
var
  InstallerPath: String;
  DownloadCommand: String;
  ResultCode: Integer;
  PowerShellPath: String;
begin
  InstallerPath := ExpandConstant('{tmp}\vs_buildtools.exe');
  PowerShellPath := ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe');
  DownloadCommand := '$ProgressPreference=''SilentlyContinue''; Invoke-WebRequest -UseBasicParsing -Uri ''' +
    VSBuildToolsURL + ''' -OutFile ''' + InstallerPath + '''';
  if not Exec(PowerShellPath,
      '-NoLogo -NoProfile -ExecutionPolicy Bypass -Command "' + DownloadCommand + '"',
      '', SW_HIDE, ewWaitUntilTerminated, ResultCode) or (ResultCode <> 0) then
  begin
    Result := False;
    Exit;
  end;
  Result := Exec(InstallerPath,
    '--quiet --wait --norestart --nocache --installPath "' +
    ExpandConstant('{commonpf32}\Microsoft Visual Studio\2022\BuildTools') +
    '" --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0);
end;

function InstallToolchain: Boolean;
var
  AdeshPath: String;
  ManifestPath: String;
  LogPath: String;
  Arguments: String;
  PowerShellCommand: String;
  ResultCode: Integer;
begin
  // adesh.exe runs from {app}\bin if available, else {tmp}\bin
  AdeshPath := ExpandConstant('{app}\bin\adesh.exe');
  if not FileExists(AdeshPath) then
    AdeshPath := ExpandConstant('{tmp}\adesh.exe');
  ManifestPath := ExpandConstant('{app}\config\toolchain-manifest.json');
  if not FileExists(ManifestPath) then
    ManifestPath := ExpandConstant('{tmp}\toolchain-manifest.json');
  LogPath := ExpandConstant('{tmp}\adesh-toolchain-install.log');
  Arguments := 'toolchain install --system --manifest ''' + ManifestPath + '''';
  if WizardIsTaskSelected('buildmlir') then
    Arguments := Arguments + ' --build-mlir-source';

  // ADESH_HOME points at the chosen destination so exposure + the resolver
  // behave exactly as they will after the core files are installed.
  // Execute completely hidden (SW_HIDE) without terminal popup.
  PowerShellCommand := '$env:ADESH_HOME = ''' + ExpandConstant('{app}') + '''; & ''' +
    AdeshPath + ''' ' + Arguments +
    ' 2>&1 | Out-File -FilePath ''' + LogPath + ''' -Encoding utf8; exit $LASTEXITCODE';
  Result := Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
    '-NoLogo -NoProfile -ExecutionPolicy Bypass -Command "' + PowerShellCommand + '"',
    ExpandConstant('{tmp}'), SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep <> ssPostInstall then
    Exit;
  if WizardIsTaskSelected('vsbuildtools') and not HasVSBuildTools then
    InstallVSBuildTools;
  if WizardIsTaskSelected('python') and not HasPython then
    InstallPython;
  if WizardIsTaskSelected('downloadtoolchain') or WizardIsTaskSelected('buildmlir') then
    InstallToolchain;
  // Promote the toolchain log into the install directory.
  if FileExists(ExpandConstant('{tmp}\adesh-toolchain-install.log')) then
    FileCopy(ExpandConstant('{tmp}\adesh-toolchain-install.log'),
      ExpandConstant('{app}\toolchain-install.log'), False);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  PathValue: String;
  BinDir: String;
  LLVMBinDir: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    BinDir := ExpandConstant('{app}\bin');
    LLVMBinDir := ExpandConstant('{app}\toolchain\llvm\bin');
    if RegQueryStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'PATH', PathValue) then
    begin
      PathValue := RemovePathEntry(PathValue, BinDir);
      PathValue := RemovePathEntry(PathValue, LLVMBinDir);
      PathValue := RemovePathEntry(PathValue, 'C:\Program Files\LLVM\bin');
      RegWriteExpandStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'PATH', PathValue);
    end;
    RegDeleteValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_HOME');
    RegDeleteValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_TOOLCHAIN');
    RegDeleteValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_CLANG');
    RegDeleteValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_LLC');
    RegDeleteValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_MLIR_OPT');
    RegDeleteValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_MLIR_TRANSLATE');
  end
  else if CurUninstallStep = usPostUninstall then
  begin
    // The toolchain AdeshLang installed lives inside the program directory
    // and was removed with it. Only a PRE-EXISTING system LLVM remains.
    if DirExists('C:\Program Files\LLVM\bin') then
      MsgBox('AdeshLang has been removed, including the LLVM toolchain it installed.' +
        Chr(13) + Chr(10) + Chr(13) + Chr(10) +
        'A separate LLVM installation at C:\Program Files\LLVM was found and left ' +
        'untouched (it existed before AdeshLang and other programs may use it).',
        mbInformation, MB_OK);
  end;
end;
