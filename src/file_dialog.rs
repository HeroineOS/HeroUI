//! Open and save dialogs through the desktop portal, the same ones every
//! other app on the desktop gets: whichever the desktop has set up (on
//! HeroWM, HeroPortal; elsewhere GTK's or KDE's). The dialog floats over
//! the app's window (its parent, through xdg-foreign). Without a portal,
//! FLTK's own chooser.
//!
//! ```ignore
//! Msg::Browse => return file_dialog::open(
//!     file_dialog::Options { title: "Choose a picture".into(), filters: vec![("Pictures".into(), vec!["*.png".into()])], ..Default::default() },
//!     Msg::Picked, // fn(Vec<PathBuf>) -> Msg; empty when cancelled
//! ),
//! ```

use std::path::PathBuf;

use crate::Task;

/// What the dialog asks.
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub title: String,
    /// (name, patterns like "*.png"); the first is chosen at first.
    pub filters: Vec<(String, Vec<String>)>,
    /// Folder to start in.
    pub folder: Option<PathBuf>,
    pub multiple: bool,
    /// Pick folders instead of files.
    pub directory: bool,
    /// Suggested name (save dialogs).
    pub name: Option<String>,
    /// The accept button's label.
    pub accept: Option<String>,
}

/// An open dialog; `done` gets the chosen paths (none if cancelled).
pub fn open<M: Send + 'static>(options: Options, done: impl FnOnce(Vec<PathBuf>) -> M + Send + 'static) -> Task<M> {
    ask(false, options, done)
}

/// A save dialog; `done` gets the chosen path (none if cancelled).
pub fn save<M: Send + 'static>(options: Options, done: impl FnOnce(Vec<PathBuf>) -> M + Send + 'static) -> Task<M> {
    ask(true, options, done)
}

fn ask<M: Send + 'static>(save: bool, options: Options, done: impl FnOnce(Vec<PathBuf>) -> M + Send + 'static) -> Task<M> {
    // The window to float over, asked here (FLTK, main thread).
    let parent = parent_window();
    #[cfg(feature = "portal")]
    {
        if portal::available() {
            return Task::perform(move || done(portal::ask(save, &options, &parent).unwrap_or_default()));
        }
    }
    let _ = parent;
    Task::message(done(native(save, &options)))
}

/// The app's window as portals name it: "wayland:HANDLE", or "".
fn parent_window() -> String {
    #[cfg(feature = "layer-shell")]
    if crate::on_wayland() {
        if let Some(w) = fltk::app::first_window() {
            use fltk::prelude::WidgetExt;
            let h = unsafe { fltk_sys::window::Fl_Window_wl_exported_handle(w.as_widget_ptr() as *mut fltk_sys::window::Fl_Window) };
            if !h.is_null() {
                return format!("wayland:{}", unsafe { std::ffi::CStr::from_ptr(h) }.to_string_lossy());
            }
        }
    }
    String::new()
}

/// FLTK's own chooser (no portal).
fn native(save: bool, o: &Options) -> Vec<PathBuf> {
    use fltk::dialog::{NativeFileChooser, NativeFileChooserOptions, NativeFileChooserType as T};
    let kind = match (save, o.directory, o.multiple) {
        (true, _, _) => T::BrowseSaveFile,
        (false, true, _) => T::BrowseDir,
        (false, false, true) => T::BrowseMultiFile,
        (false, false, false) => T::BrowseFile,
    };
    let mut fc = NativeFileChooser::new(kind);
    fc.set_title(&o.title);
    let filter: Vec<String> = o.filters.iter().map(|(n, p)| format!("{n}\t{{{}}}", p.join(","))).collect();
    if !filter.is_empty() {
        fc.set_filter(&filter.join("\n"));
    }
    if let Some(f) = &o.folder {
        let _ = fc.set_directory(f);
    }
    if let Some(n) = &o.name {
        fc.set_preset_file(n);
    }
    if save {
        fc.set_option(NativeFileChooserOptions::SaveAsConfirm);
    }
    fc.show();
    fc.filenames().into_iter().filter(|p| !p.as_os_str().is_empty()).collect()
}

#[cfg(feature = "portal")]
mod portal {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::Duration;

    use dbus::arg::{PropMap, RefArg, Variant};
    use dbus::blocking::Connection;
    use dbus::message::MatchRule;

    use super::Options;

    const DEST: &str = "org.freedesktop.portal.Desktop";
    const PATH: &str = "/org/freedesktop/portal/desktop";

