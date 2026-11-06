//! The MTAs' queues, which the runner mounts read-only, so that a test can
//! find a message an MTA kept instead of delivering.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{WAIT, text};

/// Where the runner mounts Postfix's `/var/spool/postfix`.
pub const POSTFIX_SPOOL: &str = "/mta/postfix";

/// Where the runner mounts Sendmail's `/var/spool/mqueue`.
pub const SENDMAIL_QUEUE: &str = "/mta/sendmail";

/// Waits for the message with `token` in Postfix's hold queue, and returns
/// its queue file.
pub async fn wait_for_postfix_hold(token: &str) -> PathBuf {
    let hold = Path::new(POSTFIX_SPOOL).join("hold");
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(file) = find_queue_file(&hold, "", token) {
            return file;
        }
        pause_until(deadline, token).await;
    }
}

/// Waits for the message with `token` in Sendmail's queue as a quarantined
/// queue file (`hf...`), and returns the quarantine reason from its `q`
/// line.
pub async fn wait_for_sendmail_quarantine(token: &str) -> String {
    let deadline = Instant::now() + WAIT;
    let file = loop {
        if let Some(file) = find_queue_file(Path::new(SENDMAIL_QUEUE), "hf", token) {
            break file;
        }
        pause_until(deadline, token).await;
    };
    let content = match fs::read(&file) {
        Ok(content) => content,
        Err(error) => panic!("reading {} failed: {error}", file.display()),
    };
    for line in text::lines(&content) {
        if let Some(reason) = line.strip_prefix(b"q") {
            return String::from_utf8_lossy(reason).into_owned();
        }
    }
    panic!("{} has no q line", file.display());
}

/// The first regular file under `dir` whose name starts with `prefix` and
/// whose content holds `token`.
fn find_queue_file(dir: &Path, prefix: &str, token: &str) -> Option<PathBuf> {
    let mut files = Vec::new();
    walk(dir, &mut files);
    for path in files {
        let named = match path.file_name() {
            Some(name) => name.to_string_lossy().starts_with(prefix),
            None => false,
        };
        if !named {
            continue;
        }
        // A file the MTA removes while the test looks is skipped.
        let Ok(content) = fs::read(&path) else {
            continue;
        };
        if text::contains(&content, token.as_bytes()) {
            return Some(path);
        }
    }
    None
}

/// Sleeps briefly before the next look at the queue, or fails the test if
/// `deadline` has passed.
async fn pause_until(deadline: Instant, token: &str) {
    assert!(Instant::now() < deadline, "message {token} queued in time");
    tokio::time::sleep(Duration::from_millis(200)).await;
}

/// Collects the regular files under `dir`. A directory that cannot be read,
/// such as one removed while walking, is skipped.
fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => walk(&path, files),
            Ok(kind) if kind.is_file() => files.push(path),
            _ => {}
        }
    }
}
