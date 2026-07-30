; Oats-owned model downloads live outside Tauri's standard app-data folders at
; %USERPROFILE%\.ariso\models. Remove only that subtree when the user explicitly
; selects "Delete app data"; the vault and recordings are sibling paths and
; must remain untouched.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
    ClearErrors
    ${un.GetFileAttributes} "$PROFILE\.ariso\models" "REPARSE_POINT" $0
    ${IfNot} ${Errors}
      ${If} $0 = 0
        ; NSIS RMDir /r follows nested junctions. Windows RD removes directory
        ; junction entries without traversing their targets, so use it for the
        ; managed model tree after rejecting a redirected root above.
        ; Pass the fixed path through the child environment instead of placing
        ; profile characters directly in cmd.exe syntax.
        System::Call 'kernel32::SetEnvironmentVariableW(w "OATS_MODELS_TO_DELETE", w "$PROFILE\.ariso\models") i .r0'
        ${If} $0 = 0
          DetailPrint "Failed to prepare the Oats models path for removal."
        ${Else}
          nsExec::ExecToStack '"$SYSDIR\cmd.exe" /D /V:OFF /C RD /S /Q "%OATS_MODELS_TO_DELETE%"'
          Pop $0
          Pop $1
          ${If} $0 <> 0
            DetailPrint "Failed to remove Oats models (exit $0): $1"
          ${EndIf}
          System::Call 'kernel32::SetEnvironmentVariableW(w "OATS_MODELS_TO_DELETE", p 0)'
        ${EndIf}
        ; Remove .ariso only if it became empty. Vaults, recordings, and other
        ; user data keep the parent directory in place.
        RMDir "$PROFILE\.ariso"
      ${Else}
        DetailPrint "Skipped redirected Oats models directory."
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend
