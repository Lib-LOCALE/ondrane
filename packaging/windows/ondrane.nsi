; Ondrane Windows installer (NSIS 3). Built by `cargo xtask windows-installer`
; (equivalent to: makensis -DVERSION=x.y.z -DSRC=<staged Ondrane folder> ondrane.nsi)
; Per-user install: no admin rights needed. VLC 3 is installed separately.
Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"

!define APP "Ondrane"
!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef SRC
  !define SRC "Ondrane"
!endif
!define UNKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ondrane"

Name "${APP}"
OutFile "Ondrane-Setup-${VERSION}.exe"
InstallDir "$LOCALAPPDATA\Programs\Ondrane"
InstallDirRegKey HKCU "Software\Ondrane" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetCompressorDictSize 64
BrandingText "Ondrane ${VERSION} (Rust Edition)"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "Ondrane"
VIAddVersionKey "FileDescription" "Ondrane Setup (Pure Rust Edition)"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "MIT licence, Ondrane Contributors"

!define MUI_ICON "${SRC}\ondrane.ico"
!define MUI_UNICON "${SRC}\ondrane.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "Welcome to Ondrane ${VERSION}"
!define MUI_WELCOMEPAGE_TEXT "Ondrane is a modern, pure Rust interface based on the Juxtopposed redesign for the VLC engine.$\r$\n$\r$\nPowered by Slint UI and native libVLC bindings written entirely in Rust.$\r$\n$\r$\nNo admin rights are needed: Ondrane installs just for your user account."
!define MUI_LICENSEPAGE_TEXT_TOP "Ondrane is free and open source (MIT)."
!define MUI_COMPONENTSPAGE_SMALLDESC
!define MUI_FINISHPAGE_RUN "$INSTDIR\ondrane.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start Ondrane"
!define MUI_FINISHPAGE_SHOWREADME "https://github.com/Lib-LOCALE/ondrane"
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Open Ondrane GitHub Page"
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SRC}\license-summary.txt"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

!macro CloseApp un
Function ${un}CloseRunningOndrane
  nsExec::Exec `powershell -NoProfile -Command "Get-Process ondrane -ErrorAction SilentlyContinue | Where-Object { $$_.Path -like '$INSTDIR\*' } | Stop-Process -Force"`
  Pop $0
FunctionEnd
!macroend
!insertmacro CloseApp ""
!insertmacro CloseApp "un."

Section "Ondrane (required)" SecMain
  SectionIn RO
  Call CloseRunningOndrane
  SetOutPath "$INSTDIR"
  File /r "${SRC}\*.*"
  WriteUninstaller "$INSTDIR\Uninstall ondrane.exe"
  WriteRegStr HKCU "Software\Ondrane" "InstallDir" "$INSTDIR"

  CreateShortcut "$SMPROGRAMS\Ondrane.lnk" "$INSTDIR\ondrane.exe" "" "$INSTDIR\ondrane.exe" 0 SW_SHOWNORMAL "" "Ondrane media player"

  WriteRegStr HKCU "${UNKEY}" "DisplayName" "Ondrane"
  WriteRegStr HKCU "${UNKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNKEY}" "Publisher" "Ondrane Contributors"
  WriteRegStr HKCU "${UNKEY}" "DisplayIcon" "$INSTDIR\ondrane.exe,0"
  WriteRegStr HKCU "${UNKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNKEY}" "UninstallString" '"$INSTDIR\Uninstall ondrane.exe"'
  WriteRegStr HKCU "${UNKEY}" "QuietUninstallString" '"$INSTDIR\Uninstall ondrane.exe" /S'
  WriteRegDWORD HKCU "${UNKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNKEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNKEY}" "EstimatedSize" "$0"
SectionEnd

Section "Desktop shortcut" SecDesktop
  CreateShortcut "$DESKTOP\Ondrane.lnk" "$INSTDIR\ondrane.exe" "" "$INSTDIR\ondrane.exe" 0 SW_SHOWNORMAL "" "Ondrane media player"
SectionEnd

