//! Editing single settings in one config layer. Every surface (CLI, TUI, app)
//! goes through here: values are validated, and the file keeps its comments and
//! layout because it is edited with `toml_edit`, not re-serialized.

use crate::config::{self, Config, FileConfig, parse_duration};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value};

/// A config file, in precedence order (see [`config::layer_paths`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    /// `~/.config/clove/config.toml`: every repo, only you.
    All,
    /// `.clove.toml` in the repo: shared with the team.
    Team,
    /// `~/.config/clove/repos/<repo>.toml`: this repo, only you. Where edits go
    /// unless told otherwise: it changes nothing for anyone else.
    #[default]
    You,
}

impl Layer {
    pub const ALL: [Layer; 3] = [Layer::All, Layer::Team, Layer::You];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        config::layer_paths(Path::new(""), "")[self.index()].0
    }

    pub fn path(self, repo_root: &Path, repo_name: &str) -> PathBuf {
        let [a, b, c] = config::layer_paths(repo_root, repo_name);
        [a, b, c][self.index()].1.clone()
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "all" => Ok(Layer::All),
            "team" => Ok(Layer::Team),
            "you" => Ok(Layer::You),
            _ => bail!("unknown layer {s:?}: use all, team or you"),
        }
    }
}

pub use crate::config::{DefaultValue, FIELDS, FILE_ONLY, Field, Kind};

pub fn field(key: &str) -> Result<&'static Field> {
    FIELDS.iter().find(|f| f.key == key).with_context(|| {
        let keys: Vec<_> = FIELDS.iter().map(|f| f.key).collect();
        format!("unknown setting {key:?}; one of: {}", keys.join(", "))
    })
}

/// One setting across the layers, for display.
#[derive(Debug, Clone, Serialize)]
pub struct FieldState {
    #[serde(flatten)]
    pub field: Field,
    /// The value each layer sets, in [`Layer::ALL`] order; None where unset.
    pub layers: [Option<String>; 3],
    /// What clove uses.
    pub effective: String,
    /// The layer it comes from; None for the default.
    pub source: Option<Layer>,
}

impl FieldState {
    pub fn get(&self, layer: Layer) -> Option<&str> {
        self.layers[layer.index()].as_deref()
    }
}

/// Every setting in [`FIELDS`] with its value per layer, the effective value
/// and where it comes from. `cfg` supplies defaults that depend on the repo.
pub fn fields(repo_root: &Path, repo_name: &str, cfg: &Config) -> Result<Vec<FieldState>> {
    let mut files: [Option<toml::Table>; 3] = Default::default();
    for l in Layer::ALL {
        let path = l.path(repo_root, repo_name);
        files[l.index()] = match std::fs::read_to_string(&path) {
            Ok(s) => Some(toml::from_str(&s).with_context(|| format!("parse {}", path.display()))?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
        };
    }
    Ok(resolve(&files, cfg))
}

/// [`fields`] for already parsed layers, in [`Layer::ALL`] order.
pub fn resolve(files: &[Option<toml::Table>; 3], cfg: &Config) -> Vec<FieldState> {
    let resolved = serde_json::to_value(cfg).unwrap_or_default();
    FIELDS
        .iter()
        .map(|f| {
            let layers: [Option<String>; 3] = std::array::from_fn(|i| {
                files[i]
                    .as_ref()
                    .and_then(|t| lookup(t, f.key))
                    .map(|v| show(f.kind, v))
            });
            let source = Layer::ALL.into_iter().rev().find(|l| layers[l.index()].is_some());
            let effective = match (source, f.default) {
                (Some(l), _) => layers[l.index()].clone().unwrap_or_default(),
                (None, DefaultValue::Value(v)) => v.to_string(),
                (None, DefaultValue::Resolved) => {
                    let v = f.key.split('.').try_fold(&resolved, |v, k| v.get(k));
                    match v {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        Some(v) => v.to_string(),
                        None => String::new(),
                    }
                }
            };
            FieldState {
                field: *f,
                layers,
                effective,
                source,
            }
        })
        .collect()
}

fn lookup<'a>(t: &'a toml::Table, key: &str) -> Option<&'a toml::Value> {
    let (head, rest) = match key.split_once('.') {
        Some((h, r)) => (h, Some(r)),
        None => (key, None),
    };
    let v = t.get(head)?;
    match rest {
        None => Some(v),
        Some(r) => lookup(v.as_table()?, r),
    }
}

