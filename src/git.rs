use crate::error::{Error::*, Result};
use git2::Repository;
use std::env;
use std::process::Command;

pub fn get_repo_name() -> Result<String> {
    let repo = open_repo()?;

    let path = repo.path().parent().ok_or(NoRepoName)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(NoRepoName)?;

    Ok(name.to_string())
}

pub fn get_diff_shortstat() -> Result<String> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--shortstat"])
        .output()?;

    if !output.status.success() {
        return Err(GitCommandFailed);
    }

    let stat = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(stat)
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

/// Parse git diff shortstat output to extract files changed, insertions, and deletions
/// Example input: "4 files changed, 137 insertions(+), 86 deletions(-)"
/// Returns: (files_changed, insertions, deletions)
pub fn parse_diff_stats(stats: &str) -> (u32, u32, u32) {
    let mut files_changed = 0;
    let mut insertions = 0;
    let mut deletions = 0;

    // Split by comma and parse each part
    for part in stats.split(',') {
        let part = part.trim();

        if part.contains("file") && part.contains("changed") {
            // Extract number before "file"
            if let Some(num_str) = part.split_whitespace().next() {
                files_changed = num_str.parse().unwrap_or(0);
            }
        } else if part.contains("insertion") {
            // Extract number before "insertion"
            if let Some(num_str) = part.split_whitespace().next() {
                insertions = num_str.parse().unwrap_or(0);
            }
        } else if part.contains("deletion") {
            // Extract number before "deletion"
            if let Some(num_str) = part.split_whitespace().next() {
                deletions = num_str.parse().unwrap_or(0);
            }
        }
    }

    (files_changed, insertions, deletions)
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
    fn test_get_diff_shortstat() {
        // Should return Ok even if there are no staged changes
        // The string might be empty but the operation should succeed
        let result = get_diff_shortstat();
        assert!(result.is_ok());
    }
}
