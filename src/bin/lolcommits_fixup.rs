use clap::Parser;
use git2::{Oid, Repository};
use owo_colors::OwoColorize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use sw1nn_lolcommits_rs::error::Result;

#[derive(Parser, Debug)]
#[command(name = "lolcommits_fixup")]
#[command(about = "Fix repo names and metadata keys in existing lolcommit images")]
#[command(version)]
struct Args {
    #[arg(long, value_name = "DIR", help = "Directory containing lolcommit PNGs")]
    images_dir: PathBuf,

    #[arg(
        long,
        value_name = "DIR",
        default_value = "~/workspace",
        help = "Root directory to search for git repos"
    )]
    workspace: String,

    #[arg(long, action = clap::ArgAction::SetTrue, help = "Apply changes (default is dry-run)")]
    apply: bool,

    #[arg(
        long,
        value_name = "NAME",
        help = "Keep these repo names even when commit is unresolved (repeatable)"
    )]
    keep_unresolved: Vec<String>,
}

fn expand_tilde<S>(path: S) -> PathBuf
where
    S: AsRef<str>,
{
    let path = path.as_ref();
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

struct RepoInfo {
    repo: Repository,
    remote_name: String,
    // profile will be used in upcoming fingerprinting tasks
    #[allow(dead_code)]
    profile: RepoProfile,
}

const STOPWORDS: &[&str] = &[
    "the", "an", "and", "or", "to", "for", "in", "of", "with", "from", "merge", "branch", "commit",
    "update", "add", "remove", "change", "use", "new", "set", "when", "not", "into", "this",
    "that", "be", "is", "it", "on", "at", "by",
];

struct RepoProfile {
    scopes: HashMap<String, usize>,
    types: HashMap<String, usize>,
    tokens: HashMap<String, usize>,
    messages: HashSet<String>,
    subjects: HashSet<String>,
    commit_count: usize,
}

impl RepoProfile {
    fn new() -> Self {
        Self {
            scopes: HashMap::new(),
            types: HashMap::new(),
            tokens: HashMap::new(),
            messages: HashSet::new(),
            subjects: HashSet::new(),
            commit_count: 0,
        }
    }
}

fn tokenize(text: &str) -> HashSet<String> {
    text.split(|c: char| c.is_ascii_whitespace() || c.is_ascii_punctuation())
        .map(|t| t.to_lowercase())
        .filter(|t| t.len() >= 2)
        .filter(|t| !t.chars().all(|c| c.is_ascii_digit()))
        .filter(|t| !STOPWORDS.contains(&t.as_str()))
        .collect()
}

fn build_repo_profile(repo: &Repository) -> RepoProfile {
    let mut profile = RepoProfile::new();

    let mut revwalk = match repo.revwalk() {
        Ok(rw) => rw,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to create revwalk");
            return profile;
        }
    };

    // Push all local branch heads
    if let Err(e) = revwalk.push_glob("refs/heads/*") {
        tracing::warn!(error = %e, "Failed to push branch refs");
        return profile;
    }

    for oid in revwalk.flatten() {
        let commit = match repo.find_commit(oid) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let message = match commit.message() {
            Some(m) => m.trim().to_owned(),
            None => continue,
        };

        let subject = message.lines().next().unwrap_or(&message).to_owned();

        profile.messages.insert(message.clone());
        profile.subjects.insert(subject.clone());

        // Only extract type/scope from conventional commits (must have ':')
        if subject.contains(':') {
            let commit_type = sw1nn_lolcommits_rs::git::parse_commit_type(&subject);
            if commit_type != "commit" {
                *profile.types.entry(commit_type).or_default() += 1;
            }

            let scope = sw1nn_lolcommits_rs::git::parse_commit_scope(&subject);
            if !scope.is_empty() {
                *profile.scopes.entry(scope).or_default() += 1;
            }
        }

        // Tokenize the stripped message (without conventional prefix)
        let stripped = sw1nn_lolcommits_rs::git::strip_commit_prefix(&subject);
        for token in tokenize(&stripped) {
            *profile.tokens.entry(token).or_default() += 1;
        }

        profile.commit_count += 1;
    }

    tracing::debug!(
        commit_count = profile.commit_count,
        scope_count = profile.scopes.len(),
        type_count = profile.types.len(),
        token_count = profile.tokens.len(),
        message_count = profile.messages.len(),
        "Built repo profile"
    );

    profile
}

const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git"];

fn discover_repos(workspace: &Path) -> Vec<RepoInfo> {
    let mut repos = Vec::new();
    walk_for_repos(workspace, &mut repos);
    tracing::info!(count = repos.len(), "Discovered git repos");
    for info in &repos {
        tracing::debug!(
            path = %info.repo.path().display(),
            remote_name = %info.remote_name,
            "Found repo"
        );
    }
    repos
}

