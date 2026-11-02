; Hooks del instalador NSIS (`bundle.windows.nsis.installerHooks` en tauri.conf.json).
;
; Ponen la carpeta de instalacion en el Path del usuario para que el comando `elision`
; funcione en cualquier terminal nueva, y la quitan al desinstalar.
;
; - El Path lo edita elision-path.ps1 (ver sus reglas), no NSIS: las cadenas de NSIS se
;   truncan a 1024 caracteres y un Path de usuario largo quedaria cortado.
; - La carpeta viaja en la variable de entorno ELISION_PATH_ENTRY, que hereda el proceso
;   de PowerShell: asi no hay que escapar comillas ni la barra final en la linea de
;   comandos.
; - Se usa HKCU porque `installMode` es "currentUser". Si algun dia pasa a "perMachine"
;   o "both", hay que revisar este archivo: el Path seria el del sistema (HKLM).
; - Si PowerShell falla, la instalacion sigue: la app funciona sin `elision` en el Path.
;   El codigo queda en el detalle del instalador.

!include WinMessages.nsh

; Carpeta de este archivo, capturada al incluirlo: dentro de una macro, ${__FILEDIR__}
; seria la carpeta donde Tauri genera installer.nsi (bajo target/).
!define ELISION_HOOKS_DIR "${__FILEDIR__}"

; MODE: add | remove (el primer argumento de elision-path.ps1).
!macro ELISION_UPDATE_USER_PATH MODE
  Push $0
  InitPluginsDir
  File "/oname=$PLUGINSDIR\elision-path.ps1" "${ELISION_HOOKS_DIR}\elision-path.ps1"
  System::Call 'kernel32::SetEnvironmentVariable(t "ELISION_PATH_ENTRY", t "$INSTDIR") i'
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\elision-path.ps1" ${MODE}'
  Pop $0
  ${If} $0 != 0
    DetailPrint "elision-path.ps1 ${MODE}: $0"
  ${EndIf}
  System::Call 'kernel32::SetEnvironmentVariable(t "ELISION_PATH_ENTRY", p 0) i'
  ; El Explorador recarga el entorno: las terminales que se abran despues ya ven el Path
  ; nuevo. Las que ya estaban abiertas conservan el anterior.
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
  Pop $0
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro ELISION_UPDATE_USER_PATH add
!macroend

; En una actualizacion (/UPDATE) la entrada se queda: el instalador nuevo la encuentra y
; no la duplica. Tauri tampoco borra los accesos directos en ese caso.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro ELISION_UPDATE_USER_PATH remove
  ${EndIf}
!macroend
