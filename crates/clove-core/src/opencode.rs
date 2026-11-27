//! Worktree lifecycle for OpenCode's native clove strategy.

use crate::{Clove, NewOptions, RemoveOptions, Reporter, git, history::By};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub directory: PathBuf,
    #[serde(rename = "type")]
    pub kind: &'static str,
}

/// Best-effort probe for hook registration, not for worktree operations.
pub fn managed_repo(dir: &Path) -> Option<Clove> {
    Clove::discover(dir)
        .ok()
        .filter(|c| c.record().ok().flatten().is_some())
}

pub fn create_worktree(
    source: &Path,
    directory: &Path,
    start: Option<&str>,
    request_id: Option<&str>,
    r: &dyn Reporter,
) -> Result<PathBuf> {
    let c = Clove::discover(source)?;
    c.require_record()?;
    let base = resolve_start_base(&c, source, start)?;
    let name = match choose_branch(&c, directory)? {
        Some(name) => name,
        None => {
            let generated = crate::names::generate(|n| {
                !c.clone_dir(n).exists() && !git::ref_exists(&c.root, &format!("refs/heads/{n}"))
            });
            choose_branch(&c, Path::new(&generated))?.context("could not name OpenCode clone")?
        }
    };
    let info = c.new_clone_with_request(
        &NewOptions {
            branch: Some(name.clone()),
            name: Some(name),
            base,
            ..Default::default()
        },
        request_id,
        r,
    )?;
    Ok(info.path.canonicalize().unwrap_or(info.path))
}

fn choose_branch(c: &Clove, directory: &Path) -> Result<Option<String>> {
    let name = directory.file_name().and_then(|p| p.to_str()).unwrap_or_default();
    let base = crate::repo::sanitize_name(name).replace("..", "-");
    if base.is_empty() {
        return Ok(None);
    }
    for n in 1.. {
        let branch = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        if c.clone_meta(&branch)?.is_some()
            || c.clone_dir(&branch).exists()
            || c.clone_admin(&branch).exists()
        {
            continue;
        }
        let local = format!("refs/heads/{branch}");
        if !git::ok(&c.root, &["check-ref-format", "--branch", &branch])
            || git::ref_exists(&c.root, &local)
            // A branch namespace (e.g. feature/login) reserves its parent.
            || !git::out(&c.root, &["for-each-ref", "--count=1", "--format=%(refname)", &local])?.is_empty()
            // clove new would check out this remote branch instead of using the requested base.
            || git::ref_exists(&c.root, &format!("refs/remotes/origin/{branch}"))
        {
            continue;
        }
        return Ok(Some(branch));
    }
    unreachable!()
}

fn resolve_start_base(c: &Clove, source: &Path, start: Option<&str>) -> Result<Option<String>> {
    start
        .or_else(|| (source.canonicalize().ok().as_deref() != Some(c.root.as_path())).then_some("HEAD"))
        .map(|start| resolve_base(source, start))
        .transpose()
}

fn resolve_base(source: &Path, start: &str) -> Result<String> {
    let commit = git::out(
        source,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{start}^{{commit}}"),
        ],
    )
    .with_context(|| format!("OpenCode starting ref `{start}` does not resolve to a commit"))?;
    // Relative revisions depend on the source checkout. Keep named refs for
    // future clove updates, but pin checkout-relative revisions to a commit.
    let named = !matches!(start, "HEAD" | "@")
        && !start.contains("@{")
        && !start.starts_with('-')
        && git::out(
            source,
            &[
                "rev-parse",
                "--verify",
                "--symbolic-full-name",
                "--end-of-options",
                start,
            ],
        )
        .is_ok_and(|r| r.starts_with("refs/"));
    Ok(if named { start.to_string() } else { commit })
}

pub fn remove_worktree(project: &Path, directory: &Path, force: bool, r: &dyn Reporter) -> Result<()> {
    let c = Clove::discover(project)?;
    remove_from_repo(&c, project, directory, force, r)
}

