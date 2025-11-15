use crate::error::{Error::*, Result};
use git2::Repository;
use std::env;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub struct CommitMetadata {
    pub message: String,
    pub commit_type: String,
    pub scope: String,
    pub repo_name: String,
    pub sha: String,
    pub stats: DiffStats,
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
