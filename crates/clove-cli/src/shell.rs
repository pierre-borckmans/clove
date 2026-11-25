//! Shell integration: a `clove` function that can change the shell's directory,
//! plus dynamic completions. Install with `eval "$(clove shell-init zsh)"`.

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
}

pub fn init(shell: Shell) -> String {
    match shell {
        Shell::Zsh | Shell::Bash => {
            let complete = if matches!(shell, Shell::Zsh) {
                "zsh"
            } else {
                "bash"
            };
            // zsh completion needs compinit; skip quietly where it is not loaded.
            let completion = if matches!(shell, Shell::Zsh) {
                "if (( $+functions[compdef] )); then source <(COMPLETE=zsh command clove); fi".to_string()
            } else {
                format!("source <(COMPLETE={complete} command clove)")
            };
            format!(
                r#"# clove shell integration
clove() {{
  local __clove_cd
  __clove_cd="$(mktemp "${{TMPDIR:-/tmp}}/clove-cd.XXXXXX")" || return
  CLOVE_CD_FILE="$__clove_cd" command clove "$@"
  local __clove_rc=$?
  if [ -s "$__clove_cd" ]; then builtin cd -- "$(cat "$__clove_cd")" || __clove_rc=$?; fi
  if [ -s "$__clove_cd.then" ]; then command clove $(cat "$__clove_cd.then") || __clove_rc=$?; fi
  command rm -f "$__clove_cd" "$__clove_cd.then"
  return $__clove_rc
}}
{completion}
"#
            )
        }
        Shell::Fish => r#"# clove shell integration
function clove
    set -l __clove_cd (mktemp (set -q TMPDIR; and echo $TMPDIR; or echo /tmp)/clove-cd.XXXXXX); or return
    CLOVE_CD_FILE=$__clove_cd command clove $argv
    set -l __clove_rc $status
    if test -s $__clove_cd
        builtin cd (cat $__clove_cd)
    end
    if test -s $__clove_cd.then
        command clove (cat $__clove_cd.then | string split ' '); or set __clove_rc $status
    end
    command rm -f $__clove_cd $__clove_cd.then
    return $__clove_rc
end
COMPLETE=fish command clove | source
"#
        .to_string(),
    }
}

/// Asks the wrapping shell function to cd. Returns false without integration.
pub fn request_cd(path: &std::path::Path) -> bool {
    match std::env::var_os("CLOVE_CD_FILE") {
        Some(f) => std::fs::write(f, path.as_os_str().as_encoded_bytes()).is_ok(),
        None => false,
    }
}

/// Asks the wrapper to cd to `path`, then run `clove <args>` from there.
/// Used to remove the clone the shell is standing in.
pub fn request_cd_then(path: &std::path::Path, args: &[String]) -> bool {
    let Some(f) = std::env::var_os("CLOVE_CD_FILE") else {
        return false;
    };
    let mut then = std::path::PathBuf::from(&f).into_os_string();
    then.push(".then");
    std::fs::write(&f, path.as_os_str().as_encoded_bytes()).is_ok()
        && std::fs::write(then, args.join(" ")).is_ok()
}

/// The user's shell, from $SHELL.
pub fn detect() -> Option<Shell> {
    match clove_core::shellrc::detect()? {
        "zsh" => Some(Shell::Zsh),
        "bash" => Some(Shell::Bash),
        _ => Some(Shell::Fish),
    }
}

fn name(shell: Shell) -> &'static str {
    match shell {
        Shell::Zsh => "zsh",
        Shell::Bash => "bash",
        Shell::Fish => "fish",
    }
}

pub fn rc_line(shell: Shell) -> &'static str {
    clove_core::shellrc::line(name(shell))
}

pub fn rc_path(shell: Shell) -> std::path::PathBuf {
    clove_core::shellrc::path(name(shell))
}

pub fn install(shell: Shell) -> anyhow::Result<(std::path::PathBuf, bool)> {
    clove_core::shellrc::install(name(shell))
}

/// Whether this process runs inside the shell function (so it can cd).
pub fn active() -> bool {
    std::env::var_os("CLOVE_CD_FILE").is_some()
}

/// One line telling the user how to turn the integration on.
pub const HINT: &str = "run `clove shell-init --install` (then open a new terminal) so clove can cd for you";
