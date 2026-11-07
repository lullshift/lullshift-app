//! Whether this process runs with a Windows package identity (decision 33, ADR-010 amendment).
//!
//! A program started from inside an MSIX or Store app (for example from a terminal inside the
//! Claude desktop app) inherits that app's package identity. Windows then redirects its AppData
//! writes into the package's private copy (`%LOCALAPPDATA%\Packages\<family>\LocalCache\Roaming`),
//! so agents outside that package can't find Lullshift's connection files. The app shows a
//! warning when this returns a family name.
//!
//! This is the only crate besides `macos-webrtc` that allows `unsafe`. It holds one call:
//! `GetCurrentPackageFamilyName` (kernel32, Windows 8+), checked against the Win32 docs
//! ("GetCurrentPackageFamilyName function (appmodel.h)") and the `windows-sys` 0.61.2 binding.
//! On other operating systems [`current_package_family_name`] always returns `None`.

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
}
