use crate::error::{LolcommitsError, Result};
use git2::Repository;
use std::env;

pub fn get_repo_name() -> Result<String> {
    let repo = open_repo()?;

    let path = repo.path().parent().ok_or(LolcommitsError::NoRepoName)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(LolcommitsError::NoRepoName)?;

    Ok(name.to_string())
}

fn open_repo() -> Result<Repository> {
    let current_dir = env::current_dir()?;
    Repository::discover(current_dir).map_err(|_| LolcommitsError::NotInGitRepo)
}
