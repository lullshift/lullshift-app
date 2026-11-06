; NSIS installer hooks for Lullshift (S11-T06, S12-T05, S12 review M1, ADR-010).
;
; The rule: an uninstaller always removes the agent hooks, except in update mode.
;
; Before the files are removed, run the app in `--uninstall-hooks` mode: it removes Lullshift's
; hooks from Claude Code, Codex and Cursor, and the OpenCode plugin file, with the same logic as
; Disconnect (original file restored when untouched, otherwise only Lullshift's own entries
; removed, the user's own hooks kept). It has no UI, writes nothing, always exits 0 and is
; idempotent.
;
; Why not keep the hooks on a reinstall (the earlier rule, `$EXEDIR == $INSTDIR`): a new setup
; runs the old uninstaller (`uninstall.exe [/P] _?=$INSTDIR`) BEFORE the folder page. The user
; may then cancel, or install into a different folder, and the hooks would point at a deleted
; program in a user-writable folder, a place someone else could put a program. So:
;
;   1. Update mode (`/UPDATE`, used by the updater): the program stays in the same folder and
;      is replaced in place, so the same hooks keep working. Nothing is removed.
;   2. Anything else removes the hooks. When the uninstaller was run in place by a setup
;      (`$EXEDIR == $INSTDIR`; a real uninstall copies the uninstaller to a temporary folder
;      first, checked 2026-11-06 with the bundled makensis) it is a reinstall in progress: the
;      hooks are removed but the install records stay and a time marker is written
;      (`--keep-records`). When the new install has finished, NSIS_HOOK_POSTINSTALL runs the
;      new program with `--repair-recorded-hooks`, which restores hooks for the agents that
;      have a stored install record (the user's earlier consent), pointing at the new program.
;      If the setup is cancelled, the hooks stay removed. A wrong answer for `$EXEDIR` only
;      costs the restore (the records are deleted as in a normal uninstall), never leaves hooks.
;
; Runs as the user who runs the uninstaller; the default install mode is currentUser. Never run
; by a silent per-machine uninstall of another account's profile (see docs/RELEASING.md).
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ${If} $EXEDIR == $INSTDIR
        ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-hooks --keep-records' $0
      ${Else}
        ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-hooks' $0
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

; After the files are installed: restore the hooks a reinstall removed, for agents with a stored
; install record only. The program does nothing unless the old uninstaller left a fresh marker,
; so a first install, an update and a plain start never change any agent file. No UI, exit 0.
!macro NSIS_HOOK_POSTINSTALL
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --repair-recorded-hooks' $0
  ${EndIf}
!macroend
