; NSIS installer hooks for Lullshift (S11-T06, ADR-010).
;
; Before the files are removed, run the app in `--uninstall-hooks` mode: it removes Lullshift's
; hooks from Claude Code, Codex and Cursor with the same logic as Disconnect (original file
; restored when untouched, otherwise only Lullshift's own entries removed, the user's own hooks
; kept). It has no UI, writes nothing, always exits 0 and is idempotent.
;
; Skipped in update mode (/UPDATE): the same hooks keep working with the new version.
; Runs as the user who runs the uninstaller; the default install mode is currentUser. Never run
; by a silent per-machine uninstall of another account's profile (see docs/RELEASING.md).
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-hooks' $0
    ${EndIf}
  ${EndIf}
!macroend
