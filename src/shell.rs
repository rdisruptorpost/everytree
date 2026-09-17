//! Use Windows shell item IDs so spaces, commas, and Unicode are not parsed as
//! Explorer command-line switches. Calls run on a dedicated COM-initialized thread.
pub fn reveal(path: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::reveal(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Explorer requires Windows.".into())
    }
}
pub fn open_folder(path: &str) -> Result<(), String> {
    if !std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false) {
        return Err(
            "This folder is no longer available. Refresh the index or reconnect the drive.".into(),
        );
    }
    #[cfg(windows)]
    {
        windows::open_folder(path)
    }
    #[cfg(not(windows))]
    {
        Err("Explorer requires Windows.".into())
    }
}
#[cfg(windows)]
mod windows {
    use std::ffi::c_void;
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32;
        fn CoUninitialize();
        fn CoTaskMemFree(memory: *mut c_void);
    }
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHParseDisplayName(
            name: *const u16,
            bind: *mut c_void,
            pidl: *mut *mut c_void,
            flags: u32,
            attributes: *mut u32,
        ) -> i32;
        fn SHOpenFolderAndSelectItems(
            pidl: *const c_void,
            count: u32,
            children: *const *const c_void,
            flags: u32,
        ) -> i32;
        fn ShellExecuteW(
            window: *mut c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    struct Com;
    impl Com {
        fn init() -> Result<Self, String> {
            let hr = unsafe { CoInitializeEx(std::ptr::null_mut(), 2 | 4) };
            if hr < 0 {
                Err(format!(
                    "Could not initialize Explorer integration (0x{:08X}).",
                    hr as u32
                ))
            } else {
                Ok(Self)
            }
        }
    }
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    fn wide(path: &str) -> Result<Vec<u16>, String> {
        if path.contains('\0') {
            return Err("Invalid path.".into());
        }
        Ok(path.encode_utf16().chain(Some(0)).collect())
    }
    pub fn reveal(path: &str) -> Result<(), String> {
        let _com = Com::init()?;
        let path = wide(path)?;
        let mut item = std::ptr::null_mut();
        let parsed = unsafe {
            SHParseDisplayName(
                path.as_ptr(),
                std::ptr::null_mut(),
                &mut item,
                0,
                std::ptr::null_mut(),
            )
        };
        if parsed < 0 || item.is_null() {
            return Err("This item is no longer available to Explorer. Refresh the index or reconnect the drive.".into());
        }
        let opened = unsafe { SHOpenFolderAndSelectItems(item, 0, std::ptr::null(), 0) };
        unsafe {
            CoTaskMemFree(item);
        }
        if opened < 0 {
            Err(format!(
                "Explorer could not select the item (0x{:08X}).",
                opened as u32
            ))
        } else {
            Ok(())
        }
    }
    pub fn open_folder(path: &str) -> Result<(), String> {
        let _com = Com::init()?;
        let path = wide(path)?;
        let operation = wide("open")?;
        let status = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                path.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        if status <= 32 {
            Err(format!(
                "Explorer could not open the folder (error {status})."
            ))
        } else {
            Ok(())
        }
    }
}
