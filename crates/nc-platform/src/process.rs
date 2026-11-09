//! Which process this is, as the system says: the boot it runs in, when it
//! started, and the executable it runs (`NOCTUA.md §17.1`).
//!
//! **What a PID record holds, so a process is never taken for another.** A
//! process id is reused once its process is gone, so a record of the id alone
//! names whichever process holds it next; the boot and the start, read off the
//! system, tell the process the record was written by from any later holder of
//! its id. Each system's half reads them ([`this_process`]); the texts they
//! read are parsed here, in code every system compiles, so the Linux job tests
//! the parser of each.
//!
//! **The start is the system's own spelling**, compared as text: clock ticks
//! since boot on Linux, the date `ps` prints on macOS, the creation time in
//! hundred-nanosecond intervals on Windows. Windows names no boot, so its
//! record carries none.

use std::path::PathBuf;

use crate::imp;

/// A process, as the system says it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// Its id, which the system hands out again once it is gone.
    pub pid: u32,
    /// The boot it runs in, where the system names one.
    pub boot: Option<String>,
    /// When it started, in the system's own spelling.
    pub started: String,
    /// The executable it runs, canonicalized.
    pub executable: PathBuf,
}

/// This process, as the system says it is.
///
/// # Errors
///
/// [`std::io::Error`] as the system answered it: the boot, the start or the
/// executable could not be read.
pub fn this_process() -> std::io::Result<Identity> {
    let (boot, started) = imp::this_process()?;
    let executable = std::fs::canonicalize(std::env::current_exe()?)?;
    Ok(Identity {
        pid: std::process::id(),
        boot,
        started,
        executable,
    })
}

/// Keeps this process's own standard handles from every child it spawns,
/// but as the standard handles a spawn hands one.
///
/// **What a spawner of a daemon asks first**: a daemon that held its
/// spawner's output open would keep whatever reads that output waiting for
/// its end for as long as the daemon lives. On Unix a spawn already replaces
/// each standard handle with what it hands the child, and nothing else is
/// left open across `exec`, so this does nothing; on Windows, where a child
/// is handed every inheritable handle of its parent, it clears the flag that
/// would hand it this process's own.
///
/// # Errors
///
/// [`std::io::Error`] as the system answered it.
pub fn keep_standard_handles_from_children() -> std::io::Result<()> {
    imp::keep_standard_handles_from_children()
}

/// The start a Linux `/proc/<pid>/stat` holds: its field 22, counted after the
/// `)` that ends the process's name.
///
/// **After the last `)`**, because the name is the process's to choose and may
/// hold spaces and parentheses of its own; what follows it is the system's.
#[cfg_attr(
    all(not(target_os = "linux"), not(test)),
    expect(
        dead_code,
        reason = "macOS and Windows have no `/proc`; read on Linux alone"
    )
)]
pub(crate) fn started_in_stat(stat: &str) -> Option<String> {
    let after = &stat[stat.rfind(')')? + 1..];
    // Field 3, the state, is the first after the name; field 22 is the
    // twentieth from there.
    let started = after.split_ascii_whitespace().nth(19)?;
    started
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| started.to_owned())
}

/// The start `ps -o lstart=` prints on macOS, with its runs of spaces made one
/// so a day of the month padded or not is one spelling.
#[cfg_attr(
    all(any(target_os = "linux", windows), not(test)),
    expect(
        dead_code,
        reason = "Linux reads `/proc` and Windows its own call; `ps` is macOS's"
    )
)]
pub(crate) fn started_in_ps(said: &str) -> Option<String> {
    let words: Vec<&str> = said.split_ascii_whitespace().collect();
    // `Sun Nov  8 12:24:11 2026`: a day, a month, a date, a time and a year.
    (words.len() == 5).then(|| words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::{started_in_ps, started_in_stat};

    /// The start is field 22 of a `stat` line, counted after the name however
    /// the name is spelled.
    #[test]
    fn the_start_is_field_22_counted_after_the_name() {
        let stat = "4242 (noctua-daemon) S 1 4242 4242 0 -1 4194560 120 0 0 0 1 2 0 0 20 0 \
                    4 0 987654 12345678 300 18446744073709551615";
        assert_eq!(started_in_stat(stat).as_deref(), Some("987654"));
        let named = "4242 (a) b) S 1 4242 4242 0 -1 4194560 120 0 0 0 1 2 0 0 20 0 4 0 \
                     987654 12345678 300 18446744073709551615";
        assert_eq!(started_in_stat(named).as_deref(), Some("987654"));
        assert_eq!(started_in_stat("4242 (noctua-daemon) S 1 2 3"), None);
        assert_eq!(started_in_stat("no name at all"), None);
        let lettered = stat.replace("987654", "98765x");
        assert_eq!(started_in_stat(&lettered), None);
    }

    /// The start `ps` prints is one spelling however its date is padded, and
    /// anything that is not a date is none.
    #[test]
    fn the_start_ps_prints_is_one_spelling() {
        assert_eq!(
            started_in_ps("  Sun Nov  8 12:24:11 2026\n").as_deref(),
            Some("Sun Nov 8 12:24:11 2026")
        );
        assert_eq!(
            started_in_ps("Sun Nov 18 12:24:11 2026").as_deref(),
            Some("Sun Nov 18 12:24:11 2026")
        );
        assert_eq!(started_in_ps(""), None);
        assert_eq!(started_in_ps("Sun Nov 8 12:24:11"), None);
    }

    /// This process is read as the system says it is: its own id, a start, and
    /// the executable that runs it.
    #[test]
    fn this_process_is_read_as_the_system_says_it_is() {
        let identity = super::this_process().expect("this process is read");
        assert_eq!(identity.pid, std::process::id());
        assert_ne!(identity.started, "");
        assert_eq!(
            identity.executable,
            std::fs::canonicalize(std::env::current_exe().expect("this executable"))
                .expect("canonical")
        );
        assert_eq!(
            super::this_process().expect("read again").started,
            identity.started,
            "a process's start read twice is two spellings"
        );
    }
}
