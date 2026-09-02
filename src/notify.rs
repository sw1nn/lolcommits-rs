//! Desktop notification for a completed upload.
//!
//! `lolcommits-ctl upload` normally runs from a git `post-commit` hook with
//! logging off, so a successful upload is otherwise silent. This module turns
//! that into a freedesktop.org notification on the session bus.

use crate::config::ClientConfig;
use std::{sync::mpsc, time::Duration};

const APP_NAME: &str = "lolcommits";
const SUMMARY: &str = "lolcommits uploaded";

/// How long to wait for the notification daemon to accept the notification.
///
/// zbus sets no reply timeout of its own, so a daemon that owns the bus name
/// but never answers would block the commit hook indefinitely. The upload has
/// already succeeded by then, so a wedged daemon must cost the hook a few
/// seconds rather than the whole commit.
const SHOW_TIMEOUT: Duration = Duration::from_secs(5);

/// Characters of the revision shown, matching `git log --abbrev-commit`.
const SHORT_SHA_LEN: usize = 7;

/// Characters of the commit subject kept. Long enough for a conventional
/// commit summary, short enough that the notification daemon does not elide it
/// at a width we cannot predict.
const MAX_SUBJECT_LEN: usize = 72;

/// The facts a successful upload reports.
pub struct Upload<'a> {
    pub repo_name: &'a str,
    pub revision: &'a str,
    pub message: &'a str,
}

/// Post a desktop notification for a successful upload.
///
/// Failures are logged and discarded rather than returned. The upload has
/// already succeeded by the time this runs, and a machine with no notification
/// daemon — a headless build box, a bare tty — must still see the commit hook
/// exit 0.
pub fn upload_succeeded(config: &ClientConfig, upload: &Upload<'_>) {
    if !config.desktop_notifications {
        tracing::debug!("Desktop notifications disabled, not notifying");
        return;
    }

    let body = notification_body(upload);
    tracing::debug!(body = %body, "Showing desktop notification");

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        // The handle is dropped here rather than returned: on XDG it carries no
        // Drop impl, so this does not wait for the user to dismiss anything.
        let shown = notify_rust::Notification::new()
            .appname(APP_NAME)
            .summary(SUMMARY)
            .body(&body)
            .show()
            .map(drop);

        let _ = sender.send(shown);
    });

    // The thread is left to finish on its own if it outlives the wait: it holds
    // nothing the caller needs, and exiting the process ends it.
    match receiver.recv_timeout(SHOW_TIMEOUT) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!(%error, "Could not show desktop notification"),
        Err(error) => tracing::warn!(%error, "Gave up waiting for the notification daemon"),
    }
}

/// Build the notification body: repository, abbreviated revision and subject.
fn notification_body(upload: &Upload<'_>) -> String {
    let repo = escape_markup(upload.repo_name);
    let sha: String = upload.revision.chars().take(SHORT_SHA_LEN).collect();
    let subject = escape_markup(&subject(upload.message));

    format!("{repo} {sha} — {subject}")
}

/// The commit subject: the first line, truncated to [`MAX_SUBJECT_LEN`].
fn subject(message: &str) -> String {
    let first_line = message.lines().next().unwrap_or_default().trim();

    if first_line.chars().count() <= MAX_SUBJECT_LEN {
        return first_line.to_owned();
    }

    let kept: String = first_line.chars().take(MAX_SUBJECT_LEN - 1).collect();
    format!("{kept}…")
}

/// Escape the characters a notification daemon advertising `body-markup` reads
/// as XML. Without this a commit subject such as `fix: allow a<b` loses
/// everything from the `<` onwards.
fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upload<'a>(revision: &'a str, message: &'a str) -> Upload<'a> {
        Upload {
            repo_name: "lolcommits-rs",
            revision,
            message,
        }
    }

    #[test]
    fn body_has_repo_short_sha_and_subject() {
        let body = notification_body(&upload(
            "e907082f1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f",
            "chore(version): v4.0.0",
        ));

        assert_eq!(body, "lolcommits-rs e907082 — chore(version): v4.0.0");
    }

    #[test]
    fn body_uses_only_the_first_line_of_the_message() {
        let body = notification_body(&upload(
            "e907082f1c2d3e4f",
            "feat: add notifications\n\nA longer explanation nobody wants in a toast.\n",
        ));

        assert_eq!(body, "lolcommits-rs e907082 — feat: add notifications");
    }

    #[test]
    fn body_survives_a_revision_shorter_than_the_abbreviation() {
        let body = notification_body(&upload("e907", "fix: something"));

        assert_eq!(body, "lolcommits-rs e907 — fix: something");
    }

    #[test]
    fn subject_is_kept_whole_when_short_enough() {
        let short = "a".repeat(MAX_SUBJECT_LEN);

        assert_eq!(subject(&short), short);
    }

    #[test]
    fn subject_is_truncated_with_an_ellipsis_when_too_long() {
        let long = "a".repeat(MAX_SUBJECT_LEN + 10);
        let truncated = subject(&long);

        assert_eq!(truncated.chars().count(), MAX_SUBJECT_LEN);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn subject_truncates_on_a_character_boundary() {
        let long = "é".repeat(MAX_SUBJECT_LEN + 10);
        let truncated = subject(&long);

        assert_eq!(truncated.chars().count(), MAX_SUBJECT_LEN);
    }

    #[test]
    fn subject_of_an_empty_message_is_empty() {
        assert_eq!(subject(""), "");
    }

    #[test]
    fn markup_characters_in_the_subject_are_escaped() {
        let body = notification_body(&upload("e907082f", "fix: allow a<b && c>d"));

        assert_eq!(
            body,
            "lolcommits-rs e907082 — fix: allow a&lt;b &amp;&amp; c&gt;d"
        );
    }
}
