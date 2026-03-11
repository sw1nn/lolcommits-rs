use crate::error::{Error::*, Result};
use git2::Repository;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffStats {
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
}

impl DiffStats {
    pub fn is_empty(&self) -> bool {
        self.files_changed == 0 && self.insertions == 0 && self.deletions == 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMetadata {
    #[serde(skip_serializing, default)]
    pub path: std::path::PathBuf,
    pub revision: String,
    pub message: String,
    pub commit_type: String,
    pub scope: String,
    pub timestamp: String,
    pub repo_name: String,
    pub branch_name: String,
    pub stats: DiffStats,
}

impl AsRef<std::path::Path> for CommitMetadata {
    fn as_ref(&self) -> &std::path::Path {
        &self.path
    }
}

impl CommitMetadata {
    /// Format diff stats as human-readable string for display
    /// Example: "2 files changed, 15 insertions(+), 3 deletions(-)"
    pub fn diff_stats_string(&self) -> String {
        if self.stats.is_empty() {
            return String::new();
        }

        let mut parts = vec![format!(
            "{} file{} changed",
            self.stats.files_changed,
            if self.stats.files_changed == 1 {
                ""
            } else {
                "s"
            }
        )];

        if self.stats.insertions > 0 {
            parts.push(format!(
                "{} insertion{}(+)",
                self.stats.insertions,
                if self.stats.insertions == 1 { "" } else { "s" }
            ));
        }

        if self.stats.deletions > 0 {
            parts.push(format!(
                "{} deletion{}(-)",
                self.stats.deletions,
                if self.stats.deletions == 1 { "" } else { "s" }
            ));
        }

        parts.join(", ")
    }
}

pub fn get_repo_name(repo: &Repository) -> Result<String> {
    if let Some(name) = repo_name_from_remote(repo) {
        return Ok(name);
    }

    // Fallback to directory name if no remote is configured
    let path = repo.path().parent().ok_or(NoRepoName)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(NoRepoName)?;

    Ok(name.to_owned())
}

/// Extract repo name from the "origin" remote URL.
///
/// Handles all common git remote URL formats:
/// - HTTPS:        `https://github.com/user/repo.git`
/// - SSH (scp):    `git@github.com:user/repo.git`
/// - SSH (URL):    `ssh://git@github.com/user/repo.git`
/// - Git protocol: `git://github.com/user/repo.git`
/// - Local path:   `/home/user/repos/repo.git`
/// - File URL:     `file:///path/to/repo.git`
fn repo_name_from_remote(repo: &Repository) -> Option<String> {
    let remote = repo.find_remote("origin").ok()?;
    let url = remote.url()?;
    repo_name_from_url(url)
}

pub fn repo_name_from_url(url: &str) -> Option<String> {
    let url = url.trim_end_matches('/');

    // Split on '/' or ':' (for scp-style SSH URLs) and take the last segment
    let last = url.rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);

    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

/// Get diff stats for a commit using git show --numstat
pub fn get_diff_stats(sha: &str) -> Result<DiffStats> {
    get_diff_stats_in_dir(sha, None)
}

fn get_diff_stats_in_dir(sha: &str, repo_path: Option<&std::path::Path>) -> Result<DiffStats> {
    let mut cmd = Command::new("git");

    if let Some(path) = repo_path {
        // Clear GIT_DIR/GIT_WORK_TREE so -C takes effect
        cmd.env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .arg("-C")
            .arg(path);
    }

    let output = cmd.args(["show", "--numstat", "--format=", sha]).output()?;

    if !output.status.success() {
        return Err(GitCommandFailed);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files_changed = 0;
    let mut total_insertions = 0;
    let mut total_deletions = 0;

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            // Handle binary files (marked as "-")
            if parts[0] != "-"
                && let Ok(insertions) = parts[0].parse::<u32>()
            {
                total_insertions += insertions;
            }
            if parts[1] != "-"
                && let Ok(deletions) = parts[1].parse::<u32>()
            {
                total_deletions += deletions;
            }
            files_changed += 1;
        }
    }

    Ok(DiffStats {
        files_changed,
        insertions: total_insertions,
        deletions: total_deletions,
    })
}

pub fn get_branch_name(repo: &Repository) -> Result<String> {
    let head = repo.head()?;

    if let Some(branch_name) = head.shorthand() {
        Ok(branch_name.to_string())
    } else {
        Ok("HEAD".to_string())
    }
}

/// Get the commit message for a given SHA (supports both short and long SHAs)
pub fn get_commit_message(repo: &Repository, sha: &str) -> Result<String> {
    let obj = repo.revparse_single(sha)?;
    let commit = repo.find_commit(obj.id())?;

    commit
        .message()
        .map(|s| s.to_string())
        .ok_or(GitCommandFailed)
}

/// Parse the commit type from a conventional commit message
/// Example: "feat(scope): message" -> "feat"
pub fn parse_commit_type(message: &str) -> String {
    let first_line = message.lines().next().unwrap_or(message);

    if let Some(colon_pos) = first_line.find(':') {
        let prefix = &first_line[..colon_pos];

        if let Some(paren_pos) = prefix.find('(') {
            prefix[..paren_pos].trim().to_string()
        } else {
            prefix.trim().to_string()
        }
    } else {
        "commit".to_string()
    }
}

/// Strip the conventional commit prefix from a message
/// Example: "feat(scope): message" -> "message"
pub fn strip_commit_prefix(message: &str) -> String {
    if let Some(colon_pos) = message.find(':') {
        message[colon_pos + 1..].trim().to_string()
    } else {
        message.to_string()
    }
}

/// Parse the scope from a conventional commit message
/// Example: "feat(scope): message" -> "scope"
pub fn parse_commit_scope(message: &str) -> String {
    if let Some(colon_pos) = message.find(':') {
        let prefix = &message[..colon_pos];

        if let Some(open_paren) = prefix.find('(')
            && let Some(close_paren) = prefix.find(')')
        {
            return prefix[open_paren + 1..close_paren].trim().to_string();
        }
    }

    String::new()
}

pub fn open_repo() -> Result<Repository> {
    Repository::open_from_env().map_err(|_| NotInGitRepo)
}

pub fn resolve_revision(repo: &Repository, revision: &str) -> Result<String> {
    let obj = repo.revparse_single(revision)?;
    Ok(obj.id().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use test_case::test_case;

    fn create_test_repo() -> Result<TempDir> {
        let temp_dir = TempDir::with_prefix("lolcommits-test-")?;
        let repo = git2::Repository::init(temp_dir.path())?;

        let mut config = repo.config()?;
        config.set_str("user.name", "Test User")?;
        config.set_str("user.email", "test@example.com")?;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = repo.signature()?;
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;

        // Create a second commit with some changes to test diff stats
        fs::write(temp_dir.path().join("test.txt"), "test content\n")?;
        let mut index = repo.index()?;
        index.add_path(std::path::Path::new("test.txt"))?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let parent = repo.head()?.peel_to_commit()?;
        repo.commit(Some("HEAD"), &sig, &sig, "Add test file", &tree, &[&parent])?;

        Ok(temp_dir)
    }

    #[test]
    fn test_get_repo_name_no_remote_falls_back_to_dir() -> Result<()> {
        let temp_dir = create_test_repo()?;
        let repo = Repository::open(temp_dir.path())?;

        let expected_name = temp_dir
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(NoRepoName)?;
        let name = get_repo_name(&repo)?;
        assert_eq!(name, expected_name);
        Ok(())
    }

    #[test_case("git@github.com:user/my-cool-repo.git", "my-cool-repo" ; "ssh scp-style")]
    #[test_case("https://github.com/user/another-repo.git", "another-repo" ; "https")]
    fn test_get_repo_name_uses_remote_origin(url: &str, expected: &str) -> Result<()> {
        let temp_dir = create_test_repo()?;
        let repo = Repository::open(temp_dir.path())?;
        repo.remote("origin", url)?;

        let name = get_repo_name(&repo)?;
        assert_eq!(name, expected);
        Ok(())
    }

    #[test_case("https://github.com/user/repo.git",        Some("repo") ; "https")]
    #[test_case("https://github.com/user/repo",            Some("repo") ; "https no suffix")]
    #[test_case("git@github.com:user/repo.git",            Some("repo") ; "ssh scp-style")]
    #[test_case("git@github.com:repo.git",                 Some("repo") ; "ssh scp-style no user")]
    #[test_case("ssh://git@github.com/user/repo.git",      Some("repo") ; "ssh url-style")]
    #[test_case("git://github.com/user/repo.git",          Some("repo") ; "git protocol")]
    #[test_case("file:///home/user/repos/repo.git",        Some("repo") ; "file url")]
    #[test_case("/home/user/repos/repo.git",               Some("repo") ; "local path")]
    #[test_case("https://github.com/user/repo.git/",       Some("repo") ; "trailing slash")]
    fn test_repo_name_from_url(url: &str, expected: Option<&str>) {
        assert_eq!(repo_name_from_url(url), expected.map(|s| s.to_owned()));
    }

    #[test]
    fn test_get_diff_stats() -> Result<()> {
        let temp_dir = create_test_repo()?;

        let repo = Repository::open(temp_dir.path())?;
        let head = repo.head()?;
        let commit = head.peel_to_commit()?;
        let sha = commit.id().to_string();

        let stats = get_diff_stats_in_dir(&sha, Some(temp_dir.path()))?;

        // The second commit added a file, so we should have:
        // - 1 file changed
        // - 1 insertion (the line we added)
        // - 0 deletions
        assert_eq!(stats.files_changed, 1);
        assert_eq!(stats.insertions, 1);
        assert_eq!(stats.deletions, 0);
        Ok(())
    }
}