Section "Offer Ondrane for videos and music (Open with)" SecAssoc
  WriteRegStr HKCU "Software\Classes\Ondrane.Media" "" "Media file"
  WriteRegStr HKCU "Software\Classes\Ondrane.Media\DefaultIcon" "" "$INSTDIR\ondrane.ico,0"
  WriteRegStr HKCU "Software\Classes\Ondrane.Media\shell" "" "open"
  WriteRegStr HKCU "Software\Classes\Ondrane.Media\shell\open" "FriendlyAppName" "Ondrane"
  WriteRegStr HKCU "Software\Classes\Ondrane.Media\shell\open\command" "" '"$INSTDIR\ondrane.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\Applications\ondrane.exe" "FriendlyAppName" "Ondrane"
  WriteRegStr HKCU "Software\Classes\Applications\ondrane.exe\DefaultIcon" "" "$INSTDIR\ondrane.ico,0"
  WriteRegStr HKCU "Software\Classes\Applications\ondrane.exe\shell\open" "FriendlyAppName" "Ondrane"
  WriteRegStr HKCU "Software\Classes\Applications\ondrane.exe\shell\open\command" "" '"$INSTDIR\ondrane.exe" "%1"'
  WriteRegStr HKCU "Software\Ondrane\Capabilities" "ApplicationName" "Ondrane"
  WriteRegStr HKCU "Software\Ondrane\Capabilities" "ApplicationDescription" "Ondrane media player (100% Rust)"
  WriteRegStr HKCU "Software\Ondrane\Capabilities" "ApplicationIcon" "$INSTDIR\ondrane.ico,0"
  WriteRegStr HKCU "Software\RegisteredApplications" "Ondrane" "Software\Ondrane\Capabilities"

  ; Register formats
  !macro RegisterExt EXT
    WriteRegStr HKCU "Software\Classes\.${EXT}\OpenWithProgids" "Ondrane.Media" ""
    WriteRegStr HKCU "Software\Classes\Applications\ondrane.exe\SupportedTypes" ".${EXT}" ""
    WriteRegStr HKCU "Software\Ondrane\Capabilities\FileAssociations" ".${EXT}" "Ondrane.Media"
  !macroend

  !insertmacro RegisterExt "3gp"
  !insertmacro RegisterExt "aac"
  !insertmacro RegisterExt "ac3"
  !insertmacro RegisterExt "aiff"
  !insertmacro RegisterExt "alac"
  !insertmacro RegisterExt "ape"
  !insertmacro RegisterExt "asf"
  !insertmacro RegisterExt "avi"
  !insertmacro RegisterExt "flac"
  !insertmacro RegisterExt "flv"
  !insertmacro RegisterExt "m2ts"
  !insertmacro RegisterExt "m4a"
  !insertmacro RegisterExt "m4v"
  !insertmacro RegisterExt "mka"
  !insertmacro RegisterExt "mkv"
  !insertmacro RegisterExt "mov"
  !insertmacro RegisterExt "mp3"
  !insertmacro RegisterExt "mp4"
  !insertmacro RegisterExt "mpeg"
  !insertmacro RegisterExt "mpg"
  !insertmacro RegisterExt "mts"
  !insertmacro RegisterExt "ogg"
  !insertmacro RegisterExt "ogv"
  !insertmacro RegisterExt "opus"
  !insertmacro RegisterExt "rmvb"
  !insertmacro RegisterExt "ts"
  !insertmacro RegisterExt "vob"
  !insertmacro RegisterExt "wav"
  !insertmacro RegisterExt "webm"
  !insertmacro RegisterExt "wma"
  !insertmacro RegisterExt "wmv"

  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd

Section "Uninstall"
  Call un.CloseRunningOndrane
  Delete "$SMPROGRAMS\Ondrane.lnk"
  Delete "$DESKTOP\Ondrane.lnk"

  Delete "$INSTDIR\ondrane.exe"
  Delete "$INSTDIR\ondrane.ico"
  Delete "$INSTDIR\libvlc.dll"
  Delete "$INSTDIR\libvlccore.dll"
  Delete "$INSTDIR\license-summary.txt"
  RMDir /r "$INSTDIR\plugins"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.md"
  Delete "$INSTDIR\Uninstall ondrane.exe"
  RMDir /r "$INSTDIR\logs"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "Software\Classes\Ondrane.Media"
  DeleteRegKey HKCU "Software\Classes\Applications\ondrane.exe"
  DeleteRegValue HKCU "Software\RegisteredApplications" "Ondrane"
  DeleteRegKey HKCU "Software\Ondrane"
  DeleteRegKey HKCU "${UNKEY}"

  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd
