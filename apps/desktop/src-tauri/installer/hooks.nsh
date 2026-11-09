; The update system keeps the previous program and leaves these beside the installed one.
; The installer does not know them, so without this they would stay behind.
!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$INSTDIR\troniccad.previous"
  Delete "$INSTDIR\troniccad.incoming"
  Delete "$INSTDIR\troniccad.holding"
  Delete "$INSTDIR\troniccad.failed"
  RMDir "$INSTDIR"
!macroend
