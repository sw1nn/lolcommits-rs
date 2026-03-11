# Lolcommits Fixup Utility

## Problem

Lolcommit images have two issues:

1. **Wrong repo name**: The `lolcommit:repo` metadata value and filename use the
   directory name (e.g. `worktrees`) instead of the remote-derived repo name
   (e.g. `sw1nn-lolcommits-rs`). This is especially wrong for worktrees, which
   have arbitrary directory names.

2. **Uncapitalized metadata keys**: Keys like `lolcommit:revision` display
   poorly in exiftool (shows `Lolcommitrevision`). Capitalizing the tag portion
   (`lolcommit:Revision`) makes exiftool display `LolcommitRevision` — much
   more readable.

## Solution

### New binary: `lolcommits_fixup`

A CLI utility that takes an images directory, fixes metadata and filenames.

**Usage:**

```
lolcommits_fixup --images-dir /path/to/images [--workspace ~/workspace] [--apply]
```

- `--images-dir` (required): Path to the directory containing lolcommit PNGs
- `--workspace` (optional, default `~/workspace`): Root directory to search for
  git repos
- `--apply`: Actually write changes. Without this flag, dry-run only.

**Algorithm per image:**

1. Read existing PNG metadata (both old `lolcommit:x` and new `lolcommit:X`
   keys)
2. Extract the `revision` (commit SHA) from metadata
3. Search all git repos under `--workspace` for this commit:
   - Walk `--workspace` recursively for `.git` directories (skip nested `.git`
     inside `.git`)
   - Open each as a `Repository`, try `repo.find_commit(oid)`
   - If found, derive repo name from `origin` remote URL using existing
     `repo_name_from_url()`
4. Build updated metadata:
   - If commit found: update `repo` value to remote-derived name
   - If commit not found: keep existing `repo` value unchanged
   - All keys use capitalized format (`lolcommit:Revision`, `lolcommit:Repo`,
     etc.)
5. Re-encode the PNG: read pixels via `image` crate, write new file via
   `save_png_with_metadata` with updated metadata
6. Rename file if repo name changed: `{old}-{ts}-{sha}.png` →
   `{new}-{ts}-{sha}.png`

**Dry-run output example:**

```
[fix] worktrees-20260210-123033-be6fb8f...png
  repo: worktrees -> sw1nn-lolcommits-rs
  rename: worktrees-20260210-... -> sw1nn-lolcommits-rs-20260210-...
  keys: lolcommit:revision -> lolcommit:Revision (and 10 others)

[keys-only] other-repo-20260301-...png
  keys: lolcommit:revision -> lolcommit:Revision (and 10 others)
  (commit not found in workspace repos, repo name unchanged)

Dry run: 15 images would be updated. Pass --apply to write changes.
```

### Forward-compatible key changes

**`src/image_metadata.rs` changes:**

Write keys (in `save_png_with_metadata`):

| Old key                    | New key                     |
|----------------------------|-----------------------------|
| `lolcommit:revision`       | `lolcommit:Revision`        |
| `lolcommit:message`        | `lolcommit:Message`         |
| `lolcommit:type`           | `lolcommit:Type`            |
| `lolcommit:scope`          | `lolcommit:Scope`           |
| `lolcommit:timestamp`      | `lolcommit:Timestamp`       |
| `lolcommit:repo`           | `lolcommit:Repo`            |
| `lolcommit:branch`         | `lolcommit:Branch`          |
| `lolcommit:diff`           | `lolcommit:Diff`            |
| `lolcommit:files_changed`  | `lolcommit:Files_changed`   |
| `lolcommit:insertions`     | `lolcommit:Insertions`      |
| `lolcommit:deletions`      | `lolcommit:Deletions`       |

Read keys (in `read_png_metadata`): Accept both old and new format. New key
takes priority if both are present (unlikely but defensive).

### Repo discovery

At startup, walk `--workspace` to build a map of all git repos:

- Skip directories named `target`, `node_modules`, `.git`
  — don't recurse into them
- Skip `packaging/arch` subdirectories within repos (these contain cloned
  git repos from `makepkg` that would produce false commit matches)
- For each repo found, cache: `repo_path -> (Repository, remote_repo_name)`
- For commit lookup, iterate all cached repos and try
  `repo.find_commit(Oid::from_str(sha))`

This is O(repos * images) but both numbers are small (tens of repos, hundreds of
images).

### Safety

- Dry-run by default — no changes without `--apply`
- Write to a temp file first, then atomically rename (same pattern as
  `process_image_async` in server.rs)
- If the target filename already exists (collision), log a warning and skip
- Re-encoding is lossless (RGBA 8-bit PNG)

### Files changed

| File | Change |
|------|--------|
| `src/image_metadata.rs` | Capitalize keys in write, accept both in read |
| `src/git.rs` | Make `repo_name_from_url` public |
| `src/bin/lolcommits_fixup.rs` | New binary |
| `Cargo.toml` | Add `clap` if not already present (for arg parsing) |

### Workflow: fixing images on a remote server

The images live on a remote server (e.g. `/srv/lolcommits/images`). To fix them:

```bash
# 1. Copy images from remote to a local working directory
rsync -av user@server:/srv/lolcommits/images/ /tmp/lolcommit-fixup/

# 2. Dry run to review what will change
cargo run --bin lolcommits_fixup -- --images-dir /tmp/lolcommit-fixup

# 3. Apply fixes
cargo run --bin lolcommits_fixup -- --images-dir /tmp/lolcommit-fixup --apply

# 4. Copy fixed images back to the remote server
rsync -av /tmp/lolcommit-fixup/ user@server:/srv/lolcommits/images/

# 5. Clean up local copy
rm -rf /tmp/lolcommit-fixup
```

### Out of scope

- Changing the filename format itself (stays `{repo}-{timestamp}-{sha}.png`)
- Handling images without any metadata (legacy filename-only images)
