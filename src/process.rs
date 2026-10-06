//! Starting apps that don't depend on the app that started them.

use std::ffi::c_int;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

extern "C" {
    fn setsid() -> c_int;
    fn fork() -> c_int;
    fn _exit(status: c_int) -> !;
    fn sigprocmask(how: c_int, set: *const u64, old: *mut u64) -> c_int;
    fn signal(sig: c_int, handler: usize) -> usize;
}

/// Linux (glibc and musl, all our architectures).
const SIG_SETMASK: c_int = 2;
const SIG_DFL: usize = 0;
/// SIGHUP, SIGINT, SIGQUIT, SIGUSR1, SIGUSR2, SIGTERM.
const RESET: [c_int; 6] = [1, 2, 3, 10, 12, 15];

/// Starts `command` (a shell command line) entirely on its own: in a new
/// session, so Ctrl-C in the terminal running this app, or that terminal
/// closing, doesn't reach it; and not as this app's child (the system
/// adopts it), so restarting or killing this app leaves it running and it
/// never becomes a zombie here. Signals are as normal in it: none blocked
/// or ignored (not as with `sh -c "cmd &"`, which ignores Ctrl-C, and a
/// terminal started that way would pass that on to everything run in it).
/// Returns once it has been started.
pub fn launch(command: &str) -> std::io::Result<()> {
    let mut sh = Command::new("sh");
    sh.arg("-c").arg(command).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    unsafe {
        sh.pre_exec(|| {
            // Between fork and exec: only async-signal-safe calls.
            setsid();
            // A clean start: nothing blocked (the launcher blocks the
            // signal that closes it; Rust keeps the mask), nothing ignored
            // (the starter may ignore Ctrl-C or hangups).
            let empty = [0u64; 16];
            sigprocmask(SIG_SETMASK, empty.as_ptr(), std::ptr::null_mut());
            for sig in RESET {
                signal(sig, SIG_DFL);
            }
            match fork() {
                -1 => Err(std::io::Error::last_os_error()),
                // The grandchild runs the command.
                0 => Ok(()),
                // The child leaves at once; we reap it below.
                _ => _exit(0),
            }
        });
    }
    sh.status().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The started command runs in its own session, with a parent that
    /// isn't us.
    #[test]
    fn detached() {
        // As if started by something that blocks and ignores signals.
        unsafe {
            let mut set = [0u64; 16];
            set[0] = 1 << 9; // SIGUSR1
            sigprocmask(0, set.as_ptr(), std::ptr::null_mut());
            signal(2, 1); // SIGINT ignored
        }
        let out = std::env::temp_dir().join(format!("heroui-launch-{}", std::process::id()));
        launch(&format!("ps -o sid=,ppid= -p $$ > {0}; grep -E 'SigBlk|SigIgn' /proc/$$/status >> {0}", out.display())).unwrap();
        let mut text = String::new();
        for _ in 0..50 {
            text = std::fs::read_to_string(&out).unwrap_or_default();
            if text.contains("SigIgn") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = std::fs::remove_file(&out);
        let first = text.lines().next().unwrap_or("");
        let ids: Vec<u32> = first.split_whitespace().filter_map(|v| v.parse().ok()).collect();
        assert!(text.contains("SigBlk:\t0000000000000000"), "{text}");
        // Not ignored: SIGINT (bit 2).
        let ign = text.lines().find(|l| l.starts_with("SigIgn")).and_then(|l| u64::from_str_radix(l.split_whitespace().nth(1)?, 16).ok()).unwrap();
        assert_eq!(ign & 0b10, 0, "{text}");
        let me = std::process::id();
        let my_sid = std::fs::read_to_string("/proc/self/stat").unwrap().rsplit(')').next().unwrap().split_whitespace().nth(3).unwrap().parse::<u32>().unwrap();
        assert_eq!(ids.len(), 2, "{text:?}");
        assert_ne!(ids[0], my_sid, "same session");
        assert_ne!(ids[1], me, "still our child");
    }
}
