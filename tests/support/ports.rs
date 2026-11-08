//! Leases on the fixed test ports, as `docs/reference/test-environment.md`
//! specifies them.

use std::fs::{File, TryLockError};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// An exclusive lease on one test port. Dropping it releases the port.
pub struct PortLease {
    port: u16,
    _lock: File,
}

impl PortLease {
    /// Takes the lowest free port of the range, waiting up to 60 seconds.
    pub async fn acquire() -> PortLease {
        let (base, count) = range();
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            for port in base..base + count {
                if let Some(lease) = try_lease(port) {
                    return lease;
                }
            }
            assert!(
                Instant::now() < deadline,
                "all test ports {base} to {} are leased or in use",
                base + count - 1
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// `127.0.0.1` and the leased port.
    pub fn v4(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
    }

    /// `[::1]` and the leased port, or `None` with a skip notice where IPv6
    /// tests are off or `::1` cannot be bound.
    pub fn v6_or_skip(&self) -> Option<SocketAddr> {
        let addr = SocketAddr::from((Ipv6Addr::LOCALHOST, self.port));
        let setting = match std::env::var("RUST_MILTER_TEST_IPV6") {
            Ok(setting) => setting,
            Err(_) => "auto".to_owned(),
        };
        let available = TcpListener::bind(addr).is_ok();
        match (setting.as_str(), available) {
            ("off", _) | ("auto", false) => {
                eprintln!("skipped: IPv6 tests are off or ::1 cannot be bound");
                None
            }
            ("on", false) => panic!("RUST_MILTER_TEST_IPV6=on, but ::1 cannot be bound"),
            _ => Some(addr),
        }
    }
}

/// The number in the environment variable `name`, or `default` without it.
fn read(name: &str, default: u16) -> u16 {
    let Ok(value) = std::env::var(name) else {
        return default;
    };
    match value.parse() {
        Ok(number) => number,
        Err(_) => panic!("{name} is not a number"),
    }
}

fn range() -> (u16, u16) {
    let count = read("RUST_MILTER_TEST_PORT_COUNT", 10);
    assert!(
        (3..=100).contains(&count),
        "RUST_MILTER_TEST_PORT_COUNT must be 3 to 100"
    );
    (read("RUST_MILTER_TEST_PORT_BASE", 61_000), count)
}

#[expect(
    clippy::incompatible_msrv,
    reason = "tests build with the pinned toolchain; the MSRV check builds the library only"
)]
fn try_lease(port: u16) -> Option<PortLease> {
    let dir: PathBuf = std::env::temp_dir().join("rust-milter-test-ports");
    std::fs::create_dir_all(&dir).expect("cannot create the port lock directory");
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(format!("{port}.lock")))
        .expect("cannot open a port lock file");
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return None,
        Err(TryLockError::Error(error)) => panic!("cannot lock port {port}: {error}"),
    }
    // Another program may use the port; then try the next one.
    TcpListener::bind((Ipv4Addr::LOCALHOST, port)).ok()?;
    Some(PortLease { port, _lock: lock })
}
