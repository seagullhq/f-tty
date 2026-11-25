//! Shell integration: starts bash and zsh with OSC 133 marks already wired
//! in (prompt start and command end), leaving the user's own config intact.

use portable_pty::CommandBuilder;

#[cfg(unix)]
const BASH_RC: &str = r#"[ -f ~/.bashrc ] && . ~/.bashrc
__ftty_prompt() {
  printf '\e]133;D\a'
  [[ $PWD == *[[:cntrl:]]* ]] || printf '\e]7;file://%s%s\a' "$HOSTNAME" "${PWD//\%/%25}"
  case $PS1 in *133\;A*) ;; *) PS1='\[\e]133;A\a\]'$PS1 ;; esac
}
PROMPT_COMMAND="__ftty_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
"#;

// zsh reads $ZDOTDIR/.zshenv first: restore the user's ZDOTDIR, source their
// .zshenv, then hook precmd, which runs after their .zshrc set the prompt.
#[cfg(unix)]
const ZSHENV: &str = r#"export ZDOTDIR="${FTTY_ZDOTDIR:-$HOME}"
[[ -f "$ZDOTDIR/.zshenv" ]] && source "$ZDOTDIR/.zshenv"
autoload -Uz add-zsh-hook
_ftty_prompt() {
  print -n '\e]133;D\a'
  [[ $PWD == *[[:cntrl:]]* ]] || printf '\e]7;file://%s%s\a' "$HOST" "${PWD//\%/%25}"
  [[ $PS1 == *'133;A'* ]] || PS1=$'%{\e]133;A\a%}'$PS1
}
add-zsh-hook precmd _ftty_prompt
"#;

#[cfg(unix)]
pub(super) fn apply(cmd: &mut CommandBuilder, shell: &str) {
    let name = std::path::Path::new(shell)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let Some(dir) = crate::app::config_dir().map(|d| d.join("shell")) else {
        return;
    };
    let write = |file: &str, text: &str| -> Option<std::path::PathBuf> {
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join(file);
        (std::fs::read_to_string(&path).ok().as_deref() == Some(text)
            || std::fs::write(&path, text).is_ok())
        .then_some(path)
    };
    match name {
        "bash" => {
            if let Some(rc) = write("bashrc", BASH_RC) {
                cmd.arg("--rcfile");
                cmd.arg(rc);
            }
        }
        "zsh" => {
            if let Some(path) = write(".zshenv", ZSHENV) {
                let original = std::env::var_os("ZDOTDIR").unwrap_or_default();
                cmd.env("FTTY_ZDOTDIR", original);
                cmd.env("ZDOTDIR", path.parent().unwrap_or(&dir));
            }
        }
        _ => {}
    }
}

// ponytail: PowerShell and others still use the README snippet.
#[cfg(not(unix))]
pub(super) fn apply(_cmd: &mut CommandBuilder, _shell: &str) {}
