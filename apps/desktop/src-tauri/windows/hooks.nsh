; NSIS installer hooks for Lullshift (S11-T06, S12-T05, ADR-010).
;
; Before the files are removed, run the app in `--uninstall-hooks` mode: it removes Lullshift's
; hooks from Claude Code, Codex and Cursor with the same logic as Disconnect (original file
; restored when untouched, otherwise only Lullshift's own entries removed, the user's own hooks
; kept). It has no UI, writes nothing, always exits 0 and is idempotent.
;
; The hooks are KEPT (this macro does nothing) when the uninstaller is not a real uninstall:
;
;   1. Update mode (`/UPDATE`, used by the updater): the same hooks keep working.
;   2. A reinstall. Running a new setup over an existing install makes the setup run the old
;      uninstaller itself, as `uninstall.exe [/P] _?=<install dir>`. With `_?=` NSIS runs the
;      uninstaller in place, so `$EXEDIR` (the folder the running uninstaller lives in) equals
;      `$INSTDIR`. A real uninstall (Settings > Apps, or running uninstall.exe by hand) is
;      different: NSIS copies the uninstaller to a temporary folder (`...\~nsuN.tmp`) and runs
;      that copy, so `$EXEDIR` is the temporary folder. Checked on 2026-11-06 with a test
;      installer built by the bundled makensis: `$EXEDIR` was the temp folder for a normal
;      uninstall (also with /S) and the install folder for `uninstall.exe _?=<dir>`, with or
;      without a trailing backslash on <dir>.
;
; The test is deliberately one-sided: whenever `$EXEDIR` and `$INSTDIR` differ for any reason,
; hooks are removed, the safe behaviour from before this check existed. The reinstall installs
; into the same folder, so the recorded executable path stays valid and nothing is rewritten.
; Running `uninstall.exe _?=<dir>` by hand therefore also keeps the hooks; use Disconnect.
;
; Runs as the user who runs the uninstaller; the default install mode is currentUser. Never run
; by a silent per-machine uninstall of another account's profile (see docs/RELEASING.md).
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
  ${AndIf} $EXEDIR != $INSTDIR
    ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-hooks' $0
    ${EndIf}
  ${EndIf}
!macroend
