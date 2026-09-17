//! Virtual tree rows represented by ranges of children, not one row per file.
use crate::model::{Dataset, Id, ROOT};
use std::collections::{HashMap, HashSet};

struct Span {
    parent: Id,
    start: usize,
    end: usize,
    depth: usize,
    first: usize,
}
#[derive(Default)]
pub struct Tree {
    expanded: HashSet<Id>,
    spans: Vec<Span>,
    pub len: usize,
}
impl Tree {
    pub fn new(data: &Dataset) -> Self {
        let mut tree = Self::default();
        tree.expanded.insert(ROOT);
        tree.expanded.extend(
            data.child_ids(ROOT)
                .iter()
                .copied()
                .filter(|id| data.node(*id).is_dir()),
        );
        tree.rebuild(data);
        tree
    }
    pub fn is_expanded(&self, id: Id) -> bool {
        self.expanded.contains(&id)
    }
    pub fn toggle(&mut self, data: &Dataset, id: Id) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        self.rebuild(data);
    }
    pub fn reveal(&mut self, data: &Dataset, id: Id) -> Option<usize> {
        let mut changed = false;
        let mut parent = data.node(id).parent;
        loop {
            changed |= self.expanded.insert(parent);
            if parent == ROOT {
                break;
            }
            parent = data.node(parent).parent;
        }
        if changed {
            self.rebuild(data);
        }
        self.row_of(data, id)
    }
    pub fn row(&self, data: &Dataset, index: usize) -> Option<(Id, usize)> {
        if index >= self.len {
            return None;
        }
        let span = &self.spans[self.spans.partition_point(|s| s.first <= index) - 1];
        Some((
            data.child_ids(span.parent)[span.start + index - span.first],
            span.depth,
        ))
    }
    pub fn row_of(&self, data: &Dataset, id: Id) -> Option<usize> {
        if id == ROOT {
            return None;
        }
        let parent = data.node(id).parent;
        let index = child_index(data, parent, id)?;
        self.spans
            .iter()
            .find(|s| s.parent == parent && (s.start..s.end).contains(&index))
            .map(|s| s.first + index - s.start)
    }
    fn rebuild(&mut self, data: &Dataset) {
        self.spans.clear();
        self.len = 0;
        let mut by_parent: HashMap<Id, Vec<(usize, Id)>> = HashMap::new();
        for &id in &self.expanded {
            if id == ROOT {
                continue;
            }
            let parent = data.node(id).parent;
            if let Some(index) = child_index(data, parent, id) {
                by_parent.entry(parent).or_default().push((index, id));
            }
        }
        for children in by_parent.values_mut() {
            children.sort_unstable();
        }
        enum Task {
            Children(Id, usize),
            Span(Id, usize, usize, usize),
        }
        let mut stack = vec![Task::Children(ROOT, 0)];
        while let Some(task) = stack.pop() {
            match task {
                Task::Span(parent, start, end, depth) if start < end => {
                    self.spans.push(Span {
                        parent,
                        start,
                        end,
                        depth,
                        first: self.len,
                    });
                    self.len += end - start;
                }
                Task::Span(..) => {}
                Task::Children(parent, depth) => {
                    let count = data.child_ids(parent).len();
                    let mut cursor = count;
                    if let Some(expanded) = by_parent.get(&parent) {
                        for &(index, id) in expanded.iter().rev() {
                            stack.push(Task::Span(parent, index + 1, cursor, depth));
                            stack.push(Task::Children(id, depth + 1));
                            cursor = index + 1;
                        }
                    }
                    stack.push(Task::Span(parent, 0, cursor, depth));
                }
            }
        }
    }
}
fn child_index(data: &Dataset, parent: Id, id: Id) -> Option<usize> {
    data.child_ids(parent)
        .binary_search_by(|candidate| {
            data.node(id)
                .bytes
                .cmp(&data.node(*candidate).bytes)
                .then(candidate.cmp(&id))
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Builder;
    use std::sync::atomic::AtomicBool;
    #[test]
    fn expansion_and_reveal_preserve_order_without_materializing_rows() {
        let mut b = Builder::new(100_000);
        let folder = b.folder("C:\\big").unwrap();
        let nested = b.folder("C:\\big\\nested").unwrap();
        let target = b.add_file(nested, "target", Some(999_999)).unwrap();
        for _ in 0..100_000 {
            b.add_file(folder, "file", Some(1)).unwrap();
        }
        let data = b.finish(&AtomicBool::new(false), |_, _, _| {}).unwrap();
        let mut tree = Tree::new(&data);
        assert_eq!(tree.len, 2); // drive and its folder
        let row = tree.reveal(&data, target).unwrap();
        assert_eq!(tree.row(&data, row), Some((target, 3)));
        assert_eq!(tree.len, 100_004);
        assert!(tree.spans.len() < 10);
        let big_row = tree.row_of(&data, folder).unwrap();
        assert_eq!(tree.row(&data, big_row).unwrap().0, folder);
        tree.toggle(&data, folder);
        assert_eq!(tree.len, 2);
        assert!(tree.row_of(&data, target).is_none());
    }
}