/// A stored value as the UIs show it and [`parse_value`] accepts it back.
fn show(kind: Kind, v: &toml::Value) -> String {
    use toml::Value as V;
    match (kind, v) {
        (Kind::Seconds, V::Integer(n)) => fmt_secs((*n).max(0) as u64),
        (_, V::String(s)) => s.clone(),
        (_, V::Array(a)) => a
            .iter()
            .map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string()))
            .collect::<Vec<_>>()
            .join(", "),
        (_, V::Table(t)) => t
            .iter()
            .map(|(k, x)| {
                format!(
                    "{k}={}",
                    x.as_str().map(String::from).unwrap_or_else(|| x.to_string())
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
        (_, v) => v.to_string(),
    }
}

/// Seconds as the largest exact unit: 300 -> "5m", 90 -> "90s".
pub fn fmt_secs(s: u64) -> String {
    match s {
        0 => "0s".into(),
        s if s % 86_400 == 0 => format!("{}d", s / 86_400),
        s if s % 3600 == 0 => format!("{}h", s / 3600),
        s if s % 60 == 0 => format!("{}m", s / 60),
        s => format!("{s}s"),
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Some(true),
        "false" | "off" | "no" | "0" => Some(false),
        _ => None,
    }
}

/// "300G", "300g", "300GiB" -> "300G".
fn parse_size(s: &str) -> Result<String> {
    let s = s.trim();
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: u64 = num
        .parse()
        .with_context(|| format!("bad size {s:?}: use e.g. 300G"))?;
    if n == 0 {
        bail!("size must be more than 0");
    }
    let unit = unit.trim().trim_end_matches(['B', 'b']).trim_end_matches('i');
    let unit = unit.to_ascii_uppercase();
    if !matches!(unit.as_str(), "K" | "M" | "G" | "T") {
        bail!("bad size {s:?}: use K, M, G or T, e.g. 300G");
    }
    Ok(format!("{n}{unit}"))
}

/// Validates `input` for the field and turns it into a TOML value.
pub fn parse_value(f: &Field, input: &str) -> Result<Value> {
    let s = input.trim();
    let key = f.key;
    Ok(match f.kind {
        Kind::Text => {
            if s.is_empty() {
                bail!("{key} can't be empty; unset it to use the default");
            }
            Value::from(s)
        }
        Kind::Bool => {
            Value::from(parse_bool(s).with_context(|| format!("{key}: use true or false, not {s:?}"))?)
        }
        Kind::Seconds => {
            let secs = parse_duration(s).map_err(|e| anyhow::anyhow!("{key}: {e}"))?;
            Value::from(i64::try_from(secs).context("duration too long")?)
        }
        Kind::Duration => {
            parse_duration(s).map_err(|e| anyhow::anyhow!("{key}: {e}"))?;
            Value::from(s)
        }
        Kind::Int => {
            let n: u32 = s
                .parse()
                .with_context(|| format!("{key}: use a whole number, not {s:?}"))?;
            Value::from(i64::from(n))
        }
        Kind::Size => Value::from(parse_size(s).with_context(|| key.to_string())?),
        Kind::Choice { options } => {
            let v = s.to_ascii_lowercase();
            if !options.contains(&v.as_str()) {
                bail!("{key}: use one of {}, not {s:?}", options.join(", "));
            }
            Value::from(v)
        }
        Kind::List => {
            let items = s.split(',').map(str::trim).filter(|p| !p.is_empty());
            Value::Array(Array::from_iter(items))
        }
        Kind::Map => {
            let mut t = InlineTable::new();
            for pair in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                let (k, v) = pair
                    .split_once('=')
                    .with_context(|| format!("{key}: write NAME=value pairs, not {pair:?}"))?;
                if k.trim().is_empty() {
                    bail!("{key}: a name is missing in {pair:?}");
                }
                t.insert(k.trim(), Value::from(v.trim()));
            }
            Value::InlineTable(t)
        }
    })
}

