; Inno Setup – installer per utente corrente (non richiede privilegi di amministratore)
#define AppName "Gestione Condominio"
#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

[Setup]
AppId={{6B1E5C7A-3F52-4C0E-9A57-CONDOMINIO001}
AppName={#AppName}
AppVersion={#AppVersion}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=Gestione_Condominio_Setup
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
UninstallDisplayName={#AppName}

[Languages]
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"

[Tasks]
Name: "desktopicon"; Description: "Crea un'icona sul desktop"; Flags: unchecked

[Files]
Source: "..\target\release\condominio.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\condominio.exe"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\condominio.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\condominio.exe"; Description: "Avvia {#AppName}"; Flags: nowait postinstall skipifsilent
; I dati degli utenti (in %APPDATA%) NON vengono toccati dalla disinstallazione.
