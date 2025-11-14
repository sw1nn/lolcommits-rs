use crate::error::{LolcommitsError, Result};
use git2::Repository;
use std::env;
use std::process::Command;

pub fn get_repo_name() -> Result<String> {
    let repo = open_repo()?;

    let path = repo.path().parent().ok_or(LolcommitsError::NoRepoName)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(LolcommitsError::NoRepoName)?;

    Ok(name.to_string())
}

pub fn get_diff_shortstat() -> Result<String> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--shortstat"])
        .output()?;

    if !output.status.success() {
        return Err(LolcommitsError::GitCommandFailed);
    }

    let stat = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(stat)
}

fn open_repo() -> Result<Repository> {
    let current_dir = env::current_dir()?;
    Repository::discover(current_dir).map_err(|_| LolcommitsError::NotInGitRepo)
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