fn load_doc(path: &Path) -> Result<(DocumentMut, bool)> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok((
            s.parse()
                .with_context(|| format!("parse {}; fix it in an editor first", path.display()))?,
            true,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((config::TEMPLATE.parse()?, false)),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

fn save(path: &Path, doc: &DocumentMut) -> Result<()> {
    let text = doc.to_string();
    // Never write a file the loader would then reject.
    toml::from_str::<FileConfig>(&text).with_context(|| format!("{} would not parse", path.display()))?;
    // Write through a symlink (dotfiles) rather than replacing it.
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::util::write_atomic(&target, text.as_bytes())
}

/// Sets `key` to `input` in the config file at `path`, creating it from the
/// template when missing. Returns the value as stored, for display.
pub fn set(path: &Path, key: &str, input: &str) -> Result<String> {
    let f = field(key)?;
    let value = parse_value(f, input)?;
    let (mut doc, _) = load_doc(path)?;
    let (table, leaf) = match key.split_once('.') {
        Some((t, leaf)) => (Some(t), leaf),
        None => (None, key),
    };
    let empty = doc.as_table().is_empty();
    let parent: &mut dyn toml_edit::TableLike = match table {
        None => doc.as_table_mut(),
        Some(t) => {
            if !doc.contains_key(t) {
                doc.insert(t, Item::Table(Table::new()));
            }
            doc[t]
                .as_table_like_mut()
                .with_context(|| format!("{t} in {} is not a table", path.display()))?
        }
    };
    match parent.get_mut(leaf) {
        Some(Item::Value(old)) => {
            let decor = old.decor().clone();
            *old = value;
            *old.decor_mut() = decor;
        }
        Some(item @ Item::Table(_)) => {
            // `[env]` style: refill the table so its header and comments stay.
            let (Item::Table(t), Value::InlineTable(it)) = (&mut *item, &value) else {
                *item = Item::Value(value);
                return finish(path, &doc, key, f);
            };
            t.clear();
            for (k, v) in it.iter() {
                t.insert(k, Item::Value(v.clone()));
            }
        }
        _ => {
            parent.insert(leaf, Item::Value(value));
        }
    }
    // A file of comments only (the template) keeps them on top, not below the key.
    if empty && table.is_none() {
        let trailing = doc.trailing().as_str().unwrap_or("").to_string();
        if !trailing.trim().is_empty() {
            doc.set_trailing("");
            if let Some(mut k) = doc.as_table_mut().key_mut(leaf) {
                let sep = if trailing.ends_with("\n\n") { "" } else { "\n" };
                k.leaf_decor_mut().set_prefix(format!("{trailing}{sep}"));
            }
        }
    }
    finish(path, &doc, key, f)
}

fn finish(path: &Path, doc: &DocumentMut, key: &str, f: &Field) -> Result<String> {
    save(path, doc)?;
    let t: toml::Table = toml::from_str(&doc.to_string())?;
    Ok(lookup(&t, key).map(|v| show(f.kind, v)).unwrap_or_default())
}

/// Removes `key` from the config file at `path`. Returns false when it was not
/// set there. An emptied `[refresh]` table goes too.
pub fn unset(path: &Path, key: &str) -> Result<bool> {
    field(key)?;
    let (mut doc, exists) = load_doc(path)?;
    if !exists {
        return Ok(false);
    }
    let removed = match key.split_once('.') {
        None => match doc.as_table_mut().remove_entry(key) {
            Some((k, _)) => {
                let keep = detached(k.leaf_decor().prefix().and_then(|p| p.as_str()).unwrap_or(""));
                rehome(&mut doc, keep);
                true
            }
            None => false,
        },
        Some((t, leaf)) => {
            let Some(tbl) = doc.get_mut(t).and_then(Item::as_table_like_mut) else {
                return Ok(false);
            };
            let removed = tbl.remove(leaf).is_some();
            if removed
                && tbl.is_empty()
                && let Some((_, item)) = doc.as_table_mut().remove_entry(t)
            {
                let prefix = item
                    .as_table()
                    .and_then(|t| t.decor().prefix())
                    .and_then(|p| p.as_str());
                rehome(&mut doc, detached(prefix.unwrap_or("")));
            }
            removed
        }
    };
    if removed {
        save(path, &doc)?;
    }
    Ok(removed)
}

/// The comments above a key that a blank line separates from it: they are
/// about something else (the file header) and outlive the key.
fn detached(prefix: &str) -> String {
    let mut end = 0;
    let mut at = 0;
    for line in prefix.split_inclusive('\n') {
        at += line.len();
        if line.trim().is_empty() {
            end = at;
        }
    }
    let kept = &prefix[..end];
    if kept.contains('#') {
        kept.to_string()
    } else {
        String::new()
    }
}

/// Puts comments left by a removed root key above whatever now comes first.
fn rehome(doc: &mut DocumentMut, text: String) {
    if text.is_empty() {
        return;
    }
    let root = doc.as_table_mut();
    let first_value = root
        .iter()
        .find(|(_, i)| i.is_value())
        .map(|(k, _)| k.to_string());
    if let Some(k) = first_value {
        if let Some(mut key) = root.key_mut(&k) {
            let old = key
                .leaf_decor()
                .prefix()
                .and_then(|p| p.as_str())
                .unwrap_or("")
                .to_string();
            key.leaf_decor_mut().set_prefix(format!("{text}{old}"));
        }
        return;
    }
    let first_table = root
        .iter_mut()
        .filter_map(|(_, i)| i.as_table_mut())
        .min_by_key(|t| t.position());
    match first_table {
        Some(t) => {
            let old = t
                .decor()
                .prefix()
                .and_then(|p| p.as_str())
                .unwrap_or("")
                .to_string();
            t.decor_mut().set_prefix(format!("{text}{old}"));
        }
        None => {
            let old = doc.trailing().as_str().unwrap_or("").to_string();
            doc.set_trailing(format!("{text}{old}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(body: &str) -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.toml");
        std::fs::write(&p, body).unwrap();
        (d, p)
    }

    const FILE: &str = "\
# team settings
base = \"origin/main\" # the trunk

# fetch rarely
fetch_interval = 60

[refresh]
# a day
max_age = \"24h\"

[[step]]
name = \"pnpm\"
warm = \"pnpm install\"
";

    #[test]
    fn set_keeps_comments_and_layout() {
        let (_d, p) = tmp(FILE);
        assert_eq!(set(&p, "base", "origin/dev").unwrap(), "origin/dev");
        assert_eq!(set(&p, "fetch_interval", "5m").unwrap(), "5m");
        set(&p, "refresh.max_age", "2d").unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        assert_eq!(
            out,
            FILE.replace("origin/main", "origin/dev")
                .replace("= 60", "= 300")
                .replace("\"24h\"", "\"2d\"")
        );
    }

    #[test]
    fn set_adds_new_keys_and_tables() {
        let (_d, p) = tmp(FILE);
        set(&p, "exec", "Mise").unwrap();
        set(&p, "refresh.auto", "off").unwrap();
        set(&p, "copy", ".env, apps/*/.env.local").unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        let fc: FileConfig = toml::from_str(&out).unwrap();
        assert_eq!(fc.exec, Some(config::Exec::Mise));
        assert_eq!(fc.refresh.unwrap().auto, Some(false));
        assert_eq!(fc.copy.unwrap(), [".env", "apps/*/.env.local"]);
        assert_eq!(fc.steps.len(), 1, "steps untouched");
        assert!(out.contains("# a day") && out.contains("# the trunk"), "{out}");
        // Root keys stay above the tables.
        assert!(
            out.find("exec =").unwrap() < out.find("[refresh]").unwrap(),
            "{out}"
        );
    }

    #[test]
    fn unset_round_trips() {
        let (_d, p) = tmp(FILE);
        set(&p, "image_size", "50g").unwrap();
        assert!(
            std::fs::read_to_string(&p)
                .unwrap()
                .contains("image_size = \"50G\"")
        );
        assert!(unset(&p, "image_size").unwrap());
        assert!(!unset(&p, "image_size").unwrap(), "already gone");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), FILE);
        assert!(unset(&p, "refresh.max_age").unwrap());
        let out = std::fs::read_to_string(&p).unwrap();
        assert!(!out.contains("[refresh]"), "empty table removed: {out}");
        assert!(out.contains("# the trunk") && out.contains("[[step]]"), "{out}");
    }

    #[test]
    fn missing_file_starts_from_template() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("repos/app.toml");
        assert!(!unset(&p, "base").unwrap());
        assert!(!p.exists(), "unset never creates a file");
        set(&p, "fetch", "false").unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        assert!(out.starts_with("# clove settings"), "{out}");
        assert!(out.trim_end().ends_with("fetch = false"), "{out}");
        set(&p, "refresh.max_behind", "7").unwrap();
        let fc: FileConfig = toml::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(fc.fetch, Some(false));
        assert_eq!(fc.refresh.unwrap().max_behind, Some(7));
        // Unsetting the only key keeps the template's header comments.
        assert!(unset(&p, "fetch").unwrap());
        let out = std::fs::read_to_string(&p).unwrap();
        assert!(out.starts_with("# clove settings"), "{out}");
        assert!(!out.contains("fetch ="), "{out}");
        assert!(unset(&p, "refresh.max_behind").unwrap());
        assert_eq!(
            std::fs::read_to_string(&p).unwrap().trim(),
            config::TEMPLATE.trim()
        );
    }

    #[test]
    fn rejects_bad_values_without_writing() {
        let (_d, p) = tmp(FILE);
        for (k, v) in [
            ("exec", "nix"),
            ("fetch", "maybe"),
            ("refresh.max_age", "soon"),
            ("fetch_interval", "-1"),
            ("refresh.max_behind", "lots"),
            ("image_size", "300X"),
            ("image_size", "big"),
            ("base", "  "),
            ("nope", "1"),
        ] {
            let e = set(&p, k, v).unwrap_err();
            assert!(!format!("{e:#}").is_empty());
        }
        assert_eq!(std::fs::read_to_string(&p).unwrap(), FILE);
    }

    #[test]
    fn broken_file_is_left_alone() {
        let (_d, p) = tmp("base = \n");
        assert!(set(&p, "fetch", "true").is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "base = \n");
    }

    #[test]
    fn writes_through_symlinks() {
        let (d, real) = tmp("fetch = true\n");
        let link = d.path().join("link.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        set(&link, "fetch", "false").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "fetch = false\n");
    }

    #[test]
    fn states_pick_the_highest_layer() {
        let team: toml::Table =
            toml::from_str("fetch = false\nenv = { A = \"1\" }\n[refresh]\nmax_behind = 5\n").unwrap();
        let you: toml::Table = toml::from_str("fetch = true\nfetch_interval = 600\n").unwrap();
        let cfg = Config {
            base: "origin/trunk".into(),
            clone_root: "/c".into(),
            image_size: "300G".into(),
            fetch: true,
            fetch_interval: 300,
            exec: config::Exec::Auto,
            env: Default::default(),
            steps: vec![],
            steps_detected: true,
            copy: None,
            refresh: config::RefreshPolicy {
                auto: true,
                max_age_secs: 1,
                max_behind: 1,
            },
            sources: vec![],
        };
        let st = resolve(&[None, Some(team), Some(you)], &cfg);
        let get = |k: &str| st.iter().find(|s| s.field.key == k).unwrap();
        let fetch = get("fetch");
        assert_eq!(fetch.layers, [None, Some("false".into()), Some("true".into())]);
        assert_eq!(
            (fetch.effective.as_str(), fetch.source),
            ("true", Some(Layer::You))
        );
        assert_eq!(get("fetch_interval").effective, "10m");
        assert_eq!(get("env").effective, "A=1");
        let behind = get("refresh.max_behind");
        assert_eq!(
            (behind.effective.as_str(), behind.source),
            ("5", Some(Layer::Team))
        );
        let base = get("base");
        assert_eq!((base.effective.as_str(), base.source), ("origin/trunk", None));
        assert_eq!(get("image_size").effective, "300G");
    }

    #[test]
    fn maps_round_trip() {
        let (_d, p) = tmp("[env]\n# keep me\nOLD = \"x\"\n");
        assert_eq!(set(&p, "env", "A=1, B = two").unwrap(), "A=1, B=two");
        let out = std::fs::read_to_string(&p).unwrap();
        assert!(out.starts_with("[env]") && !out.contains("OLD"), "{out}");
        assert!(set(&p, "env", "nope").is_err());
        let (_d2, p2) = tmp("");
        set(&p2, "env", "K=v").unwrap();
        let fc: FileConfig = toml::from_str(&std::fs::read_to_string(&p2).unwrap()).unwrap();
        assert_eq!(fc.env["K"], "v");
    }

    #[test]
    fn formats() {
        assert_eq!(fmt_secs(300), "5m");
        assert_eq!(fmt_secs(90), "90s");
        assert_eq!(fmt_secs(7200), "2h");
        assert_eq!(parse_size("300GiB").unwrap(), "300G");
        assert_eq!(Layer::Team.label(), "team");
        assert_eq!(Layer::parse("you").unwrap(), Layer::You);
    }
}
