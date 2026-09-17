//! A process boundary keeps synchronous SDK calls cancellable, even if the
//! Everything client stops responding inside a Windows SendMessage call.
use crate::model::{Dataset, Directory, LoadStats, Node};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

enum Message {
    Progress(String, usize, usize),
    Done(Result<Box<Dataset>, String>),
}
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

pub fn load(
    query: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&str, usize, usize),
) -> Result<Dataset, String> {
    let started = Instant::now();
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .arg("--import-worker")
        .arg(query)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not start the SDK worker: {e}"))?;
    let stdout = child.stdout.take().ok_or("Missing SDK worker output")?;
    let (tx, rx) = mpsc::sync_channel(8);
    let reader = std::thread::spawn(move || {
        let mut input = BufReader::new(stdout);
        let result = (|| -> Result<Dataset, String> {
            loop {
                let mut tag = [0u8; 1];
                input
                    .read_exact(&mut tag)
                    .map_err(|_| "The SDK worker exited before completing the import.")?;
                match tag[0] {
                    1 => {
                        let stage = read_string(&mut input).map_err(|e| e.to_string())?;
                        let done = read_u64(&mut input).map_err(|e| e.to_string())? as usize;
                        let total = read_u64(&mut input).map_err(|e| e.to_string())? as usize;
                        let _ = tx.try_send(Message::Progress(stage, done, total));
                    }
                    2 => {
                        return read_dataset(&mut input)
                            .map_err(|e| format!("Invalid SDK worker response: {e}"));
                    }
                    3 => return Err(read_string(&mut input).map_err(|e| e.to_string())?),
                    _ => return Err("Unexpected SDK worker response.".into()),
                }
            }
        })();
        let _ = tx.send(Message::Done(result.map(Box::new)));
    });
    let mut last_progress = Instant::now();
    let result = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err("Cancelled".into());
        }
        match rx.recv_timeout(Duration::from_millis(30)) {
            Ok(Message::Progress(stage, done, total)) => {
                last_progress = Instant::now();
                progress(&stage, done, total);
            }
            Ok(Message::Done(result)) => break result.map(|data| *data),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Err("The SDK worker stopped unexpectedly.".into());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if last_progress.elapsed() >= IDLE_TIMEOUT {
            break Err("Everything did not respond within 45 seconds. Check that Everything is responding, then retry with a narrower query such as C:\\Users\\. Your existing view is still available.".into());
        }
    };
    // This child belongs solely to this import. Never terminate Everything.
    let _ = child.kill();
    let _ = child.wait();
    drop(rx);
    let _ = reader.join();
    result.map(|mut data| {
        data.elapsed = started.elapsed();
        data
    })
}

pub fn worker(query: &str) -> io::Result<()> {
    let mut output = BufWriter::new(io::stdout().lock());
    let result = crate::everything::load(query, &AtomicBool::new(false), |stage, done, total| {
        let _ = output.write_all(&[1]);
        let _ = write_string(&mut output, stage);
        let _ = write_u64(&mut output, done as u64);
        let _ = write_u64(&mut output, total as u64);
        let _ = output.flush();
    });
    match result {
        Ok(mut data) => {
            data.import_peak_memory = peak_working_set().unwrap_or(0) as u64;
            output.write_all(&[1])?;
            write_string(&mut output, "Transferring the completed hierarchy")?;
            write_u64(&mut output, 0)?;
            write_u64(&mut output, 0)?;
            output.flush()?;
            output.write_all(&[2])?;
            write_dataset(&mut output, &data)?;
        }
        Err(error) => {
            output.write_all(&[3])?;
            write_string(&mut output, &error)?;
        }
    }
    output.flush()
}
fn write_u64(w: &mut impl Write, n: u64) -> io::Result<()> {
    w.write_all(&n.to_le_bytes())
}
fn read_u64(r: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0; 8];
    r.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid worker protocol")
}
fn write_vector<T: bytemuck::Pod>(w: &mut impl Write, values: &[T]) -> io::Result<()> {
    write_u64(w, values.len() as u64)?;
    w.write_all(bytemuck::cast_slice(values))
}
fn read_vector<T: bytemuck::Pod + bytemuck::Zeroable>(
    r: &mut impl Read,
    limit: u64,
) -> io::Result<Vec<T>> {
    let count = read_u64(r)?;
    if count > limit {
        return Err(invalid());
    }
    let mut out = Vec::new();
    out.try_reserve_exact(count as usize)
        .map_err(|_| io::Error::other("Not enough memory to receive the hierarchy"))?;
    out.resize(count as usize, T::zeroed());
    r.read_exact(bytemuck::cast_slice_mut(&mut out))?;
    Ok(out)
}
fn write_string(w: &mut impl Write, text: &str) -> io::Result<()> {
    write_vector(w, text.as_bytes())
}
fn read_string(r: &mut impl Read) -> io::Result<String> {
    String::from_utf8(read_vector(r, 1_048_576)?).map_err(|_| invalid())
}
fn write_dataset(w: &mut impl Write, data: &Dataset) -> io::Result<()> {
    w.write_all(b"ETR3")?;
    write_string(w, &data.source)?;
    write_string(w, &data.query)?;
    write_u64(w, data.elapsed.as_micros() as u64)?;
    write_u64(w, data.unknown_sizes)?;
    write_u64(w, data.import_peak_memory)?;
    w.write_all(bytemuck::bytes_of(&data.load_stats))?;
    write_string(w, &data.capture_note)?;
    write_vector(w, &data.nodes)?;
    write_vector(w, &data.directories)?;
    write_vector(w, &data.names)?;
    write_vector(w, &data.children)?;
    write_vector(w, &data.prefix_bytes)
}
fn read_dataset(r: &mut impl Read) -> io::Result<Dataset> {
    let mut magic = [0; 4];
    r.read_exact(&mut magic)?;
    if &magic != b"ETR3" {
        return Err(invalid());
    }
    let source = read_string(r)?;
    let query = read_string(r)?;
    let elapsed = Duration::from_micros(read_u64(r)?);
    let unknown_sizes = read_u64(r)?;
    let import_peak_memory = read_u64(r)?;
    let mut load_stats = LoadStats::default();
    r.read_exact(bytemuck::bytes_of_mut(&mut load_stats))?;
    let capture_note = read_string(r)?;
    let nodes: Vec<Node> = read_vector(r, u32::MAX as u64)?;
    let directories: Vec<Directory> = read_vector(r, nodes.len() as u64)?;
    let names = read_vector(r, u32::MAX as u64)?;
    let children = read_vector(r, nodes.len().saturating_sub(1) as u64)?;
    let prefix_bytes = read_vector(r, children.len() as u64)?;
    if nodes.is_empty() || children.len() != nodes.len() - 1 || prefix_bytes.len() != children.len()
    {
        return Err(invalid());
    }
    if directories.is_empty()
        || !nodes[0].is_dir()
        || nodes
            .iter()
            .any(|n| n.is_dir() && n.directory as usize >= directories.len())
        || directories
            .iter()
            .any(|dir| dir.child_start as usize + dir.child_count as usize > children.len())
    {
        return Err(invalid());
    }
    Ok(Dataset {
        nodes,
        directories,
        names,
        children,
        prefix_bytes,
        source,
        query,
        elapsed,
        unknown_sizes,
        synthetic: false,
        import_peak_memory,
        load_stats,
        capture_note,
    })
}

