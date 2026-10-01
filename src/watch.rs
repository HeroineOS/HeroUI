//! Following the theme file live. A small thread (64 KB stack) blocks in
//! inotify on the theme's directory and wakes the event loop when
//! theme.conf is written or replaced: no polling, nothing runs while
//! nothing changes.

use std::ffi::{c_char, c_int, c_void, CString};
use std::sync::mpsc;

extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
}

const IN_CLOEXEC: c_int = 0o2000000;
const IN_CLOSE_WRITE: u32 = 0x8;
const IN_MOVED_TO: u32 = 0x80;
const IN_CREATE: u32 = 0x100;

/// Starts watching; the receiver gets `()` each time the theme file
/// changes. `None` if the file has no location or inotify isn't available.
pub(crate) fn theme_file() -> Option<mpsc::Receiver<()>> {
    let path = crate::Theme::path()?;
    let dir = path.parent()?.to_path_buf();
    let name = path.file_name()?.to_str()?.as_bytes().to_vec();
    // Watching needs the directory; creating an empty one is harmless.
    std::fs::create_dir_all(&dir).ok()?;
    let fd = unsafe { inotify_init1(IN_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    let cdir = CString::new(dir.into_os_string().into_encoded_bytes()).ok()?;
    if unsafe { inotify_add_watch(fd, cdir.as_ptr(), IN_CLOSE_WRITE | IN_MOVED_TO | IN_CREATE) } < 0 {
        return None;
    }
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("heroui-theme".into())
        .stack_size(64 * 1024)
        .spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                let n = unsafe { read(fd, buf.as_mut_ptr() as *mut c_void, buf.len()) };
                if n <= 0 {
                    return;
                }
                // struct inotify_event { int wd; u32 mask, cookie, len; char name[len]; }
                let (mut i, n) = (0usize, n as usize);
                let mut hit = false;
                while i + 16 <= n {
                    let len = u32::from_ne_bytes(buf[i + 12..i + 16].try_into().unwrap()) as usize;
                    let raw = &buf[i + 16..(i + 16 + len).min(n)];
                    let ev_name = raw.split(|&b| b == 0).next().unwrap_or(&[]);
                    hit |= ev_name == name.as_slice();
                    i += 16 + len;
                }
                if hit {
                    if tx.send(()).is_err() {
                        return;
                    }
                    fltk::app::awake();
                }
            }
        })
        .ok()?;
    Some(rx)
}
