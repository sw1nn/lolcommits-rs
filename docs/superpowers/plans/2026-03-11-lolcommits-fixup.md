# Lolcommits Fixup Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development
> (if subagents available) or superpowers:executing-plans to implement this plan.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a `lolcommits_fixup` binary that corrects repo names (from
directory-derived to remote-derived) and capitalizes metadata keys in existing
lolcommit PNG images.

**Architecture:** A single-pass CLI tool: discover git repos under a workspace
directory, scan PNG images from a local images directory, match each image's
commit SHA to a repo to derive the correct name, then re-encode each PNG with
updated metadata and rename the file. Dry-run by default.

**Tech Stack:** Rust, clap (already a dependency), git2 (already a dependency),
image + png crates (already dependencies), tempfile (already a dependency).

**Spec:** `docs/superpowers/specs/2026-03-11-lolcommits-fixup-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/image_metadata.rs` (modify) | Capitalize metadata keys in write; accept both old and new keys in read |
| `src/git.rs` (modify) | Make `repo_name_from_url` public |
| `src/bin/lolcommits_fixup.rs` (create) | CLI binary: arg parsing, repo discovery, image scanning, fixup logic |

---

## Chunk 1: Capitalize metadata keys (forward-compatible)

### Task 1: Make `repo_name_from_url` public in git.rs

**Files:**
- Modify: `src/git.rs:107`

- [ ] **Step 1: Change visibility of `repo_name_from_url`**

In `src/git.rs`, change line 107 from:

```rust
fn repo_name_from_url(url: &str) -> Option<String> {
```

to:

```rust
pub fn repo_name_from_url(url: &str) -> Option<String> {
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: success (no callers yet outside the module, but tests already use it internally)

- [ ] **Step 3: Commit**

```
feat(git): make repo_name_from_url public for use by fixup utility
```

### Task 2: Capitalize metadata keys in `save_png_with_metadata`

**Files:**
- Modify: `src/image_metadata.rs:26-52`

- [ ] **Step 1: Update all key strings in `save_png_with_metadata`**

Change each `add_itxt_chunk` call to use capitalized tag names:

```rust
encoder.add_itxt_chunk("lolcommit:Revision".to_owned(), metadata.revision.clone())?;
encoder.add_itxt_chunk("lolcommit:Message".to_owned(), metadata.message.clone())?;
encoder.add_itxt_chunk("lolcommit:Type".to_owned(), metadata.commit_type.clone())?;
```

And for the conditional scope:

```rust
if !metadata.scope.is_empty() {
    encoder.add_itxt_chunk("lolcommit:Scope".to_owned(), metadata.scope.clone())?;
}
```

And the remaining keys:

```rust
encoder.add_itxt_chunk("lolcommit:Timestamp".to_owned(), metadata.timestamp.clone())?;
encoder.add_itxt_chunk("lolcommit:Repo".to_owned(), metadata.repo_name.clone())?;
encoder.add_itxt_chunk("lolcommit:Branch".to_owned(), metadata.branch_name.clone())?;
encoder.add_itxt_chunk("lolcommit:Diff".to_owned(), metadata.diff_stats_string())?;
encoder.add_itxt_chunk("lolcommit:Files_changed".to_owned(), metadata.stats.files_changed.to_string())?;
encoder.add_itxt_chunk("lolcommit:Insertions".to_owned(), metadata.stats.insertions.to_string())?;
encoder.add_itxt_chunk("lolcommit:Deletions".to_owned(), metadata.stats.deletions.to_string())?;
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: success

- [ ] **Step 3: Commit**

```
feat(metadata): capitalize metadata tag names for better exiftool display
```

### Task 3: Accept both old and new keys in `read_png_metadata`

**Files:**
- Modify: `src/image_metadata.rs:83-101`

- [ ] **Step 1: Write a helper to read a key with fallback**

Add this helper function inside `image_metadata.rs` (above `read_png_metadata`
or at the bottom of the file, before tests):

```rust
/// Remove a key from the map, trying the new capitalized key first,
/// then falling back to the old lowercase key.
fn remove_key(chunks: &mut HashMap<&str, String>, new_key: &str, old_key: &str) -> String {
    chunks
        .remove(new_key)
        .or_else(|| chunks.remove(old_key))
        .unwrap_or_default()
}
```

- [ ] **Step 2: Update `read_png_metadata` to use the helper**

Replace the block of `chunks.remove(...)` calls (lines 83-101) with:

