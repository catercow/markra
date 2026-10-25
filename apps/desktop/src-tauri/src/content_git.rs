//! Git for the CaterCow managed content folder: clone, status, save to `studio`,
//! and get latest. The GitHub token reaches git only through GIT_CONFIG_* env
//! (an `http.<github>.extraheader`), never through argv, a URL, the repo's
//! config, or logs.

use std::path::{Path, PathBuf};

use base64::Engine;

use crate::content_preview::git_command;

pub(crate) const STUDIO_BRANCH: &str = "studio";
const REMOTE_REF: &str = "refs/remotes/origin/studio";

/// Where to fetch from and push to, and the env that authenticates it.
pub(crate) struct Remote {
    pub(crate) url: String,
    pub(crate) env: Vec<(String, String)>,
}

pub(crate) struct Author {
    pub(crate) name: String,
    pub(crate) email: String,
}

impl Author {
    pub(crate) fn from_email(email: &str) -> Self {
        let name = email.split('@').next().filter(|name| !name.is_empty()).unwrap_or(email);
        Author { name: name.to_string(), email: email.to_string() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentChange {
    pub(crate) path: String,
    pub(crate) status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentGitStatus {
    pub(crate) branch: String,
    pub(crate) changes: Vec<ContentChange>,
    pub(crate) ahead: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncResult {
    pub(crate) committed: bool,
    pub(crate) head: String,
}

/// GIT_CONFIG_* env that adds the token as a header for github.com only and
/// turns off credential helpers, so git neither prompts nor stores anything.
pub(crate) fn auth_env(token: &str) -> Vec<(String, String)> {
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
    vec![
        ("GIT_CONFIG_COUNT".into(), "2".into()),
        ("GIT_CONFIG_KEY_0".into(), "http.https://github.com/.extraheader".into()),
        ("GIT_CONFIG_VALUE_0".into(), format!("AUTHORIZATION: basic {basic}")),
        ("GIT_CONFIG_KEY_1".into(), "credential.helper".into()),
        ("GIT_CONFIG_VALUE_1".into(), String::new()),
    ]
}

#[cfg(test)]
thread_local! {
    static RECORDED_ARGS: std::cell::RefCell<Vec<Vec<String>>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[derive(Debug)]
struct GitFailure {
    output: String,
}

fn run(git: &Path, dir: &Path, args: &[&str], env: &[(String, String)]) -> Result<String, GitFailure> {
    #[cfg(test)]
    RECORDED_ARGS.with(|log| log.borrow_mut().push(args.iter().map(|arg| arg.to_string()).collect()));

    let output = git_command(git)
        .args(args)
        .current_dir(dir)
        .envs(env.iter().map(|(key, value)| (key.as_str(), value.as_str())))
        .output()
        .map_err(|error| GitFailure { output: error.to_string() })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(GitFailure { output: format!("{stdout}{stderr}").trim().to_string() })
}

fn plain(failure: GitFailure, what: &str) -> String {
    let detail = failure.output.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("").trim();
    if detail.contains("Could not resolve host") || detail.contains("unable to access") {
        return format!("offline: Couldn't reach GitHub to {what}. Check your internet connection.");
    }
    if detail.is_empty() {
        format!("Couldn't {what}.")
    } else {
        format!("Couldn't {what}: {detail}")
    }
}

fn identity(author: &Author) -> [String; 4] {
    [
        "-c".into(),
        format!("user.name={}", author.name),
        "-c".into(),
        format!("user.email={}", author.email),
    ]
}

fn with_identity<'a>(identity: &'a [String; 4], args: &[&'a str]) -> Vec<&'a str> {
    identity.iter().map(String::as_str).chain(args.iter().copied()).collect()
}

pub(crate) fn clone_into(git: &Path, remote: &Remote, dest: &Path) -> Result<(), String> {
    if dest.exists() && dest.read_dir().map(|mut entries| entries.next().is_some()).unwrap_or(true) {
        return Err(format!("{} already has files in it. Choose an empty or new folder.", dest.display()));
    }
    let parent = dest.parent().ok_or("Choose a folder for the content.")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let dest_arg = dest.to_string_lossy();
    // Keep files byte-for-byte as committed (LF), even where git defaults to
    // CRLF checkouts on Windows; the site and the editor expect LF.
    run(
        git,
        parent,
        &["clone", "--config", "core.autocrlf=false", "--branch", STUDIO_BRANCH, &remote.url, &dest_arg],
        &remote.env,
    )
        .map(|_| ())
        .map_err(|failure| plain(failure, "download the content"))
}

pub(crate) fn is_checkout(git: &Path, root: &Path) -> bool {
    root.join(".git").exists() && run(git, root, &["rev-parse", "--is-inside-work-tree"], &[]).is_ok()
}

fn parse_porcelain(output: &str) -> Vec<ContentChange> {
    let mut changes = Vec::new();
    let mut entries = output.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let (code, path) = entry.split_at(3);
        let code = code.trim_end();
        let status = if code == "??" || code.contains('A') {
            "added"
        } else if code.contains('D') {
            "deleted"
        } else if code.contains('R') || code.contains('C') {
            entries.next(); // the original path
            "renamed"
        } else {
            "modified"
        };
        changes.push(ContentChange { path: path.to_string(), status: status.to_string() });
    }
    changes
}

pub(crate) fn status(git: &Path, root: &Path) -> Result<ContentGitStatus, String> {
    let branch = run(git, root, &["rev-parse", "--abbrev-ref", "HEAD"], &[])
        .map_err(|failure| plain(failure, "read the folder's branch"))?
        .trim()
        .to_string();
    let porcelain = run(git, root, &["status", "--porcelain=v1", "-z", "--untracked-files=all"], &[])
        .map_err(|failure| plain(failure, "list your changes"))?;
    let ahead = if run(git, root, &["rev-parse", "--verify", "-q", REMOTE_REF], &[]).is_ok() {
        run(git, root, &["rev-list", "--count", &format!("{REMOTE_REF}..HEAD")], &[])
            .ok()
            .and_then(|count| count.trim().parse().ok())
            .unwrap_or(0)
    } else {
        0
    };
    Ok(ContentGitStatus { branch, changes: parse_porcelain(&porcelain), ahead })
}

fn head(git: &Path, root: &Path) -> String {
    run(git, root, &["rev-parse", "--short", "HEAD"], &[]).map(|head| head.trim().to_string()).unwrap_or_default()
}

fn fetch(git: &Path, root: &Path, remote: &Remote) -> Result<(), String> {
    let refspec = format!("+refs/heads/{STUDIO_BRANCH}:{REMOTE_REF}");
    run(git, root, &["fetch", "--quiet", &remote.url, &refspec], &remote.env)
        .map(|_| ())
        .map_err(|failure| plain(failure, "get the latest content"))
}

/// Rebases onto the fetched studio branch. On a conflict, puts everything back
/// as it was and names the files.
fn rebase(git: &Path, root: &Path, author: &Author, autostash: bool) -> Result<(), String> {
    let identity = identity(author);
    let mut args = vec!["rebase"];
    if autostash {
        args.push("--autostash");
    }
    args.push(REMOTE_REF);
    match run(git, root, &with_identity(&identity, &args), &[]) {
        Ok(output) if output.contains("Applying autostash resulted in conflicts") => Err(
            "conflict: Your unsaved changes overlap with changes someone else saved. They're kept in git's stash; ask a developer for help."
                .into(),
        ),
        Ok(_) => Ok(()),
        Err(failure) => {
            let files = run(git, root, &["diff", "--name-only", "--diff-filter=U"], &[])
                .unwrap_or_default()
                .lines()
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            let _ = run(git, root, &with_identity(&identity, &["rebase", "--abort"]), &[]);
            if files.is_empty() {
                Err(plain(failure, "combine your changes with the latest content"))
            } else {
                Err(format!(
                    "conflict: Someone else changed the same part of {files}. Your changes are still saved on this computer; ask a developer for help."
                ))
            }
        }
    }
}

fn require_studio(git: &Path, root: &Path) -> Result<(), String> {
    let branch = run(git, root, &["rev-parse", "--abbrev-ref", "HEAD"], &[])
        .map_err(|failure| plain(failure, "read the folder's branch"))?;
    if branch.trim() != STUDIO_BRANCH {
        return Err(format!(
            "This folder is on the `{}` branch. Saving only works on `{STUDIO_BRANCH}`; ask a developer to switch it.",
            branch.trim()
        ));
    }
    Ok(())
}

pub(crate) fn save(git: &Path, root: &Path, remote: &Remote, author: &Author, message: &str) -> Result<SyncResult, String> {
    require_studio(git, root)?;
    run(git, root, &["add", "-A"], &[]).map_err(|failure| plain(failure, "stage your changes"))?;
    let committed = run(git, root, &["diff", "--cached", "--quiet"], &[]).is_err();
    if committed {
        let message = if message.trim().is_empty() { "Update content" } else { message.trim() };
        let identity = identity(author);
        run(git, root, &with_identity(&identity, &["commit", "--quiet", "-m", message]), &[])
            .map_err(|failure| plain(failure, "save your changes"))?;
    }

    // Someone may push between our fetch and push; one retry covers that.
    let refspec = format!("HEAD:refs/heads/{STUDIO_BRANCH}");
    let mut attempts = 0;
    loop {
        attempts += 1;
        fetch(git, root, remote)?;
        rebase(git, root, author, false)?;
        match run(git, root, &["push", "--quiet", &remote.url, &refspec], &remote.env) {
            Ok(_) => break,
            Err(failure) if attempts < 2 && (failure.output.contains("rejected") || failure.output.contains("fetch first")) => {
                continue
            }
            Err(failure) => return Err(plain(failure, "send your changes to GitHub")),
        }
    }
    // Keep the tracking ref in step so `ahead` drops to zero.
    let _ = run(git, root, &["update-ref", REMOTE_REF, "HEAD"], &[]);
    Ok(SyncResult { committed, head: head(git, root) })
}

pub(crate) fn update(git: &Path, root: &Path, remote: &Remote, author: &Author) -> Result<SyncResult, String> {
    require_studio(git, root)?;
    fetch(git, root, remote)?;
    rebase(git, root, author, true)?;
    Ok(SyncResult { committed: false, head: head(git, root) })
}

pub(crate) fn default_folder(documents: Option<PathBuf>) -> PathBuf {
    documents
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Documents")))
        .or_else(|| std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("Documents")))
        .unwrap_or_default()
        .join("CaterCow Content")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_preview::find_git;
    use std::fs;

    const SECRET: &str = "ghs_SECRET-TOKEN-123";

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("markra-content-git-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git_ok(git: &Path, dir: &Path, args: &[&str]) -> String {
        run(git, dir, args, &[]).unwrap_or_else(|failure| panic!("git {args:?}: {}", failure.output))
    }

    /// A bare repository standing in for GitHub, with a `studio` branch.
    fn remote_with_studio(git: &Path, base: &Path) -> Remote {
        let bare = base.join("remote.git");
        let seed = base.join("seed");
        fs::create_dir_all(&seed).unwrap();
        git_ok(git, base, &["init", "--quiet", "--bare", "remote.git"]);
        git_ok(git, &seed, &["init", "--quiet", "-b", "studio"]);
        fs::create_dir_all(seed.join("content/blog")).unwrap();
        fs::write(seed.join("content/blog/a.md"), "line one\nline two\n").unwrap();
        git_ok(git, &seed, &["add", "."]);
        git_ok(git, &seed, &["-c", "user.name=Seed", "-c", "user.email=seed@example.com", "commit", "--quiet", "-m", "seed"]);
        git_ok(git, &seed, &["push", "--quiet", &bare.to_string_lossy(), "studio"]);
        Remote { url: bare.to_string_lossy().into_owned(), env: auth_env(SECRET) }
    }

    #[test]
    fn auth_env_carries_the_token_as_a_github_header_only() {
        let env = auth_env(SECRET);
        let value = |key: &str| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(value("GIT_CONFIG_COUNT"), "2");
        assert_eq!(value("GIT_CONFIG_KEY_0"), "http.https://github.com/.extraheader");
        let expected = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{SECRET}"));
        assert_eq!(value("GIT_CONFIG_VALUE_0"), format!("AUTHORIZATION: basic {expected}"));
        assert_eq!(value("GIT_CONFIG_KEY_1"), "credential.helper");
        assert_eq!(value("GIT_CONFIG_VALUE_1"), "");
    }

    #[test]
    fn parses_porcelain_status() {
        let output = " M content/a.md\0?? public/new.png\0 D content/old.md\0R  content/b.md\0content/was.md\0A  content/c.md\0";
        assert_eq!(
            parse_porcelain(output),
            vec![
                ContentChange { path: "content/a.md".into(), status: "modified".into() },
                ContentChange { path: "public/new.png".into(), status: "added".into() },
                ContentChange { path: "content/old.md".into(), status: "deleted".into() },
                ContentChange { path: "content/b.md".into(), status: "renamed".into() },
                ContentChange { path: "content/c.md".into(), status: "added".into() },
            ]
        );
    }

    #[test]
    fn default_folder_is_in_documents() {
        assert_eq!(
            default_folder(Some(PathBuf::from("/Users/a/Documents"))),
            PathBuf::from("/Users/a/Documents/CaterCow Content")
        );
    }

    #[test]
    fn clones_saves_and_updates_without_the_token_in_argv() {
        let Some(git) = find_git() else { return };
        let base = temp("flow");
        let remote = remote_with_studio(&git, &base);
        let mine = base.join("mine");
        let theirs = base.join("theirs");
        let me = Author::from_email("editor@catercow.com");
        RECORDED_ARGS.with(|log| log.borrow_mut().clear());

        clone_into(&git, &remote, &mine).unwrap();
        clone_into(&git, &remote, &theirs).unwrap();
        assert!(is_checkout(&git, &mine));
        assert!(clone_into(&git, &remote, &mine).unwrap_err().contains("already has files"));

        fs::write(mine.join("content/blog/new.md"), "# New\n").unwrap();
        let before = status(&git, &mine).unwrap();
        assert_eq!(before.branch, "studio");
        assert_eq!(before.changes, vec![ContentChange { path: "content/blog/new.md".into(), status: "added".into() }]);

        let saved = save(&git, &mine, &remote, &me, "Add a post").unwrap();
        assert!(saved.committed);
        let after = status(&git, &mine).unwrap();
        assert!(after.changes.is_empty());
        assert_eq!(after.ahead, 0);
        let author = git_ok(&git, &mine, &["log", "-1", "--format=%an <%ae> %s"]);
        assert_eq!(author.trim(), "editor <editor@catercow.com> Add a post");

        // The other clone gets it, keeping an unrelated unsaved edit.
        fs::write(theirs.join("content/blog/a.md"), "line one\nline two\nline three\n").unwrap();
        update(&git, &theirs, &remote, &Author::from_email("other@catercow.com")).unwrap();
        assert!(theirs.join("content/blog/new.md").exists());
        assert!(fs::read_to_string(theirs.join("content/blog/a.md")).unwrap().contains("line three"));

        let recorded = RECORDED_ARGS.with(|log| log.borrow().clone());
        assert!(!recorded.is_empty());
        for args in recorded {
            for arg in args {
                assert!(!arg.contains(SECRET), "token leaked into argv: {arg}");
                assert!(!arg.contains("x-access-token"), "header leaked into argv: {arg}");
            }
        }
        let config = fs::read_to_string(mine.join(".git/config")).unwrap();
        assert!(!config.contains(SECRET) && !config.contains("extraheader"));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_conflicting_save_is_undone_and_names_the_file() {
        let Some(git) = find_git() else { return };
        let base = temp("conflict");
        let remote = remote_with_studio(&git, &base);
        let mine = base.join("mine");
        let theirs = base.join("theirs");
        clone_into(&git, &remote, &mine).unwrap();
        clone_into(&git, &remote, &theirs).unwrap();

        fs::write(theirs.join("content/blog/a.md"), "line one\ntheir line\n").unwrap();
        save(&git, &theirs, &remote, &Author::from_email("other@catercow.com"), "Theirs").unwrap();

        fs::write(mine.join("content/blog/a.md"), "line one\nmy line\n").unwrap();
        let error = save(&git, &mine, &remote, &Author::from_email("editor@catercow.com"), "Mine").unwrap_err();
        assert!(error.starts_with("conflict:"), "{error}");
        assert!(error.contains("content/blog/a.md"), "{error}");

        // Rebase aborted, local commit kept, file as the editor wrote it.
        assert!(!mine.join(".git/rebase-merge").exists() && !mine.join(".git/rebase-apply").exists());
        let subject = git_ok(&git, &mine, &["log", "-1", "--format=%s"]);
        assert_eq!(subject.trim(), "Mine");
        assert_eq!(fs::read_to_string(mine.join("content/blog/a.md")).unwrap(), "line one\nmy line\n");
        assert_eq!(status(&git, &mine).unwrap().ahead, 1);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn saving_requires_the_studio_branch() {
        let Some(git) = find_git() else { return };
        let base = temp("branch");
        let remote = remote_with_studio(&git, &base);
        let mine = base.join("mine");
        clone_into(&git, &remote, &mine).unwrap();
        git_ok(&git, &mine, &["switch", "--quiet", "-c", "other"]);
        let error = save(&git, &mine, &remote, &Author::from_email("e@catercow.com"), "x").unwrap_err();
        assert!(error.contains("`other` branch"), "{error}");
        let _ = fs::remove_dir_all(&base);
    }
}
