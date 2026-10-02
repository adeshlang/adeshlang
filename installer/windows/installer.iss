; =====================================================================
; AdeshLang Windows Distribution Installer Script (Inno Setup 6)
; Official Website: https://adeshlang.org
; Repository: https://github.com/adeshlang/adeshlang
;
; SELF-CONTAINED NATIVE TOOLCHAIN:
;   The installer bundles the complete Adesh native toolchain — the native
;   codegen engine, the ADOB object format, the adeshlink native linker
;   (bin\adeshlink.exe), the adeshlang runtime library (lib\), and the
;   standard library (std\). Setup NEVER downloads or asks the user to
;   install LLVM, Clang, GCC, MSVC, or Visual Studio Build Tools.
;
;   External LLVM remains an opt-in bridge after installation:
;     adesh gpu-check --external-linker     (verify an external LLVM)
;     adesh toolchain --external install    (register external toolchains)
;     adesh build --codegen=cranelift --external-linker
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
AppComments=Self-contained native toolchain: no LLVM, GCC, or MSVC required
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
; Headroom on top of the bundled payload (binaries, runtime lib, stdlib,
; AI model): the native toolchain ships inside this package, so no multi-GB
; toolchain download allowance is required anymore.
ExtraDiskSpaceRequired=209715200

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "fileassoc"; Description: "Associate .adl and .adesh source files with AdeshLang"; Flags: unchecked
Name: "python"; Description: "Install Python 3.12 (optional; only for AI model training: `adesh ai train` and MLIR source builds)"; Flags: unchecked; Check: ShouldShowPython

[Files]
; Core application files — the full self-contained native toolchain is
; staged by scripts\release\build_windows_dist.ps1 into dist\windows-x86_64:
;   bin\  adesh.exe, adl.exe, als.exe, adesh-editor.exe, adeshlink.exe
;   lib\  adeshlang.dll / adeshlang.lib (runtime library)
;   std\  standard library sources
;   ai\   bundled AI model + deployment configs
Source: "..\..\dist\windows-x86_64\bin\*"; DestDir: "{app}\bin"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\..\dist\windows-x86_64\lib\*"; DestDir: "{app}\lib"; Flags: ignoreversion recursesubdirs createallsubdirs skipifsourcedoesntexist
Source: "..\..\dist\windows-x86_64\std\*"; DestDir: "{app}\std"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\..\dist\windows-x86_64\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\..\dist\windows-x86_64\ai\*"; DestDir: "{app}\ai"; Flags: ignoreversion recursesubdirs createallsubdirs skipifsourcedoesntexist
; Pinned manifest for the OPT-IN external LLVM bridge
; (`adesh toolchain --external install`). Not used by the native pipeline.
Source: "..\manifests\toolchain-manifest.json"; DestDir: "{app}\config"; Flags: ignoreversion

[Icons]
Name: "{group}\AdeshLang Doctor"; Filename: "{app}\bin\{#MyAppExeName}"; Parameters: "doctor"
Name: "{group}\AdeshLang TUI Editor"; Filename: "{app}\bin\adesh-editor.exe"; Check: EditorExists
Name: "{group}\AdeshLang Native Toolchain Check"; Filename: "{app}\bin\{#MyAppExeName}"; Parameters: "toolchain check"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"

[Registry]
; ADESH_HOME is machine-wide because this installer runs elevated. Only the
; native installation paths are registered: the bundled native toolchain
; needs no ADESH_TOOLCHAIN/ADESH_CLANG/ADESH_LLC variables, so we do not set
; them (they would shadow an external LLVM the user configured later).
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: string; ValueName: "ADESH_HOME"; ValueData: "{app}"; Flags: preservestringtype
; Preserve existing system PATH and append the AdeshLang bin directory
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: expandsz; ValueName: "PATH"; ValueData: "{code:GetSystemPathWithBin}"; Flags: preservestringtype