    /// Whether a desktop portal answers on the session bus (or would be
    /// started by it).
    pub fn available() -> bool {
        let Ok(c) = Connection::new_session() else { return false };
        let proxy = c.with_proxy("org.freedesktop.DBus", "/org/freedesktop/DBus", Duration::from_millis(500));
        let (activatable,): (Vec<String>,) = proxy.method_call("org.freedesktop.DBus", "ListActivatableNames", ()).unwrap_or_default();
        let (has_owner,): (bool,) = proxy.method_call("org.freedesktop.DBus", "NameHasOwner", (DEST,)).unwrap_or((false,));
        has_owner || activatable.iter().any(|n| n == DEST)
    }

    /// Asks the portal; waits for the answer (on a worker thread). None if
    /// the portal failed (the caller gets no paths, like a cancel).
    pub fn ask(save: bool, o: &Options, parent: &str) -> Option<Vec<PathBuf>> {
        let c = Connection::new_session().ok()?;
        // The request's object path is known ahead (sender + token), so
        // its answer can't be missed.
        let token = format!("heroui{}", std::process::id());
        let sender = c.unique_name().trim_start_matches(':').replace('.', "_");
        let request = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
        let answer: std::sync::Arc<std::sync::Mutex<Option<(u32, PropMap)>>> = Default::default();
        {
            let answer = answer.clone();
            let rule = MatchRule::new_signal("org.freedesktop.portal.Request", "Response").with_path(request.clone());
            c.add_match(rule, move |(code, results): (u32, PropMap), _, _| {
                *answer.lock().unwrap() = Some((code, results));
                false
            })
            .ok()?;
        }
        let mut opts: PropMap = HashMap::new();
        opts.insert("handle_token".into(), Variant(Box::new(token)));
        opts.insert("modal".into(), Variant(Box::new(true)));
        if o.multiple {
            opts.insert("multiple".into(), Variant(Box::new(true)));
        }
        if o.directory {
            opts.insert("directory".into(), Variant(Box::new(true)));
        }
        if let Some(a) = &o.accept {
            opts.insert("accept_label".into(), Variant(Box::new(a.clone())));
        }
        if !o.filters.is_empty() {
            let filters: Vec<(String, Vec<(u32, String)>)> = o.filters.iter().map(|(n, p)| (n.clone(), p.iter().map(|g| (0u32, g.clone())).collect())).collect();
            opts.insert("current_filter".into(), Variant(Box::new(filters[0].clone())));
            opts.insert("filters".into(), Variant(Box::new(filters)));
        }
        if let Some(f) = &o.folder {
            let mut b = f.as_os_str().as_encoded_bytes().to_vec();
            b.push(0);
            opts.insert("current_folder".into(), Variant(Box::new(b)));
        }
        if let Some(n) = &o.name {
            opts.insert("current_name".into(), Variant(Box::new(n.clone())));
        }
        let proxy = c.with_proxy(DEST, PATH, Duration::from_secs(30));
        let method = if save { "SaveFile" } else { "OpenFile" };
        let (_handle,): (dbus::Path,) = proxy.method_call("org.freedesktop.portal.FileChooser", method, (parent, o.title.as_str(), opts)).ok()?;
        // The person takes their time.
        while answer.lock().unwrap().is_none() {
            c.process(Duration::from_secs(1)).ok()?;
        }
        let (code, results) = answer.lock().unwrap().take()?;
        if code != 0 {
            return Some(vec![]);
        }
        let uris: Vec<String> = results
            .get("uris")
            .and_then(|v| v.0.as_iter())
            .map(|it| it.filter_map(|u| u.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        Some(uris.iter().filter_map(|u| path_of(u)).collect())
    }

    /// A `file://` URI's path (percent-decoded).
    pub fn path_of(uri: &str) -> Option<PathBuf> {
        let rest = uri.strip_prefix("file://")?;
        let mut out = Vec::with_capacity(rest.len());
        let b = rest.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' && i + 2 < b.len() {
                if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).ok()?, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
            out.push(b[i]);
            i += 1;
        }
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(std::ffi::OsString::from_vec(out)))
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn uris() {
            assert_eq!(super::path_of("file:///home/a%20b/%C3%BC.png").unwrap().to_str(), Some("/home/a b/ü.png"));
            assert_eq!(super::path_of("file:///x%2"), Some("/x%2".into()));
        }
    }
}
