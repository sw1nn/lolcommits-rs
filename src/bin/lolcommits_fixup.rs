use clap::Parser;
use git2::{Oid, Repository};
use owo_colors::OwoColorize;
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

                    repos.push(RepoInfo { repo, remote_name });
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
