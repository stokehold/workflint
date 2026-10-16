//! Step 1: the actionlint checkout at the pinned commit.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::Outcome;

/// Makes `dir` a checkout of `url` at `commit`.
///
/// A missing `dir` becomes a shallow clone. An existing clean checkout at
/// another commit moves to `commit`. A checkout with modified tracked files is
/// an error that lists them, and nothing changes. Untracked files stay.
pub(crate) fn ensure(dir: &Path, url: &str, commit: &str) -> Result<Outcome> {
    if !dir.exists() {
        create(dir, url, commit)?;
        return Ok(Outcome::Created);
    }
    // Without this check, git would find the workflint repository above `dir`.
    if !dir.join(".git").exists() {
        bail!(
            "{} exists but is not a git checkout. Move it away and run the command again.",
            dir.display()
        );
    }
    let changed = changed_tracked_files(dir)?;
    if !changed.is_empty() {
        bail!(
            "{} has local changes to tracked files, so it was left as it is. \
             Commit, stash, or revert them and run the command again:\n  {}",
            dir.display(),
            changed.join("\n  ")
        );
    }
    if head(dir)?.as_deref() == Some(commit) {
        return Ok(Outcome::UpToDate);
    }
    if !has_commit(dir, commit)? {
        let shallow = git(dir, &["rev-parse", "--is-shallow-repository"])? == "true";
        let has_head = head(dir)?.is_some();
        // A depth limit on a full clone would make it shallow.
        if shallow || !has_head {
            git(dir, &["fetch", "--quiet", "--depth", "1", url, commit])?;
        } else {
            git(dir, &["fetch", "--quiet", url, commit])?;
        }
    }
    git(dir, &["checkout", "--quiet", "--detach", commit])?;
    Ok(Outcome::Updated)
}

/// Clones `commit` into the missing `dir`, and removes `dir` again if that
/// fails.
fn create(dir: &Path, url: &str, commit: &str) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let result = (|| {
        git(dir, &["init", "--quiet"])?;
        // Keep the files byte for byte as upstream has them, also where git
        // converts line endings by default, as on Windows.
        git(dir, &["config", "core.autocrlf", "false"])?;
        git(dir, &["remote", "add", "origin", url])?;
        git(dir, &["fetch", "--quiet", "--depth", "1", "origin", commit])?;
        git(dir, &["checkout", "--quiet", "--detach", "FETCH_HEAD"])?;
        Ok(())
    })();
    if result.is_err() {
        // The directory did not exist before this call, so nothing else is lost.
        let _ = fs::remove_dir_all(dir);
    }
    result
}

/// The tracked files that differ from `HEAD` or the index.
fn changed_tracked_files(dir: &Path) -> Result<Vec<String>> {
    let status = git_untrimmed(
        dir,
        &["status", "--porcelain=v1", "-z", "--untracked-files=no"],
    )?;
    // Each entry is `XY path`. A rename or copy is followed by its source path.
    let mut entries = status.split('\0').filter(|entry| !entry.is_empty());
    let mut changed = Vec::new();
    while let Some(entry) = entries.next() {
        changed.push(entry.get(3..).unwrap_or(entry).to_owned());
        if entry.starts_with(['R', 'C']) {
            entries.next();
        }
    }
    Ok(changed)
}

/// The commit at `HEAD`, or `None` before the first checkout.
fn head(dir: &Path) -> Result<Option<String>> {
    let output = run(dir, &["rev-parse", "--verify", "--quiet", "HEAD"])?;
    Ok(output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned()))
}

fn has_commit(dir: &Path, commit: &str) -> Result<bool> {
    let object = format!("{commit}^{{commit}}");
    Ok(run(dir, &["cat-file", "-e", &object])?.status.success())
}

/// Runs git in `dir` and returns its trimmed standard output, or an error with
/// its standard error.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    Ok(git_untrimmed(dir, args)?.trim().to_owned())
}

