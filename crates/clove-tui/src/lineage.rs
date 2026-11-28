//! Forks of forks as a tree, with the desktop app's folding rules
//! (`apps/desktop/src/components/map/lineage.ts`).

use clove_core::CloneInfo;
use std::collections::{HashMap, HashSet};

/// A lineage folds once its parent has more forks than this (at any depth)...
pub const MAX_FORKS: usize = 3;
/// ...or goes deeper than this many forks of forks.
pub const MAX_DEPTH: usize = 2;

/// Lineages opened or closed by hand; the rest follow [`Forest::foldable`].
#[derive(Debug, Clone, Default)]
pub struct Folds {
    pub open: HashSet<String>,
    pub closed: HashSet<String>,
}

impl Folds {
    pub fn set(&mut self, name: &str, open: bool) {
        if open {
            self.closed.remove(name);
            self.open.insert(name.to_string());
        } else {
            self.open.remove(name);
            self.closed.insert(name.to_string());
        }
    }
}

/// Who forked from whom, among the clones listed. A clone whose parent is
/// gone (removed without its forks) is a root.
pub struct Forest<'a> {
    parent: HashMap<&'a str, &'a str>,
    kids: HashMap<&'a str, Vec<&'a CloneInfo>>,
}

impl<'a> Forest<'a> {
    pub fn new(clones: &'a [CloneInfo]) -> Self {
        let names: HashSet<&str> = clones.iter().map(|c| c.name.as_str()).collect();
        let mut parent: HashMap<&str, &str> = clones
            .iter()
            .filter_map(|c| {
                let p = c.forked_from.as_deref()?;
                (p != c.name && names.contains(p)).then_some((c.name.as_str(), p))
            })
            .collect();
        // A loop of names (they get reused) breaks at the clone that closes it.
        for c in clones {
            let mut seen = HashSet::new();
            let mut up = parent.get(c.name.as_str()).copied();
            while let Some(u) = up {
                if u == c.name {
                    parent.remove(c.name.as_str());
                    break;
                }
                if !seen.insert(u) {
                    break;
                }
                up = parent.get(u).copied();
            }
        }
        let mut kids: HashMap<&str, Vec<&CloneInfo>> = HashMap::new();
        for c in clones {
            if let Some(p) = parent.get(c.name.as_str()) {
                kids.entry(p).or_default().push(c);
            }
        }
        for v in kids.values_mut() {
            v.sort_by_key(|c| c.created_at);
        }
        Self { parent, kids }
    }

    /// Some clone here has a fork listed with it.
    pub fn has_forks(&self) -> bool {
        !self.parent.is_empty()
    }

    pub fn parent(&self, name: &str) -> Option<&'a str> {
        self.parent.get(name).copied()
    }

    pub fn kids(&self, name: &str) -> &[&'a CloneInfo] {
        self.kids.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Forks under it at any depth.
    pub fn descendants(&self, name: &str) -> usize {
        self.kids(name)
            .iter()
            .map(|k| 1 + self.descendants(&k.name))
            .sum()
    }

    /// Levels of forks below it: 0 for none, 1 for forks only, ...
    pub fn height(&self, name: &str) -> usize {
        self.kids(name)
            .iter()
            .map(|k| 1 + self.height(&k.name))
            .max()
            .unwrap_or(0)
    }

    /// Big enough to fold by default.
    pub fn foldable(&self, name: &str) -> bool {
        self.descendants(name) > MAX_FORKS || self.height(name) > MAX_DEPTH
    }

    /// Its forks are hidden: by hand if opened or closed, else when big.
    pub fn folded(&self, name: &str, folds: &Folds) -> bool {
        if self.kids(name).is_empty() || folds.open.contains(name) {
            return false;
        }
        folds.closed.contains(name) || self.foldable(name)
    }

    /// Its forks at any depth, deepest first: the order to remove them in.
    pub fn forks_of(&self, name: &str) -> Vec<String> {
        let mut out: Vec<(usize, String)> = vec![];
        let mut stack: Vec<(&str, usize)> = vec![(name, 1)];
        while let Some((n, depth)) = stack.pop() {
            for k in self.kids(n) {
                out.push((depth, k.name.clone()));
                stack.push((&k.name, depth + 1));
            }
        }
        out.sort_by_key(|x| std::cmp::Reverse(x.0));
        out.into_iter().map(|(_, n)| n).collect()
    }
}

/// A row of the clone list.
#[derive(Debug, Clone)]
pub struct TreeRow<'a> {
    pub clone: &'a CloneInfo,
    pub depth: usize,
    /// The clone it was forked from, when that's listed.
    pub parent: Option<&'a str>,
    /// Per column left of its connector, whether an ancestor's later sibling
    /// keeps the line going.
    pub rails: Vec<bool>,
    /// The last of its parent's forks (└ rather than ├).
    pub last: bool,
    /// Forks under it at any depth.
    pub forks: usize,
    pub open: bool,
}

impl TreeRow<'_> {
    /// The connectors left of its name: `│  ├─ `; empty for a root.
    pub fn gutter(&self) -> String {
        if self.depth == 0 {
            return String::new();
        }
        let mut s: String = self
            .rails
            .iter()
            .map(|on| if *on { "│  " } else { "   " })
            .collect();
        s.push_str(if self.last { "└─ " } else { "├─ " });
        s
    }

    /// What a folded parent says after its name.
    pub fn folded_note(&self) -> Option<String> {
        (!self.open && self.forks > 0)
            .then(|| format!("+{} fork{}", self.forks, if self.forks == 1 { "" } else { "s" }))
    }
}

