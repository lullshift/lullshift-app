//! Which executable the agent hooks should call (S12-T05, ADR-010).
//!
//! Normally the running executable. An AppImage is different: it mounts itself at a temporary
//! path (`/tmp/.mount_XXXX/...`) that disappears when the app quits, and a hook pointing there
//! would break on the next start. The AppImage runtime sets `APPIMAGE` to the real file, so
//! hooks point at that instead (the file starts the app, or runs hook mode, from `main`).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The executable path to write into hook commands.
pub fn hook_exe() -> std::io::Result<PathBuf> {
    let current = std::env::current_exe()?;
    Ok(choose(
        current,
        std::env::var_os("APPIMAGE"),
        std::env::var_os("APPDIR"),
        |p| p.is_file(),
    ))
}

/// The decision, with the environment and the file check injected so tests need neither.
///
/// `APPIMAGE` is used only on Linux and only when it is an absolute path to an existing file
/// whose name starts with `lullshift` (the hook files recognise their own entries by that
/// prefix, so a renamed AppImage would leave entries Lullshift could not find again). If
/// `APPDIR` is set, the running executable must also be inside it, so an `APPIMAGE` variable
/// inherited from some other AppImage that started this process is ignored.
pub(crate) fn choose(
    current: PathBuf,
    appimage: Option<OsString>,
    appdir: Option<OsString>,
    is_file: impl Fn(&Path) -> bool,
) -> PathBuf {
    choose_on(
        cfg!(target_os = "linux"),
        current,
        appimage,
        appdir,
        is_file,
    )
}

fn choose_on(
    linux: bool,
    current: PathBuf,
    appimage: Option<OsString>,
    appdir: Option<OsString>,
    is_file: impl Fn(&Path) -> bool,
) -> PathBuf {
    if !linux {
        return current;
    }
    let Some(image) = appimage.filter(|v| !v.is_empty()).map(PathBuf::from) else {
        return current;
    };
    let named_ok = image
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_lowercase().starts_with("lullshift"));
    let inside_appdir = match appdir.filter(|v| !v.is_empty()) {
        Some(dir) => current.starts_with(PathBuf::from(dir)),
        None => true,
    };
    if image.is_absolute() && named_ok && inside_appdir && is_file(&image) {
        image
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Absolute paths on every host (the Linux decision is tested on all of them).
    fn abs(rest: &str) -> PathBuf {
        std::env::temp_dir().join(rest)
    }
    fn mount() -> PathBuf {
        abs(".mount_LullshXYZ")
    }
    fn current() -> PathBuf {
        mount().join("usr").join("lullshift-desktop")
    }
    fn image() -> PathBuf {
        abs("Apps").join("Lullshift_0.1.0_amd64.AppImage")
    }
    fn os(p: &Path) -> Option<OsString> {
        Some(p.as_os_str().to_owned())
    }
    fn pick(
        linux: bool,
        appimage: Option<OsString>,
        appdir: Option<OsString>,
        exists: bool,
    ) -> PathBuf {
        choose_on(linux, current(), appimage, appdir, |_| exists)
    }

    #[test]
    fn points_at_the_appimage_file_not_the_temporary_mount() {
        assert_eq!(pick(true, os(&image()), os(&mount()), true), image());
        assert_eq!(
            pick(true, os(&image()), None, true),
            image(),
            "APPDIR is optional"
        );
    }

    #[test]
    fn falls_back_to_the_running_executable() {
        let cur = current();
        assert_eq!(pick(true, None, None, true), cur, "not an AppImage");
        assert_eq!(
            pick(true, Some(OsString::new()), None, true),
            cur,
            "empty variable"
        );
        assert_eq!(
            pick(true, Some("relative/Lullshift.AppImage".into()), None, true),
            cur
        );
        assert_eq!(pick(true, os(&image()), None, false), cur, "file is gone");
        assert_eq!(
            pick(true, os(&abs("Apps").join("renamed.AppImage")), None, true),
            cur,
            "the name would not be recognised as ours"
        );
        assert_eq!(
            pick(true, os(&image()), os(&abs(".mount_OtherApp")), true),
            cur,
            "APPIMAGE inherited from another AppImage"
        );
        assert_eq!(
            pick(false, os(&image()), None, true),
            cur,
            "only Linux has AppImages"
        );
    }

    #[test]
    fn the_real_environment_gives_the_running_executable_outside_an_appimage() {
        // Test runs are not AppImages; whatever the host, the choice is a real absolute path.
        if std::env::var_os("APPIMAGE").is_none() {
            assert_eq!(
                hook_exe().expect("exe"),
                std::env::current_exe().expect("exe")
            );
        }
    }
}
