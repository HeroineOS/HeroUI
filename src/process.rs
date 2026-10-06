//! Starting apps that don't depend on the app that started them.

use std::ffi::c_int;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

extern "C" {
    fn setsid() -> c_int;
    fn fork() -> c_int;
    fn _exit(status: c_int) -> !;
}

/// Starts `command` (a shell command line) entirely on its own: in a new
/// session, so Ctrl-C in the terminal running this app, or that terminal
/// closing, doesn't reach it; and not as this app's child (the system
/// adopts it), so restarting or killing this app leaves it running and it
/// never becomes a zombie here. Signals are as normal in it (not ignored,
/// as with `sh -c "cmd &"`, which a terminal started that way would pass
/// on to everything run in it). Returns once it has been started.
pub fn launch(command: &str) -> std::io::Result<()> {
    let mut sh = Command::new("sh");
    sh.arg("-c").arg(command).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    unsafe {
        sh.pre_exec(|| {
            // Between fork and exec: only async-signal-safe calls.
            setsid();
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
        let out = std::env::temp_dir().join(format!("heroui-launch-{}", std::process::id()));
        launch(&format!("ps -o sid=,ppid= -p $$ > {}", out.display())).unwrap();
        let mut text = String::new();
        for _ in 0..50 {
            text = std::fs::read_to_string(&out).unwrap_or_default();
            if !text.trim().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = std::fs::remove_file(&out);
        let ids: Vec<u32> = text.split_whitespace().filter_map(|v| v.parse().ok()).collect();
        let me = std::process::id();
        let my_sid = std::fs::read_to_string("/proc/self/stat").unwrap().rsplit(')').next().unwrap().split_whitespace().nth(3).unwrap().parse::<u32>().unwrap();
        assert_eq!(ids.len(), 2, "{text:?}");
        assert_ne!(ids[0], my_sid, "same session");
        assert_ne!(ids[1], me, "still our child");
    }
}
