//! Finding and invoking git: the system `git`, or the copy inside GitHub Desktop.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("git")];
    if cfg!(target_os = "macos") {
        candidates.push("/Applications/GitHub Desktop.app/Contents/Resources/app/git/bin/git".into());
    }
    if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let desktop = PathBuf::from(local).join("GitHubDesktop");
            let mut versions: Vec<PathBuf> = fs::read_dir(&desktop)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("app-"))
                })
                .collect();
            versions.sort();
            versions.reverse(); // newest app-<version> first
            candidates.extend(
                versions
                    .into_iter()
                    .map(|dir| dir.join("resources/app/git/cmd/git.exe")),
            );
        }
    }
    candidates
}

/// A git command that never prompts and, on Windows, opens no console window.
pub fn git_command(git: &Path) -> Command {
    let mut command = Command::new(git);
    command.env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

pub fn find_git() -> Option<PathBuf> {
    git_candidates().into_iter().find(|candidate| {
        git_command(candidate)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    })
}

/// [`find_git`], or an error telling the editor how to install git.
pub fn require_git() -> Result<PathBuf, String> {
    find_git().ok_or_else(|| {
        "Git isn't installed. On a Mac, run any `git` command in Terminal to install it; on Windows, install Git for Windows."
            .to_string()
    })
}
