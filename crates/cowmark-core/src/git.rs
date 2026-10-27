//! Git for the CaterCow managed content folder: clone, status, save to `studio`,
//! and get latest. The GitHub token reaches git only through GIT_CONFIG_* env
//! (an `http.<github>.extraheader`), never through argv, a URL, the repo's
//! config, or logs.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use base64::Engine;

use crate::git_tool::git_command;
use crate::merge::{has_conflict_markers, merge_text};

pub const STUDIO_BRANCH: &str = "studio";
const REMOTE_REF: &str = "refs/remotes/origin/studio";
/// Every combine first records the whole working tree here (one reflog entry
/// per combine), so a developer can recover anything a combine got wrong.
pub const BACKUP_REF: &str = "refs/cowmark/backup";
/// Follows a `conflict:` message: a JSON array of the files that need resolving.
pub const CONFLICT_FILES_PREFIX: &str = "\nconflict-files: ";

/// Where to fetch from and push to, and the env that authenticates it.
pub struct Remote {
    pub url: String,
    pub env: Vec<(String, String)>,
}

pub struct Author {
    pub name: String,
    pub email: String,
}

impl Author {
    pub fn from_email(email: &str) -> Self {
        let name = email.split('@').next().filter(|name| !name.is_empty()).unwrap_or(email);
        Author { name: name.to_string(), email: email.to_string() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentChange {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentGitStatus {
    pub branch: String,
    pub changes: Vec<ContentChange>,
    pub ahead: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub committed: bool,
    pub head: String,
}

/// GIT_CONFIG_* env that adds the token as a header for github.com only and
/// turns off credential helpers, so git neither prompts nor stores anything.
pub fn auth_env(token: &str) -> Vec<(String, String)> {
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

fn run_bytes(git: &Path, dir: &Path, args: &[&str], env: &[(String, String)]) -> Result<Vec<u8>, GitFailure> {
    #[cfg(test)]
    RECORDED_ARGS.with(|log| log.borrow_mut().push(args.iter().map(|arg| arg.to_string()).collect()));

    let output = git_command(git)
        .args(args)
        .current_dir(dir)
        .envs(env.iter().map(|(key, value)| (key.as_str(), value.as_str())))
        .output()
        .map_err(|error| GitFailure { output: error.to_string() })?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(GitFailure { output: format!("{stdout}{stderr}").trim().to_string() })
}

fn run(git: &Path, dir: &Path, args: &[&str], env: &[(String, String)]) -> Result<String, GitFailure> {
    run_bytes(git, dir, args, env).map(|stdout| String::from_utf8_lossy(&stdout).into_owned())
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

pub fn clone_into(git: &Path, remote: &Remote, dest: &Path) -> Result<(), String> {
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

pub fn is_checkout(git: &Path, root: &Path) -> bool {
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

pub fn status(git: &Path, root: &Path) -> Result<ContentGitStatus, String> {
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

/// "a", "a and b", "a, b and c".
fn file_list(files: &[String]) -> String {
    match files {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn conflict_error(message: String, files: &[String]) -> String {
    let list = serde_json::to_string(files).unwrap_or_else(|_| "[]".into());
    format!("conflict: {message}{CONFLICT_FILES_PREFIX}{list}")
}

fn overlap_error(files: &[String]) -> String {
    conflict_error(
        format!(
            "Someone else changed the same part of {}. Both versions are in the file now, marked “yours” and “theirs”. Resolve the highlighted conflicts, then save again.",
            file_list(files)
        ),
        files,
    )
}

fn rev_parse(git: &Path, root: &Path, rev: &str) -> Option<String> {
    run(git, root, &["rev-parse", "-q", "--verify", rev], &[]).ok().map(|oid| oid.trim().to_string())
}

/// A scratch file in the git dir: an index of our own, so building trees never
/// touches the folder's real index, or a blob to hash. Removed on drop.
struct GitScratch(PathBuf);

impl GitScratch {
    fn new(git: &Path, root: &Path, name: &str, copy_real_index: bool) -> Result<Self, String> {
        let git_path = |name: &str| {
            run(git, root, &["rev-parse", "--git-path", name], &[])
                .map(|path| root.join(path.trim()))
                .map_err(|failure| plain(failure, "read the folder's git data"))
        };
        let path = git_path(name)?;
        let _ = std::fs::remove_file(&path);
        // Starting from the real index lets git skip rehashing unchanged files.
        if copy_real_index {
            let _ = std::fs::copy(git_path("index")?, &path);
        }
        Ok(GitScratch(path))
    }

    fn index_env(&self) -> Vec<(String, String)> {
        vec![("GIT_INDEX_FILE".into(), self.0.to_string_lossy().into_owned())]
    }
}

impl Drop for GitScratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The working tree as a tree object, untracked files included (ignored ones are not).
fn snapshot(git: &Path, root: &Path) -> Result<String, String> {
    let what = "keep a copy of your changes";
    let index = GitScratch::new(git, root, "cowmark-snapshot-index", true)?;
    run(git, root, &["add", "-A", "--", "."], &index.index_env()).map_err(|failure| plain(failure, what))?;
    run(git, root, &["write-tree"], &index.index_env())
        .map(|tree| tree.trim().to_string())
        .map_err(|failure| plain(failure, what))
}

fn backup(git: &Path, root: &Path, author: &Author, tree: &str, parent: &str) -> Result<(), String> {
    let what = "keep a copy of your changes";
    let identity = identity(author);
    let message = "CowMark: everything in the folder before combining with the latest content";
    let commit = run(git, root, &with_identity(&identity, &["commit-tree", tree, "-p", parent, "-m", message]), &[])
        .map_err(|failure| plain(failure, what))?;
    run(git, root, &["update-ref", "--create-reflog", "-m", "cowmark backup", BACKUP_REF, commit.trim()], &[])
        .map(|_| ())
        .map_err(|failure| plain(failure, what))
}

#[derive(Clone, PartialEq, Eq)]
struct Entry {
    mode: String,
    oid: String,
}

fn ls_tree(git: &Path, root: &Path, treeish: &str) -> Result<HashMap<String, Entry>, String> {
    let output = run(git, root, &["ls-tree", "-r", "-z", "--full-tree", treeish], &[])
        .map_err(|failure| plain(failure, "combine your changes with the latest content"))?;
    Ok(output
        .split('\0')
        .filter_map(|record| {
            let (meta, path) = record.split_once('\t')?;
            let mut parts = meta.split(' ');
            let mode = parts.next()?.to_string();
            let oid = parts.nth(1)?.to_string();
            Some((path.to_string(), Entry { mode, oid }))
        })
        .collect())
}

/// A blob's text; "" for a missing file, None when it isn't text.
fn read_text(git: &Path, root: &Path, entry: Option<&Entry>) -> Option<String> {
    let Some(entry) = entry else { return Some(String::new()) };
    if !entry.mode.starts_with("100") {
        return None; // symlinks and submodules aren't merged line by line
    }
    let bytes = run_bytes(git, root, &["cat-file", "blob", &entry.oid], &[]).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// Merges a file both sides changed into a new blob: (oid, has conflicts).
/// None when it isn't text.
fn merge_file(
    git: &Path,
    root: &Path,
    base: Option<&Entry>,
    mine: &Entry,
    theirs: &Entry,
) -> Result<Option<(String, bool)>, String> {
    let (Some(base), Some(mine_text), Some(theirs_text)) =
        (read_text(git, root, base), read_text(git, root, Some(mine)), read_text(git, root, Some(theirs)))
    else {
        return Ok(None);
    };
    let merged = merge_text(&base, &mine_text, &theirs_text);
    let blob = GitScratch::new(git, root, "cowmark-merged-blob", false)?;
    std::fs::write(&blob.0, merged.text.as_bytes()).map_err(|error| error.to_string())?;
    let oid = run(git, root, &["hash-object", "-w", "--no-filters", "--", &blob.0.to_string_lossy()], &[])
        .map_err(|failure| plain(failure, "combine your changes with the latest content"))?;
    Ok(Some((oid.trim().to_string(), merged.conflicts > 0)))
}

/// Moves HEAD to `target` and leaves `mine`'s changes since `base` in the
/// working tree on top of it, uncommitted. Files both sides changed are merged;
/// overlapping lines get conflict markers and those files are returned.
/// Nothing is dropped: a file one side deleted and the other edited keeps the
/// edit, and a non-text file both changed keeps yours (theirs is in `target`).
fn replay_onto(git: &Path, root: &Path, base: &str, mine: &str, target: &str) -> Result<Vec<String>, String> {
    let what = "combine your changes with the latest content";
    let (base_entries, mine_entries, theirs_entries) =
        (ls_tree(git, root, base)?, ls_tree(git, root, mine)?, ls_tree(git, root, target)?);
    let result = GitScratch::new(git, root, "cowmark-combine-index", false)?;
    let index_env = result.index_env();
    run(git, root, &["read-tree", target], &index_env).map_err(|failure| plain(failure, what))?;

    let paths: BTreeSet<&String> = base_entries.keys().chain(mine_entries.keys()).chain(theirs_entries.keys()).collect();
    let mut conflicts = Vec::new();
    for path in paths {
        let (b, m, t) = (base_entries.get(path), mine_entries.get(path), theirs_entries.get(path));
        if m == b || m == t {
            continue;
        }
        let entry = if t == b {
            m.cloned()
        } else {
            match (m, t) {
                (Some(m), Some(t)) => match merge_file(git, root, b, m, t)? {
                    Some((oid, conflicted)) => {
                        if conflicted {
                            conflicts.push(path.clone());
                        }
                        Some(Entry { mode: m.mode.clone(), oid })
                    }
                    None => Some(m.clone()),
                },
                // They deleted it and you edited it: keep your edit.
                (Some(m), None) => Some(m.clone()),
                // You deleted it and they edited it: keep their edit.
                (None, _) => continue,
            }
        };
        let outcome = match entry {
            Some(entry) => run(
                git,
                root,
                &["update-index", "--add", "--cacheinfo", &format!("{},{},{path}", entry.mode, entry.oid)],
                &index_env,
            ),
            None => run(git, root, &["update-index", "--force-remove", "--", path], &index_env),
        };
        outcome.map_err(|failure| plain(failure, what))?;
    }
    let tree = run(git, root, &["write-tree"], &index_env).map_err(|failure| plain(failure, what))?;

    // The real index must describe the working tree for read-tree -u to know
    // which files to rewrite or remove.
    run(git, root, &["add", "-A", "--", "."], &[]).map_err(|failure| plain(failure, what))?;
    run(git, root, &["read-tree", "--reset", "-u", tree.trim()], &[]).map_err(|failure| plain(failure, what))?;
    run(git, root, &["reset", "--quiet", "--soft", target], &[]).map_err(|failure| plain(failure, what))?;
    run(git, root, &["reset", "--quiet"], &[]).map_err(|failure| plain(failure, what))?;
    Ok(conflicts)
}

enum CatchUp {
    /// Local commits sit on top of the latest content.
    Clean,
    /// Local commits became uncommitted changes on the latest content, without conflicts.
    Flattened,
    /// HEAD is on the latest content (plus local commits that rebased cleanly)
    /// and these files have conflict markers.
    Conflicts(Vec<String>),
}

/// Rebases onto the fetched studio branch, carrying uncommitted changes along.
/// When git can't (overlapping edits, or a new file of theirs where you have
/// an unsaved one), every local change is replayed onto the latest content
/// with conflict markers instead, leaving the folder ready to resolve.
fn catch_up(git: &Path, root: &Path, author: &Author) -> Result<CatchUp, String> {
    let before = rev_parse(git, root, "HEAD").ok_or("Couldn't read the folder's latest save.")?;
    let stash_before = rev_parse(git, root, "refs/stash");
    let mine = snapshot(git, root)?;
    backup(git, root, author, &mine, &before)?;
    let drop_new_autostash = || {
        let stash_now = rev_parse(git, root, "refs/stash");
        if stash_now.is_some() && stash_now != stash_before {
            let _ = run(git, root, &["stash", "drop", "--quiet"], &[]);
        }
    };

    let identity = identity(author);
    match run(git, root, &with_identity(&identity, &["rebase", "--autostash", REMOTE_REF]), &[]) {
        Ok(_) => {
            // git keeps the autostash only when re-applying it conflicted.
            if rev_parse(git, root, "refs/stash") == stash_before {
                return Ok(CatchUp::Clean);
            }
            let rebased = rev_parse(git, root, "HEAD").ok_or("Couldn't read the folder's latest save.")?;
            let conflicts = replay_onto(git, root, &before, &mine, &rebased)?;
            // Only now are the stashed edits safely back in the working tree.
            drop_new_autostash();
            Ok(if conflicts.is_empty() { CatchUp::Clean } else { CatchUp::Conflicts(conflicts) })
        }
        Err(_) => {
            let _ = run(git, root, &with_identity(&identity, &["rebase", "--abort"]), &[]);
            let base = run(git, root, &["merge-base", &before, REMOTE_REF], &[])
                .map_err(|failure| plain(failure, "combine your changes with the latest content"))?;
            let conflicts = replay_onto(git, root, base.trim(), &mine, REMOTE_REF)?;
            drop_new_autostash();
            Ok(if conflicts.is_empty() { CatchUp::Flattened } else { CatchUp::Conflicts(conflicts) })
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

/// Ticked files that still have conflict markers in them.
fn unresolved(root: &Path, paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| {
            std::fs::read(root.join(path))
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .is_some_and(|text| has_conflict_markers(&text))
        })
        .cloned()
        .collect()
}

/// Commits only `paths` (the files the editor ticked) and pushes. Ticked files with
/// conflict markers or lint errors are refused first (`conflict:`, `lint:`). Every other
/// change stays in the working tree as a local draft. Paths that no longer have
/// changes are ignored, so a stale selection can't stage anything else.
pub fn save(
    git: &Path,
    root: &Path,
    remote: &Remote,
    author: &Author,
    message: &str,
    paths: &[String],
) -> Result<SyncResult, String> {
    require_studio(git, root)?;
    let marked = unresolved(root, paths);
    if !marked.is_empty() {
        let verb = if marked.len() == 1 { "still has" } else { "still have" };
        return Err(conflict_error(
            format!(
                "{} {verb} conflicts marked “yours” and “theirs”. Choose a version for each highlighted conflict, then save again.",
                file_list(&marked)
            ),
            &marked,
        ));
    }
    let problems = crate::content_check::first_errors(root, paths);
    if !problems.is_empty() {
        return Err(crate::content_check::lint_error(&problems));
    }
    let mut committed = commit_selected(git, root, author, message, paths)?;

    // Someone may push between our fetch and push; one retry covers that.
    let refspec = format!("HEAD:refs/heads/{STUDIO_BRANCH}");
    let mut attempts = 0;
    loop {
        attempts += 1;
        fetch(git, root, remote)?;
        match catch_up(git, root, author)? {
            CatchUp::Clean => {}
            CatchUp::Conflicts(files) => return Err(overlap_error(&files)),
            // The ticked files combined cleanly but are uncommitted again.
            CatchUp::Flattened => committed = commit_selected(git, root, author, message, paths)?,
        }
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

pub fn update(git: &Path, root: &Path, remote: &Remote, author: &Author) -> Result<SyncResult, String> {
    require_studio(git, root)?;
    fetch(git, root, remote)?;
    if let CatchUp::Conflicts(files) = catch_up(git, root, author)? {
        return Err(overlap_error(&files));
    }
    Ok(SyncResult { committed: false, head: head(git, root) })
}

/// Commits the ticked `paths` that still have changes; false when none do.
fn commit_selected(git: &Path, root: &Path, author: &Author, message: &str, paths: &[String]) -> Result<bool, String> {
    run(git, root, &["reset", "--quiet"], &[]).map_err(|failure| plain(failure, "stage your changes"))?;
    let changed: std::collections::HashSet<String> =
        status(git, root)?.changes.into_iter().map(|change| change.path).collect();
    let selected: Vec<&str> = paths.iter().map(String::as_str).filter(|path| changed.contains(*path)).collect();
    if !selected.is_empty() {
        let mut args = vec!["--literal-pathspecs", "add", "-A", "--"];
        args.extend(selected);
        run(git, root, &args, &[]).map_err(|failure| plain(failure, "stage your changes"))?;
    }
    let committed = run(git, root, &["diff", "--cached", "--quiet"], &[]).is_err();
    if committed {
        let message = if message.trim().is_empty() { "Update content" } else { message.trim() };
        let identity = identity(author);
        run(git, root, &with_identity(&identity, &["commit", "--quiet", "-m", message]), &[])
            .map_err(|failure| plain(failure, "save your changes"))?;
    }
    Ok(committed)
}

pub fn default_folder(documents: Option<PathBuf>) -> PathBuf {
    documents
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Documents")))
        .or_else(|| std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("Documents")))
        .unwrap_or_default()
        .join("catercow-content")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_tool::find_git;
    use std::fs;

    const SECRET: &str = "ghs_SECRET-TOKEN-123";

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cowmark-core-git-{name}-{}", std::process::id()));
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
        fs::write(seed.join("content/blog/b.md"), "b one\nb two\n").unwrap();
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
            PathBuf::from("/Users/a/Documents/catercow-content")
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

        let saved = save(&git, &mine, &remote, &me, "Add a post", &["content/blog/new.md".to_string()]).unwrap();
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

    fn conflict_files(error: &str) -> Vec<String> {
        let (_, list) = error.split_once(CONFLICT_FILES_PREFIX).unwrap_or_else(|| panic!("no file list: {error}"));
        serde_json::from_str(list).unwrap()
    }

    fn remote_head(git: &Path, base: &Path) -> String {
        git_ok(git, &base.join("remote.git"), &["rev-parse", "studio"]).trim().to_string()
    }

    fn read(path: PathBuf) -> String {
        fs::read_to_string(path).unwrap()
    }

    fn ticked(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| path.to_string()).collect()
    }

    #[test]
    fn a_conflicting_save_leaves_markers_on_the_latest_content_and_keeps_drafts() {
        let Some(git) = find_git() else { return };
        let base = temp("conflict");
        let remote = remote_with_studio(&git, &base);
        let mine = base.join("mine");
        let theirs = base.join("theirs");
        let check = base.join("check");
        let me = Author::from_email("editor@catercow.com");
        clone_into(&git, &remote, &mine).unwrap();
        clone_into(&git, &remote, &theirs).unwrap();

        fs::write(theirs.join("content/blog/a.md"), "line one\ntheir line\n").unwrap();
        fs::write(theirs.join("content/blog/b.md"), "b zero\nb one\nb two\n").unwrap();
        save(&git, &theirs, &remote, &Author::from_email("other@catercow.com"), "Theirs", &ticked(&["content/blog/a.md", "content/blog/b.md"])).unwrap();

        fs::write(mine.join("content/blog/a.md"), "line one\nmy line\n").unwrap();
        fs::write(mine.join("content/blog/b.md"), "b one\nb two\nmy b three\n").unwrap();
        fs::write(mine.join("content/blog/draft.md"), "# Draft\n").unwrap();
        let error = save(&git, &mine, &remote, &me, "Mine", &ticked(&["content/blog/a.md"])).unwrap_err();
        assert!(error.starts_with("conflict: Someone else changed the same part of content/blog/a.md."), "{error}");
        assert_eq!(conflict_files(&error), vec!["content/blog/a.md".to_string()]);

        // On the latest content, the editor's commit turned back into changes, both sides kept.
        assert!(!mine.join(".git/rebase-merge").exists() && !mine.join(".git/rebase-apply").exists());
        assert_eq!(git_ok(&git, &mine, &["rev-parse", "HEAD"]).trim(), remote_head(&git, &base));
        assert_eq!(status(&git, &mine).unwrap().ahead, 0);
        assert_eq!(
            read(mine.join("content/blog/a.md")),
            "line one\n<<<<<<< yours\nmy line\n=======\ntheir line\n>>>>>>> theirs\n"
        );
        assert_eq!(read(mine.join("content/blog/b.md")), "b zero\nb one\nb two\nmy b three\n");
        assert_eq!(read(mine.join("content/blog/draft.md")), "# Draft\n");
        assert_eq!(git_ok(&git, &mine, &["stash", "list"]).trim(), "");
        assert!(git_ok(&git, &mine, &["rev-parse", BACKUP_REF]).trim().len() == 40);

        // Saving with the markers still in is refused and changes nothing.
        let error = save(&git, &mine, &remote, &me, "Mine", &ticked(&["content/blog/a.md"])).unwrap_err();
        assert!(error.starts_with("conflict: content/blog/a.md still has conflicts"), "{error}");
        assert_eq!(conflict_files(&error), vec!["content/blog/a.md".to_string()]);
        assert_eq!(git_ok(&git, &mine, &["rev-parse", "HEAD"]).trim(), remote_head(&git, &base));

        // Resolved, the second save goes through and the drafts stay behind.
        fs::write(mine.join("content/blog/a.md"), "line one\nmy line\ntheir line\n").unwrap();
        assert!(save(&git, &mine, &remote, &me, "Mine", &ticked(&["content/blog/a.md"])).unwrap().committed);
        clone_into(&git, &remote, &check).unwrap();
        assert_eq!(read(check.join("content/blog/a.md")), "line one\nmy line\ntheir line\n");
        assert_eq!(read(check.join("content/blog/b.md")), "b zero\nb one\nb two\n");
        assert!(!check.join("content/blog/draft.md").exists());
        let mut left: Vec<String> = status(&git, &mine).unwrap().changes.into_iter().map(|c| c.path).collect();
        left.sort();
        assert_eq!(left, vec!["content/blog/b.md".to_string(), "content/blog/draft.md".to_string()]);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn refuses_ticked_pages_with_lint_errors_until_they_are_fixed() {
        let Some(git) = find_git() else { return };
        let base = temp("lint");
        let remote = remote_with_studio(&git, &base);
        let mine = base.join("mine");
        let me = Author::from_email("editor@catercow.com");
        clone_into(&git, &remote, &mine).unwrap();
        let before = remote_head(&git, &base);

        fs::write(mine.join("content/blog/a.md"), "line one\n<script>alert(1)</script>\n").unwrap();
        fs::write(mine.join("content/blog/b.md"), "b one\n\n[x](javascript:alert(1))\n").unwrap();
        let error = save(&git, &mine, &remote, &me, "Mine", &ticked(&["content/blog/a.md", "content/blog/b.md"])).unwrap_err();
        assert!(error.starts_with("lint: Fix these problems, then save again.\ncontent/blog/a.md, line 2: HTML isn't allowed"), "{error}");
        assert!(error.contains("\ncontent/blog/b.md, line 3: This link can't be used"), "{error}");
        assert!(error.contains(crate::content_check::LINT_PROBLEMS_PREFIX), "{error}");
        assert_eq!(remote_head(&git, &base), before);
        assert_eq!(status(&git, &mine).unwrap().ahead, 0);

        // An unticked file's errors don't block; a fixed file saves.
        fs::write(mine.join("content/blog/a.md"), "line one\n**bold**\n").unwrap();
        assert!(save(&git, &mine, &remote, &me, "Mine", &ticked(&["content/blog/a.md"])).unwrap().committed);
        assert_ne!(remote_head(&git, &base), before);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn names_every_conflicting_file_including_an_unsaved_new_file_they_also_added() {
        let Some(git) = find_git() else { return };
        let base = temp("multi");
        let remote = remote_with_studio(&git, &base);
        let mine = base.join("mine");
        let theirs = base.join("theirs");
        clone_into(&git, &remote, &mine).unwrap();
        clone_into(&git, &remote, &theirs).unwrap();

        fs::write(theirs.join("content/blog/a.md"), "line one\ntheir line\n").unwrap();
        fs::write(theirs.join("content/blog/b.md"), "b one\ntheir b\n").unwrap();
        fs::write(theirs.join("content/blog/new.md"), "# New\n\ntheir text\n").unwrap();
        save(&git, &theirs, &remote, &Author::from_email("other@catercow.com"), "Theirs", &ticked(&["content/blog/a.md", "content/blog/b.md", "content/blog/new.md"])).unwrap();

        fs::write(mine.join("content/blog/a.md"), "line one\nmy line\n").unwrap();
        fs::write(mine.join("content/blog/b.md"), "b one\nmy b\n").unwrap();
        fs::write(mine.join("content/blog/new.md"), "# New\n\nmy text\n").unwrap();
        fs::write(mine.join("content/blog/keep.md"), "# Keep\n").unwrap();
        let error = save(&git, &mine, &remote, &Author::from_email("editor@catercow.com"), "Mine", &ticked(&["content/blog/a.md", "content/blog/b.md"])).unwrap_err();
        assert_eq!(
            conflict_files(&error),
            ticked(&["content/blog/a.md", "content/blog/b.md", "content/blog/new.md"])
        );
        assert!(error.contains("content/blog/a.md, content/blog/b.md and content/blog/new.md"), "{error}");
        assert_eq!(git_ok(&git, &mine, &["rev-parse", "HEAD"]).trim(), remote_head(&git, &base));
        assert_eq!(read(mine.join("content/blog/b.md")), "b one\n<<<<<<< yours\nmy b\n=======\ntheir b\n>>>>>>> theirs\n");
        assert_eq!(
            read(mine.join("content/blog/new.md")),
            "# New\n\n<<<<<<< yours\nmy text\n=======\ntheir text\n>>>>>>> theirs\n"
        );
        assert_eq!(read(mine.join("content/blog/keep.md")), "# Keep\n");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_conflicting_draft_keeps_the_saved_commit_waiting_to_send() {
        let Some(git) = find_git() else { return };
        let base = temp("draft-conflict");
        let remote = remote_with_studio(&git, &base);
        let mine = base.join("mine");
        let theirs = base.join("theirs");
        let me = Author::from_email("editor@catercow.com");
        clone_into(&git, &remote, &mine).unwrap();
        clone_into(&git, &remote, &theirs).unwrap();

        fs::write(theirs.join("content/blog/b.md"), "b one\ntheir b\n").unwrap();
        save(&git, &theirs, &remote, &Author::from_email("other@catercow.com"), "Theirs", &ticked(&["content/blog/b.md"])).unwrap();

        fs::write(mine.join("content/blog/a.md"), "line one\nmy line\n").unwrap();
        fs::write(mine.join("content/blog/b.md"), "b one\nmy b\n").unwrap();
        let error = save(&git, &mine, &remote, &me, "Mine", &ticked(&["content/blog/a.md"])).unwrap_err();
        assert_eq!(conflict_files(&error), vec!["content/blog/b.md".to_string()]);
        assert_eq!(read(mine.join("content/blog/b.md")), "b one\n<<<<<<< yours\nmy b\n=======\ntheir b\n>>>>>>> theirs\n");
        assert_eq!(read(mine.join("content/blog/a.md")), "line one\nmy line\n");
        assert_eq!(git_ok(&git, &mine, &["rev-parse", "HEAD^"]).trim(), remote_head(&git, &base));
        assert_eq!(git_ok(&git, &mine, &["log", "-1", "--format=%s"]).trim(), "Mine");
        assert_eq!(git_ok(&git, &mine, &["stash", "list"]).trim(), "");

        // The draft isn't ticked, so the waiting commit can still be sent.
        save(&git, &mine, &remote, &me, "", &[]).unwrap();
        assert_eq!(git_ok(&git, &mine, &["rev-parse", "HEAD"]).trim(), remote_head(&git, &base));
        assert!(has_conflict_markers(&read(mine.join("content/blog/b.md"))));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn get_latest_puts_overlapping_unsaved_edits_into_the_file_and_drops_the_stash() {
        let Some(git) = find_git() else { return };
        let base = temp("update-conflict");
        let remote = remote_with_studio(&git, &base);
        let mine = base.join("mine");
        let theirs = base.join("theirs");
        clone_into(&git, &remote, &mine).unwrap();
        clone_into(&git, &remote, &theirs).unwrap();

        fs::write(theirs.join("content/blog/a.md"), "line one\ntheir line\n").unwrap();
        save(&git, &theirs, &remote, &Author::from_email("other@catercow.com"), "Theirs", &ticked(&["content/blog/a.md"])).unwrap();

        fs::write(mine.join("content/blog/a.md"), "line one\nmy line\n").unwrap();
        fs::write(mine.join("content/blog/draft.md"), "# Draft\n").unwrap();
        let error = update(&git, &mine, &remote, &Author::from_email("editor@catercow.com")).unwrap_err();
        assert_eq!(conflict_files(&error), vec!["content/blog/a.md".to_string()]);
        assert_eq!(git_ok(&git, &mine, &["rev-parse", "HEAD"]).trim(), remote_head(&git, &base));
        assert_eq!(read(mine.join("content/blog/a.md")), "line one\n<<<<<<< yours\nmy line\n=======\ntheir line\n>>>>>>> theirs\n");
        assert_eq!(read(mine.join("content/blog/draft.md")), "# Draft\n");
        assert_eq!(git_ok(&git, &mine, &["stash", "list"]).trim(), "");
        assert_eq!(git_ok(&git, &mine, &["diff", "--name-only", "--diff-filter=U"]).trim(), "");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn an_unsaved_copy_of_a_file_they_added_does_not_block_the_save() {
        let Some(git) = find_git() else { return };
        let base = temp("same-new");
        let remote = remote_with_studio(&git, &base);
        let mine = base.join("mine");
        let theirs = base.join("theirs");
        let check = base.join("check");
        clone_into(&git, &remote, &mine).unwrap();
        clone_into(&git, &remote, &theirs).unwrap();

        fs::write(theirs.join("content/blog/same.md"), "# Same\n").unwrap();
        save(&git, &theirs, &remote, &Author::from_email("other@catercow.com"), "Theirs", &ticked(&["content/blog/same.md"])).unwrap();

        fs::write(mine.join("content/blog/same.md"), "# Same\n").unwrap();
        fs::write(mine.join("content/blog/a.md"), "line one\nmy line\n").unwrap();
        let saved = save(&git, &mine, &remote, &Author::from_email("editor@catercow.com"), "Mine", &ticked(&["content/blog/a.md"])).unwrap();
        assert!(saved.committed);
        clone_into(&git, &remote, &check).unwrap();
        assert_eq!(read(check.join("content/blog/a.md")), "line one\nmy line\n");
        assert_eq!(read(check.join("content/blog/same.md")), "# Same\n");
        assert!(status(&git, &mine).unwrap().changes.is_empty());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn saves_only_the_selected_files_and_keeps_the_rest_as_drafts() {
        let Some(git) = find_git() else { return };
        let base = temp("select");
        let remote = remote_with_studio(&git, &base);
        let mine = base.join("mine");
        let check = base.join("check");
        clone_into(&git, &remote, &mine).unwrap();
        let me = Author::from_email("editor@catercow.com");

        fs::create_dir_all(mine.join("content/blog/ready")).unwrap();
        fs::write(mine.join("content/blog/ready.md"), "# Ready\n").unwrap();
        fs::write(mine.join("content/blog/ready/photo.jpg"), "jpg").unwrap();
        fs::write(mine.join("content/blog/draft.md"), "# Draft\n").unwrap();
        fs::write(mine.join("content/blog/a.md"), "line one\nedited\n").unwrap();

        let selected = vec![
            "content/blog/ready.md".to_string(),
            "content/blog/ready/photo.jpg".to_string(),
            // No longer changed: ignored rather than an error.
            "content/blog/gone.md".to_string(),
        ];
        assert!(save(&git, &mine, &remote, &me, "Publish ready", &selected).unwrap().committed);

        let mut left: Vec<String> = status(&git, &mine).unwrap().changes.into_iter().map(|c| c.path).collect();
        left.sort();
        assert_eq!(left, vec!["content/blog/a.md".to_string(), "content/blog/draft.md".to_string()]);
        assert_eq!(fs::read_to_string(mine.join("content/blog/a.md")).unwrap(), "line one\nedited\n");

        clone_into(&git, &remote, &check).unwrap();
        assert!(check.join("content/blog/ready.md").exists());
        assert!(check.join("content/blog/ready/photo.jpg").exists());
        assert!(!check.join("content/blog/draft.md").exists());
        assert_eq!(fs::read_to_string(check.join("content/blog/a.md")).unwrap(), "line one\nline two\n");

        // A selected deletion is published; drafts survive a save after someone else's push.
        fs::remove_file(check.join("content/blog/ready/photo.jpg")).unwrap();
        save(&git, &check, &remote, &Author::from_email("other@catercow.com"), "Remove photo", &["content/blog/ready/photo.jpg".to_string()]).unwrap();
        fs::write(mine.join("content/blog/ready.md"), "# Ready v2\n").unwrap();
        save(&git, &mine, &remote, &me, "Edit ready", &["content/blog/ready.md".to_string()]).unwrap();
        assert!(!mine.join("content/blog/ready/photo.jpg").exists());
        assert!(mine.join("content/blog/draft.md").exists());
        assert_eq!(fs::read_to_string(mine.join("content/blog/a.md")).unwrap(), "line one\nedited\n");
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
        let error = save(&git, &mine, &remote, &Author::from_email("e@catercow.com"), "x", &[]).unwrap_err();
        assert!(error.contains("`other` branch"), "{error}");
        let _ = fs::remove_dir_all(&base);
    }
}
