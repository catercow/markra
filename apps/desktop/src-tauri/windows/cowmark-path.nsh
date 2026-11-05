; CowMark builds bundle the `cowmark` command next to the app. These put the install folder on
; the current user's PATH so `cowmark check` works in new terminals, and take it off again on
; uninstall. Stock Markra has no cowmark.exe, so nothing happens there.
;
; The PATH edit itself is in cowmark-path.ps1 (see why there). An update runs the old
; uninstaller and then this installer, so the folder ends up on PATH once.

; Set here: inside a macro, ${__FILEDIR__} is the folder of the script that inserts it.
!define COWMARK_PATH_SCRIPT "${__FILEDIR__}\cowmark-path.ps1"

!macro CowMarkPath ACTION
  ${If} ${FileExists} "$INSTDIR\cowmark.exe"
    InitPluginsDir
    File "/oname=$PLUGINSDIR\cowmark-path.ps1" "${COWMARK_PATH_SCRIPT}"
    Push $0
    nsExec::Exec '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\cowmark-path.ps1" -Action ${ACTION} -Dir "$INSTDIR"'
    Pop $0 ; exit code: a failure leaves PATH as it was, which isn't worth failing the install over
    Pop $0
    ; Tell Explorer and other programs that the environment changed (HWND_BROADCAST, WM_SETTINGCHANGE).
    SendMessage 0xFFFF 0x1A 0 "STR:Environment" /TIMEOUT=5000
  ${EndIf}
!macroend