```rust
let revision = remove_key(&mut chunks, "lolcommit:Revision", "lolcommit:revision");
let message = remove_key(&mut chunks, "lolcommit:Message", "lolcommit:message");
let commit_type = remove_key(&mut chunks, "lolcommit:Type", "lolcommit:type");
let scope = remove_key(&mut chunks, "lolcommit:Scope", "lolcommit:scope");
let timestamp = remove_key(&mut chunks, "lolcommit:Timestamp", "lolcommit:timestamp");
let repo_name = remove_key(&mut chunks, "lolcommit:Repo", "lolcommit:repo");
let branch_name = remove_key(&mut chunks, "lolcommit:Branch", "lolcommit:branch");
let files_changed = remove_key(&mut chunks, "lolcommit:Files_changed", "lolcommit:files_changed")
    .parse()
    .unwrap_or(0);
let insertions = remove_key(&mut chunks, "lolcommit:Insertions", "lolcommit:insertions")
    .parse()
    .unwrap_or(0);
let deletions = remove_key(&mut chunks, "lolcommit:Deletions", "lolcommit:deletions")
    .parse()
    .unwrap_or(0);
```

- [ ] **Step 3: Run existing tests**

Run: `cargo test`
Expected: all tests pass (existing tests don't directly test read_png_metadata
round-trip, but this verifies nothing is broken)

- [ ] **Step 4: Commit**

```
feat(metadata): read both old and new capitalized metadata keys
```

### Task 4: Add round-trip test for metadata read/write

**Files:**
- Modify: `src/image_metadata.rs` (add `#[cfg(test)] mod tests` block at end)

- [ ] **Step 1: Write the test**

Add at the end of `src/image_metadata.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::git::DiffStats;

    #[test]
    fn test_round_trip_new_keys() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.png");

        let img = DynamicImage::new_rgba8(2, 2);
        let metadata = CommitMetadata {
            path: std::path::PathBuf::new(),
            revision: "abc123".to_owned(),
            message: "feat: add thing".to_owned(),
            commit_type: "feat".to_owned(),
            scope: "core".to_owned(),
            timestamp: "2026-03-11 12:00:00".to_owned(),
            repo_name: "my-repo".to_owned(),
            branch_name: "main".to_owned(),
            stats: DiffStats {
                files_changed: 3,
                insertions: 10,
                deletions: 2,
            },
        };

        save_png_with_metadata(&img, &path, &metadata)?;
        let read_back = read_png_metadata(&path)?;

        assert!(read_back.is_some());
        let m = read_back.unwrap();
        assert_eq!(m.revision, "abc123");
        assert_eq!(m.message, "feat: add thing");
        assert_eq!(m.commit_type, "feat");
        assert_eq!(m.scope, "core");
        assert_eq!(m.timestamp, "2026-03-11 12:00:00");
        assert_eq!(m.repo_name, "my-repo");
        assert_eq!(m.branch_name, "main");
        assert_eq!(m.stats.files_changed, 3);
        assert_eq!(m.stats.insertions, 10);
        assert_eq!(m.stats.deletions, 2);
        Ok(())
    }

    #[test]
    fn test_reads_old_lowercase_keys() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("old.png");

        // Manually write a PNG with old lowercase keys
        let img = DynamicImage::new_rgba8(2, 2);
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let file = File::create(&path)?;
        let writer = BufWriter::new(file);
        let mut encoder = Encoder::new(writer, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.add_itxt_chunk("lolcommit:revision".to_owned(), "old-sha".to_owned())?;
        encoder.add_itxt_chunk("lolcommit:repo".to_owned(), "old-repo".to_owned())?;
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&rgba)?;
        drop(writer);

        let read_back = read_png_metadata(&path)?;
        assert!(read_back.is_some());
        let m = read_back.unwrap();
        assert_eq!(m.revision, "old-sha");
        assert_eq!(m.repo_name, "old-repo");
        Ok(())
    }
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test --lib image_metadata::tests`
Expected: PASS

- [ ] **Step 3: Commit**

```
test(metadata): add round-trip test for PNG metadata read/write
```

---

## Chunk 2: The fixup binary

**Note:** Tasks 5-7 build up `src/bin/lolcommits_fixup.rs` incrementally. The
`use` statements shown in each task should be consolidated at the top of the
file, not duplicated.

### Task 5: Create `lolcommits_fixup` binary with arg parsing

**Files:**
- Create: `src/bin/lolcommits_fixup.rs`

- [ ] **Step 1: Write the binary with arg parsing and stub main**

```rust
use clap::Parser;
use std::path::PathBuf;
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
}

fn expand_tilde<S>(path: S) -> PathBuf
where
    S: AsRef<str>,
{
    let path = path.as_ref();
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
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

    // TODO: discover repos, scan images, apply fixes
    Ok(())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check --bin lolcommits_fixup`
Expected: success

- [ ] **Step 3: Commit**

```
feat(fixup): scaffold lolcommits_fixup binary with arg parsing
```

### Task 6: Implement repo discovery

**Files:**
- Modify: `src/bin/lolcommits_fixup.rs`

- [ ] **Step 1: Add repo discovery function**

Add this after the `expand_tilde` function:

