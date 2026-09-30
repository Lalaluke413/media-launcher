#ifndef AppVersion
  #error Pass /DAppVersion=<Cargo package version>
#endif
#ifndef BundleDir
  #error Pass /DBundleDir=<assembled bundle>
#endif
#ifndef OutputDir
  #error Pass /DOutputDir=<output directory>
#endif

[Setup]
; Keep this ID and install mode stable across releases.
AppId={{CC11E30A-1568-4B61-94F7-CF31D087709E}
AppName=Media Launcher
AppVersion={#AppVersion}
AppPublisher=Media Launcher contributors
AppPublisherURL=https://github.com/Lalaluke413/media-launcher
DefaultDirName={localappdata}\Programs\Media Launcher
DefaultGroupName=Media Launcher
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
UsePreviousAppDir=yes
DisableProgramGroupPage=yes
OutputDir={#OutputDir}
OutputBaseFilename=media-launcher-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\media-launcher.exe
CloseApplications=yes
RestartApplications=no

[Files]
Source: "{#BundleDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\Media Launcher"; Filename: "{app}\media-launcher.exe"; WorkingDir: "{app}"

[Run]
Filename: "{app}\media-launcher.exe"; Description: "Launch Media Launcher"; Flags: nowait postinstall skipifsilent
; Configuration and plugins live outside {app}. No uninstall deletion of user data.