fn git_untrimmed(dir: &Path, args: &[&str]) -> Result<String> {
    let output = run(dir, args)?;
    if !output.status.success() {
        bail!(
            "`git {}` failed in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run(dir: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "advice.detachedHead=false"])
        .args(args)
        .output()
        .context("cannot run git. Install git and make sure it is on PATH.")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    use super::*;

    /// A local repository with two commits that stands in for GitHub.
    struct Upstream {
        _dir: tempfile::TempDir,
        url: String,
        first: String,
        second: String,
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=workflint",
                "-c",
                "user.email=workflint@example.com",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn file_url(path: &Path) -> String {
        let path = path.to_str().unwrap().replace('\\', "/");
        if path.starts_with('/') {
            format!("file://{path}")
        } else {
            format!("file:///{path}")
        }
    }

    fn upstream() -> Upstream {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        git(repo, &["init", "--quiet"]);
        fs::write(repo.join("README.md"), "first\n").unwrap();
        git(repo, &["add", "README.md"]);
        git(repo, &["commit", "--quiet", "-m", "first"]);
        let first = git(repo, &["rev-parse", "HEAD"]);
        fs::write(repo.join("README.md"), "second\n").unwrap();
        git(repo, &["commit", "--quiet", "-am", "second"]);
        let second = git(repo, &["rev-parse", "HEAD"]);
        Upstream {
            url: file_url(repo),
            _dir: dir,
            first,
            second,
        }
    }

    fn checkout_path(parent: &tempfile::TempDir) -> PathBuf {
        parent.path().join("actionlint")
    }

    #[test]
    fn missing_checkout_is_cloned_at_the_commit() {
        let upstream = upstream();
        let parent = tempfile::tempdir().unwrap();
        let dir = checkout_path(&parent);

        let outcome = ensure(&dir, &upstream.url, &upstream.first).unwrap();

        assert_eq!(outcome, Outcome::Created);
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]), upstream.first);
        assert_eq!(
            fs::read_to_string(dir.join("README.md")).unwrap(),
            "first\n"
        );
        assert_eq!(git(&dir, &["remote", "get-url", "origin"]), upstream.url);
        assert_eq!(git(&dir, &["config", "core.autocrlf"]), "false");
    }

    #[test]
    fn checkout_at_the_commit_is_up_to_date() {
        let upstream = upstream();
        let parent = tempfile::tempdir().unwrap();
        let dir = checkout_path(&parent);
        ensure(&dir, &upstream.url, &upstream.first).unwrap();

        let outcome = ensure(&dir, &upstream.url, &upstream.first).unwrap();

        assert_eq!(outcome, Outcome::UpToDate);
    }

    #[test]
    fn untracked_files_do_not_block_the_checkout() {
        let upstream = upstream();
        let parent = tempfile::tempdir().unwrap();
        let dir = checkout_path(&parent);
        ensure(&dir, &upstream.url, &upstream.first).unwrap();
        fs::write(dir.join("notes.txt"), "mine\n").unwrap();

        let outcome = ensure(&dir, &upstream.url, &upstream.first).unwrap();

        assert_eq!(outcome, Outcome::UpToDate);
        assert_eq!(fs::read_to_string(dir.join("notes.txt")).unwrap(), "mine\n");
    }

    #[test]
    fn clean_checkout_at_another_commit_moves_to_the_commit() {
        let upstream = upstream();
        let parent = tempfile::tempdir().unwrap();
        let dir = checkout_path(&parent);
        ensure(&dir, &upstream.url, &upstream.first).unwrap();

        let outcome = ensure(&dir, &upstream.url, &upstream.second).unwrap();

        assert_eq!(outcome, Outcome::Updated);
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]), upstream.second);
        assert_eq!(
            fs::read_to_string(dir.join("README.md")).unwrap(),
            "second\n"
        );
    }

    #[test]
    fn modified_tracked_file_is_an_error_that_names_it() {
        let upstream = upstream();
        let parent = tempfile::tempdir().unwrap();
        let dir = checkout_path(&parent);
        ensure(&dir, &upstream.url, &upstream.first).unwrap();
        fs::write(dir.join("README.md"), "edited\n").unwrap();

        let error = ensure(&dir, &upstream.url, &upstream.second).unwrap_err();

        assert!(format!("{error:#}").contains("README.md"), "{error:#}");
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]), upstream.first);
        assert_eq!(
            fs::read_to_string(dir.join("README.md")).unwrap(),
            "edited\n"
        );
    }

    #[test]
    fn directory_that_is_not_a_checkout_is_an_error_and_stays() {
        let upstream = upstream();
        let parent = tempfile::tempdir().unwrap();
        let dir = checkout_path(&parent);
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("keep.txt"), "mine\n").unwrap();

        let error = ensure(&dir, &upstream.url, &upstream.first).unwrap_err();

        assert!(
            format!("{error:#}").contains("not a git checkout"),
            "{error:#}"
        );
        assert_eq!(fs::read_to_string(dir.join("keep.txt")).unwrap(), "mine\n");
        assert!(!dir.join(".git").exists());
    }

    #[test]
    fn failed_clone_removes_the_directory_it_created() {
        let upstream = upstream();
        let parent = tempfile::tempdir().unwrap();
        let dir = checkout_path(&parent);
        let unknown = "0123456789abcdef0123456789abcdef01234567";

        assert!(ensure(&dir, &upstream.url, unknown).is_err());

        assert!(!dir.exists());
    }

    #[test]
    fn checkout_interrupted_before_the_first_commit_is_completed() {
        let upstream = upstream();
        let parent = tempfile::tempdir().unwrap();
        let dir = checkout_path(&parent);
        fs::create_dir(&dir).unwrap();
        git(&dir, &["init", "--quiet"]);

        let outcome = ensure(&dir, &upstream.url, &upstream.first).unwrap();

        assert_eq!(outcome, Outcome::Updated);
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]), upstream.first);
    }
}
