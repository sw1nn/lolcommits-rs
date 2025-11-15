use crate::error::{Error::*, Result};
use git2::Repository;
use serde::{Deserialize, Serialize};
use std::env;
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
    pub sha: String,
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

pub fn get_repo_name() -> Result<String> {
    let repo = open_repo()?;

    let path = repo.path().parent().ok_or(NoRepoName)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(NoRepoName)?;

    Ok(name.to_string())
}

/// Get diff stats for a commit using git show --numstat
pub fn get_diff_stats(sha: &str) -> Result<DiffStats> {
    let output = Command::new("git")
        .args(["show", "--numstat", "--format=", sha])
        .output()?;

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
            if parts[0] != "-" {
                if let Ok(insertions) = parts[0].parse::<u32>() {
                    total_insertions += insertions;
                }
            }
            if parts[1] != "-" {
                if let Ok(deletions) = parts[1].parse::<u32>() {
                    total_deletions += deletions;
                }
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

pub fn get_branch_name() -> Result<String> {
    let repo = open_repo()?;
    let head = repo.head()?;

    if let Some(branch_name) = head.shorthand() {
        Ok(branch_name.to_string())
    } else {
        Ok("HEAD".to_string())
    }
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

fn open_repo() -> Result<Repository> {
    let current_dir = env::current_dir()?;
    Repository::discover(current_dir).map_err(|_| NotInGitRepo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_repo_name() {
        // This test will work when run from within a git repo
        // Since we're in sw1nn-lolcommits-rs repo, it should succeed
        let result = get_repo_name();
        assert!(result.is_ok());
        let repo_name = result.unwrap();
        assert_eq!(repo_name, "sw1nn-lolcommits-rs");
    }

    #[test]
    fn test_open_repo() {
        // Should successfully open the current repo
        let result = open_repo();
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_diff_stats() {
        // Should return Ok for HEAD commit with valid stats
        let result = get_diff_stats("HEAD");
        assert!(result.is_ok());
        let stats = result.unwrap();
        // All values should be non-negative (may be 0 for empty commits)
        assert!(stats.files_changed >= 0);
        assert!(stats.insertions >= 0);
        assert!(stats.deletions >= 0);
    }
}
