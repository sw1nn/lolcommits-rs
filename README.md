# lolcommits-rs

A Rust implementation of [lolcommits](https://lolcommits.github.io/) - automatically capture webcam snapshots when you make git commits!

## Overview

`lolcommits-rs` integrates with your git workflow to take a photo using your webcam every time you commit. Each snapshot is annotated with commit information including:

- Commit message and SHA
- Repository name
- Diff statistics
- Conventional commit type badge (feat, fix, chore, etc.)

The tool uses OpenCV for face detection and segmentation to replace the background, creating fun and personalized commit snapshots that are stored locally in `~/.local/share/lolcommits-rs/`.

## Features

- **Webcam Integration**: Automatically captures photos during git commits
- **Face Detection**: Uses OpenCV DNN with face segmentation models
- **Background Replacement**: Applies customizable background colors/images
- **Commit Type Badges**: Displays conventional commit type badges on snapshots
- **Configurable**: Customize colors, fonts, and image processing settings via TOML config
- **Automatic Cleanup**: Optional systemd-tmpfiles integration for managing old snapshots

## Installation

### Building from Source

```bash
cargo build --release
```

### Git Hook Setup

To automatically capture snapshots on every commit, add this project as a git post-commit hook:

```bash
# In your repository
echo '#!/bin/sh' > .git/hooks/post-commit
echo 'lolcommits "$1" "$2"' >> .git/hooks/post-commit
chmod +x .git/hooks/post-commit
```

## Configuration

Configuration is stored in `~/.config/lolcommits-rs/config.toml`. The tool will use sensible defaults if no config file exists.

---

# Automatic Cleanup with systemd-tmpfiles

The included sample configuration file can be used with `systemd-tmpfiles` to automatically clean up old lolcommit images.

## Installation

Copy the sample configuration file to your user tmpfiles directory:

```bash
mkdir -p ~/.config/user-tmpfiles.d
cp assets/user-tmpfiles.d.sample ~/.config/user-tmpfiles.d/lolcommits.conf
```

## Usage

### Manual Cleanup

To manually trigger cleanup based on the rules:

```bash
systemd-tmpfiles --user --clean
```

### Automatic Cleanup

Enable the systemd-provided timer for automatic periodic cleanup:

```bash
systemctl --user enable --now systemd-tmpfiles-clean.timer
```

Check the timer status:

```bash
systemctl --user status systemd-tmpfiles-clean.timer
```

## Configuration

The default configuration deletes PNG images older than 30 days from `~/.local/share/lolcommits-rs/`.

To customize, edit `~/.config/user-tmpfiles.d/lolcommits.conf`:

- Change `30d` to a different value (e.g., `60d`, `90d`, `1y`)
- Uncomment alternative rules as needed

## Testing

To test without actually deleting files:

```bash
systemd-tmpfiles --user --clean --dry-run
```

## References

- [systemd-tmpfiles(8)](https://www.freedesktop.org/software/systemd/man/systemd-tmpfiles.html)
- [tmpfiles.d(5)](https://www.freedesktop.org/software/systemd/man/tmpfiles.d.html)
