Unicode true
!include "MUI2.nsh"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!define ROOT "..\..\.."

Name "CodexPilot"
OutFile "${ROOT}\dist\windows\CodexPilot-${VERSION}-windows-x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\CodexPilot"
InstallDirRegKey HKCU "Software\CodexPilot" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"

  nsExec::ExecToLog 'taskkill /IM codex-pilot-launcher.exe /F'
  Pop $0
  nsExec::ExecToLog 'taskkill /IM codex-pilot-manager.exe /F'
  Pop $0

  File "${ROOT}\dist\windows\app\codex-pilot-launcher.exe"
  File "${ROOT}\dist\windows\app\codex-pilot-manager.exe"

  CreateShortcut "$DESKTOP\CodexPilot.lnk" "$INSTDIR\codex-pilot-manager.exe" "" "$INSTDIR\codex-pilot-manager.exe"
  CreateDirectory "$SMPROGRAMS\CodexPilot"
  CreateShortcut "$SMPROGRAMS\CodexPilot\CodexPilot.lnk" "$INSTDIR\codex-pilot-manager.exe" "" "$INSTDIR\codex-pilot-manager.exe"
  CreateShortcut "$SMPROGRAMS\CodexPilot\卸载 CodexPilot.lnk" "$INSTDIR\uninstall.exe" "" "$INSTDIR\codex-pilot-manager.exe"

  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\CodexPilot" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CodexPilot" "DisplayName" "CodexPilot"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CodexPilot" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CodexPilot" "Publisher" "hl9565"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CodexPilot" "DisplayIcon" "$INSTDIR\codex-pilot-manager.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CodexPilot" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CodexPilot" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CodexPilot" "QuietUninstallString" "$INSTDIR\uninstall.exe /S"
SectionEnd

Section "Uninstall"
  nsExec::ExecToLog 'taskkill /IM codex-pilot-launcher.exe /F'
  Pop $0
  nsExec::ExecToLog 'taskkill /IM codex-pilot-manager.exe /F'
  Pop $0

  Delete "$DESKTOP\CodexPilot.lnk"
  Delete "$SMPROGRAMS\CodexPilot\CodexPilot.lnk"
  Delete "$SMPROGRAMS\CodexPilot\卸载 CodexPilot.lnk"
  RMDir "$SMPROGRAMS\CodexPilot"

  Delete "$INSTDIR\codex-pilot-launcher.exe"
  Delete "$INSTDIR\codex-pilot-manager.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\CodexPilot"
  DeleteRegKey HKCU "Software\CodexPilot"
SectionEnd
