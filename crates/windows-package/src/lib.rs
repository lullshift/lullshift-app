//! Windows package context and handle-checked file access (decision 33, ADR-010 amendment).
//!
//! A program started from inside an MSIX or Store app (for example from a terminal inside the
//! Claude desktop app) inherits that app's package identity. Windows then redirects its AppData
//! writes into the package's private copy (`%LOCALAPPDATA%\Packages\<family>\LocalCache\Roaming`),
//! so agents outside that package can't find Lullshift's connection files. The app shows a
//! warning when [`current_package_family_name`] returns a name.
//!
//! The app also reads, and on request deletes, out-of-date copies of its own connection files in
//! other packages' private storage. Checking each path component and then deleting by name would
//! let a junction swapped in between redirect the delete. [`file`] therefore opens the file
//! itself (never following a reparse point at the last component), confirms through the open
//! handle that it is a plain file at exactly the expected final path, and reads or deletes
//! through that same handle.
//!
//! This is one of two crates (with `macos-webrtc`) that allow `unsafe`. It holds exactly five
//! `unsafe` blocks, each a single Win32 call checked against the Win32 docs and the
//! `windows-sys` 0.61.2 bindings: `GetCurrentPackageFamilyName` (twice: length, then name),
//! `GetFinalPathNameByHandleW`, and `SetFileInformationByHandle` (`FileDispositionInfoEx`, then
//! `FileDispositionInfo` as a fallback). A test in `lullshift-desktop` (src/webrtc.rs) counts
//! them. On other operating systems [`current_package_family_name`] always returns `None` and
//! [`file`] does not exist.

#![deny(clippy::undocumented_unsafe_blocks, unsafe_op_in_unsafe_fn)]

/// Longest family name accepted, in UTF-16 units including the terminator. Real ones are well
/// under 100 (`PACKAGE_FAMILY_NAME_MAX_LENGTH` is 64 + 1 + 13); anything larger is refused.
#[cfg(windows)]
const MAX_LEN: u32 = 256;

/// The package family name of this process (for example `Claude_pzs8sxrjxfjjc`), or `None`
/// when the process has no package identity or the name can't be read.
#[cfg(windows)]
pub fn current_package_family_name() -> Option<String> {
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFamilyName;

    // First call with no buffer: returns ERROR_INSUFFICIENT_BUFFER and the length needed when
    // the process has an identity, APPMODEL_ERROR_NO_PACKAGE (15700) when it has none.
    let mut len: u32 = 0;
    // SAFETY: `len` is a valid, writable u32; a null buffer with length 0 is the documented way
    // to ask for the required length.
    let rc = unsafe { GetCurrentPackageFamilyName(&mut len, std::ptr::null_mut()) };
    if rc != ERROR_INSUFFICIENT_BUFFER || len == 0 || len > MAX_LEN {
        return None;
    }
    let mut buf = vec![0u16; len as usize];
    let mut written = len;
    // SAFETY: `buf` holds exactly `written` UTF-16 units and outlives the call; the function
    // writes at most that many (it fails with ERROR_INSUFFICIENT_BUFFER otherwise).
    let rc = unsafe { GetCurrentPackageFamilyName(&mut written, buf.as_mut_ptr()) };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let name = String::from_utf16(&buf[..end]).ok()?;
    (!name.is_empty()).then_some(name)
}

/// Not Windows: there is no package identity.
#[cfg(not(windows))]
pub fn current_package_family_name() -> Option<String> {
    None
}

