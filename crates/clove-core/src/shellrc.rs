//! The shell integration line in the user's rc file: detect the shell, check
//! for the line, add it. The integration itself (`clove shell-init`) lives in
//! the CLI; this is what the CLI, TUI and app share to install it.

use crate::util::home;
use anyhow::Result;
use std::path::PathBuf;

/// "zsh", "bash" or "fish", from $SHELL.
pub fn detect() -> Option<&'static str> {
    let sh = std::env::var("SHELL").ok()?;
    match sh.rsplit('/').next()? {
        "zsh" => Some("zsh"),
        "bash" => Some("bash"),
        "fish" => Some("fish"),
        _ => None,
    }
}

/// The line that loads the integration.
pub fn line(shell: &str) -> &'static str {
    match shell {
        "bash" => r#"eval "$(clove shell-init bash)""#,
        "fish" => "clove shell-init fish | source",
        _ => r#"eval "$(clove shell-init zsh)""#,
    }
}

/// The interactive rc file the line goes in.
pub fn path(shell: &str) -> PathBuf {
    match shell {
        "bash" => home().join(".bashrc"),
        "fish" => std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".config"))
            .join("fish/config.fish"),
        _ => std::env::var_os("ZDOTDIR")
            .map(PathBuf::from)
            .unwrap_or_else(home)
            .join(".zshrc"),
    }
}

/// The rc file already loads the integration (a new terminal will have it).
pub fn installed(shell: &str) -> bool {
    std::fs::read_to_string(path(shell)).is_ok_and(|s| s.contains("clove shell-init"))
}

/// Appends the line to the rc file unless it is already there. Returns the
/// file and whether it changed.
pub fn install(shell: &str) -> Result<(PathBuf, bool)> {
    let path = path(shell);
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current.contains("clove shell-init") {
        return Ok((path, false));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let sep = if current.is_empty() || current.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let block = format!("{sep}\n# clove: cd into clones, completions\n{}\n", line(shell));
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?
        .write_all(block.as_bytes())?;
    Ok((path, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("clove-rc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rc = dir.join(".zshrc");
        std::fs::write(&rc, "export A=1").unwrap();
        // SAFETY: single-threaded test that owns ZDOTDIR for its duration.
        unsafe { std::env::set_var("ZDOTDIR", &dir) };
        let (p, changed) = install("zsh").unwrap();
        assert_eq!(p, rc);
        assert!(changed);
        let (_, again) = install("zsh").unwrap();
        assert!(!again);
        let body = std::fs::read_to_string(&rc).unwrap();
        assert!(body.starts_with("export A=1\n\n# clove"));
        assert_eq!(body.matches("clove shell-init zsh").count(), 1);
        unsafe { std::env::remove_var("ZDOTDIR") };
        let _ = std::fs::remove_dir_all(&dir);
    }
}