#[cfg(windows)]
pub fn peak_working_set() -> Option<usize> {
    #[repr(C)]
    struct Counters {
        cb: u32,
        faults: u32,
        peak_working: usize,
        working: usize,
        peak_paged: usize,
        paged: usize,
        peak_nonpaged: usize,
        nonpaged: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut Counters,
            size: u32,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }
    let mut counters: Counters = unsafe { std::mem::zeroed() };
    counters.cb = size_of::<Counters>() as u32;
    if unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            size_of::<Counters>() as u32,
        )
    } != 0
    {
        Some(counters.peak_working)
    } else {
        None
    }
}
#[cfg(not(windows))]
pub fn peak_working_set() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transfer_handles_empty_index_and_rejects_invalid_directory_metadata() {
        let mut data = crate::model::Builder::default()
            .finish(&AtomicBool::new(false), |_, _, _| {})
            .unwrap();
        let mut encoded = Vec::new();
        write_dataset(&mut encoded, &data).unwrap();
        let copy = read_dataset(&mut &encoded[..]).unwrap();
        assert_eq!(copy.file_count(0), 0);
        assert_eq!(copy.folder_count(), 0);
        assert!(copy.child_ids(0).is_empty());
        encoded[..4].copy_from_slice(b"ETR2");
        assert!(read_dataset(&mut &encoded[..]).is_err());

        data.nodes[0].directory = u32::MAX;
        encoded.clear();
        write_dataset(&mut encoded, &data).unwrap();
        assert!(read_dataset(&mut &encoded[..]).is_err());
        data.nodes[0].directory = 0;
        data.directories[0].child_count = 1;
        encoded.clear();
        write_dataset(&mut encoded, &data).unwrap();
        assert!(read_dataset(&mut &encoded[..]).is_err());
    }
    #[test]
    fn transfer_preserves_hierarchy_and_rejects_truncation() {
        let data =
            crate::model::synthetic(500, false, &AtomicBool::new(false), |_, _, _| {}).unwrap();
        let mut encoded = Vec::new();
        write_dataset(&mut encoded, &data).unwrap();
        let copy = read_dataset(&mut &encoded[..]).unwrap();
        assert_eq!(copy.node(0).bytes, data.node(0).bytes);
        assert_eq!(copy.children, data.children);
        for id in 0..copy.nodes.len() as u32 {
            assert_eq!(copy.path(id), data.path(id));
            assert_eq!(copy.file_count(id), data.file_count(id));
            assert_eq!(copy.child_ids(id), data.child_ids(id));
            assert_eq!(copy.child_prefix_bytes(id), data.child_prefix_bytes(id));
        }
        assert!(read_dataset(&mut &encoded[..encoded.len() - 1]).is_err());
    }
}
