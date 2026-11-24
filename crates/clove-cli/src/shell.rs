//! Shell integration: a `clove` function that can change the shell's directory,
//! plus dynamic completions. Install with `eval "$(clove shell-init zsh)"`.

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, ValueEnum)]
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
            format!(
                r#"# clove shell integration
clove() {{
  local __clove_cd
  __clove_cd="$(mktemp -t clove-cd.XXXXXX)" || return
  CLOVE_CD_FILE="$__clove_cd" command clove "$@"
  local __clove_rc=$?
  if [ -s "$__clove_cd" ]; then builtin cd -- "$(cat "$__clove_cd")" || __clove_rc=$?; fi
  command rm -f "$__clove_cd"
  return $__clove_rc
}}
source <(COMPLETE={complete} command clove)
"#
            )
        }
        Shell::Fish => r#"# clove shell integration
function clove
    set -l __clove_cd (mktemp -t clove-cd.XXXXXX); or return
    CLOVE_CD_FILE=$__clove_cd command clove $argv
    set -l __clove_rc $status
    if test -s $__clove_cd
        builtin cd (cat $__clove_cd)
    end
    command rm -f $__clove_cd
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
