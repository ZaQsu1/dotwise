#define MyAppName "Dotwise"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "Dotwise"
#define MyAppExeName "dotwise_gui.exe"

[Setup]
AppId=Dotwise.Editor
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\Dotwise
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\{#MyAppExeName}
OutputDir=.
OutputBaseFilename=Dotwise-Setup-{#MyAppVersion}-x64
SetupIconFile=..\assets\dotwise.ico
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
CloseApplications=yes
RestartApplications=no
WizardStyle=modern
Compression=lzma2
SolidCompression=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "polish"; MessagesFile: "compiler:Languages\Polish.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "WebView2Bootstrapper.exe"; DestDir: "{tmp}"; Flags: deleteafterinstall

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{tmp}\WebView2Bootstrapper.exe"; Parameters: "/silent /install"; StatusMsg: "Installing Microsoft Edge WebView2 Runtime..."; Flags: runhidden waituntilterminated; Check: not IsWebView2Installed
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: postinstall nowait skipifsilent

[Code]
function HasWebView2Runtime(const Root: string): Boolean;
var
	FindRec: TFindRec;
begin
	Result := False;
	if not FindFirst(Root + '\*', FindRec) then
		Exit;
	try
		repeat
			if (FindRec.Name <> '.') and (FindRec.Name <> '..') and
				 FileExists(Root + '\' + FindRec.Name + '\msedgewebview2.exe') then
			begin
				Result := True;
				Exit;
			end;
		until not FindNext(FindRec);
	finally
		FindClose(FindRec);
	end;
end;

function IsWebView2Installed: Boolean;
begin
	Result := HasWebView2Runtime(ExpandConstant('{autopf}\Microsoft\EdgeWebView\Application')) or
		HasWebView2Runtime(ExpandConstant('{pf32}\Microsoft\EdgeWebView\Application')) or
		HasWebView2Runtime(ExpandConstant('{localappdata}\Microsoft\EdgeWebView\Application'));
end;