```rust
use git2::{Oid, Repository};

struct RepoInfo {
    repo: Repository,
    remote_name: String,
}

/// Walk workspace directory to find git repos and their remote-derived names.
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

const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git"];

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

        // Skip well-known non-repo directories
        if SKIP_DIRS.contains(&name) {
            continue;
        }

        // Skip packaging/arch subdirectories (contain makepkg clones)
        if path.ends_with("packaging/arch") {
            continue;
        }

        // If this directory contains a .git, it's a repo
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
            // Still recurse into the repo directory for nested repos (e.g.
            // submodules), but the SKIP_DIRS list will prevent recursing into
            // .git itself
        }

        walk_for_repos(&path, repos);
    }
}

/// Find which repo contains a given commit SHA.
fn find_commit_repo<'a>(repos: &'a [RepoInfo], sha: &str) -> Option<&'a RepoInfo> {
    let oid = match Oid::from_str(sha) {
        Ok(oid) => oid,
        Err(_) => return None,
    };

    repos.iter().find(|info| info.repo.find_commit(oid).is_ok())
}
```

- [ ] **Step 2: Wire discovery into main**

Replace the `// TODO` comment in main with:

```rust
let repos = discover_repos(&workspace);

if repos.is_empty() {
    eprintln!("Warning: no git repos found under {}", workspace.display());
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check --bin lolcommits_fixup`
Expected: success

- [ ] **Step 4: Commit**

```
feat(fixup): implement workspace repo discovery with skip list
```

### Task 7: Implement image scanning and fixup logic

**Files:**
- Modify: `src/bin/lolcommits_fixup.rs`

- [ ] **Step 1: Add the image scanning and fixup function**

Add this after `find_commit_repo`:

```rust
use owo_colors::OwoColorize;
use std::path::Path;

enum FixAction {
    /// Repo name changed + keys capitalized
    Fix {
        old_repo: String,
        new_repo: String,
        old_filename: String,
        new_filename: String,
    },
    /// Only keys capitalized (commit not found or repo unchanged)
    KeysOnly,
    /// No metadata found, skip
    Skip,
}

fn plan_fix(
    path: &Path,
    repos: &[RepoInfo],
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

    match found_repo {
        Some(info) if info.remote_name != metadata.repo_name => {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let new_filename =
                filename.replacen(&metadata.repo_name, &info.remote_name, 1);

            (
                FixAction::Fix {
                    old_repo: metadata.repo_name.clone(),
                    new_repo: info.remote_name.clone(),
                    old_filename: filename.to_owned(),
                    new_filename,
                },
                Some(metadata),
            )
        }
        _ => (FixAction::KeysOnly, Some(metadata)),
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

    sw1nn_lolcommits_rs::image_metadata::save_png_with_metadata(
        &img,
        temp_file.path(),
        metadata,
    )?;

    temp_file
        .persist(new_path)
        .map_err(|e| e.error)?;

    // If the new path differs from the old, remove the old file
    if path != new_path && path.exists() {
        std::fs::remove_file(path)?;
    }

    Ok(())
}

fn run_fixup(images_dir: &Path, repos: &[RepoInfo], apply: bool) -> Result<()> {
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

    for entry in &entries {
        let path = entry.path();
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        let (action, metadata) = plan_fix(&path, repos);

        match action {
            FixAction::Fix {
                ref old_repo,
                ref new_repo,
                ref old_filename,
                ref new_filename,
            } => {
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
                println!(
                    "  keys: lolcommit:revision -> lolcommit:Revision (and others)"
                );

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
            "Done: {} repo fixes, {} key-only updates, {} skipped",
            fix_count, keys_only_count, skip_count
        );
    } else {
        println!(
            "Dry run: {} repo fixes, {} key-only updates, {} skipped. Pass {} to write changes.",
            fix_count,
            keys_only_count,
            skip_count,
            "--apply".cyan()
        );
    }

    Ok(())
}
```

- [ ] **Step 2: Wire `run_fixup` into main**

Add after the repo discovery block in main:

```rust
run_fixup(&args.images_dir, &repos, args.apply)?;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check --bin lolcommits_fixup`
Expected: success

- [ ] **Step 4: Commit**

```
feat(fixup): implement image scanning, metadata fixup, and file renaming
```

---

## Chunk 3: Lint, full test, and final commit

### Task 8: Run full test suite and lint

- [ ] **Step 1: Format**

Run: `cargo fmt`

- [ ] **Step 2: Clippy**

Run: `cargo clippy --all-targets`
Expected: no warnings (fix any that appear)

- [ ] **Step 3: Test**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 4: Commit any lint fixes**

```
style: apply cargo fmt and clippy fixes
```

### Task 9: Manual smoke test

- [ ] **Step 1: Build**

Run: `cargo build --bin lolcommits_fixup`

- [ ] **Step 2: Test help output**

Run: `cargo run --bin lolcommits_fixup -- --help`
Expected: shows usage with `--images-dir`, `--workspace`, `--apply` flags

- [ ] **Step 3: Test dry run against a test directory**

Create a small test directory with a known lolcommit image (if available) or
verify the tool runs cleanly against an empty directory:

Run: `cargo run --bin lolcommits_fixup -- --images-dir /tmp/test-images --workspace ~/workspace`
Expected: completes without error, shows summary line
