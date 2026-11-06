//! Server lifecycle: INT-050 to INT-052.

use std::net::TcpStream;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use rust_milter_interop::{Mail, Mta, Reply, Smtp, WAIT, milter, sink};

/// How many messages each lifecycle scenario sends through one MTA.
const MESSAGES: usize = 50;

/// The milter binary of this crate, running as a child process.
struct ChildMilter {
    child: Child,
}

impl ChildMilter {
    /// Starts `int-milter` and waits until it accepts connections on port
    /// 8891. Starts the sink too, if it is not running.
    fn start() -> ChildMilter {
        sink::start();
        let child = match Command::new(env!("CARGO_BIN_EXE_int-milter")).spawn() {
            Ok(child) => child,
            Err(error) => panic!("int-milter starts: {error}"),
        };
        let deadline = Instant::now() + WAIT;
        while TcpStream::connect(("127.0.0.1", milter::PORT)).is_err() {
            assert!(Instant::now() < deadline, "int-milter listens in time");
            std::thread::sleep(Duration::from_millis(50));
        }
        ChildMilter { child }
    }

    /// Sends `signal`, such as `TERM`, with the shell's `kill`.
    fn signal(&self, signal: &str) {
        let script = format!("kill -s {signal} {}", self.child.id());
        let status = match Command::new("sh").args(["-c", &script]).status() {
            Ok(status) => status,
            Err(error) => panic!("sh runs: {error}"),
        };
        assert!(status.success(), "kill -s {signal}");
    }

    /// Waits for the milter to exit, and returns its exit code.
    async fn exit_code(mut self) -> Option<i32> {
        let deadline = Instant::now() + WAIT;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return status.code(),
                Ok(None) => {}
                Err(error) => panic!("waiting for int-milter failed: {error}"),
            }
            assert!(Instant::now() < deadline, "int-milter exits in time");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

impl Drop for ChildMilter {
    /// Kills a milter that a failing test left running, so that the next
    /// test can bind port 8891.
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// The replies to one attempt to send a message.
#[derive(Debug)]
struct Attempt {
    token: String,
    replies: Vec<Reply>,
    accepted: bool,
}

impl Attempt {
    /// A reply was a temporary failure, and none was a permanent one.
    fn failed_temporarily(&self) -> bool {
        let mut temporary = false;
        for reply in &self.replies {
            if reply.code / 100 == 5 {
                return false;
            }
            if reply.code / 100 == 4 {
                temporary = true;
            }
        }
        temporary
    }
}

/// Sends `mail` in a session of its own, and keeps every reply. Unlike
/// [`Smtp::send_one`], it accepts a refused greeting or EHLO, and stops at
/// a `421`, after which the MTA closes the connection.
async fn attempt(mta: Mta, mail: &Mail) -> Attempt {
    let mut smtp = Smtp::connect(mta).await;
    let mut replies = vec![smtp.greeting.clone()];
    let mut attempt = Attempt {
        token: mail.token.clone(),
        replies: Vec::new(),
        accepted: false,
    };
    if smtp.greeting.code != 220 {
        attempt.replies = replies;
        return attempt;
    }
    let ehlo = smtp.command("EHLO client.void.invalid").await;
    let ehlo_closes = ehlo.code == 421;
    let ehlo_accepted = ehlo.is_positive();
    replies.push(ehlo);
    if ehlo_closes {
        attempt.replies = replies;
        return attempt;
    }
    if ehlo_accepted {
        let sent = smtp.send(mail).await;
        attempt.accepted = sent.accepted();
        replies.push(sent.mail);
        for reply in sent.rcpts {
            replies.push(reply);
        }
        if let Some(reply) = sent.data {
            replies.push(reply);
        }
    }
    let mut closed = false;
    for reply in &replies {
        if reply.code == 421 {
            closed = true;
        }
    }
    if !closed {
        smtp.quit().await;
    }
    attempt.replies = replies;
    attempt
}

/// Checks that every accepted message reached the sink, and that every
/// other message got a temporary failure.
async fn assert_nothing_lost(mta: Mta, attempts: &[Attempt]) {
    for attempt in attempts {
        if attempt.accepted {
            sink::wait_for(&attempt.token).await;
        } else {
            assert!(attempt.failed_temporarily(), "{mta}: {attempt:?}");
        }
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_050_sigterm_loses_no_message() {
    for mta in Mta::BOTH {
        let milter = ChildMilter::start();
        let mut attempts = Vec::new();

        for index in 0..MESSAGES {
            if index == 10 {
                milter.signal("TERM");
            }
            attempts.push(attempt(mta, &Mail::new()).await);
        }

        assert_nothing_lost(mta, &attempts).await;
        assert_eq!(milter.exit_code().await, Some(0), "{mta}");
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_051_sighup_during_mail_loses_no_message() {
    for mta in Mta::BOTH {
        let milter = ChildMilter::start();
        let mut attempts = Vec::new();

        for index in 0..MESSAGES {
            if index == 10 || index == 20 || index == 30 {
                milter.signal("HUP");
            }
            attempts.push(attempt(mta, &Mail::new()).await);
        }

        for attempt in &attempts {
            assert!(attempt.accepted, "{mta}: {attempt:?}");
            for reply in &attempt.replies {
                assert!(reply.is_positive(), "{mta}: {attempt:?}");
            }
        }
        assert_nothing_lost(mta, &attempts).await;
        milter.signal("TERM");
        assert_eq!(milter.exit_code().await, Some(0), "{mta}");
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_052_a_stopped_milter_makes_the_mta_tempfail() {
    sink::start();
    for mta in Mta::BOTH {
        let mail = Mail::new();

        let attempt = attempt(mta, &mail).await;

        assert!(!attempt.accepted, "{mta}: {attempt:?}");
        assert!(attempt.failed_temporarily(), "{mta}: {attempt:?}");
        sink::assert_none_within(&mail.token, Duration::from_secs(3)).await;
    }
}
