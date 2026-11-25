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
            let token = clove_core::repo::env_token().unwrap_or_default();
            let hook = if matches!(shell, Shell::Zsh) {
                "autoload -Uz add-zsh-hook && add-zsh-hook chpwd __clove_env"
            } else {
                r#"case ";${PROMPT_COMMAND:-};" in *";__clove_env;"*) ;; *) PROMPT_COMMAND="__clove_env${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;; esac"#
            };
            format!(
                r##"# clove shell integration
# Inside a clone, load the environment clove wrote next to its worktree
# (only files carrying this token), and put things back on the way out.
__clove_env() {{
  local d="$PWD" f="" line k v first
  while :; do
    if [ -f "$d/.clove-env" ]; then f="$d/.clove-env"; break; fi
    [ "$d" = / ] && break
    d="${{d%/*}}"; [ -z "$d" ] && d=/
  done
  if [ -n "$f" ]; then
    IFS= read -r first < "$f" || first=""
    [ "$first" = "#clove-env {token}" ] || f=""
  fi
  [ "$f" = "${{__CLOVE_ENV_FILE-}}" ] && return 0
  if [ -n "${{__CLOVE_ENV_RESTORE-}}" ]; then eval "$__CLOVE_ENV_RESTORE"; fi
  __CLOVE_ENV_RESTORE=""
  __CLOVE_ENV_FILE="$f"
  [ -z "$f" ] && return 0
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in "#"*|"") continue ;; esac
    k="${{line%%=*}}"; v="${{line#*=}}"
    case "$k" in ""|[0-9]*|*[!A-Za-z0-9_]*) continue ;; esac
    if eval "[ -n \"\${{$k+x}}\" ]"; then
      __CLOVE_ENV_RESTORE="$__CLOVE_ENV_RESTORE export $k=$(eval "printf '%q' \"\$$k\"");"
    else
      __CLOVE_ENV_RESTORE="$__CLOVE_ENV_RESTORE unset $k;"
    fi
    export "$k=$v"
  done < "$f"
}}
{hook}
__clove_env
export CLOVE_SHELL={version}
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
"##,
                version = VERSION
            )
        }
        Shell::Fish => format!(
            r##"# clove shell integration
# Inside a clone, load the environment clove wrote next to its worktree
# (only files carrying this token), and put things back on the way out.
function __clove_env --on-variable PWD
    set -l d $PWD
    set -l f
    while true
        if test -f "$d/.clove-env"
            set f "$d/.clove-env"
            break
        end
        test "$d" = /; and break
        set d (string replace -r '/[^/]*$' '' -- $d)
        test -z "$d"; and set d /
    end
    if test -n "$f"; and test (head -n1 "$f") != "#clove-env {token}"
        set f
    end
    test "$f" = "$__clove_env_file"; and return
    for k in $__clove_env_keys
        set -l saved __clove_env_prev_$k
        if set -q $saved
            set -gx $k $$saved
            set -e $saved
        else
            set -e $k
        end
    end
    set -g __clove_env_keys
    set -g __clove_env_file $f
    test -z "$f"; and return
    for line in (cat "$f")
        string match -qr '^#' -- $line; and continue
        set -l k (string split -m1 = -- $line)[1]
        set -l v (string split -m1 = -- $line)[2]
        string match -qr '^[A-Za-z_][A-Za-z0-9_]*$' -- $k; or continue
        if set -q $k
            set -g __clove_env_prev_$k $$k
        end
        set -gx $k $v
        set -a __clove_env_keys $k
    end
end
__clove_env
set -gx CLOVE_SHELL {version}
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
"##,
            token = clove_core::repo::env_token().unwrap_or_default(),
            version = VERSION
        ),
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

/// Bumped when the integration gains behaviour, so a shell started before an
/// upgrade can be told to reload. 2: loads each clone's environment.
pub const VERSION: u32 = 2;

/// The integration is loaded but older than this clove.
pub fn outdated() -> bool {
    active()
        && std::env::var("CLOVE_SHELL")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            != Some(VERSION)
}

/// One line telling the user how to turn the integration on.
pub const HINT: &str = "run `clove shell-init --install` (then open a new terminal) so clove can cd for you";
