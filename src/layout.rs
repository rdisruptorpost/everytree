//! Weighted binary treemap. Prefix sums allow bounded traversal even when a
//! single folder has millions of children. No full-tree walk on redraw/hover.
use crate::model::{Dataset, Id};
use eframe::egui::{Rect, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub parent: Id,
    pub start: usize,
    pub end: usize,
}
impl View {
    pub fn folder(data: &Dataset, parent: Id) -> Self {
        Self {
            parent,
            start: 0,
            end: data.child_ids(parent).len(),
        }
    }
    pub fn bytes(&self, data: &Dataset) -> u64 {
        data.range_bytes(self.parent, self.start, self.end)
    }
    pub fn is_group(&self, data: &Dataset) -> bool {
        self.start != 0 || self.end != data.child_ids(self.parent).len()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TileKind {
    Entry(Id),
    Group(View),
}
#[derive(Clone, Copy, Debug)]
pub struct Tile {
    pub rect: Rect,
    pub kind: TileKind,
    pub depth: u8,
    pub bytes: u64,
    /// Space reserved for a readable directory title; compact directories use 0.
    pub header_height: f32,
}
#[derive(Default)]
pub struct Layout {
    pub tiles: Vec<Tile>,
    pub visited: usize,
}
#[derive(Clone, Copy)]
pub struct Settings {
    pub budget: usize,
    pub min_area: f32,
    pub depth: u8,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Detail {
    Balanced,
    #[default]
    Fine,
    Pixel,
}
impl Detail {
    pub fn label(self) -> &'static str {
        match self {
            Self::Balanced => "Balanced",
            Self::Fine => "Fine",
            Self::Pixel => "Pixel",
        }
    }
    pub fn settings(self, depth: u8) -> Settings {
        let (budget, min_area) = match self {
            Self::Balanced => (16_000, 16.0),
            Self::Fine => (75_000, 4.0),
            Self::Pixel => (200_000, 1.0),
        };
        Settings {
            budget,
            min_area,
            depth,
        }
    }
}
impl Default for Settings {
    fn default() -> Self {
        Detail::Fine.settings(32)
    }
}

pub fn build(data: &Dataset, view: View, rect: Rect, settings: Settings) -> Layout {
    let mut layout = Layout::default();
    if rect.width() <= 0.0 || rect.height() <= 0.0 || view.bytes(data) == 0 || settings.budget == 0
    {
        return layout;
    }
    split(data, view, rect, 0, settings.budget, settings, &mut layout);
    layout
}

fn split(
    data: &Dataset,
    view: View,
    rect: Rect,
    depth: u8,
    budget: usize,
    settings: Settings,
    out: &mut Layout,
) {
    out.visited += 1;
    let bytes = view.bytes(data);
    if bytes == 0 || view.start >= view.end {
        return;
    }
    let children = data.child_ids(view.parent);
    // Exclude zero/unknown-size tail from area subdivision; it remains in the list.
    let positive_end =
        view.start + children[view.start..view.end].partition_point(|id| data.node(*id).bytes > 0);
    let view = View {
        end: positive_end,
        ..view
    };
    if view.end - view.start == 1 {
        let id = children[view.start];
        let node = data.node(id);
        let header_height = if node.is_dir() && rect.width() >= 100.0 && rect.height() >= 64.0 {
            19.0
        } else {
            0.0
        };
        out.tiles.push(Tile {
            rect,
            kind: TileKind::Entry(id),
            bytes,
            depth,
            header_height,
        });
        // Titles and padding are optional. A tiny directory can still contain
        // visible files; don't stop descending just because its name won't fit.
        let inset = if header_height > 0.0 { 1.0 } else { 0.0 };
        let inner = Rect::from_min_max(
            rect.min + vec2(inset, header_height),
            rect.max - vec2(inset, inset),
        );
        if node.is_dir()
            && !data.child_ids(id).is_empty()
            && depth < settings.depth
            && budget > 1
            && inner.width() >= 1.0
            && inner.height() >= 1.0
            && inner.area() >= settings.min_area
        {
            split(
                data,
                View::folder(data, id),
                inner,
                depth + 1,
                budget - 1,
                settings,
                out,
            );
        }
        return;
    }
    if budget <= 1
        || rect.area() < settings.min_area * 2.0
        || rect.width().min(rect.height()) < 0.75
    {
        out.tiles.push(Tile {
            rect,
            kind: TileKind::Group(view),
            bytes,
            depth,
            header_height: 0.0,
        });
        return;
    }
    let prefix = data.child_prefix_bytes(view.parent);
    let before = if view.start == 0 {
        0
    } else {
        prefix[view.start - 1]
    };
    let target = before + bytes / 2;
    let range = &prefix[view.start..view.end];
    let mut middle = view.start + range.partition_point(|sum| *sum < target) + 1;
    middle = middle.clamp(view.start + 1, view.end - 1);
    if middle > view.start + 1 {
        let current = data.range_bytes(view.parent, view.start, middle);
        let previous = data.range_bytes(view.parent, view.start, middle - 1);
        if previous.abs_diff(bytes / 2) < current.abs_diff(bytes / 2) {
            middle -= 1;
        }
    }
    let left = View {
        end: middle,
        ..view
    };
    let right = View {
        start: middle,
        ..view
    };
    let ratio = (left.bytes(data) as f64 / bytes as f64) as f32;
    let (a, b) = if rect.width() >= rect.height() {
        let x = rect.left() + rect.width() * ratio;
        (
            Rect::from_min_max(rect.min, pos2(x, rect.bottom())),
            Rect::from_min_max(pos2(x, rect.top()), rect.max),
        )
    } else {
        let y = rect.top() + rect.height() * ratio;
        (
            Rect::from_min_max(rect.min, pos2(rect.right(), y)),
            Rect::from_min_max(pos2(rect.left(), y), rect.max),
        )
    };
    let mut left_budget = ((budget as f32 * ratio) as usize).clamp(1, budget - 1);
    // A large file needs only one rectangle. Don't reserve most of the budget
    // for it while forcing thousands of smaller files into one group.
    let is_file =
        |range: View| range.end - range.start == 1 && !data.node(children[range.start]).is_dir();
    if is_file(left) {
        left_budget = 1;
    } else if is_file(right) {
        left_budget = budget - 1;
    }
    let before = out.tiles.len();
    split(data, left, a, depth, left_budget, settings, out);
    let used = out.tiles.len() - before;
    split(data, right, b, depth, budget - used, settings, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Builder, ROOT};
    use std::sync::atomic::AtomicBool;
    #[test]
    fn flat_million_has_bounded_layout_and_conserves_area() {
        let mut b = Builder::new(1_000_000);
        for _ in 0..1_000_000 {
            b.add_file(ROOT, "x", Some(1)).unwrap();
        }
        let data = b.finish(&AtomicBool::new(false), |_, _, _| {}).unwrap();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0));
        let l = build(
            &data,
            View::folder(&data, ROOT),
            rect,
            Settings {
                budget: 1000,
                ..Settings::default()
            },
        );
        assert!(l.tiles.len() <= 1000);
        assert!(l.visited <= 2000);
        assert_eq!(l.tiles.iter().map(|t| t.bytes).sum::<u64>(), 1_000_000);
        let area: f32 = l.tiles.iter().map(|t| t.rect.area()).sum();
        assert!((area - rect.area()).abs() < 10.0);
        for tile in &l.tiles {
            assert!(rect.contains_rect(tile.rect));
        }
        let group = l
            .tiles
            .iter()
            .find_map(|t| {
                if let TileKind::Group(v) = t.kind {
                    Some(v)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(group.end - group.start < 1_000_000);
    }
    #[test]
    fn extreme_skew_and_zero_size_are_finite_and_conserve_bytes() {
        let mut b = Builder::new(100);
        b.add_file(ROOT, "huge", Some(1 << 60)).unwrap();
        for _ in 0..50 {
            b.add_file(ROOT, "tiny", Some(1)).unwrap();
        }
        b.add_file(ROOT, "zero", Some(0)).unwrap();
        let d = b.finish(&AtomicBool::new(false), |_, _, _| {}).unwrap();
        let l = build(
            &d,
            View::folder(&d, ROOT),
            Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 600.0)),
            Settings::default(),
        );
        assert_eq!(l.tiles.iter().map(|t| t.bytes).sum::<u64>(), (1 << 60) + 50);
        assert!(l.tiles.iter().all(|t| t.rect.is_finite()));
    }
    #[test]
    fn tiny_deep_directories_reach_individual_files() {
        let mut b = Builder::new(100);
        let mut path = String::from("C:");
        for _ in 0..24 {
            path.push_str("\\nested");
        }
        let dir = b.folder(&path).unwrap();
        let file = b.add_file(dir, "target.vhdx", Some(100)).unwrap();
        let data = b.finish(&AtomicBool::new(false), |_, _, _| {}).unwrap();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(30.0, 40.0));
        let shallow = build(
            &data,
            View::folder(&data, ROOT),
            rect,
            Detail::Fine.settings(8),
        );
        let deep = build(
            &data,
            View::folder(&data, ROOT),
            rect,
            Detail::Fine.settings(32),
        );
        assert!(
            !shallow
                .tiles
                .iter()
                .any(|t| matches!(t.kind, TileKind::Entry(id) if id == file))
        );
        assert!(
            deep.tiles
                .iter()
                .any(|t| matches!(t.kind, TileKind::Entry(id) if id == file))
        );
        assert!(deep.tiles.iter().all(|t| rect.contains_rect(t.rect)));
        assert!(deep.tiles.iter().any(|t| t.depth > 8));
    }
    #[test]
    fn a_large_file_does_not_starve_smaller_files_of_tiles() {
        let mut b = Builder::new(100);
        b.add_file(ROOT, "huge", Some(9_000)).unwrap();
        for _ in 0..100 {
            b.add_file(ROOT, "small", Some(1)).unwrap();
        }
        let data = b.finish(&AtomicBool::new(false), |_, _, _| {}).unwrap();
        let l = build(
            &data,
            View::folder(&data, ROOT),
            Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0)),
            Settings {
                budget: 101,
                min_area: 1.0,
                depth: 32,
            },
        );
        assert_eq!(l.tiles.len(), 101);
        assert!(l.tiles.iter().all(|t| matches!(t.kind, TileKind::Entry(_))));
        assert_eq!(l.tiles.iter().map(|t| t.bytes).sum::<u64>(), 9_100);
    }
    #[test]
    fn fine_detail_resolves_files_that_balanced_groups() {
        let mut b = Builder::new(50_000);
        for _ in 0..50_000 {
            b.add_file(ROOT, "file", Some(1)).unwrap();
        }
        let data = b.finish(&AtomicBool::new(false), |_, _, _| {}).unwrap();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let balanced = build(
            &data,
            View::folder(&data, ROOT),
            rect,
            Detail::Balanced.settings(32),
        );
        let fine = build(
            &data,
            View::folder(&data, ROOT),
            rect,
            Detail::Fine.settings(32),
        );
        assert!(
            balanced
                .tiles
                .iter()
                .any(|t| matches!(t.kind, TileKind::Group(_)))
        );
        assert_eq!(fine.tiles.len(), 50_000);
        assert!(
            fine.tiles
                .iter()
                .all(|t| matches!(t.kind, TileKind::Entry(_)))
        );
        for (l, settings) in [
            (&balanced, Detail::Balanced.settings(32)),
            (&fine, Detail::Fine.settings(32)),
        ] {
            assert!(l.tiles.len() <= settings.budget);
            assert!(l.visited <= settings.budget * 2);
            assert_eq!(l.tiles.iter().map(|t| t.bytes).sum::<u64>(), 50_000);
            assert!(
                (l.tiles
                    .iter()
                    .map(|t| f64::from(t.rect.width()) * f64::from(t.rect.height()))
                    .sum::<f64>()
                    - f64::from(rect.area()))
                .abs()
                    < 1.0
            );
        }
    }
}
