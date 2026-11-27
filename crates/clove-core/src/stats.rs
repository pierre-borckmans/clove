//! How fast clones really come: every clone creation and golden image build
//! logged with its step timings, and the stats people look for (median, p90,
//! where the time goes, which clones had to rerun setup).

use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::events::Timing;
use crate::repo::{CloneMeta, Clove, GoldenMeta};

/// One `clove new`, as it went.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Created {
    pub at: u64,
    pub name: String,
    pub branch: Option<String>,
    pub base: String,
    pub golden_generation: u32,
    /// Wall-clock time of the whole command.
    pub total_ms: f64,
    pub timings: Vec<Timing>,
    #[serde(default)]
    pub failed_steps: Vec<String>,
}

impl Created {
    pub fn from_meta(m: &CloneMeta) -> Self {
        Self {
            at: m.created_at,
            name: m.name.clone(),
            branch: m.branch.clone(),
            base: m.base.clone(),
            golden_generation: m.golden_generation,
            total_ms: m.total_ms,
            timings: m.timings.clone(),
            failed_steps: m.failed_steps.clone(),
        }
    }

    /// Time spent re-running setup steps (`setup …`) in the clone.
    pub fn setup_ms(&self) -> f64 {
        self.timings
            .iter()
            .filter(|t| t.step.starts_with("setup "))
            .map(|t| t.ms)
            .sum()
    }

    /// Wall-clock time no step accounts for: locks, checks, git bookkeeping.
    /// Fetching overlaps the mount, so this can be slightly negative; it is
    /// clamped at zero.
    pub fn untimed_ms(&self) -> f64 {
        (self.total_ms - self.timings.iter().map(|t| t.ms).sum::<f64>()).max(0.0)
    }

    /// Setup reran for real (beyond trusting the environment): its inputs
    /// changed since the golden image.
    pub fn reran(&self) -> bool {
        self.setup_ms() > RERAN_MS
    }
}

/// Setup time past which a clone counts as having rerun setup.
pub const RERAN_MS: f64 = 1000.0;
/// What counts as fast: the promise is about a second, this leaves room.
pub const FAST_MS: f64 = 5000.0;

/// One golden image build (`clove init` or a refresh).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoldenBuild {
    pub at: u64,
    pub generation: u32,
    pub commit: String,
    pub base: String,
    pub total_ms: f64,
    pub timings: Vec<Timing>,
}

impl GoldenBuild {
    pub fn from_meta(m: &GoldenMeta) -> Self {
        Self {
            at: m.created_at,
            generation: m.generation,
            commit: m.commit.clone(),
            base: m.base.clone(),
            total_ms: m.timings.iter().map(|t| t.ms).sum(),
            timings: m.timings.clone(),
        }
    }
}

/// A step's share of clone creation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StepStat {
    pub step: String,
    /// Clones that ran it.
    pub runs: usize,
    pub median_ms: f64,
    pub p90_ms: f64,
    pub max_ms: f64,
}

/// The numbers for one repo.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Stats {
    pub clones: usize,
    pub median_ms: f64,
    pub p90_ms: f64,
    pub fastest_ms: f64,
    pub slowest_ms: f64,
    /// Clones made in under [`FAST_MS`].
    pub fast: usize,
    /// Clones that reran setup (see [`Created::reran`]).
    pub reran: usize,
    /// The same, for clones that didn't rerun setup: clove's own cost.
    pub clean_median_ms: Option<f64>,
    /// Per step, in the order clones run them; "untimed" last.
    pub steps: Vec<StepStat>,
    pub golden_builds: usize,
    pub golden_median_ms: Option<f64>,
}

/// Lines kept per log; older ones go when a file reaches twice this.
const KEEP: usize = 1000;

impl Clove {
    fn created_path(&self) -> PathBuf {
        self.state.join("created.jsonl")
    }

    fn golden_log_path(&self) -> PathBuf {
        self.state.join("golden.jsonl")
    }

    /// Logs a clone creation. Never fails the creation it records.
    pub fn record_created(&self, m: &CloneMeta) {
        append(&self.created_path(), &Created::from_meta(m));
    }

