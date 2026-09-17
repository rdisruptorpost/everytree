//! Resume path-sorted pages using a small tail of exact UTF-16 identities.
//! Count drift is normal. This remains a best-effort capture of a live index.
use crate::model::{Builder, LoadStats};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

pub struct Record<'a> {
    pub name: &'a [u16],
    pub path: &'a [u16],
    pub bytes: Option<u64>,
}
pub struct Page {
    pub count: usize,
    pub total: usize,
}
pub trait Source {
    fn query(&mut self, offset: usize, limit: usize) -> Result<Page, String>;
    fn record(&self, index: usize) -> Result<Record<'_>, String>;
}
#[derive(Clone)]
struct Anchor {
    name: Vec<u16>,
    path: Vec<u16>,
}
impl Anchor {
    fn matches(&self, record: &Record<'_>) -> bool {
        self.name == record.name && self.path == record.path
    }
}

pub struct Capture {
    pub builder: Builder,
    pub stats: LoadStats,
}
pub const MOVED: &str = "The index moved too far to resume safely";

pub fn collect(
    source: &mut impl Source,
    page_size: usize,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&str, usize, usize),
) -> Result<Capture, String> {
    assert!(page_size >= 4);
    let overlap = (page_size / 4).clamp(1, 128);
    let mut stats = LoadStats::default();
    progress("Counting indexed files", 0, 0);
    let first = timed_query(source, 0, 0, &mut stats)?;
    let mut total = first.total;
    let mut builder = Builder::new(total);
    let mut path_wide = Vec::<u16>::new();
    let mut path_text = String::new();
    let mut name_text = String::new();
    let mut parent = 0;
    let mut added = 0;
    let mut next = 0usize;
    let mut anchors: Vec<Anchor> = Vec::new();
    // Never chase a rapidly growing index forever.
    let request_budget = total.div_ceil(page_size - overlap).saturating_add(64);
    for _ in 0..request_budget {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        if next >= total && anchors.is_empty() {
            break;
        }
        let mut offset = next.saturating_sub(anchors.len());
        progress("Requesting indexed files", added, total.max(added));
        let previous_total = total;
        let mut page = timed_query(source, offset, page_size, &mut stats)?;
        if page.total != total {
            stats.changes += 1;
        }
        total = page.total;
        if page.count == 0 && anchors.is_empty() {
            break;
        }
        let mut resume = if anchors.is_empty() {
            Some(0)
        } else {
            find_resume(source, &anchors, page.count)?
        };
        if resume.is_none() {
            // A large shift before our cursor: move the search window by the
            // observed count delta, then try one wider window around that point.
            stats.changes += 1;
            let shifted = (next as i128 + total as i128 - previous_total as i128).max(0) as usize;
            offset = shifted.saturating_sub(page_size / 2);
            progress("Realigning changed index", added, total.max(added));
            page = timed_query(source, offset, page_size, &mut stats)?;
            total = page.total;
            resume = find_resume(source, &anchors, page.count)?;
        }
        let Some(resume) = resume else {
            return Err(MOVED.into());
        };
        if !anchors.is_empty() && resume != anchors.len() {
            stats.changes += 1;
        }
        let decode_start = Instant::now();
        for i in resume..page.count {
            if i % 65536 == 0 {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Cancelled".into());
                }
                progress("Building the file hierarchy", added, total.max(added));
            }
            let record = source.record(i)?;
            if record.path != path_wide || added == 0 {
                decode(record.path, &mut path_text)?;
                parent = builder.folder(&path_text)?;
                path_wide.clear();
                path_wide.extend_from_slice(record.path);
            }
            decode(record.name, &mut name_text)?;
            builder.add_file(parent, &name_text, record.bytes)?;
            added += 1;
        }
        stats.decode_us += decode_start.elapsed().as_micros() as u64;
        let end = offset + page.count;
        if end >= total {
            return Ok(Capture { builder, stats });
        }
        if page.count <= overlap || (end <= next && resume == page.count) {
            return Err(MOVED.into());
        }
        anchors.clear();
        for i in page.count.saturating_sub(overlap)..page.count {
            let record = source.record(i)?;
            anchors.push(Anchor {
                name: record.name.to_vec(),
                path: record.path.to_vec(),
            });
        }
        next = end;
    }
    if first.total == 0 {
        Ok(Capture { builder, stats })
    } else {
        Err(MOVED.into())
    }
}

