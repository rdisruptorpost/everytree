//! Bounded, resumable SDK imports. The caller isolates blocking IPC in a helper.
use crate::{ingest, model::Dataset};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

pub fn load(
    query: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&str, usize, usize),
) -> Result<Dataset, String> {
    #[cfg(not(windows))]
    {
        let _ = (query, cancel, progress);
        Err("Everything integration requires Windows.".into())
    }
    #[cfg(windows)]
    {
        let start = Instant::now();
        let _lock = windows::SDK_LOCK
            .lock()
            .map_err(|_| "Restart the SDK worker.")?;
        progress("Connecting to Everything", 0, 0);
        let sdk = windows::Sdk::open()?;
        let version = sdk.version()?;
        if !sdk.size_indexed()? {
            return Err(
                "Enable Tools > Options > Indexes > Index file size in Everything, then reload."
                    .into(),
            );
        }
        let search = if query.trim().is_empty() {
            "file:".into()
        } else {
            format!("file: <{}>", query.trim())
        };
        let mut source = windows::Search {
            sdk: &sdk,
            text: search,
        };
        let mut result = ingest::collect(&mut source, 500_000, cancel, &mut progress);
        if result.as_ref().is_err_and(|e| e == ingest::MOVED) {
            progress("Resuming after large index changes; restarting once", 0, 0);
            result = ingest::collect(&mut source, 500_000, cancel, &mut progress);
        }
        let capture = result.map_err(|error| if error == ingest::MOVED {
            "The index is changing too rapidly to resume safely. Your existing view is preserved; refresh after the bulk operation finishes.".into()
        } else { error })?;
        drop(sdk);
        let finalize = Instant::now();
        let mut data = capture.builder.finish(cancel, progress)?;
        data.load_stats = capture.stats;
        data.load_stats.finalize_us = finalize.elapsed().as_micros() as u64;
        if data.load_stats.changes > 0 {
            data.capture_note =
                "Files changed while loading. Refresh to include the latest changes.".into();
        }
        data.source = format!("Everything {version}");
        data.query = query.trim().to_owned();
        data.elapsed = start.elapsed();
        Ok(data)
    }
}
#[cfg(windows)]
mod windows {
    use crate::ingest::{Page, Record, Source};
    use libloading::{Library, Symbol};
    use std::path::PathBuf;
    use std::sync::Mutex;
    pub static SDK_LOCK: Mutex<()> = Mutex::new(());
    pub struct Sdk {
        lib: Library,
        get_name: GetText,
        get_path: GetText,
        get_size: unsafe extern "system" fn(u32, *mut i64) -> i32,
    }
    type GetU32 = unsafe extern "system" fn() -> u32;
    type SetU32 = unsafe extern "system" fn(u32);
    type GetText = unsafe extern "system" fn(u32) -> *const u16;
    pub struct Search<'a> {
        pub sdk: &'a Sdk,
        pub text: String,
    }
    impl Source for Search<'_> {
        fn query(&mut self, offset: usize, limit: usize) -> Result<Page, String> {
            self.sdk.query(
                &self.text,
                u32::try_from(offset).map_err(|_| "Too many results")?,
                limit as u32,
            )?;
            Ok(Page {
                count: self.sdk.count()? as usize,
                total: self.sdk.total()? as usize,
            })
        }
        fn record(&self, index: usize) -> Result<Record<'_>, String> {
            self.sdk.result(index as u32)
        }
    }
    impl Sdk {
        pub fn open() -> Result<Self, String> {
            let mut paths = Vec::new();
            if let Ok(exe) = std::env::current_exe()
                && let Some(dir) = exe.parent()
            {
                paths.push(dir.join("Everything64.dll"));
            }
            paths.push(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("vendor/everything-sdk/dll/Everything64.dll"),
            );
            for path in paths {
                if path.is_file() {
                    // Only explicit, app-local locations are searched.
                    let lib = unsafe { Library::new(&path) }
                        .map_err(|e| format!("Cannot load {}: {e}", path.display()))?;
                    // Resolve hot-loop functions once, never millions of GetProcAddress calls.
                    let get_name =
                        *unsafe { lib.get::<GetText>(b"Everything_GetResultFileNameW\0") }
                            .map_err(|e| e.to_string())?;
                    let get_path = *unsafe { lib.get::<GetText>(b"Everything_GetResultPathW\0") }
                        .map_err(|e| e.to_string())?;
                    let get_size = *unsafe {
                        lib.get::<unsafe extern "system" fn(u32, *mut i64) -> i32>(
                            b"Everything_GetResultSize\0",
                        )
                    }
                    .map_err(|e| e.to_string())?;
                    let sdk = Self {
                        lib,
                        get_name,
                        get_path,
                        get_size,
                    };
                    unsafe {
                        sdk.function::<unsafe extern "system" fn()>(b"Everything_Reset\0")?();
                    }
                    return Ok(sdk);
                }
            }
            Err("Everything64.dll is missing. Run scripts/setup.ps1, or place the official x64 Everything SDK DLL beside the executable.".into())
        }
        unsafe fn function<T>(&self, name: &[u8]) -> Result<Symbol<'_, T>, String> {
            unsafe { self.lib.get(name) }.map_err(|e| format!("Incompatible Everything SDK: {e}"))
        }
        fn error(&self) -> String {
            let code = unsafe {
                self.function::<GetU32>(b"Everything_GetLastError\0")
                    .map(|f| f())
                    .unwrap_or(u32::MAX)
            };
            match code {
                2 => "Everything is not reachable. Start the Everything desktop client (the service alone is insufficient), then reload.".into(),
                1 => "Everything ran out of memory. Try a single drive or a narrower search.".into(),
                _ => format!("Everything SDK error {code}. Make sure the desktop client is running and its index has finished loading."),
            }
        }
        pub fn version(&self) -> Result<String, String> {
            let mut v = Vec::new();
            for name in [
                b"Everything_GetMajorVersion\0".as_slice(),
                b"Everything_GetMinorVersion\0",
                b"Everything_GetRevision\0",
                b"Everything_GetBuildNumber\0",
            ] {
                v.push(unsafe { self.function::<GetU32>(name)?() });
            }
            if v[0] == 0 {
                return Err(self.error());
            }
            Ok(format!("{}.{}.{}.{}", v[0], v[1], v[2], v[3]))
        }
        pub fn size_indexed(&self) -> Result<bool, String> {
            let indexed = unsafe {
                self.function::<unsafe extern "system" fn(u32) -> i32>(
                    b"Everything_IsFileInfoIndexed\0",
                )?(1)
            };
            Ok(indexed != 0)
        }
        pub fn query(&self, search: &str, offset: u32, limit: u32) -> Result<(), String> {
            if search.contains('\0') {
                return Err("The search contains a NUL character.".into());
            }
            let wide: Vec<u16> = search.encode_utf16().chain(Some(0)).collect();
            unsafe {
                self.function::<unsafe extern "system" fn(*const u16)>(b"Everything_SetSearchW\0")?(
                    wide.as_ptr(),
                );
                self.function::<SetU32>(b"Everything_SetRequestFlags\0")?(0x1 | 0x2 | 0x10);
                self.function::<SetU32>(b"Everything_SetMax\0")?(limit);
                self.function::<SetU32>(b"Everything_SetOffset\0")?(offset);
                // Path order remains stable when file sizes change.
                self.function::<SetU32>(b"Everything_SetSort\0")?(3);
                let ok = self
                    .function::<unsafe extern "system" fn(i32) -> i32>(b"Everything_QueryW\0")?(
                    1
                );
                if ok == 0 {
                    return Err(self.error());
                }
            }
            Ok(())
        }
        pub fn count(&self) -> Result<u32, String> {
            unsafe { Ok(self.function::<GetU32>(b"Everything_GetNumResults\0")?()) }
        }
        pub fn total(&self) -> Result<u32, String> {
            unsafe { Ok(self.function::<GetU32>(b"Everything_GetTotResults\0")?()) }
        }
        pub fn result(&self, index: u32) -> Result<Record<'_>, String> {
            unsafe {
                let name = read_wide((self.get_name)(index))?;
                let path = read_wide((self.get_path)(index))?;
                let mut size = -1i64;
                let ok = (self.get_size)(index, &mut size);
                Ok(Record {
                    name,
                    path,
                    bytes: if ok != 0 && size >= 0 {
                        Some(size as u64)
                    } else {
                        None
                    },
                })
            }
        }
    }
    impl Drop for Sdk {
        fn drop(&mut self) {
            unsafe {
                if let Ok(f) = self.function::<unsafe extern "system" fn()>(b"Everything_CleanUp\0")
                {
                    f();
                }
            }
        }
    }
    unsafe fn read_wide<'a>(ptr: *const u16) -> Result<&'a [u16], String> {
        if ptr.is_null() {
            return Err("Everything returned a missing filename or path.".into());
        }
        let mut len = 0;
        while unsafe { *ptr.add(len) } != 0 {
            len += 1;
        }
        // The enclosing result() ties this borrow to the SDK, whose result
        // buffer is immutable until the next mutable Source::query call.
        Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
    }
}
