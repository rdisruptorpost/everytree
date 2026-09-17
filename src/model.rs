use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub type Id = u32;
pub const ROOT: Id = 0;
const DIRECTORY: u8 = 1;
const UNKNOWN_SIZE: u8 = 1;

/// Exactly 24 bytes. Folder-only metadata lives in a separate, compact buffer.
/// Names and children remain contiguous; file records carry no child counters.
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Node {
    pub bytes: u64,
    pub name_offset: u32,
    pub parent: Id,
    pub(crate) directory: u32,
    pub name_len: u16,
    kind: u8,
    flags: u8,
}

impl Node {
    pub fn is_dir(&self) -> bool {
        self.kind == DIRECTORY
    }
    pub fn size_unknown(&self) -> bool {
        self.flags & UNKNOWN_SIZE != 0
    }
}

/// Only directories need child ranges and recursive file counts (12 bytes).
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub(crate) struct Directory {
    pub child_start: u32,
    pub child_count: u32,
    pub file_count: u32,
}

pub struct Dataset {
    pub nodes: Vec<Node>,
    pub(crate) directories: Vec<Directory>,
    pub(crate) names: Vec<u8>,
    pub children: Vec<Id>,
    /// Inclusive prefix sums, restarted for each parent's sorted child range.
    pub prefix_bytes: Vec<u64>,
    pub source: String,
    pub query: String,
    pub elapsed: Duration,
    pub unknown_sizes: u64,
    pub synthetic: bool,
    pub import_peak_memory: u64,
    pub load_stats: LoadStats,
    pub capture_note: String,
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct LoadStats {
    pub query_us: u64,
    pub decode_us: u64,
    pub finalize_us: u64,
    pub queries: u64,
    pub changes: u64,
}

impl Dataset {
    pub fn node(&self, id: Id) -> &Node {
        &self.nodes[id as usize]
    }
    pub fn name(&self, id: Id) -> &str {
        let n = self.node(id);
        let start = n.name_offset as usize;
        // Builder only appends valid UTF-8 strings.
        std::str::from_utf8(&self.names[start..start + n.name_len as usize]).unwrap()
    }
    pub fn child_ids(&self, id: Id) -> &[Id] {
        let Some(dir) = self.directory(id) else {
            return &[];
        };
        let start = dir.child_start as usize;
        &self.children[start..start + dir.child_count as usize]
    }
    fn directory(&self, id: Id) -> Option<&Directory> {
        let node = self.node(id);
        node.is_dir()
            .then(|| &self.directories[node.directory as usize])
    }
    pub fn file_count(&self, id: Id) -> u32 {
        self.directory(id).map_or(1, |dir| dir.file_count)
    }
    pub fn child_prefix_bytes(&self, id: Id) -> &[u64] {
        let Some(dir) = self.directory(id) else {
            return &[];
        };
        let start = dir.child_start as usize;
        &self.prefix_bytes[start..start + dir.child_count as usize]
    }
    pub fn range_bytes(&self, parent: Id, start: usize, end: usize) -> u64 {
        if start == end {
            return 0;
        }
        let prefix = self.child_prefix_bytes(parent);
        let before = if start == 0 { 0 } else { prefix[start - 1] };
        prefix[end - 1] - before
    }
    pub fn path(&self, id: Id) -> String {
        if id == ROOT {
            return "All indexed files".into();
        }
        let mut parts = Vec::new();
        let mut current = id;
        while current != ROOT {
            parts.push(self.name(current));
            current = self.node(current).parent;
        }
        parts.reverse();
        let mut path = parts.join("\\");
        if path.len() == 2 && path.ends_with(':') {
            path.push('\\');
        }
        path
    }
    pub fn ancestors(&self, id: Id) -> Vec<Id> {
        let mut out = vec![id];
        let mut current = id;
        while current != ROOT {
            current = self.node(current).parent;
            out.push(current);
        }
        out.reverse();
        out
    }
    pub fn storage_bytes(&self) -> usize {
        self.nodes.capacity() * size_of::<Node>()
            + self.directories.capacity() * size_of::<Directory>()
            + self.names.capacity()
            + self.children.capacity() * size_of::<Id>()
            + self.prefix_bytes.capacity() * size_of::<u64>()
    }
    pub fn folder_count(&self) -> usize {
        self.directories.len() - 1
    }
}

pub struct Builder {
    nodes: Vec<Node>,
    directories: Vec<Directory>,
    names: Vec<u8>,
    folders: HashMap<String, Id>,
    unknown_sizes: u64,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Builder {
    pub fn new(expected_files: usize) -> Self {
        let mut out = Self {
            nodes: Vec::with_capacity(
                expected_files
                    .saturating_add(expected_files / 16)
                    .saturating_add(1),
            ),
            directories: Vec::new(),
            names: Vec::with_capacity(expected_files.saturating_mul(20)),
            folders: HashMap::new(),
            unknown_sizes: 0,
        };
        out.push(ROOT, "All indexed files", 0, true, false).unwrap();
        out
    }
    fn push(
        &mut self,
        parent: Id,
        name: &str,
        bytes: u64,
        dir: bool,
        unknown: bool,
    ) -> Result<Id, String> {
        let id = u32::try_from(self.nodes.len())
            .map_err(|_| "The 32-bit node ID capacity has been reached.")?;
        let offset =
            u32::try_from(self.names.len()).map_err(|_| "The filename pool exceeded 4 GiB.")?;
        let len = u16::try_from(name.len()).map_err(|_| "A filename is too long to represent.")?;
        if self.names.len().saturating_add(name.len()) > u32::MAX as usize {
            return Err("The filename pool exceeded 4 GiB.".into());
        }
        let directory = if dir {
            let index = u32::try_from(self.directories.len())
                .map_err(|_| "The directory ID capacity has been reached.")?;
            self.directories.push(Directory::default());
            index
        } else {
            0 // Unused for files.
        };
        self.names.extend_from_slice(name.as_bytes());
        self.nodes.push(Node {
            bytes,
            name_offset: offset,
            name_len: len,
            parent,
            kind: if dir { DIRECTORY } else { 0 },
            flags: if unknown { UNKNOWN_SIZE } else { 0 },
            directory,
        });
        if id != ROOT {
            self.directories[self.nodes[parent as usize].directory as usize].child_count += 1;
        }
        self.unknown_sizes += u64::from(unknown);
        Ok(id)
    }
    /// Folder-only lookup table. Full file paths are never retained.
    /// Exact case is preserved, including on case-sensitive Windows directories.
    pub fn folder(&mut self, path: &str) -> Result<Id, String> {
        let path = path.trim_end_matches('\\');
        if path.is_empty() {
            return Ok(ROOT);
        }
        if let Some(&id) = self.folders.get(path) {
            return Ok(id);
        }
        let mut missing = Vec::new();
        let mut rest = path;
        let parent = loop {
            if let Some(&id) = self.folders.get(rest) {
                break id;
            }
            missing.push(rest);
            // Treat \\server\share as one root, never traverse a network path.
            let root_end = if let Some(unc) = rest.strip_prefix("\\\\") {
                unc.find('\\')
                    .map(|i| i + 3)
                    .and_then(|i| rest[i..].find('\\').map(|j| i + j))
                    .unwrap_or(rest.len())
            } else {
                0
            };
            match rest.rfind('\\') {
                Some(pos) if pos >= root_end && pos > 0 => rest = &rest[..pos],
                _ => break ROOT,
            }
        };
        let mut parent = parent;
        for full in missing.into_iter().rev() {
            let name = if parent == ROOT {
                full
            } else {
                full.rsplit('\\').next().unwrap()
            };
            let id = self.push(parent, name, 0, true, false)?;
            self.folders.insert(full.to_owned(), id);
            parent = id;
        }
        Ok(parent)
    }
    pub fn add_file(&mut self, parent: Id, name: &str, bytes: Option<u64>) -> Result<Id, String> {
        self.push(parent, name, bytes.unwrap_or(0), false, bytes.is_none())
    }
    pub fn finish(
        mut self,
        cancel: &AtomicBool,
        mut progress: impl FnMut(&str, usize, usize),
    ) -> Result<Dataset, String> {
        drop(self.folders);
        let count = self.nodes.len();
        progress("Adding folder totals", 0, count);
        // Every parent precedes its children, so one reverse pass is sufficient.
        for i in (1..count).rev() {
            if i % 65536 == 0 && cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let n = self.nodes[i];
            let p = &mut self.nodes[n.parent as usize];
            p.bytes = p
                .bytes
                .checked_add(n.bytes)
                .ok_or("The total byte count overflowed u64.")?;
            p.flags |= n.flags;
            let parent_directory = p.directory as usize;
            let files = if n.is_dir() {
                self.directories[n.directory as usize].file_count
            } else {
                1
            };
            let p = &mut self.directories[parent_directory];
            p.file_count = p
                .file_count
                .checked_add(files)
                .ok_or("The file count overflowed u32.")?;
        }
        progress("Building child index", 0, count);
        let mut offset = 0u32;
        for n in &mut self.directories {
            n.child_start = offset;
            offset += n.child_count;
            n.child_count = 0; // Reuse as insertion cursor.
        }
        let mut children = vec![0; count - 1];
        for i in 1..count {
            if i % 65536 == 0 && cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let parent = self.nodes[i].parent as usize;
            let p = &mut self.directories[self.nodes[parent].directory as usize];
            children[(p.child_start + p.child_count) as usize] = i as Id;
            p.child_count += 1;
        }
        progress("Sorting folders by size", 0, self.directories.len());
        let mut prefix_bytes = vec![0; children.len()];
        for (i, n) in self.directories.iter().enumerate() {
            if i % 65536 == 0 {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Cancelled".into());
                }
                progress("Sorting folders by size", i, self.directories.len());
            }
            if n.child_count == 0 {
                continue;
            }
            let start = n.child_start as usize;
            let end = start + n.child_count as usize;
            children[start..end].sort_unstable_by(|a, b| {
                self.nodes[*b as usize]
                    .bytes
                    .cmp(&self.nodes[*a as usize].bytes)
                    .then(a.cmp(b))
            });
            let mut sum = 0;
            for j in start..end {
                sum += self.nodes[children[j] as usize].bytes;
                prefix_bytes[j] = sum;
            }
        }
        self.nodes.shrink_to_fit();
        self.directories.shrink_to_fit();
        self.names.shrink_to_fit();
        Ok(Dataset {
            nodes: self.nodes,
            directories: self.directories,
            names: self.names,
            children,
            prefix_bytes,
            source: String::new(),
            query: String::new(),
            elapsed: Duration::ZERO,
            unknown_sizes: self.unknown_sizes,
            synthetic: false,
            import_peak_memory: 0,
            load_stats: LoadStats::default(),
            capture_note: String::new(),
        })
    }
}

pub fn synthetic(
    files: usize,
    flat: bool,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&str, usize, usize),
) -> Result<Dataset, String> {
    let start = Instant::now();
    let mut b = Builder::new(files);
    let categories = [
        "Projects",
        "Video",
        "Games",
        "Photos",
        "Archives",
        "Applications",
        "Documents",
        "Cache",
    ];
    let extensions = ["rs", "mp4", "pak", "jpg", "zip", "dll", "pdf", "bin"];
    let mut seed = 0x123456789abcdefu64;
    let flat_parent = if flat {
        b.folder("DEMO:\\One million siblings")?
    } else {
        ROOT
    };
    let mut folders = Vec::new();
    if !flat {
        for i in 0..(files / 256).clamp(8, 200_000) {
            folders.push(b.folder(&format!(
                "DEMO:\\{}\\Collection {:04}\\Batch {:03}",
                categories[i % 8],
                i / 128,
                i % 128
            ))?);
        }
    }
    for i in 0..files {
        if i % 65536 == 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            progress("Generating sample files", i, files);
        }
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let category = if flat { i % 8 } else { (i % folders.len()) % 8 };
        let size = if i % 997 == 0 {
            (seed % 8_000 + 500) * 1_048_576
        } else {
            (seed % 500_000 + 256) * [1, 64, 16, 8, 16, 2, 1, 1][category]
        };
        let parent = if flat {
            flat_parent
        } else {
            folders[i % folders.len()]
        };
        b.add_file(
            parent,
            &format!("file_{i:08}.{}", extensions[category]),
            Some(size),
        )?;
    }
    let mut data = b.finish(cancel, progress)?;
    data.source = if flat {
        "Synthetic / flat hierarchy"
    } else {
        "Synthetic / mixed hierarchy"
    }
    .into();
    data.synthetic = true;
    data.elapsed = start.elapsed();
    Ok(data)
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn format_count(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_nodes_and_exact_totals() {
        assert_eq!(size_of::<Node>(), 24);
        assert_eq!(size_of::<Directory>(), 12);
        let mut b = Builder::default();
        let a = b.folder("C:\\Users\\Example").unwrap();
        let c = b.folder("C:\\Users\\Example\\child").unwrap();
        b.add_file(a, "zero", Some(0)).unwrap();
        b.add_file(a, "unknown", None).unwrap();
        b.add_file(c, "large.bin", Some(9_007_199_254_740_993))
            .unwrap();
        let d = b.finish(&AtomicBool::new(false), |_, _, _| {}).unwrap();
        assert_eq!(d.node(ROOT).bytes, 9_007_199_254_740_993);
        assert_eq!(d.file_count(ROOT), 3);
        assert!(d.node(a).size_unknown());
        assert_eq!(d.path(c), "C:\\Users\\Example\\child");
        assert_eq!(d.range_bytes(a, 0, d.child_ids(a).len()), d.node(a).bytes);
    }
    #[test]
    fn folder_metadata_stays_correct_with_interleaved_files_and_empty_folders() {
        let mut b = Builder::default();
        let root_file = b.add_file(ROOT, "root.bin", Some(7)).unwrap();
        let a = b.folder("C:\\a").unwrap();
        let first = b.add_file(a, "first", Some(11)).unwrap();
        let empty = b.folder("C:\\empty").unwrap();
        let nested = b.folder("C:\\a\\nested").unwrap();
        let unknown = b.add_file(nested, "unknown", None).unwrap();
        let last = b.add_file(a, "last", Some(13)).unwrap();
        let d = b.finish(&AtomicBool::new(false), |_, _, _| {}).unwrap();
        assert_eq!(d.folder_count(), 4);
        assert_eq!(d.file_count(ROOT), 4);
        assert_eq!(d.file_count(a), 3);
        assert_eq!(d.file_count(nested), 1);
        assert_eq!(d.file_count(empty), 0);
        assert_eq!(d.child_ids(a), [last, first, nested]);
        assert_eq!(d.child_prefix_bytes(a), [13, 24, 24]);
        assert_eq!(d.node(ROOT).bytes, 31);
        assert_eq!(d.node(a).bytes, 24);
        assert!(d.node(a).size_unknown());
        assert!(!d.node(empty).size_unknown());
        for id in [root_file, first, unknown, last] {
            assert_eq!(d.file_count(id), 1);
            assert!(d.child_ids(id).is_empty());
            assert!(d.child_prefix_bytes(id).is_empty());
            assert_eq!(d.range_bytes(id, 0, 0), 0);
        }
        assert!(d.child_ids(empty).is_empty());
        assert!(d.child_prefix_bytes(empty).is_empty());
    }
    #[test]
    fn paths_handle_unc_unicode_and_distinct_case() {
        let mut b = Builder::default();
        let unc = b.folder("\\\\server\\share\\日本語").unwrap();
        let a = b.folder("C:\\Case").unwrap();
        let other = b.folder("C:\\case").unwrap();
        assert_ne!(a, other);
        let id = b.add_file(unc, "写真.jpg", Some(42)).unwrap();
        let d = b.finish(&AtomicBool::new(false), |_, _, _| {}).unwrap();
        assert_eq!(d.path(id), "\\\\server\\share\\日本語\\写真.jpg");
    }
    #[test]
    fn children_sorted_and_prefix_ranges_match() {
        let mut b = Builder::default();
        let p = b.folder("C:").unwrap();
        for size in [5, 0, 21, 1, 8] {
            b.add_file(p, "x", Some(size)).unwrap();
        }
        let d = b.finish(&AtomicBool::new(false), |_, _, _| {}).unwrap();
        let sizes: Vec<_> = d.child_ids(p).iter().map(|id| d.node(*id).bytes).collect();
        assert_eq!(sizes, [21, 8, 5, 1, 0]);
        assert_eq!(d.range_bytes(p, 1, 4), 14);
    }
}