; Registry file associations for .adl and .adesh source code files
Root: HKA; Subkey: "Software\Classes\.adl"; ValueType: string; ValueName: ""; ValueData: "AdeshLangSourceFile"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\.adesh"; ValueType: string; ValueName: ""; ValueData: "AdeshLangSourceFile"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\AdeshLangSourceFile"; ValueType: string; ValueName: ""; ValueData: "AdeshLang Source Code File"; Flags: uninsdeletekey; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\AdeshLangSourceFile\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\bin\{#MyAppExeName},0"; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\AdeshLangSourceFile\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\bin\{#MyAppExeName}"" run ""%1"""; Tasks: fileassoc

[UninstallDelete]
Type: files; Name: "{app}\install.log"
; {app}\toolchain only exists when the user later registered an external
; LLVM bridge there (`adesh toolchain --external install`), never from setup.
Type: filesandordirs; Name: "{app}\toolchain"

[Code]
const
  SystemEnvironmentKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';
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
  // Create a live terminal log memo on the Installing page for real-time
  // progress visibility of the native toolchain verification.
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
begin
  // Only the AdeshLang bin directory is appended: the native toolchain is
  // bundled there, and the external-bridge toolchain (if a user registers
  // one later) manages its own PATH exposure.
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
begin
  LogMessage('▶ Starting ' + Title + '...');
  TempLogPath := ExpandConstant('{tmp}\adesh_step_output.log');
  DoneFilePath := ExpandConstant('{tmp}\adesh_step_done.flag');
  ExitCodeFilePath := ExpandConstant('{tmp}\adesh_step_exit.txt');
  DeleteFile(TempLogPath);
  DeleteFile(DoneFilePath);
  DeleteFile(ExitCodeFilePath);

  PowerShellPath := ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe');

  // Launch the command via PowerShell and stream output to a temporary log
  // file while tracking status.
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

  while not Done do
  begin
    Sleep(120);
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
      end
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
  LogMessage('Python 3.12 Setup (optional; AI model training only)');
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

function VerifyNativeToolchain: Boolean;
var
  AdeshPath: String;
  LinkerPath: String;
  RuntimeLib: String;
begin
  LogMessage('====================================================');
  LogMessage('Verifying Bundled Native Toolchain Components');
  LogMessage('====================================================');

  AdeshPath := ExpandConstant('{app}\bin\adesh.exe');
  LinkerPath := ExpandConstant('{app}\bin\adeshlink.exe');

  if not FileExists(AdeshPath) then
  begin
    LogMessage('✗ Core compiler binary missing: ' + AdeshPath);
    Result := False;
    Exit;
  end;
  LogMessage('✓ Core compiler        : ' + AdeshPath);

  if FileExists(LinkerPath) then
    LogMessage('✓ Native linker       : ' + LinkerPath)
  else
    LogMessage('! Native linker CLI   : adeshlink.exe not bundled (builds use the in-process linker)');

  RuntimeLib := ExpandConstant('{app}\lib\adeshlang.dll');
  if FileExists(RuntimeLib) then
    LogMessage('✓ Runtime library     : ' + RuntimeLib)
  else
    LogMessage('! Runtime library     : adeshlang.dll not bundled (AOT native builds may need it)');

  if DirExists(ExpandConstant('{app}\std')) then
    LogMessage('✓ Standard library    : ' + ExpandConstant('{app}\std'))
  else
    LogMessage('! Standard library    : std\ missing');

  // Ask adesh itself to self-verify its native components.
  Result := RunCommandWithLiveLog('Native Toolchain Self-Check',
    AdeshPath,
    'toolchain check',
    ExpandConstant('{app}'));
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ToolchainOk: Boolean;
begin
  if CurStep = ssInstall then
  begin
    InstallLogFilePath := ExpandConstant('{app}\install.log');
    LogMessage('====================================================');
    LogMessage('AdeshLang v0.3.0 Installation Started');
    LogMessage('Target: ' + ExpandConstant('{app}'));
    LogMessage('Bundled native toolchain: codegen + adeshlink + ADOB + runtime');
    LogMessage('(no external LLVM, GCC, or MSVC downloads required)');
    LogMessage('====================================================');
  end
  else if CurStep = ssPostInstall then
  begin
    LogMessage('✓ Core binaries, native linker, runtime, standard library extracted.');

    // Optional AI-model-training Python install (unchecked task; never part
    // of compilation).
    if WizardIsTaskSelected('python') and not HasPython then
      InstallPython;

    // Self-verify the bundled native toolchain with live logs.
    ToolchainOk := VerifyNativeToolchain;

    LogMessage('====================================================');
    if ToolchainOk then
      LogMessage('✓ AdeshLang Installation Complete! Native toolchain verified.')
    else
      LogMessage('! AdeshLang installed, but native toolchain verification reported issues.');
    LogMessage('Run `adesh doctor` in a terminal to double-check health.');
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

    FinishText := FinishText +
      '═══════════════════════════════════════════════════════' + #13#10 +
      ' [Self-Contained Native Toolchain]' + #13#10 +
      '═══════════════════════════════════════════════════════' + #13#10 +
      ' • Native codegen, ADOB object format, adeshlink linker, and the' + #13#10 +
      '   adeshlang runtime are bundled — nothing else to install.' + #13#10 +
      ' • No external LLVM, Clang, GCC, MSVC, or Visual Studio Build' + #13#10 +
      '   Tools are required to compile and run Adesh programs.' + #13#10 + #13#10;

    if not HasModel then
    begin
      FinishText := FinishText +
        '═══════════════════════════════════════════════════════' + #13#10 +
        ' [AI Features & Model Setup]' + #13#10 +
        '═══════════════════════════════════════════════════════' + #13#10 +
        ' • AI models are not bundled with this installer.' + #13#10 +
        ' • To download and set up the local AI neural coder model (~275 MB):' + #13#10 +
        '     adesh ai setup' + #13#10 +
        ' • AI model training (optional) additionally needs Python 3.12:' + #13#10 +
        '     winget install Python.Python.3.12' + #13#10 + #13#10;
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
      ' [Quick Start]' + #13#10 +
      '═══════════════════════════════════════════════════════' + #13#10 +
      ' • Verify health & toolchains:  adesh doctor' + #13#10 +
      ' • GPU device compatibility:    adesh gpu-check' + #13#10 +
      ' • Launch TUI editor:           adesh edit (or adesh-editor)' + #13#10 +
      ' • Run Adesh source file:       adesh run hello.adesh' + #13#10 +
      ' • AOT native compilation:      adesh build hello.adesh' + #13#10 +
      ' • GPU kernel compilation:      adesh build --gpu=cuda kernel.adesh' + #13#10 + #13#10 +
      ' [Optional External LLVM Bridge]' + #13#10 +
      ' • Verify an external LLVM:     adesh gpu-check --external-linker' + #13#10 +
      ' • Register one:                adesh toolchain --external install' + #13#10 +
      ' • Link through it:             adesh build --codegen=cranelift --external-linker' + #13#10 + #13#10 +
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
  EnvValue: String;
  BinDir: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    BinDir := ExpandConstant('{app}\bin');
    if RegQueryStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'PATH', PathValue) then
    begin
      PathValue := RemovePathEntry(PathValue, BinDir);
      RegWriteExpandStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'PATH', PathValue);
    end;
    RegDeleteValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_HOME');

    // Only remove toolchain variables that still point into this
    // installation (i.e. the user registered an external bridge there);
    // never touch values pointing at an external LLVM elsewhere.
    if RegQueryStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_TOOLCHAIN', EnvValue) and
       (Pos(ExpandConstant('{app}'), EnvValue) = 1) then
      RegDeleteValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_TOOLCHAIN');
    if RegQueryStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_CLANG', EnvValue) and
       (Pos(ExpandConstant('{app}'), EnvValue) = 1) then
      RegDeleteValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_CLANG');
    if RegQueryStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_LLC', EnvValue) and
       (Pos(ExpandConstant('{app}'), EnvValue) = 1) then
      RegDeleteValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_LLC');
    if RegQueryStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_MLIR_OPT', EnvValue) and
       (Pos(ExpandConstant('{app}'), EnvValue) = 1) then
      RegDeleteValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_MLIR_OPT');
    if RegQueryStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_MLIR_TRANSLATE', EnvValue) and
       (Pos(ExpandConstant('{app}'), EnvValue) = 1) then
      RegDeleteValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'ADESH_MLIR_TRANSLATE');
  end
  else if CurUninstallStep = usPostUninstall then
  begin
    if DirExists(ExpandConstant('{pf}\LLVM\bin')) or DirExists(ExpandConstant('{pf64}\LLVM\bin')) then
      MsgBox('AdeshLang has been removed, including the bundled native toolchain.' +
        Chr(13) + Chr(10) + Chr(13) + Chr(10) +
        'A separate system LLVM installation was found and left untouched.',
        mbInformation, MB_OK);
  end;
end;
