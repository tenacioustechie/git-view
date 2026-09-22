//! Replace a reflog only if its destination still exists. Git's `reflog drop`
//! deliberately bypasses ref locks; an ordinary rename would resurrect that log.
use std::path::Path;

pub(crate) fn supported() -> bool {
    cfg!(any(target_os = "macos", windows))
}

/// On success `old` contains the replaced file, for validation and recovery.
#[cfg(target_os = "macos")]
pub(crate) fn replace_existing(new: &Path, destination: &Path, old: &Path) -> std::io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    unsafe extern "C" {
        fn renamex_np(
            from: *const std::ffi::c_char,
            to: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
    }
    let from = CString::new(new.as_os_str().as_bytes())?;
    let to = CString::new(destination.as_os_str().as_bytes())?;
    // SAFETY: both C strings remain valid for the call; RENAME_SWAP exchanges
    // two existing directory entries and does not recreate a missing target.
    if unsafe { renamex_np(from.as_ptr(), to.as_ptr(), 2) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    std::fs::rename(new, old)
}

#[cfg(windows)]
pub(crate) fn replace_existing(new: &Path, destination: &Path, old: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(
            replaced: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> i32;
    }
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let dest = wide(destination);
    let new = wide(new);
    let old = wide(old);
    // SAFETY: null-terminated buffers live across this synchronous Win32 call.
    // Unlike MoveFileEx, ReplaceFileW opens and requires the existing destination.
    if unsafe {
        ReplaceFileW(
            dest.as_ptr(),
            new.as_ptr(),
            old.as_ptr(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) fn replace_existing(_: &Path, _: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Safe conditional reflog replacement is not supported on this platform.",
    ))
}