fn walk_for_repos(dir: &Path, repos: &mut Vec<RepoInfo>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(path = %dir.display(), error = %e, "Cannot read directory");
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if SKIP_DIRS.contains(&name) {
            continue;
        }

        // Skip packaging/arch subdirectories (contain makepkg clones)
        if path.ends_with("packaging/arch") {
            continue;
        }

        if path.join(".git").exists() {
            match Repository::open(&path) {
                Ok(repo) => {
                    let remote_name = sw1nn_lolcommits_rs::git::repo_name_from_url(
                        repo.find_remote("origin")
                            .ok()
                            .and_then(|r| r.url().map(|s| s.to_owned()))
                            .as_deref()
                            .unwrap_or(""),
                    )
                    .unwrap_or_else(|| name.to_owned());

                    let profile = build_repo_profile(&repo);
                    tracing::debug!(remote_name, commits = profile.commit_count, "Built profile");
                    repos.push(RepoInfo {
                        repo,
                        remote_name,
                        profile,
                    });
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Cannot open repo");
                }
            }
        }

        walk_for_repos(&path, repos);
    }
}

fn find_commit_repo<'a>(repos: &'a [RepoInfo], sha: &str) -> Option<&'a RepoInfo> {
    let oid = match Oid::from_str(sha) {
        Ok(oid) => oid,
        Err(_) => return None,
    };

    repos.iter().find(|info| info.repo.find_commit(oid).is_ok())
}

enum FixAction {
    Fix {
        old_repo: String,
        new_repo: String,
        old_filename: String,
        new_filename: String,
    },
    KeysOnly,
    Skip,
}

fn plan_fix(
    path: &Path,
    repos: &[RepoInfo],
    keep_unresolved: &[String],
) -> (FixAction, Option<sw1nn_lolcommits_rs::git::CommitMetadata>) {
    let metadata = match sw1nn_lolcommits_rs::image_metadata::read_png_metadata(path) {
        Ok(Some(m)) => m,
        Ok(None) => return (FixAction::Skip, None),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "Cannot read metadata");
            return (FixAction::Skip, None);
        }
    };

    if metadata.revision.is_empty() {
        return (FixAction::Skip, Some(metadata));
    }

    let found_repo = find_commit_repo(repos, &metadata.revision);

    let new_repo_name = match found_repo {
        Some(info) if info.remote_name != metadata.repo_name => Some(info.remote_name.clone()),
        None if !keep_unresolved.contains(&metadata.repo_name) => Some("unknown".to_owned()),
        _ => None,
    };

    match new_repo_name {
        Some(new_repo) => {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let new_filename = filename.replacen(&metadata.repo_name, &new_repo, 1);

            (
                FixAction::Fix {
                    old_repo: metadata.repo_name.clone(),
                    new_repo,
                    old_filename: filename.to_owned(),
                    new_filename,
                },
                Some(metadata),
            )
        }
        None => (FixAction::KeysOnly, Some(metadata)),
    }
}

fn apply_fix(
    path: &Path,
    metadata: &sw1nn_lolcommits_rs::git::CommitMetadata,
    new_path: &Path,
) -> Result<()> {
    let img = image::open(path)?;

    let temp_file = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| std::io::Error::other("Invalid path"))?,
    )?;

    sw1nn_lolcommits_rs::image_metadata::save_png_with_metadata(&img, temp_file.path(), metadata)?;

    temp_file.persist(new_path).map_err(|e| e.error)?;

    // If the new path differs from the old, remove the old file
    if path != new_path && path.exists() {
        std::fs::remove_file(path)?;
    }

    Ok(())
}

