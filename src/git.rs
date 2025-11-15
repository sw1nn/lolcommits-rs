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
    use std::fs;
    use tempfile::TempDir;

    // Helper function to create a temporary git repository for testing
    fn create_test_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(temp_dir.path()).unwrap();

        // Configure user for commits
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();

        // Create an initial commit
        let mut index = repo.index().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Initial commit",
            &tree,
            &[],
        )
        .unwrap();

        // Create a second commit with some changes to test diff stats
        fs::write(temp_dir.path().join("test.txt"), "test content\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("test.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Add test file",
            &tree,
            &[&parent],
        )
        .unwrap();

        temp_dir
    }

    #[test]
    fn test_get_repo_name() {
        let temp_dir = create_test_repo();

        // Change to the test repo directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Get the expected name from the actual directory we're in
        let repo_name = temp_dir.path().file_name().unwrap().to_str().unwrap();

        let result = get_repo_name();
        assert!(result.is_ok());
        let name = result.unwrap();
        assert_eq!(name, repo_name);

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_open_repo() {
        let temp_dir = create_test_repo();

        // Change to the test repo directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = open_repo();
        assert!(result.is_ok());

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_get_diff_stats() {
        let temp_dir = create_test_repo();

        // Change to the test repo directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Should return Ok for HEAD commit with valid stats
        let result = get_diff_stats("HEAD");
        assert!(result.is_ok());
        let stats = result.unwrap();

        // The second commit added a file, so we should have:
        // - 1 file changed
        // - 1 insertion (the line we added)
        // - 0 deletions
        assert_eq!(stats.files_changed, 1);
        assert_eq!(stats.insertions, 1);
        assert_eq!(stats.deletions, 0);

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }
}