fn remove_from_repo(
    c: &Clove,
    project: &Path,
    directory: &Path,
    force: bool,
    r: &dyn Reporter,
) -> Result<()> {
    let name = if c.record()?.is_some() {
        find_clone(c, directory)?
    } else {
        None
    };
    let canonical = if directory.exists() {
        let path = directory.canonicalize()?;
        match git::common_dir(&path) {
            Ok(actual) if actual.canonicalize()? != c.common_dir.canonicalize()? => {
                bail!(
                    "refusing to remove {}: belongs to a different repository",
                    directory.display()
                );
            }
            Err(error) if name.is_none() => {
                return Err(error)
                    .with_context(|| format!("identify repository for {}", directory.display()));
            }
            // A registered clone may be unmounted or broken, without a live Git checkout.
            _ => {}
        }
        Some(path)
    } else {
        None
    };
    if let Some(name) = name {
        if !force {
            c.require_clean_clone(&name)?;
        }
        c.remove(
            &name,
            &RemoveOptions {
                force,
                delete_branch: false,
                by: By::OpencodeHook,
                ..Default::default()
            },
            r,
        )?;
    } else if let Some(path) = canonical {
        let mut cmd = git::cmd(project);
        cmd.args(["worktree", "remove"]);
        if force {
            cmd.arg("--force");
        }
        crate::util::output(cmd.arg("--").arg(path)).context("git worktree remove")?;
    }
    Ok(())
}

/// Resolve aliases in the existing part of a path, even when an unmounted
/// checkout (and possibly its mount point) is absent.
fn canonicalize_existing_prefix(path: &Path) -> Option<PathBuf> {
    let path = std::path::absolute(path).ok()?;
    path.ancestors().find_map(|parent| {
        let canonical = parent.canonicalize().ok()?;
        Some(canonical.join(path.strip_prefix(parent).ok()?))
    })
}

fn find_clone(c: &Clove, path: &Path) -> Result<Option<String>> {
    let canonical = canonicalize_existing_prefix(path);
    for name in c.clone_names()? {
        if let Some(meta) = c.clone_meta(&name)?
            && (path == meta.path
                || canonical
                    .as_ref()
                    .is_some_and(|p| canonicalize_existing_prefix(&meta.path).as_ref() == Some(p)))
        {
            return Ok(Some(name));
        }
    }
    Ok(None)
}

pub fn list_worktrees(source: &Path) -> Result<Vec<WorktreeEntry>> {
    let c = Clove::discover(source)?;
    list_for_repo(&c)
}

