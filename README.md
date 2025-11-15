# lolcommits

A Rust implementation of [lolcommits](https://lolcommits.github.io/) - automatically capture webcam snapshots when you make git commits!

## Overview

`lolcommits` integrates with your git workflow to take a photo using your webcam every time you commit. Each snapshot is annotated with commit information including:

- Commit message and SHA
- Repository name
- Diff statistics
- Conventional commit type badge (feat, fix, chore, etc.)

The tool uses OpenCV for face detection and segmentation to replace the background, creating fun and personalized commit snapshots that are stored locally in `~/.local/share/lolcommits/`.

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

Configuration is stored in `~/.config/lolcommits/config.toml`. The tool will automatically create a default configuration file on first run if none exists.

### Configuration Options

Below are all available configuration options with their default values:

```toml
# Default font used for all text unless overridden by specific font options
default_font_name = "monospace"

# Optional: Override fonts for specific text elements
# message_font_name = "Arial"
# info_font_name = "DejaVu Sans"
# sha_font_name = "Courier New"
# stats_font_name = "Liberation Sans"

# Background image specification
# Can be either:
# - An absolute path (starts with /): "/path/to/your/background.png"
# - A basename (no /): "mybackground" searches for mybackground.png in XDG data dirs
# Default: "background" (searches for background.png in standard locations)
background_path = "/home/user/.local/share/lolcommits/background.png"

# Camera device index (usually 0 for built-in webcam)
camera_index = 0

# Number of frames to capture before taking the final snapshot
# (allows the camera to adjust white balance and exposure)
camera_warmup_frames = 3

# Opacity of the information overlay (0.0 = transparent, 1.0 = opaque)
chyron_opacity = 0.75

# Font size for the commit message title
title_font_size = 28.0

# Font size for commit info (SHA, stats, repo name)
info_font_size = 18.0

# Whether to center the detected person in the frame
center_person = true
```

### Font Configuration

The font system uses a hierarchical fallback approach:

1. **default_font_name** - The base font used for all text elements
2. **Specific font overrides** - Optional per-element font customization:
   - `message_font_name` - Commit message text
   - `info_font_name` - Repository and commit information
   - `sha_font_name` - Commit SHA hash
   - `stats_font_name` - Diff statistics

If a specific font is not set, it falls back to `default_font_name`. This allows you to easily change all fonts at once or customize individual elements.

### Camera Configuration

- **camera_index**: Set to the device index of your webcam (typically 0 for built-in cameras, 1+ for external)
- **camera_warmup_frames**: Number of frames to capture and discard before taking the final snapshot. This gives the camera time to adjust exposure and white balance, resulting in better image quality.

### Visual Customization

- **background_path**: Specifies the background image for compositing. The face detection system will segment you from the webcam capture and composite you over this background. Can be specified in two ways:
  - **Absolute path** (starts with `/`): Direct path to the image file, e.g., `/home/user/pictures/bg.png`
  - **Basename** (no `/`): Searches for `{basename}.png` in XDG data directories, checking these locations in order:
    - `$XDG_DATA_HOME/{basename}.png` (typically `~/.local/share/{basename}.png`)
    - `$XDG_DATA_HOME/backgrounds/{basename}.png`
    - `$XDG_DATA_HOME/pixmaps/{basename}.png`
    - `$XDG_DATA_HOME/wallpapers/{basename}.png`
    - Same pattern in `/usr/local/share/` and `/usr/share/`
  
  Example: `background_path = "mybackground"` will search for `mybackground.png` in the above locations.

- **chyron_opacity**: Controls transparency of the text overlay (0.0-1.0)
- **title_font_size**: Size of the commit message text
- **info_font_size**: Size of the metadata text (SHA, stats, repo)
- **center_person**: When enabled, the detected face is centered in the frame

### Example Custom Configuration

```toml
# Use a fancy font for the commit message
default_font_name = "monospace"
message_font_name = "Liberation Serif"

# Use a different camera
camera_index = 1

# Larger fonts for high-DPI displays
title_font_size = 42.0
info_font_size = 24.0

# More subtle overlay
chyron_opacity = 0.5
```

## Automatic Cleanup

For information on setting up automatic cleanup of old lolcommit images using systemd-tmpfiles, see [docs/automatic-cleanup.md](docs/automatic-cleanup.md).
