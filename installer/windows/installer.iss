; =====================================================================
; AdeshLang Windows Distribution Installer Script (Inno Setup 6)
; Official Website: https://adeshlang.org
; Repository: https://github.com/adeshlang/adeshlang
; =====================================================================

#define MyAppURL "https://adeshlang.org"
#define MyAppDocsURL "https://adeshlang.org/docs"
#define MyAppRepoURL "https://github.com/adeshlang/adeshlang"
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
AppCopyright=Copyright (C) 2026 AdeshLang (adeshlang.org)
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription=AdeshLang Programming Language Setup
VersionInfoVersion={#MyAppVersion}
VersionInfoCopyright=Copyright (C) 2026 AdeshLang (adeshlang.org)
DefaultDirName={autopf}\AdeshLang
DefaultGroupName=AdeshLang Programming Language
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE
OutputBaseFilename=AdeshLang-0.3.0-x86_64-windows-setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesEnvironment=yes
SetupIconFile=resources\installer.ico
UninstallDisplayName={#MyAppName} {#MyAppVersion}
UninstallDisplayIcon={app}\bin\{#MyAppExeName}
ExtraDiskSpaceRequired=1939865600

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "downloadtoolchain"; Description: "Install pinned LLVM 18.1.8 Toolchain (Clang compiler, LLD linker, llvm-ar, llc; required for AOT & GPU compilation, ~1.5 GB)"; Flags: checkedonce; Check: ShouldShowToolchain
Name: "vsbuildtools"; Description: "Install Visual Studio Build Tools + Windows SDK (required for Windows MSVC CRT linking: ucrt.lib, msvcrt.lib, legacy_stdio_definitions.lib, ~2 GB)"; Flags: checkedonce; Check: ShouldShowVSBuildTools
Name: "python"; Description: "Install Python 3.12 (required for full AI features: `adesh ai train/evaluate/generate` and MLIR source builds)"; Flags: unchecked; Check: ShouldShowPython
Name: "buildmlir"; Description: "Build MLIR GPU tools from source (adds 30–90 minutes; requires Visual Studio C++ Build Tools + CMake + Python 3)"; Flags: unchecked
Name: "fileassoc"; Description: "Associate .adl and .adesh source files with AdeshLang"; Flags: unchecked

[Files]
; The distribution staging script supplies all files below. The toolchain is
; downloaded and configured by `adesh toolchain install` at install time with live logs.
;
; Temporary files unpacked to {tmp} for bootstrapping
Source: "..\..\dist\windows-x86_64\bin\adesh.exe"; DestDir: "{tmp}"; Flags: ignoreversion deleteafterinstall
Source: "..\manifests\toolchain-manifest.json"; DestDir: "{tmp}"; Flags: ignoreversion deleteafterinstall
; Core application files
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

[Registry]
; ADESH_HOME is machine-wide because this installer runs elevated.
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: string; ValueName: "ADESH_HOME"; ValueData: "{app}"; Flags: preservestringtype
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: string; ValueName: "ADESH_TOOLCHAIN"; ValueData: "{app}\toolchain\llvm"; Flags: preservestringtype
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: string; ValueName: "ADESH_CLANG"; ValueData: "{app}\toolchain\llvm\bin\clang.exe"; Flags: preservestringtype
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: string; ValueName: "ADESH_LLC"; ValueData: "{app}\toolchain\llvm\bin\llc.exe"; Flags: preservestringtype
; Preserve existing system PATH and append AdeshLang bin and toolchain bin directories
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: expandsz; ValueName: "PATH"; ValueData: "{code:GetSystemPathWithBin}"; Flags: preservestringtype

; Registry file associations for .adl and .adesh source code files
Root: HKA; Subkey: "Software\Classes\.adl"; ValueType: string; ValueName: ""; ValueData: "AdeshLangSourceFile"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\.adesh"; ValueType: string; ValueName: ""; ValueData: "AdeshLangSourceFile"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\AdeshLangSourceFile"; ValueType: string; ValueName: ""; ValueData: "AdeshLang Source Code File"; Flags: uninsdeletekey; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\AdeshLangSourceFile\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\bin\{#MyAppExeName},0"; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\AdeshLangSourceFile\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\bin\{#MyAppExeName}"" run ""%1"""; Tasks: fileassoc

[UninstallDelete]
Type: files; Name: "{app}\install.log"
Type: files; Name: "{app}\toolchain-install.log"
Type: filesandordirs; Name: "{app}\toolchain"

[Code]
const
  SystemEnvironmentKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';
  VSBuildToolsURL = 'https://aka.ms/vs/17/release/vs_buildtools.exe';
  WM_VSCROLL = $0115;
  SB_BOTTOM = 7;

function SendMessage(hWnd: HWND; Msg: Cardinal; wParam, lParam: LongInt): LongInt;
  external 'SendMessageW@user32.dll stdcall';

var
  LogMemo: TNewMemo;
  LogLabel: TLabel;
  FinishedMemo: TNewMemo;
  InstallLogFilePath: String;

procedure LogMessage(const Msg: String);
var
  Timestamp: String;
  FullLine: String;
begin
  Timestamp := GetDateTimeString('yyyy-mm-dd hh:nn:ss', '-', ':');
  FullLine := '[' + Timestamp + '] ' + Msg;
  if LogMemo <> nil then
  begin
    LogMemo.Lines.Add(FullLine);
    // Scroll to the bottom of the log memo
    SendMessage(LogMemo.Handle, WM_VSCROLL, SB_BOTTOM, 0);
  end;
  WizardForm.StatusLabel.Caption := Msg;
  WizardForm.FilenameLabel.Caption := '';
  if InstallLogFilePath <> '' then
  begin
    SaveStringToFile(InstallLogFilePath, FullLine + #13#10, True);
  end;
  if LogMemo <> nil then
    LogMemo.Refresh;
  WizardForm.Refresh;
end;

procedure InitializeWizard;
begin
  // Create a live terminal log memo on the Installing page for real-time progress visibility
  LogLabel := TLabel.Create(WizardForm);
  LogLabel.Parent := WizardForm.InstallingPage;
  LogLabel.Left := WizardForm.ProgressGauge.Left;
  LogLabel.Top := WizardForm.ProgressGauge.Top + WizardForm.ProgressGauge.Height + ScaleY(8);
  LogLabel.Caption := 'Installation Process Logs:';
  LogLabel.Font.Style := [fsBold];

  LogMemo := TNewMemo.Create(WizardForm);
  LogMemo.Parent := WizardForm.InstallingPage;
  LogMemo.Left := WizardForm.ProgressGauge.Left;
  LogMemo.Top := LogLabel.Top + LogLabel.Height + ScaleY(4);
  LogMemo.Width := WizardForm.ProgressGauge.Width;
  LogMemo.Height := WizardForm.InstallingPage.Height - LogMemo.Top - ScaleY(8);
  LogMemo.ReadOnly := True;
  LogMemo.ScrollBars := ssVertical;
  LogMemo.Font.Name := 'Consolas';
  LogMemo.Font.Size := 8;
  LogMemo.Lines.Add('[Setup] Initializing AdeshLang Installer...');

  // Create post-install instructions box on the Finished Page
  FinishedMemo := TNewMemo.Create(WizardForm);
  FinishedMemo.Parent := WizardForm.FinishedPage;
  FinishedMemo.Left := WizardForm.FinishedLabel.Left;
  FinishedMemo.Top := WizardForm.FinishedLabel.Top + WizardForm.FinishedLabel.Height + ScaleY(8);
  FinishedMemo.Width := WizardForm.FinishedLabel.Width;
  FinishedMemo.Height := WizardForm.FinishedPage.Height - FinishedMemo.Top - ScaleY(10);
  FinishedMemo.ReadOnly := True;
  FinishedMemo.ScrollBars := ssVertical;
  FinishedMemo.Font.Name := 'Consolas';
  FinishedMemo.Font.Size := 8;
end;

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
  LLVMBinDir: String;
begin
  BinDir := ExpandConstant('{app}\bin');
  LLVMBinDir := ExpandConstant('{app}\toolchain\llvm\bin');
  if not RegQueryStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'PATH', PathValue) then
    PathValue := '';
  if not PathHasEntry(PathValue, BinDir) then
  begin
    if PathValue = '' then
      PathValue := BinDir
    else
      PathValue := PathValue + ';' + BinDir;
  end;
  if not PathHasEntry(PathValue, LLVMBinDir) then
  begin
    PathValue := PathValue + ';' + LLVMBinDir;
  end;
  Result := PathValue;
end;

function HasToolchain: Boolean;
var
  ToolchainBin: String;
begin
  Result := False;
  ToolchainBin := ExpandConstant('{app}\toolchain\llvm\bin');
  if (FileExists(ToolchainBin + '\clang.exe') and (FileExists(ToolchainBin + '\lld-link.exe') or FileExists(ToolchainBin + '\lld.exe'))) or
     (FileExists(ExpandConstant('{pf}\LLVM\bin\clang.exe')) and (FileExists(ExpandConstant('{pf}\LLVM\bin\lld-link.exe')) or FileExists(ExpandConstant('{pf}\LLVM\bin\lld.exe')))) or
     (FileExists(ExpandConstant('{pf64}\LLVM\bin\clang.exe')) and (FileExists(ExpandConstant('{pf64}\LLVM\bin\lld-link.exe')) or FileExists(ExpandConstant('{pf64}\LLVM\bin\lld.exe')))) or
     (FileExists(ExpandConstant('{pf32}\LLVM\bin\clang.exe')) and (FileExists(ExpandConstant('{pf32}\LLVM\bin\lld-link.exe')) or FileExists(ExpandConstant('{pf32}\LLVM\bin\lld.exe')))) or
     (FileExists(ExpandConstant('{sd}\LLVM\bin\clang.exe')) and (FileExists(ExpandConstant('{sd}\LLVM\bin\lld-link.exe')) or FileExists(ExpandConstant('{sd}\LLVM\bin\lld.exe')))) then
  begin
    Result := True;
    Exit;
  end;
  if (ExpandConstant('{%ADESH_TOOLCHAIN}') <> '') or (ExpandConstant('{%ADESH_CLANG}') <> '') then
  begin
    Result := True;
    Exit;
  end;
end;

function ShouldShowToolchain: Boolean;
begin
  Result := not HasToolchain;
end;

function HasVSBuildTools: Boolean;
var
  VSWherePath: String;
  ResultCode: Integer;
  PFDirs: array[0..3] of String;
  Years: array[0..3] of String;
  Editions: array[0..4] of String;
  i, j, k: Integer;
  Candidate: String;
begin
  Result := False;

  // 1. Check vswhere.exe query dynamically across all Program Files folders
  VSWherePath := ExpandConstant('{pf32}\Microsoft Visual Studio\Installer\vswhere.exe');
  if not FileExists(VSWherePath) then
    VSWherePath := ExpandConstant('{pf64}\Microsoft Visual Studio\Installer\vswhere.exe');
  if not FileExists(VSWherePath) then
    VSWherePath := ExpandConstant('{pf}\Microsoft Visual Studio\Installer\vswhere.exe');

  if FileExists(VSWherePath) then
  begin
    if Exec(VSWherePath, '-latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath', '', SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0) then
    begin
      Result := True;
      Exit;
    end;
  end;

  // 2. Dynamically check all Program Files directories, system drive, and editions
  PFDirs[0] := ExpandConstant('{pf64}\Microsoft Visual Studio');
  PFDirs[1] := ExpandConstant('{pf32}\Microsoft Visual Studio');
  PFDirs[2] := ExpandConstant('{pf}\Microsoft Visual Studio');
  PFDirs[3] := ExpandConstant('{sd}\Microsoft Visual Studio');

  Years[0] := '2022';
  Years[1] := '2019';
  Years[2] := '2017';
  Years[3] := '2025';

  Editions[0] := 'BuildTools';
  Editions[1] := 'Community';
  Editions[2] := 'Professional';
  Editions[3] := 'Enterprise';
  Editions[4] := 'Preview';

  for i := 0 to 3 do
  begin
    for j := 0 to 3 do
    begin
      for k := 0 to 4 do
      begin
        Candidate := PFDirs[i] + '\' + Years[j] + '\' + Editions[k] + '\VC\Tools\MSVC';
        if DirExists(Candidate) then
        begin
          Result := True;
          Exit;
        end;
      end;
    end;
  end;

  // 3. Check Visual Studio Installer Instances registry
  if RegKeyExists(HKEY_LOCAL_MACHINE, 'SOFTWARE\Microsoft\VisualStudio\Setup\Instances') or
     RegKeyExists(HKEY_LOCAL_MACHINE, 'SOFTWARE\WOW6432Node\Microsoft\VisualStudio\Setup\Instances') then
  begin
    Result := True;
    Exit;
  end;

  // 4. Check environment variables
  if (ExpandConstant('{%VCToolsInstallDir}') <> '') or (ExpandConstant('{%VSINSTALLDIR}') <> '') then
  begin
    Result := True;
    Exit;
  end;
end;

function ShouldShowVSBuildTools: Boolean;
begin
  Result := not HasVSBuildTools;
end;

function HasPython: Boolean;
var
  ResultCode: Integer;
  PyDirs: array[0..3] of String;
  PyVers: array[0..3] of String;
  i, j: Integer;
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

  PyDirs[0] := ExpandConstant('{pf64}');
  PyDirs[1] := ExpandConstant('{pf}');
  PyDirs[2] := ExpandConstant('{sd}');
  PyDirs[3] := ExpandConstant('{localappdata}\Programs\Python');

  PyVers[0] := 'Python313';
  PyVers[1] := 'Python312';
  PyVers[2] := 'Python311';
  PyVers[3] := 'Python310';

  for i := 0 to 3 do
  begin
    for j := 0 to 3 do
    begin
      if FileExists(PyDirs[i] + '\' + PyVers[j] + '\python.exe') then
      begin
        Result := True;
        Exit;
      end;
    end;
  end;

  if Exec('py.exe', '-3 --version', '', SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0) then
  begin
    Result := True;
    Exit;
  end;
  if Exec('python.exe', '--version', '', SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0) then
  begin
    Result := True;
    Exit;
  end;
end;

function ShouldShowPython: Boolean;
begin
  Result := not HasPython;
end;

function RunCommandWithLiveLog(const Title, ExePath, Args, WorkingDir: String): Boolean;
var
  TempLogPath: String;
  RunnerScript: String;
  PowerShellPath: String;
  ResultCode: Integer;
  LastLineRead: Integer;
  Lines: TArrayOfString;
  Done: Boolean;
  DoneFilePath: String;
  ExitCodeFilePath: String;
  IterCount: Integer;
begin
  LogMessage('▶ Starting ' + Title + '...');
  TempLogPath := ExpandConstant('{tmp}\adesh_step_output.log');
  DoneFilePath := ExpandConstant('{tmp}\adesh_step_done.flag');
  ExitCodeFilePath := ExpandConstant('{tmp}\adesh_step_exit.txt');
  DeleteFile(TempLogPath);
  DeleteFile(DoneFilePath);
  DeleteFile(ExitCodeFilePath);

  PowerShellPath := ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe');

  // Launch the command via PowerShell and stream output to a temporary log file while tracking status
  RunnerScript :=
    '$env:ADESH_HOME = ''' + ExpandConstant('{app}') + '''; ' +
    '& ''' + ExePath + ''' ' + Args + ' 2>&1 | Tee-Object -FilePath ''' + TempLogPath + '''; ' +
    '$code = $LASTEXITCODE; if ($null -eq $code) { $code = 0 }; ' +
    'Set-Content -Path ''' + ExitCodeFilePath + ''' -Value $code; ' +
    'Set-Content -Path ''' + DoneFilePath + ''' -Value "done"; exit $code';

  if not Exec(PowerShellPath,
              '-NoLogo -NoProfile -ExecutionPolicy Bypass -Command "' + RunnerScript + '"',
              WorkingDir, SW_HIDE, ewNoWait, ResultCode) then
  begin
    LogMessage('✗ Error: Failed to execute ' + Title);
    Result := False;
    Exit;
  end;

  LastLineRead := 0;
  Done := False;
  IterCount := 0;

  while not Done do
  begin
    Sleep(120);
    IterCount := IterCount + 1;
    if LogMemo <> nil then
      LogMemo.Refresh;
    WizardForm.Refresh;

    if FileExists(TempLogPath) then
    begin
      if LoadStringsFromFile(TempLogPath, Lines) then
      begin
        while LastLineRead < GetArrayLength(Lines) do
        begin
          if Trim(Lines[LastLineRead]) <> '' then
          begin
            LogMessage('  ' + Lines[LastLineRead]);
          end;
          LastLineRead := LastLineRead + 1;
        end;
      end;
    end;

    if FileExists(DoneFilePath) then
    begin
      Done := True;
    end;
  end;

  // Flush any final lines
  if FileExists(TempLogPath) then
  begin
    if LoadStringsFromFile(TempLogPath, Lines) then
    begin
      while LastLineRead < GetArrayLength(Lines) do
      begin
        if Trim(Lines[LastLineRead]) <> '' then
        begin
          LogMessage('  ' + Lines[LastLineRead]);
        end;
        LastLineRead := LastLineRead + 1;
      end;
    end;
  end;

  ResultCode := 0;
  if FileExists(ExitCodeFilePath) then
  begin
    if LoadStringsFromFile(ExitCodeFilePath, Lines) and (GetArrayLength(Lines) > 0) then
    begin
      ResultCode := StrToIntDef(Trim(Lines[0]), 0);
    end;
  end;

  // Clean up temporary execution marker files
  DeleteFile(DoneFilePath);
  DeleteFile(ExitCodeFilePath);

  if (ResultCode = 0) or (ResultCode = 3010) then
  begin
    LogMessage('✓ ' + Title + ' completed successfully.');
    Result := True;
  end
  else
  begin
    LogMessage('✗ ' + Title + ' returned non-zero exit code: ' + IntToStr(ResultCode));
    Result := False;
  end;
end;

function InstallPython: Boolean;
var
  InstallerPath: String;
  DownloadCommand: String;
  ResultCode: Integer;
  PowerShellPath: String;
  PythonURL: String;
begin
  LogMessage('====================================================');
  LogMessage('Python 3.12 Setup (for full AI neural engine)');
  LogMessage('====================================================');

  PythonURL := 'https://www.python.org/ftp/python/3.12.8/python-3.12.8-amd64.exe';
  InstallerPath := ExpandConstant('{tmp}\python_installer.exe');
  PowerShellPath := ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe');

  // Try winget first if available
  LogMessage('Checking system package manager (winget)...');
  if Exec('winget.exe', 'install --id Python.Python.3.12 --source winget --silent --accept-package-agreements --accept-source-agreements',
      '', SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0) then
  begin
    LogMessage('✓ Python 3.12 installed via winget.');
    Result := True;
    Exit;
  end;

  // Fallback to web download
  LogMessage('Downloading Python 3.12.8 from python.org...');
  DownloadCommand := '$ProgressPreference=''SilentlyContinue''; Invoke-WebRequest -UseBasicParsing -Uri ''' +
    PythonURL + ''' -OutFile ''' + InstallerPath + '''';
  if not Exec(PowerShellPath,
      '-NoLogo -NoProfile -ExecutionPolicy Bypass -Command "' + DownloadCommand + '"',
      '', SW_HIDE, ewWaitUntilTerminated, ResultCode) or (ResultCode <> 0) then
  begin
    LogMessage('✗ Failed to download Python 3.12 installer.');
    Result := False;
    Exit;
  end;

  Result := RunCommandWithLiveLog('Python 3.12 Installation',
    InstallerPath,
    '/quiet InstallAllUsers=1 PrependPath=1 Include_pip=1',
    ExpandConstant('{tmp}'));
end;

function InstallVSBuildTools: Boolean;
var
  InstallerPath: String;
  DownloadCommand: String;
  ResultCode: Integer;
  PowerShellPath: String;
begin
  LogMessage('====================================================');
  LogMessage('Visual Studio Build Tools & Windows SDK Setup');
  LogMessage('====================================================');

  InstallerPath := ExpandConstant('{tmp}\vs_buildtools.exe');
  PowerShellPath := ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe');

  // Try winget first if available
  LogMessage('Checking system package manager (winget) for VS Build Tools...');
  if Exec('winget.exe', 'install --id Microsoft.VisualStudio.2022.BuildTools --override "--passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended" --source winget --silent --accept-package-agreements --accept-source-agreements',
      '', SW_HIDE, ewWaitUntilTerminated, ResultCode) and ((ResultCode = 0) or (ResultCode = 3010)) then
  begin
    LogMessage('✓ Visual Studio Build Tools installed via winget.');
    Result := True;
    Exit;
  end;

  // Fallback to web download
  LogMessage('Downloading Visual Studio Build Tools bootstrapper...');
  DownloadCommand := '$ProgressPreference=''SilentlyContinue''; Invoke-WebRequest -UseBasicParsing -Uri ''' +
    VSBuildToolsURL + ''' -OutFile ''' + InstallerPath + '''';
  if not Exec(PowerShellPath,
      '-NoLogo -NoProfile -ExecutionPolicy Bypass -Command "' + DownloadCommand + '"',
      '', SW_HIDE, ewWaitUntilTerminated, ResultCode) or (ResultCode <> 0) then
  begin
    LogMessage('✗ Failed to download Visual Studio Build Tools bootstrapper.');
    Result := False;
    Exit;
  end;

  LogMessage('Running Visual Studio Build Tools installer (MSVC C++ Tools + Windows SDK)...');
  Result := RunCommandWithLiveLog('Visual Studio Build Tools',
    InstallerPath,
    '--passive --wait --norestart --nocache --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended',
    ExpandConstant('{tmp}'));
end;

function InstallToolchain: Boolean;
var
  AdeshPath: String;
  ManifestPath: String;
  Arguments: String;
begin
  LogMessage('====================================================');
  LogMessage('AdeshLang Pinned LLVM 18.1.8 Toolchain Setup');
  LogMessage('====================================================');

  AdeshPath := ExpandConstant('{app}\bin\adesh.exe');
  if not FileExists(AdeshPath) then
    AdeshPath := ExpandConstant('{tmp}\adesh.exe');

  ManifestPath := ExpandConstant('{app}\config\toolchain-manifest.json');
  if not FileExists(ManifestPath) then
    ManifestPath := ExpandConstant('{tmp}\toolchain-manifest.json');

  Arguments := 'toolchain install --system --manifest "' + ManifestPath + '"';
  if WizardIsTaskSelected('buildmlir') then
    Arguments := Arguments + ' --build-mlir-source';

  Result := RunCommandWithLiveLog('LLVM/Clang Toolchain Installer',
    AdeshPath,
    Arguments,
    ExpandConstant('{app}'));
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ToolchainBin: String;
begin
  if CurStep = ssInstall then
  begin
    InstallLogFilePath := ExpandConstant('{app}\install.log');
    LogMessage('====================================================');
    LogMessage('AdeshLang v0.3.0 Installation Started');
    LogMessage('Target: ' + ExpandConstant('{app}'));
    LogMessage('====================================================');
  end
  else if CurStep = ssPostInstall then
  begin
    LogMessage('✓ Core binaries, standard library, and runtime extracted.');

    if WizardIsTaskSelected('vsbuildtools') and not HasVSBuildTools then
      InstallVSBuildTools;

    if WizardIsTaskSelected('python') and not HasPython then
      InstallPython;

    if WizardIsTaskSelected('downloadtoolchain') or WizardIsTaskSelected('buildmlir') then
      InstallToolchain;

    // Register active toolchain executables into HKLM
    ToolchainBin := ExpandConstant('{app}\toolchain\llvm\bin');
    if FileExists(ToolchainBin + '\clang.exe') then
      RegWriteStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_CLANG', ToolchainBin + '\clang.exe');
    if FileExists(ToolchainBin + '\llc.exe') then
      RegWriteStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_LLC', ToolchainBin + '\llc.exe');
    if FileExists(ToolchainBin + '\mlir-opt.exe') then
      RegWriteStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_MLIR_OPT', ToolchainBin + '\mlir-opt.exe');
    if FileExists(ToolchainBin + '\mlir-translate.exe') then
      RegWriteStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_MLIR_TRANSLATE', ToolchainBin + '\mlir-translate.exe');

    // Promote the toolchain log into the program directory
    if FileExists(ExpandConstant('{tmp}\adesh-toolchain-install.log')) then
      FileCopy(ExpandConstant('{tmp}\adesh-toolchain-install.log'),
        ExpandConstant('{app}\toolchain-install.log'), False);

    LogMessage('====================================================');
    LogMessage('✓ AdeshLang Installation Complete!');
    LogMessage('Run `adesh doctor` or `adl doctor` in terminal to verify health.');
    LogMessage('====================================================');
  end;
end;

procedure CurPageChanged(CurPageID: Integer);
var
  HasModel: Boolean;
  FinishText: String;
begin
  if CurPageID = wpFinished then
  begin
    HasModel := FileExists(ExpandConstant('{app}\ai\models\adesh-coder-0.5b-q4_0.gguf'));

    FinishText := 'AdeshLang v0.3.0 has been installed successfully!' + #13#10 + #13#10;

    if not HasModel then
    begin
      FinishText := FinishText +
        '═══════════════════════════════════════════════════════' + #13#10 +
        ' [AI Features & Model Setup]' + #13#10 +
        '═══════════════════════════════════════════════════════' + #13#10 +
        ' • AI models are not bundled with this lightweight installer.' + #13#10 +
        ' • To download and set up the local AI neural coder model (~275 MB):' + #13#10 +
        '     adesh ai setup' + #13#10 +
        ' • To train or fine-tune custom AI models, Python 3.12 is required:' + #13#10 +
        '     winget install Python.Python.3.12' + #13#10 +
        ' • To generate code once model is setup:' + #13#10 +
        '     adesh ai generate "create an HTTP server"' + #13#10 + #13#10;
    end
    else
    begin
      FinishText := FinishText +
        '═══════════════════════════════════════════════════════' + #13#10 +
        ' [AI Features Ready]' + #13#10 +
        '═══════════════════════════════════════════════════════' + #13#10 +
        ' • Bundled AI neural coder model is configured.' + #13#10 +
        ' • Generate code with: adesh ai generate "prompt"' + #13#10 + #13#10;
    end;

    FinishText := FinishText +
      '═══════════════════════════════════════════════════════' + #13#10 +
      ' [Quick Start & Toolchain Verification]' + #13#10 +
      '═══════════════════════════════════════════════════════' + #13#10 +
      ' • Verify health & toolchains:  adesh doctor' + #13#10 +
      ' • Launch TUI editor:           adesh edit (or adesh-editor)' + #13#10 +
      ' • Run Adesh source file:        adesh run hello.adesh' + #13#10 +
      ' • AOT native compilation:       adesh build hello.adesh' + #13#10 +
      ' • GPU kernel compilation:       adesh build --gpu=cuda kernel.adesh' + #13#10 + #13#10 +
      'Documentation & Guides: https://adeshlang.org';

    if FinishedMemo <> nil then
    begin
      FinishedMemo.Text := FinishText;
    end;
  end;
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
      PathValue := RemovePathEntry(PathValue, ExpandConstant('{pf}\LLVM\bin'));
      PathValue := RemovePathEntry(PathValue, ExpandConstant('{pf64}\LLVM\bin'));
      PathValue := RemovePathEntry(PathValue, ExpandConstant('{pf32}\LLVM\bin'));
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
    if DirExists(ExpandConstant('{pf}\LLVM\bin')) or DirExists(ExpandConstant('{pf64}\LLVM\bin')) then
      MsgBox('AdeshLang has been removed, including the LLVM toolchain it installed.' +
        Chr(13) + Chr(10) + Chr(13) + Chr(10) +
        'A separate system LLVM installation was found and left untouched.',
        mbInformation, MB_OK);
  end;
end;
