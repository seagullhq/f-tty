//! Detects the shells available on the system (Linux, macOS, Windows).

#[cfg(unix)]
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shell {
    /// Name shown in the menu.
    pub name: String,
    /// Program passed to the PTY (absolute path or PATH command).
    pub program: String,
}

impl Shell {
    /// Short tag for tab labels: PS, C:, bsh, zsh...
    pub fn badge(&self) -> String {
        let stem = std::path::Path::new(&self.program)
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        match stem.as_str() {
            "pwsh" | "powershell" => "PS".into(),
            "cmd" => "C:".into(),
            "bash" => "bsh".into(),
            other => other.chars().take(3).collect(),
        }
    }
}

pub fn available() -> Vec<Shell> {
    #[cfg(unix)]
    let shells = unix_shells();
    #[cfg(windows)]
    let shells = windows_shells();
    #[cfg(not(any(unix, windows)))]
    let shells = Vec::new();

    if shells.is_empty() {
        vec![fallback()]
    } else {
        shells
    }
}

pub fn default_shell(shells: &[Shell]) -> Option<Shell> {
    #[cfg(unix)]
    if let Some(program) = std::env::var_os("SHELL") {
        let program = program.to_string_lossy();
        if let Some(shell) = shells.iter().find(|shell| shell.program == program) {
            return Some(shell.clone());
        }
    }

    #[cfg(windows)]
    if let Some(shell) = shells
        .iter()
        .find(|shell| shell.program.to_lowercase().ends_with("powershell.exe"))
    {
        return Some(shell.clone());
    }

    shells.first().cloned()
}

#[cfg(unix)]
fn unix_shells() -> Vec<Shell> {
    let mut programs = Vec::new();

    // Current shell goes first, so it's also the menu default.
    if let Some(shell) = std::env::var_os("SHELL") {
        let program = shell.to_string_lossy().into_owned();
        if !program.is_empty() && Path::new(&program).is_file() {
            programs.push(program);
        }
    }

    if let Ok(contents) = std::fs::read_to_string("/etc/shells") {
        for line in contents.lines() {
            let program = line.trim();
            if !program.is_empty() && !program.starts_with('#') && Path::new(program).is_file() {
                programs.push(program.to_string());
            }
        }
    }

    for program in [
        "/bin/sh",
        "/bin/bash",
        "/bin/zsh",
        "/usr/bin/fish",
        "/usr/bin/pwsh",
    ] {
        if Path::new(program).is_file() {
            programs.push(program.to_string());
        }
    }

    unique(programs)
}

#[cfg(windows)]
fn windows_shells() -> Vec<Shell> {
    let mut shells = Vec::new();

    if let Some(comspec) = std::env::var_os("COMSPEC") {
        shells.push(Shell {
            name: "Command Prompt".into(),
            program: comspec.to_string_lossy().into_owned(),
        });
    } else if let Some(program) = find_in_path("cmd.exe") {
        shells.push(Shell {
            name: "Command Prompt".into(),
            program: program.to_string_lossy().into_owned(),
        });
    }

    if let Some(program) = find_in_path("powershell.exe") {
        shells.push(Shell {
            name: "PowerShell".into(),
            program: program.to_string_lossy().into_owned(),
        });
    }

    if let Some(program) = find_in_path("pwsh.exe") {
        shells.push(Shell {
            name: "PowerShell 7".into(),
            program: program.to_string_lossy().into_owned(),
        });
    }

    shells
}

#[cfg(windows)]
fn find_in_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(unix)]
fn unique(programs: Vec<String>) -> Vec<Shell> {
    let mut seen: Vec<std::path::PathBuf> = Vec::new();
    let mut shells: Vec<Shell> = Vec::new();
    for program in programs {
        // Resolve directory symlinks (/bin -> /usr/bin) but not the file:
        // /bin/sh and /bin/bash stay distinct shells even if the binary is
        // the same (sh starts in POSIX mode).
        let path = Path::new(&program);
        let key = match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => std::fs::canonicalize(parent)
                .unwrap_or_else(|_| parent.to_path_buf())
                .join(name),
            _ => std::path::PathBuf::from(&program),
        };
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&program)
            .to_string();
        shells.push(Shell { name, program });
    }
    shells
}

fn fallback() -> Shell {
    #[cfg(windows)]
    {
        Shell {
            name: "PowerShell".into(),
            program: "powershell.exe".into(),
        }
    }

    #[cfg(unix)]
    {
        Shell {
            name: "sh".into(),
            program: "/bin/sh".into(),
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        Shell {
            name: "sh".into(),
            program: "sh".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn detects_at_least_one_shell() {
        let shells = available();
        assert!(!shells.is_empty());
        assert!(
            shells
                .iter()
                .all(|shell| !shell.name.is_empty() && !shell.program.is_empty())
        );
        assert!(default_shell(&shells).is_some());
    }
}
