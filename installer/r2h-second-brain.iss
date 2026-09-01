; ============================================================================
;  R2H Second Brain - Production Offline Installer (Inno Setup 6)
;  r2h-second-brain / ai.r2h.second-brain
;
;  Design contract:
;   - Fully offline install: all runtimes and models ship in the payload.
;   - Per-machine install: app in Program Files, payloads in ProgramData.
;   - Transactional AI-pack provisioning: new payload is staged, hash-verified,
;     then activated by same-volume rename; the previous pack is kept as the
;     rollback source until verification succeeds.
;   - NEVER deletes C:\ProgramData\R2H.AI-ELE during ordinary upgrade; data and
;     model removal is an explicit uninstall-time opt-in only.
;   - Local AI services are loopback-only by the application contract.
;
;  Build:
;   ISCC.exe installer\r2h-second-brain.iss /DStageRoot=<abs path to release-staging\pack> /DMyAppVersion=<x.y.z>
; ============================================================================

#define MyAppName "R2H Second Brain"
#define MyAppPublisher "R2H Engineering"
#define MyAppExeName "r2h-second-brain-app.exe"
#define MyAppId "{{3151A1D6-1BAA-4D1D-B8CA-8CBF69899338}"
#define LegacyAiPackAppId "{{DC2B553F-96D8-4FA4-9273-A9E4E49F090D}_is1"

#ifndef MyAppVersion
#define MyAppVersion "0.1.0"
#endif

#ifndef StageRoot
#error Pass /DStageRoot=<release-staging\pack> to ISCC
#endif