    /// Logs a golden image build. Never fails the build it records.
    pub fn record_golden(&self, m: &GoldenMeta) {
        append(&self.golden_log_path(), &GoldenBuild::from_meta(m));
    }

    /// Every clone creation, newest first: the log, plus clones that predate
    /// it (from their own metadata).
    pub fn created(&self) -> Vec<Created> {
        let mut v: Vec<Created> = read(&self.created_path());
        for name in self.clone_names().unwrap_or_default() {
            if let Ok(Some(m)) = self.clone_meta(&name)
                && !v.iter().any(|c| c.name == m.name && c.at == m.created_at)
            {
                v.push(Created::from_meta(&m));
            }
        }
        v.sort_by_key(|c| std::cmp::Reverse(c.at));
        v
    }

    /// Every golden image build, newest first; the current one even if it
    /// predates the log (older generations are deleted with their metadata).
    pub fn golden_builds(&self) -> Vec<GoldenBuild> {
        let mut v: Vec<GoldenBuild> = read(&self.golden_log_path());
        if let Ok(Some(m)) = self.golden_meta()
            && !v
                .iter()
                .any(|g| g.generation == m.generation && g.at == m.created_at)
        {
            v.push(GoldenBuild::from_meta(&m));
        }
        v.sort_by_key(|g| std::cmp::Reverse(g.at));
        v
    }

    pub fn stats(&self) -> Stats {
        stats(&self.created(), &self.golden_builds())
    }
}

fn append<T: Serialize>(path: &std::path::Path, entry: &T) {
    let Ok(line) = serde_json::to_string(entry) else {
        return;
    };
    let ok = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| writeln!(f, "{line}"));
    if ok.is_err() {
        return;
    }
    let Ok(s) = std::fs::read_to_string(path) else {
        return;
    };
    let lines: Vec<&str> = s.lines().collect();
    if lines.len() >= 2 * KEEP {
        let kept = lines[lines.len() - KEEP..].join("\n") + "\n";
        let _ = crate::util::write_atomic(path, kept.as_bytes());
    }
}

fn read<T: for<'de> Deserialize<'de>>(path: &std::path::Path) -> Vec<T> {
    std::fs::read_to_string(path)
        .map(|s| s.lines().filter_map(|l| serde_json::from_str(l).ok()).collect())
        .unwrap_or_default()
}

/// The `p`th percentile (0–100) by nearest rank; None for no values.
pub fn percentile(values: &[f64], p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let rank = ((p / 100.0) * v.len() as f64).ceil() as usize;
    Some(v[rank.clamp(1, v.len()) - 1])
}