/// Files opened, checked, read and deleted by handle.
#[cfg(windows)]
pub mod file {
    use std::ffi::OsString;
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::AsRawHandle;
    use std::path::{Path, PathBuf};

    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
        FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_GENERIC_READ, FILE_NAME_NORMALIZED, FileDispositionInfo, FileDispositionInfoEx,
        GetFinalPathNameByHandleW, SetFileInformationByHandle, VOLUME_NAME_DOS,
    };

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    /// Longest final path accepted, in UTF-16 units (the Windows maximum).
    const MAX_PATH_UNITS: u32 = 32_768;

    fn refused(what: &str) -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, what.to_owned())
    }

    /// The normalized DOS path of an open handle, as `\\?\C:\...`.
    pub fn final_path(file: &File) -> io::Result<PathBuf> {
        let mut buf = vec![0u16; 1024];
        loop {
            let cap = u32::try_from(buf.len()).map_err(|_| refused("path too long"))?;
            // SAFETY: the handle is open for the lifetime of `file`; `buf` is writable for `cap`
            // UTF-16 units and outlives the call. On success the function returns the number of
            // units written (excluding the terminator), or the size needed if `cap` is too small.
            let n = unsafe {
                GetFinalPathNameByHandleW(
                    file.as_raw_handle(),
                    buf.as_mut_ptr(),
                    cap,
                    FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
                )
            };
            if n == 0 {
                return Err(io::Error::last_os_error());
            }
            if n < cap {
                return Ok(PathBuf::from(OsString::from_wide(&buf[..n as usize])));
            }
            if n > MAX_PATH_UNITS {
                return Err(refused("path too long"));
            }
            buf = vec![0u16; n as usize + 1];
        }
    }

    fn same_path(a: &Path, b: &Path) -> bool {
        let norm = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
        norm(a) == norm(b)
    }

    /// Opens `path` without following a reparse point at its last component, then checks
    /// through the handle that it is a regular file (not a reparse point) of at most `cap`
    /// bytes whose final path is exactly `expected` (case-insensitive). An intermediate junction
    /// or symlink makes the final path differ and is refused. `delete` also asks for DELETE
    /// access so [`delete`] can be used on the same handle.
    pub fn open_checked(path: &Path, expected: &Path, cap: u64, delete: bool) -> io::Result<File> {
        let access = if delete {
            FILE_GENERIC_READ | DELETE
        } else {
            FILE_GENERIC_READ
        };
        let file = OpenOptions::new()
            .access_mode(access)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        let meta = file.metadata()?;
        if !meta.file_type().is_file()
            || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || meta.len() > cap
        {
            return Err(refused("not a plain file"));
        }
        if !same_path(&final_path(&file)?, expected) {
            return Err(refused("not at the expected path"));
        }
        Ok(file)
    }

    /// Deletes the file behind `file` (opened by [`open_checked`] with `delete`), by handle,
    /// so no later change to the path can redirect it. POSIX semantics first (the name goes at
    /// once), then the classic disposition on file systems without it.
    pub fn delete(file: &File) -> io::Result<()> {
        let ex = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
        };
        // SAFETY: the handle is open with DELETE access for the lifetime of `file`; `ex` is a
        // valid FILE_DISPOSITION_INFO_EX of exactly the size passed, alive for the call.
        let ok = unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfoEx,
                std::ptr::from_ref(&ex).cast(),
                size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
            )
        };
        if ok != 0 {
            return Ok(());
        }
        let classic = FILE_DISPOSITION_INFO { DeleteFile: true };
        // SAFETY: as above, with a valid FILE_DISPOSITION_INFO of exactly the size passed.
        let ok = unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfo,
                std::ptr::from_ref(&classic).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        };
        if ok != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test` may run from a host that is itself packaged (a terminal inside a Store app),
    /// in which case the call returns that family name. Only the shape is checked.
    #[test]
    fn returns_none_or_a_plausible_family_name() {
        if let Some(name) = current_package_family_name() {
            assert!(!name.is_empty() && name.len() < 256);
            assert!(!name.contains('\0'));
        }
    }

    #[cfg(windows)]
    #[test]
    fn opens_reads_and_deletes_only_at_the_expected_path() {
        use std::io::Read;
        let dir = tempfile::tempdir().expect("tempdir");
        let root = std::fs::canonicalize(dir.path()).expect("canonical");
        let path = root.join("token");
        std::fs::write(&path, "abc").expect("write");
        // Wrong expected path: refused.
        assert!(file::open_checked(&path, &root.join("other"), 64, true).is_err());
        // Too large: refused.
        assert!(file::open_checked(&path, &path, 2, false).is_err());
        let mut f = file::open_checked(&path, &path, 64, true).expect("open");
        let mut s = String::new();
        f.read_to_string(&mut s).expect("read");
        assert_eq!(s, "abc");
        file::delete(&f).expect("delete");
        drop(f);
        assert!(!path.exists());
        // A directory isn't a plain file.
        std::fs::create_dir(root.join("d")).expect("dir");
        assert!(file::open_checked(&root.join("d"), &root.join("d"), 64, true).is_err());
    }
}
