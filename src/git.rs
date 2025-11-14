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
        return Err(LolcommitsError::GitCommandFailed.into());
    }

    let stat = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(stat)
}

fn open_repo() -> Result<Repository> {
    let current_dir = env::current_dir()?;
    Repository::discover(current_dir).map_err(|_| LolcommitsError::NotInGitRepo)
}