[Setup]
AppId={#MyAppId}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppCopyright=(c) R2H Engineering
DefaultDirName={autopf}\R2H\r2h-second-brain
UsePreviousAppDir=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesEnvironment=yes
DisableProgramGroupPage=yes
WizardStyle=modern
Compression=lzma2/normal
SolidCompression=no
LZMANumBlockThreads=4
InternalCompressLevel=normal
OutputDir=..\release-output
OutputBaseFilename=r2h-second-brain-setup-{#MyAppVersion}-x64
; The payload (~10 GB of models and runtimes) exceeds the ~4.2 GB single-EXE
; limit imposed by Windows, so the release ships as one EXE plus its .bin
; slices kept in the same folder.
DiskSpanning=yes
SlicesPerDisk=1
DiskSliceSize=2100000000
SetupIconFile=..\apps\desktop\src-tauri\icons\icon.ico
UninstallDisplayName={#MyAppName} {#MyAppVersion}
UninstallDisplayIcon={app}\{#MyAppExeName}
MinVersion=10.0

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; ---- Offline prerequisites (installed only when missing on the target) ----
Source: "{#StageRoot}\redist\vc_redist.x64.exe"; DestDir: "{tmp}"; Flags: deleteafterinstall
Source: "{#StageRoot}\redist\MicrosoftEdgeWebView2RuntimeInstallerX64.exe"; DestDir: "{tmp}"; Flags: deleteafterinstall

; ---- Application (Program Files) ----
Source: "{#StageRoot}\app\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageRoot}\app\icon.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageRoot}\app\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#StageRoot}\app\tools\verify-install.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "{#StageRoot}\app\tools\verify-payload-hashes.ps1"; DestDir: "{app}\tools"; Flags: ignoreversion

; ---- AI pack staging (ProgramData\R2H.AI-ELE\staging-<ver>) ----
Source: "{#StageRoot}\local-ai\manifests\*"; DestDir: "{commonappdata}\R2H.AI-ELE\staging-{#MyAppVersion}\local-ai\manifests"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#StageRoot}\local-ai\models\generation\*"; DestDir: "{commonappdata}\R2H.AI-ELE\staging-{#MyAppVersion}\local-ai\models\generation"; Flags: ignoreversion recursesubdirs createallsubdirs nocompression
Source: "{#StageRoot}\local-ai\models\embedding\*"; DestDir: "{commonappdata}\R2H.AI-ELE\staging-{#MyAppVersion}\local-ai\models\embedding"; Flags: ignoreversion recursesubdirs createallsubdirs nocompression
Source: "{#StageRoot}\local-ai\models\reranker\*"; DestDir: "{commonappdata}\R2H.AI-ELE\staging-{#MyAppVersion}\local-ai\models\reranker"; Flags: ignoreversion recursesubdirs createallsubdirs nocompression
Source: "{#StageRoot}\local-ai\runtimes\*"; DestDir: "{commonappdata}\R2H.AI-ELE\staging-{#MyAppVersion}\local-ai\runtimes"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#StageRoot}\local-ai\python-runtime\*"; DestDir: "{commonappdata}\R2H.AI-ELE\staging-{#MyAppVersion}\local-ai\python-runtime"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#StageRoot}\local-ai\workers\*"; DestDir: "{commonappdata}\R2H.AI-ELE\staging-{#MyAppVersion}\local-ai\workers"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#StageRoot}\local-ai-pack.json"; DestDir: "{commonappdata}\R2H.AI-ELE\staging-{#MyAppVersion}"; Flags: ignoreversion
; Payload hash manifest installs directly at the pack root (describes final
; paths; overwritten on every upgrade, never deleted).
Source: "{#StageRoot}\payload-manifest.json"; DestDir: "{commonappdata}\R2H.AI-ELE"; Flags: ignoreversion

; ---- App home staging (ProgramData\R2H\r2h-second-brain\staging-<ver>) ----
Source: "{#StageRoot}\app-home\engines\khoj\*"; DestDir: "{commonappdata}\R2H\r2h-second-brain\staging-{#MyAppVersion}\engines\khoj"; Flags: ignoreversion recursesubdirs createallsubdirs nocompression
Source: "{#StageRoot}\app-home\third_party\khoj\*"; DestDir: "{commonappdata}\R2H\r2h-second-brain\staging-{#MyAppVersion}\third_party\khoj"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#StageRoot}\app-home\scripts\run-khoj-windows.py"; DestDir: "{commonappdata}\R2H\r2h-second-brain\staging-{#MyAppVersion}\scripts"; Flags: ignoreversion

[Dirs]
; run-data is application-writable persistent runtime state (logs, locks,
; embedded Khoj PostgreSQL data). Created here so the ACL grant applies even on
; first install.
Name: "{commonappdata}\R2H\r2h-second-brain\run-data"
Name: "{commonappdata}\R2H\r2h-second-brain\run-data\khoj"
Name: "{commonappdata}\R2H\r2h-second-brain\run-data\local-models"

[InstallDelete]
; Stale staging payloads from interrupted installs (never user data).
Type: filesandordirs; Name: "{commonappdata}\R2H.AI-ELE\staging-*"
Type: filesandordirs; Name: "{commonappdata}\R2H\r2h-second-brain\staging-*"

[Registry]
; Application project root for the installed (non-development) layout.
; The Tauri runtime resolves engines/, scripts/, and run-data/ relative to it.
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: string; ValueName: "R2H_SECOND_BRAIN_PROJECT_ROOT"; ValueData: "{commonappdata}\R2H\r2h-second-brain"; Flags: uninsdeletevalue

; Neutralize the legacy R2H.AI-ELE Local AI Pack lifecycle (both registry
; views). Its uninstaller was the component able to wipe the ProgramData AI
; pack; the new installer takes ownership of the pack and must retire it.
Root: HKLM; Subkey: "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{#LegacyAiPackAppId}"; ValueType: none; ValueName: ""; Flags: deletekey
Root: HKLM; Subkey: "SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\{#LegacyAiPackAppId}"; ValueType: none; ValueName: ""; Flags: deletekey

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "icacls.exe"; Parameters: """{commonappdata}\R2H\r2h-second-brain\run-data"" /grant *S-1-5-32-545:(OI)(CI)M"; Flags: runhidden waituntilterminated
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\tools\verify-install.ps1"" -Quiet"; Flags: runhidden waituntilterminated
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Stop the desktop app (product-owned image name; the pack runtimes are
; stopped by the path-verified UninstallShutdown code section).
Filename: "taskkill.exe"; Parameters: "/IM {#MyAppExeName} /F"; Flags: runhidden

[UninstallDelete]
; Safety net for interrupted-install leftovers only; never user data.
Type: filesandordirs; Name: "{commonappdata}\R2H.AI-ELE\staging-*"
Type: filesandordirs; Name: "{commonappdata}\R2H\r2h-second-brain\staging-*"

[Code]
const
  PackRoot = 'C:\ProgramData\R2H.AI-ELE';
  AppHome = 'C:\ProgramData\R2H\r2h-second-brain';
  AppDataDir = 'ai.r2h.second-brain';
  S_OK = 0;
  WebView2ClientsKey64 = 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  WebView2ClientsKey32 = 'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

function SetEnvironmentVariableW(Name: string; Value: string): Boolean;
  external 'SetEnvironmentVariableW@kernel32.dll stdcall';

// ---------------------------------------------------------------- utilities

function DirNameMatches(Parent, Prefix: string): Boolean;
var
  Name: string;
begin
  Name := Lowercase(ExtractFileName(Parent));
  Result := (Pos(Lowercase(Prefix), Name) = 1);
end;

function FindSingleDirMatching(Parent, Prefix: string; var Found: string): Boolean;
var
  FR: TFindRec;
  Candidate: string;
begin
  Result := False;
  Found := '';
  if not DirExists(Parent) then Exit;
  if FindFirst(Parent + '\*', FR) then
  try
    repeat
      if (FR.Attributes and FILE_ATTRIBUTE_DIRECTORY) <> 0 then
      begin
        Candidate := Parent + '\' + FR.Name;
        if (FR.Name <> '.') and (FR.Name <> '..') and DirNameMatches(Candidate, Prefix) then
        begin
          Found := Candidate;
          Result := True;
          Exit;
        end;
      end;
    until not FindNext(FR);
  finally
    FindClose(FR);
  end;
end;

procedure SwapDirectory(Staged, Final, Backup: string; var Swapped: Boolean);
begin
  Swapped := False;
  if not DirExists(Staged) then Exit;
  if DirExists(Final) then
  begin
    if not RenameFile(Final, Backup) then Exit;
  end;
  if not RenameFile(Staged, Final) then
  begin
    if DirExists(Backup) then
      RenameFile(Backup, Final);
    Exit;
  end;
  Swapped := True;
end;

procedure RollbackSwap(Final, Backup: string);
begin
  if DirExists(Backup) then
  begin
    if DirExists(Final) then
      DelTree(Final, True, True, True);
    RenameFile(Backup, Final);
  end;
end;

// ------------------------------------------------- path-verified shutdown

procedure KillProcessesUnderPath(ImageRoot, Prefix: string);
var
  WMI, ProcessList, Process: Variant;
  I, Count: Integer;
  ExecutablePath, CommandLine: string;
  TaskKillArgs: string;
  KillResultCode: Integer;
begin
  if not DirExists(ImageRoot) then Exit;
  try
    WMI := CreateOleObject('WbemScripting.SWbemLocator');
    WMI := WMI.ConnectServer('.', 'root\cimv2');
    ProcessList := WMI.ExecQuery(
      'SELECT ProcessId, ExecutablePath, CommandLine FROM Win32_Process');
    Count := ProcessList.Count;
    for I := 0 to Count - 1 do
    begin
      Process := ProcessList.ItemIndex(I);
      try
        ExecutablePath := Process.ExecutablePath;
        CommandLine := Process.CommandLine;
      except
        ExecutablePath := '';
        CommandLine := '';
      end;
      if (ExecutablePath <> '') and
         (Pos(Lowercase(ImageRoot), Lowercase(ExecutablePath)) = 1) and
         ((Prefix = '') or (Pos(Lowercase(Prefix), Lowercase(ExecutablePath)) = 1) or
          (Pos(Lowercase(Prefix), Lowercase(CommandLine)) = 1)) then
      begin
        TaskKillArgs := '/PID ' + IntToStr(Process.ProcessId) + ' /T /F';
        Exec('taskkill.exe', TaskKillArgs, '', SW_HIDE, ewWaitUntilTerminated, KillResultCode);
      end;
    end;
  except
    // WMI unavailable: fall back to no kill; file renames may then require a
    // reboot, which Setup reports through its pending-rename handling.
  end;
end;

procedure ShutdownOwnedProcesses;
var
  ResultCode: Integer;
begin
  // Pack runtimes: llama-server + pack python (reranker worker).
  KillProcessesUnderPath(PackRoot + '\local-ai\runtimes', '');
  KillProcessesUnderPath(PackRoot + '\local-ai\python-runtime', '');
  // App home: Khoj python wrapper + embedded postgres under the venv tree.
  KillProcessesUnderPath(AppHome + '\engines\khoj', '');
  // Desktop app.
  Exec('taskkill.exe', '/IM {#MyAppExeName} /F', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(1500);
end;

// ------------------------------------------------------------ install flow

procedure ReconcileInterruptedSwap;
var
  OldDir: string;
begin
  // A previous install may have died between rename steps. Restore the last
  // complete payload so this run starts from a consistent state.
  OldDir := '';
  if FindSingleDirMatching(PackRoot, 'old-', OldDir) then
  begin
    if not DirExists(PackRoot + '\local-ai') then
      RenameFile(OldDir, PackRoot + '\local-ai')
    else
      DelTree(OldDir, True, True, True);
  end;
  OldDir := '';
  if FindSingleDirMatching(AppHome + '\engines', 'old-', OldDir) then
  begin
    if not DirExists(AppHome + '\engines\khoj') then
      RenameFile(OldDir, AppHome + '\engines\khoj')
    else
      DelTree(OldDir, True, True, True);
  end;
end;

procedure RetireLegacyAiPackLifecycle;
begin
  // Remove the legacy uninstaller binary so the retired lifecycle can never
  // be invoked to delete the shared ProgramData AI pack again.
  DelTree(PackRoot + '\unins000.exe', False, True, True);
  DelTree(PackRoot + '\unins000.dat', False, True, True);
  DelTree(PackRoot + '\unins000.msg', False, True, True);
end;

function VcRuntimeInstalled: Boolean;
begin
  Result := FileExists(ExpandConstant('{sys}') + '\VCRUNTIME140.dll') and
    FileExists(ExpandConstant('{sys}') + '\VCRUNTIME140_1.dll');
end;

function WebView2Installed: Boolean;
var
  Version: string;
begin
  Result := False;
  if RegQueryStringValue(HKEY_LOCAL_MACHINE, WebView2ClientsKey64, 'pv', Version) or
     RegQueryStringValue(HKEY_LOCAL_MACHINE, WebView2ClientsKey32, 'pv', Version) or
     RegQueryStringValue(HKEY_CURRENT_USER, WebView2ClientsKey64, 'pv', Version) or
     RegQueryStringValue(HKEY_CURRENT_USER, WebView2ClientsKey32, 'pv', Version) then
    Result := (Version <> '') and (Version <> '0.0.0.0');
end;

function InstallOfflinePrerequisites: Boolean;
var
  ResultCode: Integer;
begin
  Result := True;
  if not VcRuntimeInstalled then
  begin
    if not Exec(ExpandConstant('{tmp}') + '\vc_redist.x64.exe',
        '/install /quiet /norestart', '', SW_SHOW, ewWaitUntilTerminated, ResultCode) then
    begin
      Result := False;
      Exit;
    end;
    // 0 = success, 1638 = newer version already installed, 3010 = restart required.
    if not (ResultCode in [0, 1638, 3010]) then
    begin
      Result := False;
      Exit;
    end;
  end;
  if not WebView2Installed then
  begin
    if not Exec(ExpandConstant('{tmp}') + '\MicrosoftEdgeWebView2RuntimeInstallerX64.exe',
        '/silent /install', '', SW_SHOW, ewWaitUntilTerminated, ResultCode) then
    begin
      Result := False;
      Exit;
    end;
    if (ResultCode <> 0) and (ResultCode <> 3010) then
    begin
      Result := False;
      Exit;
    end;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if not InstallOfflinePrerequisites then
  begin
    Result := 'A required system runtime (Visual C++ 2015-2022 x64 or ' +
      'Microsoft Edge WebView2 Runtime) could not be installed offline. ' +
      'The installer bundles both; free disk space and admin rights are required.';
    Exit;
  end;
  ShutdownOwnedProcesses;
  ReconcileInterruptedSwap;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  PackManifest, ManifestObj: string;
begin
  Result := True;
  if CurPageID = wpReady then
  begin
    // Warn when overwriting an existing pack so upgrades are a conscious step.
    PackManifest := PackRoot + '\local-ai\manifests\r2h-multi-evidence-models.json';
    if (not WizardSilent) and FileExists(PackManifest) then
    begin
      if MsgBox(
          'An existing R2H Local AI pack was detected.' + #13#10 +
          'It will be upgraded transactionally; your knowledge data is preserved.' + #13#10 +
          'Continue?', mbConfirmation, MB_YESNO) = IDNO then
        Result := False;
    end;
  end;
end;

function ActivateStagedPayloads: string;
var
  SwappedPack, SwappedKhoj: Boolean;
  StagedPack, StagedKhoj: string;
  BackupPack, BackupKhoj: string;
  VerifyFile: string;
  VerifyOk: Boolean;
  ResultCode: Integer;
begin
  Result := '';
  StagedPack := PackRoot + '\staging-{#MyAppVersion}\local-ai';
  StagedKhoj := AppHome + '\staging-{#MyAppVersion}\engines\khoj';
  BackupPack := PackRoot + '\old-{#MyAppVersion}';
  BackupKhoj := AppHome + '\engines\old-{#MyAppVersion}';

  SwapDirectory(StagedPack, PackRoot + '\local-ai', BackupPack, SwappedPack);
  if not SwappedPack then
  begin
    Result := 'Failed to activate the staged AI pack.';
    Exit;
  end;

  SwapDirectory(StagedKhoj, AppHome + '\engines\khoj', BackupKhoj, SwappedKhoj);
  if not SwappedKhoj then
  begin
    RollbackSwap(PackRoot + '\local-ai', BackupPack);
    Result := 'Failed to activate the staged Khoj engine; the previous AI pack was restored.';
    Exit;
  end;

  // Scripts wrapper is a direct file (stateless).
  if DirExists(AppHome + '\staging-{#MyAppVersion}\scripts') then
  begin
    ForceDirectories(AppHome + '\scripts');
    FileCopy(AppHome + '\staging-{#MyAppVersion}\scripts\run-khoj-windows.py',
      AppHome + '\scripts\run-khoj-windows.py', False);
  end;

  // Essential payload verification before declaring success.
  VerifyFile := ExpandConstant('{tmp}') + '\verify-post-install.json';
  VerifyOk := Exec('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -File "' +
    ExpandConstant('{app}') + '\tools\verify-payload-hashes.ps1" -JsonOut "' +
    VerifyFile + '"', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  if (not VerifyOk) or (ResultCode <> 0) then
  begin
    RollbackSwap(AppHome + '\engines\khoj', BackupKhoj);
    RollbackSwap(PackRoot + '\local-ai', BackupPack);
    Result := 'Post-install payload verification failed; the previous AI ' +
      'pack was restored. See ' + VerifyFile;
    Exit;
  end;

  // Success: retire rollback copies and staging roots.
  if DirExists(BackupPack) then
    DelTree(BackupPack, True, True, True);
  if DirExists(BackupKhoj) then
    DelTree(BackupKhoj, True, True, True);
  DelTree(PackRoot + '\staging-{#MyAppVersion}', True, True, True);
  DelTree(AppHome + '\staging-{#MyAppVersion}', True, True, True);

  // Make the env var visible to the process that launches the app post-install.
  SetEnvironmentVariableW('R2H_SECOND_BRAIN_PROJECT_ROOT', AppHome);
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ActivateError: string;
begin
  if CurStep = ssInstall then
  begin
    RetireLegacyAiPackLifecycle;
  end;
  if CurStep = ssPostInstall then
  begin
    ActivateError := ActivateStagedPayloads;
    if ActivateError <> '' then
    begin
      MsgBox(ActivateError, mbCriticalError, MB_OK);
      Abort;
    end;
  end;
end;

// --------------------------------------------------------------- uninstall

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  RemoveData: Boolean;
  ParentDir: string;
begin
  if CurUninstallStep = usUninstall then
  begin
    KillProcessesUnderPath(PackRoot + '\local-ai\runtimes', '');
    KillProcessesUnderPath(PackRoot + '\local-ai\python-runtime', '');
    KillProcessesUnderPath(AppHome + '\engines\khoj', '');
  end;
  if CurUninstallStep = usPostUninstall then
  begin
    RemoveData := False;
    if not UninstallSilent then
      RemoveData := (MsgBox(
        'Also remove the R2H local AI models (C:\ProgramData\R2H.AI-ELE),' + #13#10 +
        'the Khoj engine runtime, and all knowledge data?' + #13#10 + #13#10 +
        'Choose No to keep models and data (safe default; a later reinstall reuses them).',
        mbConfirmation, MB_YESNO) = IDYES);
    if RemoveData then
    begin
      DelTree(PackRoot, True, True, True);
      DelTree(AppHome, True, True, True);
      DelTree(ExpandConstant('{userappdata}') + '\' + AppDataDir, True, True, True);
      // Drop the now-empty R2H ProgramData root if nothing else needs it.
      ParentDir := 'C:\ProgramData\R2H';
      if DirExists(ParentDir) then
        RemoveDir(ParentDir);
    end;
  end;
end;

function InitializeSetup(): Boolean;
begin
  Result := True;
end;