pub fn stats(created: &[Created], golden: &[GoldenBuild]) -> Stats {
    let totals: Vec<f64> = created.iter().map(|c| c.total_ms).collect();
    let clean: Vec<f64> = created
        .iter()
        .filter(|c| !c.reran())
        .map(|c| c.total_ms)
        .collect();
    // Steps in the order clones run them, by first appearance.
    let mut order: Vec<String> = vec![];
    for c in created.iter().rev() {
        for t in &c.timings {
            if !order.contains(&t.step) {
                order.push(t.step.clone());
            }
        }
    }
    let step_stat = |step: &str, values: Vec<f64>| StepStat {
        step: step.to_string(),
        runs: values.len(),
        median_ms: percentile(&values, 50.0).unwrap_or(0.0),
        p90_ms: percentile(&values, 90.0).unwrap_or(0.0),
        max_ms: values.iter().copied().fold(0.0, f64::max),
    };
    let mut steps: Vec<StepStat> = order
        .iter()
        .map(|s| {
            let values = created
                .iter()
                .flat_map(|c| c.timings.iter().filter(|t| &t.step == s).map(|t| t.ms))
                .collect();
            step_stat(s, values)
        })
        .collect();
    if !created.is_empty() {
        steps.push(step_stat(
            "untimed",
            created.iter().map(|c| c.untimed_ms()).collect(),
        ));
    }
    let golden_totals: Vec<f64> = golden.iter().map(|g| g.total_ms).collect();
    Stats {
        clones: created.len(),
        median_ms: percentile(&totals, 50.0).unwrap_or(0.0),
        p90_ms: percentile(&totals, 90.0).unwrap_or(0.0),
        fastest_ms: totals.iter().copied().fold(f64::INFINITY, f64::min).min(f64::MAX),
        slowest_ms: totals.iter().copied().fold(0.0, f64::max),
        fast: totals.iter().filter(|&&t| t < FAST_MS).count(),
        reran: created.iter().filter(|c| c.reran()).count(),
        clean_median_ms: percentile(&clean, 50.0),
        steps,
        golden_builds: golden.len(),
        golden_median_ms: percentile(&golden_totals, 50.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clone(name: &str, at: u64, steps: &[(&str, f64)], total: f64) -> Created {
        Created {
            at,
            name: name.into(),
            branch: Some(name.into()),
            base: "origin/main".into(),
            golden_generation: 1,
            total_ms: total,
            timings: steps
                .iter()
                .map(|(s, ms)| Timing {
                    step: s.to_string(),
                    ms: *ms,
                })
                .collect(),
            failed_steps: vec![],
        }
    }

    #[test]
    fn percentiles_by_nearest_rank() {
        let v = [5.0, 1.0, 4.0, 2.0, 3.0];
        assert_eq!(percentile(&v, 50.0), Some(3.0));
        assert_eq!(percentile(&v, 90.0), Some(5.0));
        assert_eq!(percentile(&v, 0.0), Some(1.0));
        assert_eq!(percentile(&[], 50.0), None);
    }

    #[test]
    fn stats_split_clove_from_setup() {
        let created = vec![
            clone(
                "a",
                3,
                &[("mount", 900.0), ("checkout", 800.0), ("setup mise-trust", 40.0)],
                2300.0,
            ),
            clone("b", 2, &[("mount", 1000.0), ("checkout", 1700.0)], 3800.0),
            clone(
                "c",
                1,
                &[("mount", 700.0), ("checkout", 1300.0), ("setup direnv", 24_000.0)],
                32_000.0,
            ),
        ];
        let s = stats(&created, &[]);
        assert_eq!(s.clones, 3);
        assert_eq!((s.fast, s.reran), (2, 1));
        assert_eq!(s.median_ms, 3800.0);
        assert_eq!(s.clean_median_ms, Some(2300.0));
        assert_eq!((s.fastest_ms, s.slowest_ms), (2300.0, 32_000.0));
        let names: Vec<_> = s.steps.iter().map(|x| x.step.as_str()).collect();
        assert_eq!(
            names,
            ["mount", "checkout", "setup direnv", "setup mise-trust", "untimed"]
        );
        let mount = &s.steps[0];
        assert_eq!((mount.runs, mount.median_ms, mount.max_ms), (3, 900.0, 1000.0));
        // a: 2300 − 1740 = 560 untimed
        assert!(s.steps.last().unwrap().max_ms >= 560.0);
    }

    #[test]
    fn logs_round_trip_and_fold_in_existing_clones() {
        let t = tempfile::tempdir().unwrap();
        let mut c = Clove::open(t.path()).unwrap();
        c.state = t.path().join("state");
        std::fs::create_dir_all(&c.state).unwrap();
        let meta = CloneMeta {
            name: "x".into(),
            branch: Some("x".into()),
            path: t.path().join("x"),
            mount: t.path().join("m"),
            base: "origin/main".into(),
            golden_commit: "abc".into(),
            golden_generation: 4,
            created_at: 100,
            timings: vec![Timing {
                step: "mount".into(),
                ms: 900.0,
            }],
            total_ms: 1500.0,
            failed_steps: vec![],
            setup_commit: None,
        };
        c.record_created(&meta);
        // An older clone, made before the log existed, only has its metadata.
        let old = CloneMeta {
            name: "old".into(),
            created_at: 50,
            ..meta.clone()
        };
        crate::repo::write_json(&c.clone_dir("old").join("meta.json"), &old).unwrap();
        crate::repo::write_json(&c.clone_dir("x").join("meta.json"), &meta).unwrap();
        let got = c.created();
        let names: Vec<_> = got.iter().map(|c| (c.name.as_str(), c.at)).collect();
        assert_eq!(
            names,
            [("x", 100), ("old", 50)],
            "logged once, old folded in, newest first"
        );
        assert_eq!(c.stats().clones, 2);
    }
}