fn timed_query(
    source: &mut impl Source,
    offset: usize,
    count: usize,
    stats: &mut LoadStats,
) -> Result<Page, String> {
    let start = Instant::now();
    let result = source.query(offset, count);
    stats.query_us += start.elapsed().as_micros() as u64;
    stats.queries += 1;
    result
}
fn find_resume(
    source: &impl Source,
    anchors: &[Anchor],
    count: usize,
) -> Result<Option<usize>, String> {
    // Normally the last anchor is at index 127. No decoding or allocation.
    let Some(last) = anchors.last() else {
        return Ok(Some(0));
    };
    let mut best: Option<(usize, usize)> = None;
    for i in 0..count {
        let record = source.record(i)?;
        if last.matches(&record) {
            return Ok(Some(i + 1));
        }
        if let Some(index) = anchors.iter().rposition(|a| a.matches(&record))
            && best.is_none_or(|(previous, _)| index > previous)
        {
            best = Some((index, i + 1));
        }
        // On unchanged data the last anchor has already returned above. If it
        // was deleted, the following records let us find the newest survivor.
        if i > anchors.len() + 1024 && best.is_some() {
            break;
        }
    }
    Ok(best.map(|(_, position)| position))
}
fn decode(wide: &[u16], out: &mut String) -> Result<(), String> {
    out.clear();
    for c in char::decode_utf16(wide.iter().copied()) {
        out.push(
            c.map_err(|_| "A filename contains invalid UTF-16; narrow the query to exclude it.")?,
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        names: Vec<Vec<u16>>,
        path: Vec<u16>,
        offset: usize,
        limit: usize,
        requests: usize,
        change: u8,
    }
    impl Source for Fake {
        fn query(&mut self, offset: usize, limit: usize) -> Result<Page, String> {
            self.requests += 1;
            if self.requests == 3 {
                match self.change {
                    1 => {
                        self.names.insert(0, "a_new".encode_utf16().collect());
                    }
                    2 => {
                        self.names.remove(0);
                    }
                    3 => {
                        self.names.remove(7);
                    } // Delete the most recent anchor.
                    4 => {
                        self.names.insert(10, "file_095".encode_utf16().collect());
                    }
                    _ => {}
                }
            }
            self.offset = offset;
            self.limit = limit;
            Ok(Page {
                count: self.names.len().saturating_sub(offset).min(limit),
                total: self.names.len(),
            })
        }
        fn record(&self, index: usize) -> Result<Record<'_>, String> {
            Ok(Record {
                name: &self.names[self.offset + index],
                path: &self.path,
                bytes: Some(10),
            })
        }
    }
    fn capture(change: u8) -> crate::model::Dataset {
        let mut source = Fake {
            names: (0..24)
                .map(|i| format!("file_{i:02}").encode_utf16().collect())
                .collect(),
            path: "C:\\Test".encode_utf16().collect(),
            offset: 0,
            limit: 0,
            requests: 0,
            change,
        };
        let capture = collect(&mut source, 8, &AtomicBool::new(false), |_, _, _| {}).unwrap();
        if change != 0 {
            assert!(capture.stats.changes > 0);
        }
        capture
            .builder
            .finish(&AtomicBool::new(false), |_, _, _| {})
            .unwrap()
    }
    #[test]
    fn unchanged_pages_include_every_file_once() {
        assert_eq!(capture(0).node(0).file_count, 24);
    }
    #[test]
    fn insertion_before_cursor_does_not_duplicate_or_abort() {
        let d = capture(1);
        assert_eq!(d.node(0).file_count, 24);
        assert_eq!(d.node(0).bytes, 240);
    }
    #[test]
    fn deletion_before_cursor_does_not_skip_unread_entries() {
        assert_eq!(capture(2).node(0).file_count, 24);
    }
    #[test]
    fn deleted_last_anchor_resumes_from_survivor() {
        assert_eq!(capture(3).node(0).file_count, 24);
    }
    #[test]
    fn insertion_ahead_of_cursor_is_included() {
        assert_eq!(capture(4).node(0).file_count, 25);
    }
}