fn list_for_repo(c: &Clove) -> Result<Vec<WorktreeEntry>> {
    if c.record()?.is_none() {
        return Ok(vec![]);
    }
    let mut entries = vec![WorktreeEntry {
        directory: c.root.clone(),
        kind: "root",
    }];
    for clone in c.list(false)? {
        if clone.mounted && clone.path.exists() {
            entries.push(WorktreeEntry {
                directory: clone.path.canonicalize().unwrap_or(clone.path),
                kind: "worktree",
            });
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CloneMeta, Silent, backend::Backend, repo::RepoRecord, repo::write_json};

    struct TestRepo {
        root: PathBuf,
        _temp: tempfile::TempDir,
    }

    impl TestRepo {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().canonicalize().unwrap().join("repo");
            std::fs::create_dir(&root).unwrap();
            let repo = Self { root, _temp: temp };
            repo.git(&["init", "--initial-branch=main", "--template="]);
            repo.commit();
            repo
        }

        fn git(&self, args: &[&str]) {
            git::run_ok(&self.root, args).unwrap();
        }

        fn commit(&self) {
            self.git(&[
                "-c",
                "user.name=clove test",
                "-c",
                "user.email=clove@example.test",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--allow-empty",
                "-m",
                "test",
            ]);
        }

        fn add_plain(&self, name: &str, start: &str) -> PathBuf {
            self.git(&["worktree", "add", "--detach", "--", name, start]);
            self.root.join(name)
        }

        fn managed_clove(&self) -> Clove {
            let mut c = Clove::open(&self.root).unwrap();
            c.state = self.root.with_file_name("state");
            c.config.clone_root = self.root.with_file_name("mounts");
            c.save_record(&RepoRecord {
                root: c.root.clone(),
                name: c.name.clone(),
                backend: if cfg!(target_os = "macos") {
                    Backend::Sparse
                } else {
                    Backend::Btrfs
                },
                golden_admin: String::new(),
                created_at: 0,
            })
            .unwrap();
            c
        }
    }

    fn record_clone(c: &Clove, name: &str, mount: &Path, branch: Option<&str>) {
        write_json(
            &c.clone_dir(name).join("meta.json"),
            &CloneMeta {
                name: name.into(),
                branch: branch.map(str::to_string),
                path: mount.join(&c.name),
                mount: mount.into(),
                base: "main".into(),
                golden_commit: String::new(),
                golden_generation: 0,
                created_at: 0,
                timings: vec![],
                total_ms: 0.0,
                failed_steps: vec![],
                setup_commit: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn names_avoid_clones_branches_and_plain_git_registrations() {
        let r = TestRepo::new();
        let c = r.managed_clove();
        std::fs::create_dir_all(c.clone_dir("new-feature")).unwrap();
        r.git(&["branch", "new-feature-2"]);
        r.git(&["branch", "new-feature-3/nested"]);
        r.git(&["update-ref", "refs/remotes/origin/new-feature-4", "HEAD"]);
        let plain = r.add_plain("clove-new-feature-5", "HEAD");
        assert_eq!(
            choose_branch(&c, Path::new("/worktrees/new feature")).unwrap(),
            Some("new-feature-6".into())
        );
        assert_eq!(choose_branch(&c, Path::new("/worktrees/!!!")).unwrap(), None);
        let err = c
            .new_clone(
                &NewOptions {
                    name: Some("new-feature-5".into()),
                    ..Default::default()
                },
                &Silent,
            )
            .unwrap_err();
        assert!(err.to_string().contains("Git worktree registration"));
        assert!(git::head_commit(&plain).is_ok());
    }

    #[test]
    fn start_refs_resolve_in_the_linked_source_checkout() {
        let r = TestRepo::new();
        let first = git::head_commit(&r.root).unwrap();
        r.commit();
        let second = git::head_commit(&r.root).unwrap();
        let source = r.add_plain("linked", &second);
        r.commit();
        r.git(&["update-ref", "refs/remotes/origin/main", &first]);

        for named in ["main", "origin/main", "refs/heads/main"] {
            assert_eq!(resolve_base(&source, named).unwrap(), named);
        }
        let c = r.managed_clove();
        assert_eq!(resolve_start_base(&c, &r.root, None).unwrap(), None);
        assert_eq!(
            resolve_start_base(&c, &source, None).unwrap(),
            Some(second.clone())
        );
        assert_eq!(resolve_base(&source, "HEAD").unwrap(), second);
        assert_eq!(resolve_base(&source, "HEAD~1").unwrap(), first);
        r.git(&["branch", "source-previous", &second]);
        git::run_ok(&source, &["switch", "source-previous"]).unwrap();
        git::run_ok(&source, &["switch", "--detach", &first]).unwrap();
        assert_eq!(resolve_base(&source, "@{-1}").unwrap(), second);
        assert!(resolve_base(&source, "no-such-ref").is_err());
    }

    #[test]
    fn unmounted_aliased_clone_requires_force_when_the_image_is_missing() {
        let r = TestRepo::new();
        let c = r.managed_clove();
        r.git(&["switch", "--detach"]);
        r.commit();
        r.git(&["branch", "user-feature"]);
        r.git(&["switch", "main"]);
        assert!(!git::ok(
            &r.root,
            &["merge-base", "--is-ancestor", "user-feature", "main"]
        ));
        std::fs::create_dir(&c.config.clone_root).unwrap();
        let alias = r.root.join("alias");
        std::os::unix::fs::symlink(&c.config.clone_root, &alias).unwrap();
        record_clone(&c, "clone", &alias.join("clone"), Some("user-feature"));
        let path = c.clone_worktree("clone");
        assert!(!path.exists());
        assert_eq!(find_clone(&c, &path).unwrap().as_deref(), Some("clone"));
        let err = remove_from_repo(&c, &r.root, &path, false, &Silent).unwrap_err();
        assert!(err.to_string().contains("cannot safely inspect"));
        assert!(!format!("{err:#}").contains("uncommitted changes"));
        assert!(c.clone_meta("clone").unwrap().is_some());
        remove_from_repo(&c, &r.root, &path, true, &Silent).unwrap();
        assert!(c.clone_meta("clone").unwrap().is_none());
        assert!(git::ref_exists(&c.root, "refs/heads/user-feature"));
        let removed = c.history().remove(0);
        assert_eq!(removed.by, By::OpencodeHook);
        assert!(!removed.branch_deleted);
    }

    #[test]
    fn cross_repo_worktree_removal_is_refused_for_managed_and_unmanaged_projects() {
        let a = TestRepo::new();
        let b = TestRepo::new();
        let plain = b.add_plain("linked", "HEAD");
        for result in [
            remove_worktree(&a.root, &plain, true, &Silent),
            remove_from_repo(&a.managed_clove(), &a.root, &plain, true, &Silent),
        ] {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("belongs to a different repository")
            );
            assert!(plain.exists());
        }

        std::fs::write(plain.join("dirty"), "keep until forced").unwrap();
        assert!(remove_worktree(&b.root, &plain, false, &Silent).is_err());
        assert!(plain.exists());
        remove_worktree(&b.root, &plain, true, &Silent).unwrap();
        assert!(!plain.exists());
    }

    #[test]
    fn abort_removes_a_completed_request_but_preserves_branches_and_other_clones() {
        let r = TestRepo::new();
        let c = r.managed_clove();
        for (name, request) in [("finished", "cancel-me"), ("other", "leave-me")] {
            r.git(&["branch", name]);
            record_clone(&c, name, &c.clone_mount(name), Some(name));
            std::fs::write(c.clone_dir(name).join(".request"), request).unwrap();
        }
        let interrupted_plain = r.add_plain("clove-interrupted", "HEAD");
        let finished_plain = r.add_plain("clove-finished", "HEAD");
        std::fs::create_dir(c.clone_dir("interrupted")).unwrap();
        std::fs::write(c.clone_dir("interrupted").join(".request"), "interrupted-request").unwrap();
        for request in ["interrupted-request", "cancel-me"] {
            c.abort_clone_by_request(request, By::OpencodeHook, &Silent)
                .unwrap();
        }
        assert!(!c.clone_dir("interrupted").exists());
        assert!(!c.clone_dir("finished").exists());
        assert!(c.clone_meta("other").unwrap().is_some());
        for (name, plain) in [("interrupted", interrupted_plain), ("finished", finished_plain)] {
            assert!(c.clone_admin(name).is_dir());
            assert!(git::head_commit(&plain).is_ok());
        }
        assert!(git::ref_exists(&c.root, "refs/heads/finished"));
        assert!(git::ref_exists(&c.root, "refs/heads/other"));
        assert_eq!(c.history()[0].by, By::OpencodeHook);
    }

    #[test]
    fn inventory_skips_unmanaged_repos_plain_worktrees_and_unmounted_clones() {
        let r = TestRepo::new();
        let plain = r.add_plain("ordinary", "HEAD");
        assert!(list_worktrees(&plain).unwrap().is_empty());
        assert!(
            create_worktree(&r.root, &r.root.join("requested"), None, None, &Silent)
                .unwrap_err()
                .to_string()
                .contains("has no golden image")
        );
        assert!(!r.root.join("requested").exists());

        let c = r.managed_clove();
        let mount = c.clone_mount("clone");
        std::fs::create_dir_all(mount.join(&c.name)).unwrap();
        record_clone(&c, "clone", &mount, None);
        assert_eq!(
            list_for_repo(&c).unwrap(),
            [WorktreeEntry {
                directory: r.root,
                kind: "root",
            }]
        );
    }
}
