# Fixing Lolcommit Images

Existing lolcommit images have two problems:

1. **Wrong repo name** — the `lolcommit:Repo` metadata and filename use the
   directory name (e.g. `worktrees`) instead of the remote-derived name (e.g.
   `sw1nn-lolcommits-rs`). Worktrees are the worst case since their directory
   names are arbitrary.

2. **Old metadata key format** — keys like `lolcommit:revision` display as
   `Lolcommitrevision` in exiftool. The new format `lolcommit:Revision`
   displays as `LolcommitRevision`.

The `lolcommits_fixup` binary fixes both issues.

## Prerequisites

- The project is built locally (`cargo build --release --bin lolcommits_fixup`)
- All git repos whose commits might appear in lolcommit images exist under
  `~/workspace` (or a directory you specify with `--workspace`)
- SSH access to the server hosting the images

## Step-by-step

### 1. Stop the lolcommitsd service on the server

This prevents new images being written while we're working.

```bash
ssh user@server sudo systemctl stop lolcommitsd
```

### 2. Copy images to a local working directory

```bash
rsync -av user@server:/srv/lolcommits/images/ /tmp/lolcommit-fixup/
```

### 3. Dry run — review what will change

```bash
cargo run --release --bin lolcommits_fixup -- \
  --images-dir /tmp/lolcommit-fixup
```

This prints a summary for each image without modifying anything:

```
[fix] worktrees-20260210-123033-be6fb8f...png
  repo: worktrees -> sw1nn-lolcommits-rs
  rename: worktrees-20260210-... -> sw1nn-lolcommits-rs-20260210-...

[keys] my-repo-20260301-...png
  keys: lolcommit:revision -> lolcommit:Revision (and others)

Dry run: 15 repo fixes, 42 key-only updates, 0 skipped. Pass --apply to write changes.
```

- **[fix]** — commit found in a workspace repo, repo name will be corrected and
  file renamed
- **[keys]** — commit not found (or repo name already correct), only metadata
  keys will be capitalized

If the `--workspace` default (`~/workspace`) doesn't contain all your repos,
specify a different root:

```bash
cargo run --release --bin lolcommits_fixup -- \
  --images-dir /tmp/lolcommit-fixup \
  --workspace /path/to/repos
```

### 4. Apply the fixes

```bash
cargo run --release --bin lolcommits_fixup -- \
  --images-dir /tmp/lolcommit-fixup \
  --apply
```

Each image is re-encoded as a lossless RGBA PNG with updated metadata. File
writes are atomic (write to temp file, then rename).

### 5. Verify a sample image

```bash
exiftool /tmp/lolcommit-fixup/sw1nn-lolcommits-rs-20260210-123033-be6fb8f*.png
```

Check that:
- `LolcommitRepo` shows the correct remote-derived name
- `LolcommitRevision`, `LolcommitMessage`, etc. all use the capitalized format
- The filename matches the `LolcommitRepo` value

### 6. Copy fixed images back to the server

```bash
rsync -av --delete /tmp/lolcommit-fixup/ user@server:/srv/lolcommits/images/
```

The `--delete` flag removes old filenames that were renamed. If you prefer to be
cautious, omit `--delete` and manually remove the old files after verifying.

### 7. Restart the lolcommitsd service

```bash
ssh user@server sudo systemctl start lolcommitsd
```

### 8. Clean up

```bash
rm -rf /tmp/lolcommit-fixup
```

## Notes

- Images without any embedded metadata (very old legacy images) are skipped
- If a target filename already exists after renaming (collision), that image is
  skipped with a warning
- The tool searches repos under `--workspace` recursively, skipping `target/`,
  `node_modules/`, `.git/`, and `packaging/arch/` directories
- New images created after this fix will automatically use the correct repo name
  (from the remote) and capitalized keys
