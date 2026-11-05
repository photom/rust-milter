//! The MTAs' queues, which the runner mounts read-only, so that a test can
//! find a message an MTA kept instead of delivering.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::WAIT;

/// Where the runner mounts Postfix's `/var/spool/postfix`.
pub const POSTFIX_SPOOL: &str = "/mta/postfix";

/// Where the runner mounts Sendmail's `/var/spool/mqueue`.
pub const SENDMAIL_QUEUE: &str = "/mta/sendmail";

/// Waits for the message with `token` in Postfix's hold queue, and returns
/// its queue file.
pub async fn wait_for_postfix_hold(token: &str) -> PathBuf {
    let hold = Path::new(POSTFIX_SPOOL).join("hold");
    wait_for_file(token, |path| path.starts_with(&hold)).await
}

/// Waits for the message with `token` in Sendmail's queue as a quarantined
/// queue file (`hf...`), and returns the quarantine reason from its `q`
/// line.
pub async fn wait_for_sendmail_quarantine(token: &str) -> String {
    let file = wait_for_file(token, |path| {
        path.starts_with(SENDMAIL_QUEUE)
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("hf"))
    })
    .await;
    let content = fs::read(&file).expect("the quarantined queue file is readable");
    content
        .split(|&b| b == b'\n')
        .find_map(|line| line.strip_prefix(b"q"))
        .map(|reason| String::from_utf8_lossy(reason).into_owned())
        .unwrap_or_else(|| panic!("{} has a q line", file.display()))
}

async fn wait_for_file(token: &str, wanted: impl Fn(&Path) -> bool) -> PathBuf {
    let deadline = Instant::now() + WAIT;
    loop {
        let mut files = Vec::new();
        walk(Path::new(POSTFIX_SPOOL), &mut files);
        walk(Path::new(SENDMAIL_QUEUE), &mut files);
        let found = files.into_iter().find(|path| {
            wanted(path)
                && fs::read(path).is_ok_and(|content| {
                    content
                        .windows(token.len())
                        .any(|window| window == token.as_bytes())
                })
        });
        if let Some(path) = found {
            return path;
        }
        assert!(Instant::now() < deadline, "message {token} queued in time");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Collects the regular files under `dir`. A directory that cannot be read,
/// such as a file removed while walking, is skipped.
fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => walk(&path, files),
            Ok(kind) if kind.is_file() => files.push(path),
            _ => {}
        }
    }
}