/// The list's rows: lineage roots in list order, each followed by its forks
/// (oldest first), depth first; folded lineages hide their forks. A filter
/// (lowercase) shows matches and their ancestors, unfolded. Without forks
/// this is the clones that match, in order.
pub fn tree_rows<'a>(clones: &'a [CloneInfo], filter: &str, folds: &Folds) -> Vec<TreeRow<'a>> {
    let f = Forest::new(clones);
    let matches = |c: &CloneInfo| {
        filter.is_empty()
            || c.name.to_lowercase().contains(filter)
            || c.branch
                .as_deref()
                .is_some_and(|b| b.to_lowercase().contains(filter))
    };
    fn keep(f: &Forest, c: &CloneInfo, m: &dyn Fn(&CloneInfo) -> bool) -> bool {
        m(c) || f.kids(&c.name).iter().any(|k| keep(f, k, m))
    }
    struct Walk<'f, 'a> {
        f: &'f Forest<'a>,
        filter: &'f str,
        folds: &'f Folds,
        m: &'f dyn Fn(&CloneInfo) -> bool,
        out: Vec<TreeRow<'a>>,
    }
    impl<'a> Walk<'_, 'a> {
        fn visit(&mut self, c: &'a CloneInfo, depth: usize, rails: Vec<bool>, last: bool) {
            if !keep(self.f, c, self.m) {
                return;
            }
            let open = !self.filter.is_empty() || !self.f.folded(&c.name, self.folds);
            self.out.push(TreeRow {
                clone: c,
                depth,
                parent: self.f.parent(&c.name),
                rails: rails.clone(),
                last,
                forks: self.f.descendants(&c.name),
                open,
            });
            if !open {
                return;
            }
            let kids: Vec<&'a CloneInfo> = self
                .f
                .kids(&c.name)
                .iter()
                .copied()
                .filter(|k| keep(self.f, k, self.m))
                .collect();
            let n = kids.len();
            for (i, k) in kids.into_iter().enumerate() {
                let mut r = rails.clone();
                if depth > 0 {
                    r.push(!last);
                }
                self.visit(k, depth + 1, r, i + 1 == n);
            }
        }
    }
    let mut w = Walk {
        f: &f,
        filter,
        folds,
        m: &matches,
        out: vec![],
    };
    for c in clones.iter().filter(|c| f.parent(&c.name).is_none()) {
        w.visit(c, 0, vec![], true);
    }
    w.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(name: &str, from: Option<&str>, at: u64) -> CloneInfo {
        CloneInfo {
            name: name.into(),
            path: Default::default(),
            mounted: true,
            branch: Some(name.into()),
            head: None,
            dirty: None,
            ahead: None,
            behind: None,
            image_bytes: 0,
            own_bytes: 0,
            created_at: at,
            golden_generation: 1,
            timings: vec![],
            total_ms: 0.0,
            failed_steps: vec![],
            issue: None,
            removing_when_free: false,
            operation: None,
            status_pending: false,
            forked_from: from.map(String::from),
            checkpoints: 0,
        }
    }

    fn names(rows: &[TreeRow]) -> Vec<String> {
        rows.iter()
            .map(|r| format!("{}{}", r.gutter(), r.clone.name))
            .collect()
    }

    #[test]
    fn without_forks_rows_are_the_list() {
        let cs = vec![c("b", None, 2), c("a", None, 1)];
        let rows = tree_rows(&cs, "", &Folds::default());
        assert_eq!(names(&rows), ["b", "a"]);
        assert!(!Forest::new(&cs).has_forks());
    }

    #[test]
    fn forks_sit_under_their_parent_oldest_first() {
        let cs = vec![
            c("x", None, 1),
            c("x-b", Some("x"), 3),
            c("y", None, 2),
            c("x-a", Some("x"), 2),
            c("x-a-a", Some("x-a"), 4),
            // Its parent is gone: a root.
            c("z-a", Some("z"), 5),
        ];
        let rows = tree_rows(&cs, "", &Folds::default());
        assert_eq!(names(&rows), ["x", "├─ x-a", "│  └─ x-a-a", "└─ x-b", "y", "z-a"]);
        let f = Forest::new(&cs);
        assert_eq!(f.forks_of("x"), ["x-a-a", "x-a", "x-b"]);
        assert_eq!(f.descendants("x"), 3);
        assert_eq!(f.height("x"), 2);
        assert!(!f.foldable("x"));
    }

    #[test]
    fn big_lineages_fold_until_opened() {
        let mut cs = vec![c("x", None, 1)];
        for i in 0..4 {
            cs.push(c(&format!("x-{i}"), Some("x"), 2 + i));
        }
        let mut folds = Folds::default();
        let rows = tree_rows(&cs, "", &folds);
        assert_eq!(names(&rows), ["x"]);
        assert_eq!(rows[0].folded_note().as_deref(), Some("+4 forks"));
        folds.set("x", true);
        assert_eq!(tree_rows(&cs, "", &folds).len(), 5);
        // Deep but small folds too.
        let deep = vec![
            c("a", None, 1),
            c("b", Some("a"), 2),
            c("c", Some("b"), 3),
            c("d", Some("c"), 4),
        ];
        assert_eq!(names(&tree_rows(&deep, "", &Folds::default())), ["a"]);
        // A filter shows a match with its ancestors, folded or not.
        assert_eq!(
            names(&tree_rows(&deep, "c", &Folds::default())),
            ["a", "└─ b", "   └─ c"]
        );
    }

    #[test]
    fn a_loop_of_names_breaks() {
        let cs = vec![c("a", Some("b"), 1), c("b", Some("a"), 2)];
        assert_eq!(tree_rows(&cs, "", &Folds::default()).len(), 2);
    }
}
