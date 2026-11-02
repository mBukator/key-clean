; KeyClean's additions to the NSIS installer (M11, ADR 0019).
;
; Tauri includes this file at the top of src-tauri/windows/installer.nsi (`installerHooks` in
; tauri.conf.json), before the template defines BUNDLEID, MAINBINARYNAME and the languages. So
; everything that needs those is a macro, expanded where the template calls it; each call site
; in the template is marked `; KEYCLEAN:`.
;
; - Start at sign-in: one value, HKCU\...\Run\KeyClean = "<install dir>\keyclean.exe"
;   --autostart, the string keyclean_core::startup::run_command builds (ADR 0018). Task
;   Manager's StartupApproved record is never written; uninstall removes it with our Run value.
;   Another copy's Run value is never touched.
; - A running KeyClean is asked to quit before its files are replaced or removed. The exit
;   ends any lock and releases input; Restart Manager (the template's check) is the fallback.
; - The user-facing text matches locales/en/strings.json (installer.*).
;
; src-tauri/src/installer.rs tests these defines and strings against the Rust side.

!define KEYCLEAN_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define KEYCLEAN_APPROVED_KEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
!define KEYCLEAN_RUN_VALUE "KeyClean"
!define KEYCLEAN_AUTOSTART_FLAG "--autostart"
!define KEYCLEAN_MAX_RUN_COMMAND 260
; How long to wait for KeyClean (the app, then its engine process) to exit: 40 x 250 ms.
!define KEYCLEAN_EXIT_POLLS 40

; Declared here, before the template's pages use them. 1 to write the Run value at install, 0
; to leave it off.
Var KeyCleanStartup
; The install folder as .onInit found it: the previous install's, unless /D= gave another.
Var KeyCleanOldDir
Var KeyCleanStartupCheckbox
Var KeyCleanSession
Var KeyCleanTmp
Var KeyCleanTmp2
Var KeyCleanTmp3

; Strings and functions. Inserted after the language files, where ${LANG_ENGLISH} and the
; template's defines exist.
!macro KEYCLEAN_DEFINITIONS
  LangString keycleanStartAtSignIn ${LANG_ENGLISH} "Start KeyClean when I sign in"
  LangString keycleanCloseRunning ${LANG_ENGLISH} "KeyClean is running and will be closed. If a lock is in progress, it ends and your keyboard and mouse work again."
  LangString keycleanStillRunning ${LANG_ENGLISH} "KeyClean is still running. Quit it from its tray icon, then try again."
  LangString keycleanDeleteSettings ${LANG_ENGLISH} "Also delete my KeyClean settings"
  LangString keycleanStartupPathTooLong ${LANG_ENGLISH} "KeyClean can't start when you sign in from this folder: its path is too long."

  ; Called from .onInit, before the reinstall page may run the old uninstaller. A fresh install
  ; starts at sign-in by default (§29); an existing one keeps what it does now, so an entry the
  ; user turned off stays off.
  Function KeyCleanStartupDefault
    StrCpy $KeyCleanOldDir $INSTDIR
    ReadRegStr $KeyCleanTmp SHCTX "${UNINSTKEY}" "UninstallString"
    ${If} $KeyCleanTmp == ""
      StrCpy $KeyCleanStartup 1
    ${Else}
      !insertmacro KEYCLEAN_RUN_IS_OURS $INSTDIR $KeyCleanStartup
    ${EndIf}
  FunctionEnd

  ; Adds the checkbox to the install-location page (MUI's IDD_DIR, 300 x 140 dialog units):
  ; under the Destination Folder box (y 70-105) and right of the space texts (x 0-150, y 115-133),
  ; where NSIS keeps its own spare checkbox. Two lines high, so a longer translation wraps.
  Function KeyCleanDirectoryShow
    FindWindow $KeyCleanTmp "#32770" "" $HWNDPARENT ; the page's inner dialog
    System::Call "*(i 158, i 109, i 300, i 131) p .s"
    Pop $KeyCleanTmp3
    ; Dialog units to pixels for this font and DPI: left $4, top $5, right $6, bottom $7.
    System::Call "user32::MapDialogRect(p $KeyCleanTmp, p $KeyCleanTmp3)"
    System::Call "*$KeyCleanTmp3(i .r4, i .r5, i .r6, i .r7)"
    System::Free $KeyCleanTmp3
    IntOp $6 $6 - $4 ; width
    IntOp $7 $7 - $5 ; height
    System::Call 'user32::CreateWindowExW(i ${__NSD_CheckBox_EXSTYLE}, w "${__NSD_CheckBox_CLASS}", w "$(keycleanStartAtSignIn)", i ${__NSD_CheckBox_STYLE}, i r4, i r5, i r6, i r7, p $KeyCleanTmp, p 0, p 0, p 0) p .s'
    Pop $KeyCleanStartupCheckbox
    SendMessage $HWNDPARENT ${WM_GETFONT} 0 0 $KeyCleanTmp2
    SendMessage $KeyCleanStartupCheckbox ${WM_SETFONT} $KeyCleanTmp2 1
    ${If} $KeyCleanStartup = 1
      SendMessage $KeyCleanStartupCheckbox ${BM_SETCHECK} ${BST_CHECKED} 0
    ${EndIf}
  FunctionEnd

  Function KeyCleanDirectoryLeave
    SendMessage $KeyCleanStartupCheckbox ${BM_GETCHECK} 0 0 $KeyCleanTmp
    ${If} $KeyCleanTmp = ${BST_CHECKED}
      StrCpy $KeyCleanStartup 1
    ${Else}
      StrCpy $KeyCleanStartup 0
    ${EndIf}
  FunctionEnd
!macroend

; Sets `out` to 1 if the Run value starts the exe in folder `dir`, else 0. Compared without
; case, like keyclean_core::startup.
!macro KEYCLEAN_RUN_IS_OURS dir out
  ReadRegStr $KeyCleanTmp HKCU "${KEYCLEAN_RUN_KEY}" "${KEYCLEAN_RUN_VALUE}"
  StrCpy $KeyCleanTmp2 '"${dir}\${MAINBINARYNAME}.exe"'
  StrLen $KeyCleanTmp3 $KeyCleanTmp2
  StrCpy $KeyCleanTmp $KeyCleanTmp $KeyCleanTmp3
  ${If} $KeyCleanTmp == $KeyCleanTmp2
    StrCpy ${out} 1
  ${Else}
    StrCpy ${out} 0
  ${EndIf}
!macroend

; Sets `out` to 1 if a process runs this install's exe (the app or its engine), else 0. Uses
; Restart Manager like the template's CheckIfAppIsRunning, so other copies don't count.
; Changes $0-$3.
!macro KEYCLEAN_IS_RUNNING out
  StrCpy ${out} 0
  !insertmacro RestartManager_StartSession $KeyCleanSession
  ${If} $KeyCleanSession != ""
    !insertmacro RestartManager_RegisterFile $KeyCleanSession "$INSTDIR\${MAINBINARYNAME}.exe"
    ${If} $0 = 0
      System::Call 'RSTRTMGR::RmGetList(i $KeyCleanSession, *i .r1, *i .r2, p 0, *i .r3) i .r0'
      ${If} $0 = ${ERROR_MORE_DATA}
        StrCpy ${out} 1
      ${EndIf}
    ${EndIf}
    !insertmacro RestartManager_EndSession $KeyCleanSession
  ${EndIf}
!macroend

; If this install's KeyClean runs, asks to close it (not in silent or passive mode), then posts
; WM_CLOSE to the single-instance plugin's window, which KeyClean guards so that it exits
; cleanly (fail-safe matrix: WM_CLOSE), and waits for both processes to exit. `onCancel` runs
; when the user says no. Whatever is still running afterwards is left to the template's
; Restart Manager check, which ends it.
!macro KEYCLEAN_CLOSE_RUNNING onCancel
  !insertmacro KEYCLEAN_IS_RUNNING $KeyCleanTmp
  ${If} $KeyCleanTmp = 1
    ${IfNot} ${Silent}
    ${AndIf} $PassiveMode <> 1
      ${If} ${Cmd} `MessageBox MB_OKCANCEL|MB_ICONINFORMATION "$(keycleanCloseRunning)" IDCANCEL`
        ${onCancel}
      ${EndIf}
    ${EndIf}
    ; Without the window (KeyClean still starting, or only an engine left) there is nothing to
    ; wait for: Restart Manager ends it.
    FindWindow $KeyCleanTmp "${BUNDLEID}-sic"
    ${If} $KeyCleanTmp P<> 0
      System::Call "user32::PostMessageW(p $KeyCleanTmp, i ${WM_CLOSE}, p 0, p 0)"
      StrCpy $KeyCleanTmp2 0
      ${Do}
        Sleep 250
        IntOp $KeyCleanTmp2 $KeyCleanTmp2 + 1
        !insertmacro KEYCLEAN_IS_RUNNING $KeyCleanTmp
        ${If} $KeyCleanTmp = 0
        ${OrIf} $KeyCleanTmp2 >= ${KEYCLEAN_EXIT_POLLS}
          ${ExitDo}
        ${EndIf}
      ${Loop}
    ${EndIf}
  ${EndIf}
!macroend

; Writes or removes the Run value after the files are in place.
!macro KEYCLEAN_APPLY_STARTUP
  ${If} $KeyCleanStartup = 1
    StrCpy $KeyCleanTmp '"$INSTDIR\${MAINBINARYNAME}.exe" ${KEYCLEAN_AUTOSTART_FLAG}'
    StrLen $KeyCleanTmp2 $KeyCleanTmp ; UTF-16 units, as Windows counts them
    ${If} $KeyCleanTmp2 > ${KEYCLEAN_MAX_RUN_COMMAND}
      DetailPrint "$(keycleanStartupPathTooLong)"
    ${Else}
      WriteRegStr HKCU "${KEYCLEAN_RUN_KEY}" "${KEYCLEAN_RUN_VALUE}" $KeyCleanTmp
    ${EndIf}
  ${Else}
    ; Ours: this folder's exe, or the previous install's if the user picked another folder.
    !insertmacro KEYCLEAN_RUN_IS_OURS $INSTDIR $KeyCleanTmp
    ${If} $KeyCleanTmp = 0
    ${AndIf} $KeyCleanOldDir != ""
      !insertmacro KEYCLEAN_RUN_IS_OURS $KeyCleanOldDir $KeyCleanTmp
    ${EndIf}
    ${If} $KeyCleanTmp = 1
      DeleteRegValue HKCU "${KEYCLEAN_RUN_KEY}" "${KEYCLEAN_RUN_VALUE}"
    ${EndIf}
  ${EndIf}
!macroend

; At uninstall: removes our Run value and Task Manager's record of it. With no Run value left,
; the record is an orphan and goes too; another copy's value and record stay.
!macro KEYCLEAN_REMOVE_STARTUP
  ReadRegStr $KeyCleanTmp HKCU "${KEYCLEAN_RUN_KEY}" "${KEYCLEAN_RUN_VALUE}"
  ${If} $KeyCleanTmp == ""
    DeleteRegValue HKCU "${KEYCLEAN_APPROVED_KEY}" "${KEYCLEAN_RUN_VALUE}"
  ${Else}
    !insertmacro KEYCLEAN_RUN_IS_OURS $INSTDIR $KeyCleanTmp
    ${If} $KeyCleanTmp = 1
      DeleteRegValue HKCU "${KEYCLEAN_RUN_KEY}" "${KEYCLEAN_RUN_VALUE}"
      DeleteRegValue HKCU "${KEYCLEAN_APPROVED_KEY}" "${KEYCLEAN_RUN_VALUE}"
    ${EndIf}
  ${EndIf}
!macroend

; Tauri's hooks.
!macro NSIS_HOOK_PREINSTALL
  !insertmacro KEYCLEAN_CLOSE_RUNNING `Abort "$(keycleanStillRunning)"`
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro KEYCLEAN_APPLY_STARTUP
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro KEYCLEAN_CLOSE_RUNNING `Abort "$(keycleanStillRunning)"`
!macroend