fn run_fixup(
    images_dir: &Path,
    repos: &[RepoInfo],
    keep_unresolved: &[String],
    apply: bool,
) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(images_dir)?
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut fix_count = 0u32;
    let mut keys_only_count = 0u32;
    let mut skip_count = 0u32;
    let mut unresolved_repos: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();

    for entry in &entries {
        let path = entry.path();
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        let (action, metadata) = plan_fix(&path, repos, keep_unresolved);

        match action {
            FixAction::Fix {
                ref old_repo,
                ref new_repo,
                ref old_filename,
                ref new_filename,
            } => {
                if new_repo == "unknown" {
                    *unresolved_repos.entry(old_repo.clone()).or_default() += 1;
                }

                println!("{} {filename}", "[fix]".green());
                println!("  repo: {old_repo} -> {}", new_repo.cyan());
                println!("  rename: {old_filename} -> {}", new_filename.cyan());

                if apply {
                    let mut updated = metadata.unwrap();
                    updated.repo_name = new_repo.clone();
                    let new_path = images_dir.join(new_filename);

                    if new_path.exists() && new_path != path {
                        eprintln!(
                            "  {} target already exists, skipping: {}",
                            "warning:".yellow(),
                            new_path.display()
                        );
                        continue;
                    }

                    apply_fix(&path, &updated, &new_path)?;
                    println!("  {}", "applied".green());
                }
                fix_count += 1;
            }
            FixAction::KeysOnly => {
                println!("{} {filename}", "[keys]".blue());
                println!("  keys: lolcommit:revision -> lolcommit:Revision (and others)");

                if apply {
                    let updated = metadata.unwrap();
                    apply_fix(&path, &updated, &path)?;
                    println!("  {}", "applied".green());
                }
                keys_only_count += 1;
            }
            FixAction::Skip => {
                tracing::debug!(filename, "No metadata, skipping");
                skip_count += 1;
            }
        }
    }

    println!();
    if apply {
        println!(
            "Done: {fix_count} repo fixes, {keys_only_count} key-only updates, {skip_count} skipped"
        );
    } else {
        println!(
            "Dry run: {fix_count} repo fixes, {keys_only_count} key-only updates, {skip_count} skipped. Pass {} to write changes.",
            "--apply".cyan()
        );
    }

    if !unresolved_repos.is_empty() {
        let mut sorted: Vec<_> = unresolved_repos.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));

        println!();
        println!(
            "{} (commit not found, will be renamed to 'unknown'):",
            "Unresolved repo names".yellow()
        );
        for (name, count) in &sorted {
            println!("  {count:>4}  {name}");
        }
        println!();
        println!(
            "If any of these are legitimate, re-run with {}",
            "--keep-unresolved <NAME>".cyan()
        );
    }

    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let workspace = expand_tilde(&args.workspace);

    tracing::info!(
        images_dir = %args.images_dir.display(),
        workspace = %workspace.display(),
        apply = args.apply,
        "Starting lolcommits fixup"
    );

    if !args.images_dir.is_dir() {
        eprintln!("Error: {} is not a directory", args.images_dir.display());
        std::process::exit(1);
    }

    if !workspace.is_dir() {
        eprintln!("Error: {} is not a directory", workspace.display());
        std::process::exit(1);
    }

    let repos = discover_repos(&workspace);

    if repos.is_empty() {
        eprintln!("Warning: no git repos found under {}", workspace.display());
    }

    run_fixup(&args.images_dir, &repos, &args.keep_unresolved, args.apply)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_basic() -> Result<()> {
        let tokens = tokenize("add webcam capture support");
        assert!(tokens.contains("webcam"));
        assert!(tokens.contains("capture"));
        assert!(tokens.contains("support"));
        assert!(!tokens.contains("add"));
        Ok(())
    }

    #[test]
    fn test_tokenize_strips_short_tokens() -> Result<()> {
        let tokens = tokenize("a b cd ef");
        assert!(!tokens.contains("a"));
        assert!(!tokens.contains("b"));
        assert!(tokens.contains("cd"));
        assert!(tokens.contains("ef"));
        Ok(())
    }

    #[test]
    fn test_tokenize_strips_pure_numbers() -> Result<()> {
        let tokens = tokenize("bump version 42 to v2");
        assert!(!tokens.contains("42"));
        assert!(tokens.contains("v2"));
        assert!(tokens.contains("version"));
        Ok(())
    }

    #[test]
    fn test_tokenize_lowercases() -> Result<()> {
        let tokens = tokenize("OpenCV Camera Module");
        assert!(tokens.contains("opencv"));
        assert!(tokens.contains("camera"));
        assert!(tokens.contains("module"));
        Ok(())
    }

    #[test]
    fn test_tokenize_splits_on_punctuation() -> Result<()> {
        let tokens = tokenize("fix(server): handle timeout/retry");
        assert!(tokens.contains("server"));
        assert!(tokens.contains("handle"));
        assert!(tokens.contains("timeout"));
        assert!(tokens.contains("retry"));
        Ok(())
    }

    #[test]
    fn test_build_profile_from_repo() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let repo = git2::Repository::init(dir.path())?;

        let mut config = repo.config()?;
        config.set_str("user.name", "Test")?;
        config.set_str("user.email", "test@test.com")?;

        let sig = repo.signature()?;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "feat(server): add upload endpoint",
            &tree,
            &[],
        )?;

        let parent = repo.head()?.peel_to_commit()?;
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "fix(server): handle timeout",
            &tree,
            &[&parent],
        )?;

        let parent = repo.head()?.peel_to_commit()?;
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "feat(capture): add webcam support",
            &tree,
            &[&parent],
        )?;

        let parent = repo.head()?.peel_to_commit()?;
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "non-conventional message about cameras",
            &tree,
            &[&parent],
        )?;

        let profile = build_repo_profile(&repo);
        assert_eq!(profile.commit_count, 4);

        assert_eq!(profile.scopes.get("server"), Some(&2));
        assert_eq!(profile.scopes.get("capture"), Some(&1));

        assert_eq!(profile.types.get("feat"), Some(&2));
        assert_eq!(profile.types.get("fix"), Some(&1));
        assert!(!profile.types.contains_key("commit"));

        assert!(
            profile
                .messages
                .contains("feat(server): add upload endpoint")
        );
        assert!(
            profile
                .subjects
                .contains("feat(server): add upload endpoint")
        );

        assert!(profile.tokens.contains_key("upload"));
        assert!(profile.tokens.contains_key("endpoint"));
        assert!(profile.tokens.contains_key("webcam"));
        assert!(profile.tokens.contains_key("cameras"));

        Ok(())
    }
}
