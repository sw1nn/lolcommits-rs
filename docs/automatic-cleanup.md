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

The default configuration deletes PNG images older than 30 days from `~/.local/share/lolcommits/`.

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
