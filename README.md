# Automatic Cleanup with systemd-tmpfiles

The included `lolcommits-rs.conf` file can be used with `systemd-tmpfiles` to automatically clean up old lolcommit images.

## Installation

Copy the configuration file to your user tmpfiles directory:

```bash
mkdir -p ~/.config/user-tmpfiles.d
cp lolcommits-rs.conf ~/.config/user-tmpfiles.d/
```

## Manual Cleanup

To manually trigger cleanup based on the rules:

```bash
systemd-tmpfiles --user --clean
```

## Automatic Cleanup

To set up automatic periodic cleanup, create a systemd user timer.

### Create the service file

`~/.config/systemd/user/tmpfiles-clean.service`:

```ini
[Unit]
Description=Cleanup old temporary files
Documentation=man:tmpfiles.d(5) man:systemd-tmpfiles(8)

[Service]
Type=oneshot
ExecStart=/usr/bin/systemd-tmpfiles --user --clean
```

### Create the timer file

`~/.config/systemd/user/tmpfiles-clean.timer`:

```ini
[Unit]
Description=Daily cleanup of temporary files
Documentation=man:tmpfiles.d(5) man:systemd-tmpfiles(8)

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
```

### Enable and start the timer

```bash
systemctl --user daemon-reload
systemctl --user enable tmpfiles-clean.timer
systemctl --user start tmpfiles-clean.timer
```

### Check timer status

```bash
systemctl --user status tmpfiles-clean.timer
systemctl --user list-timers
```

## Configuration

The default configuration deletes PNG images older than 30 days from `~/.local/share/lolcommits-rs/`.

To customize, edit `~/.config/user-tmpfiles.d/lolcommits-rs.conf`:

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
