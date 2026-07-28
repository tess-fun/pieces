//! Test-only helpers shared across the stack.
//!
//! Add as a `[dev-dependencies]` entry, never a real one.
//!
//! ```
//! # fn main() -> std::io::Result<()> {
//! pc_testkit::trace();
//!
//! let sandbox = pc_testkit::Sandbox::new()?;
//! let path = sandbox.write("etc/config.toml", "port = 8080\n")?;
//! assert!(path.is_file());
//! # Ok(())
//! # }
//! ```

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, SystemTime};

use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, fmt as tsfmt, registry};

static TRACE_INIT: Once = Once::new();

/// Route `tracing` output into libtest's capture buffer.
///
/// Safe to call from every test: only the first call installs a subscriber,
/// and it never panics if something else got there first. Output appears only
/// for failing tests, or with `--nocapture`.
///
/// Defaults to `debug`; override with `RUST_LOG`.
pub fn trace() {
    TRACE_INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));

        // `try_init` rather than `init`: a test binary may already have a
        // subscriber from another harness, and that is not a failure.
        let _ = registry()
            .with(filter)
            .with(tsfmt::layer().with_test_writer().with_target(true))
            .try_init();
    });
}

/// A temporary directory that cleans itself up.
///
/// Use instead of writing to the crate's own directory: tests run in parallel
/// and share a working directory, so anything written relative to `.` is a
/// race waiting to be debugged.
#[derive(Debug)]
pub struct Sandbox {
    root: tempfile::TempDir,
}

impl Sandbox {
    /// Create an empty sandbox.
    ///
    /// # Errors
    /// If the temporary directory cannot be created.
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            root: tempfile::TempDir::new()?,
        })
    }

    /// The sandbox root.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// Absolute path for a sandbox-relative path. Creates nothing.
    #[must_use]
    pub fn path(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.root.path().join(relative)
    }

    /// Write a file, creating parent directories as needed.
    ///
    /// # Errors
    /// If the directories or the file cannot be created.
    pub fn write(
        &self,
        relative: impl AsRef<Path>,
        contents: impl AsRef<[u8]>,
    ) -> io::Result<PathBuf> {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, contents)?;
        Ok(path)
    }

    /// Read a file back as a string.
    ///
    /// # Errors
    /// If the file is missing or is not valid UTF-8.
    pub fn read(&self, relative: impl AsRef<Path>) -> io::Result<String> {
        std::fs::read_to_string(self.path(relative))
    }

    /// Keep the directory after the sandbox drops, and return its path.
    ///
    /// For debugging a failing test. Leaks the directory by design.
    #[must_use]
    pub fn persist(self) -> PathBuf {
        self.root.keep()
    }
}

/// A deterministic replacement for a random ID generator.
///
/// Snapshot tests cannot assert on values that change every run. Inject this
/// wherever production code would reach for a UUID.
#[derive(Debug, Default)]
pub struct Seq {
    next: AtomicU64,
}

impl Seq {
    /// A generator starting at zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next: AtomicU64::new(0),
        }
    }

    /// The next integer in the sequence.
    pub fn next_id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    /// The next id rendered as `"<prefix>-<n>"`, e.g. `"user-0"`.
    pub fn next_named(&self, prefix: &str) -> String {
        format!("{prefix}-{}", self.next_id())
    }
}

/// A clock that only moves when a test moves it.
///
/// Removes the two standard flavours of time-dependent flake: a sleep that is
/// occasionally too short, and an assertion that straddles a second boundary.
#[derive(Debug)]
pub struct Clock {
    now: Mutex<SystemTime>,
}

impl Clock {
    /// A clock frozen at the Unix epoch — the same instant on every machine.
    #[must_use]
    pub fn epoch() -> Self {
        Self {
            now: Mutex::new(SystemTime::UNIX_EPOCH),
        }
    }

    /// A clock frozen at a specific instant.
    #[must_use]
    pub fn at(instant: SystemTime) -> Self {
        Self {
            now: Mutex::new(instant),
        }
    }

    /// The current time. Does not advance.
    ///
    /// # Panics
    /// If a previous caller panicked while holding the lock.
    #[must_use]
    pub fn now(&self) -> SystemTime {
        *self
            .now
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Move time forward.
    ///
    /// # Panics
    /// If a previous caller panicked while holding the lock.
    pub fn advance(&self, by: Duration) {
        let mut guard = self
            .now
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard += by;
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::epoch()
    }
}

#[cfg(test)]
mod tests {
    use super::{Clock, Sandbox, Seq, trace};
    use std::time::{Duration, SystemTime};

    #[test]
    fn trace_is_safe_to_call_repeatedly() {
        trace();
        trace();
        tracing::debug!("this must not panic");
    }

    #[test]
    fn sandbox_creates_parent_directories() {
        let sandbox = Sandbox::new().unwrap();
        let path = sandbox.write("a/b/c.toml", "x = 1\n").unwrap();
        assert!(path.is_file());
        assert_eq!(sandbox.read("a/b/c.toml").unwrap(), "x = 1\n");
    }

    #[test]
    fn sandbox_paths_are_inside_the_root() {
        let sandbox = Sandbox::new().unwrap();
        assert!(sandbox.path("x").starts_with(sandbox.root()));
    }

    #[test]
    fn sandbox_is_removed_on_drop() {
        let root = {
            let sandbox = Sandbox::new().unwrap();
            sandbox.write("f", "1").unwrap();
            sandbox.root().to_path_buf()
        };
        assert!(!root.exists());
    }

    #[test]
    fn seq_counts_from_zero() {
        let seq = Seq::new();
        assert_eq!(seq.next_id(), 0);
        assert_eq!(seq.next_id(), 1);
        assert_eq!(seq.next_named("user"), "user-2");
    }

    #[test]
    fn clock_does_not_move_on_its_own() {
        let clock = Clock::epoch();
        let first = clock.now();
        for _ in 0..1000 {
            assert_eq!(clock.now(), first);
        }
    }

    #[test]
    fn clock_advances_only_when_told() {
        let clock = Clock::at(SystemTime::UNIX_EPOCH);
        clock.advance(Duration::from_secs(90));
        assert_eq!(
            clock
                .now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            90
        );
    }
}
