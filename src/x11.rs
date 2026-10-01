//! EWMH hints for panels, docks and desktop widgets (X11, and XWayland
//! compositors that honor them). libX11 is already linked by fltk; this is
//! a handful of raw calls, run once at startup.

use std::ffi::{c_char, c_int, c_long, c_uchar, c_ulong, c_void, CString};

use fltk::prelude::*;
use fltk::window::Window;

use crate::{Edge, Settings, WindowKind};

type Atom = c_ulong;
type XWindow = c_ulong;

#[link(name = "X11")]
extern "C" {
    fn XInternAtom(d: *mut c_void, name: *const c_char, only_if_exists: c_int) -> Atom;
    fn XChangeProperty(
        d: *mut c_void,
        w: XWindow,
        property: Atom,
        kind: Atom,
        format: c_int,
        mode: c_int,
        data: *const c_uchar,
        n: c_int,
    ) -> c_int;
    fn XWithdrawWindow(d: *mut c_void, w: XWindow, screen: c_int) -> c_int;
    fn XMapWindow(d: *mut c_void, w: XWindow) -> c_int;
    fn XDefaultScreen(d: *mut c_void) -> c_int;
    fn XSync(d: *mut c_void, discard: c_int) -> c_int;
}

const XA_ATOM: Atom = 4;
const XA_CARDINAL: Atom = 6;
const PROP_MODE_REPLACE: c_int = 0;

/// True if `settings` asks for anything this module sets.
pub(crate) fn needed(s: &Settings) -> bool {
    s.kind != WindowKind::Normal || s.above || s.below || s.sticky || s.skip_taskbar || s.reserve.is_some()
}

/// Sets the hints on a shown window. Window-manager hints are read when a
/// window is mapped, so it is withdrawn, tagged and mapped again.
pub(crate) fn apply(win: &Window, s: &Settings) {
    let d = fltk::app::display();
    // An XID; typed as a pointer in Wayland-enabled (hybrid) builds.
    let w = win.raw_handle() as usize as XWindow;
    let atom = |name: &str| {
        let c = CString::new(name).expect("atom name");
        unsafe { XInternAtom(d, c.as_ptr(), 0) }
    };
    let set = |prop: &str, kind: Atom, values: &[c_long]| unsafe {
        // Format-32 properties are passed as C longs.
        XChangeProperty(d, w, atom(prop), kind, 32, PROP_MODE_REPLACE, values.as_ptr() as _, values.len() as c_int);
    };
    unsafe {
        XWithdrawWindow(d, w, XDefaultScreen(d));
        XSync(d, 0);
    }

    let kind = match s.kind {
        WindowKind::Normal => "_NET_WM_WINDOW_TYPE_NORMAL",
        WindowKind::Dock => "_NET_WM_WINDOW_TYPE_DOCK",
        WindowKind::Desktop => "_NET_WM_WINDOW_TYPE_DESKTOP",
        WindowKind::Dialog => "_NET_WM_WINDOW_TYPE_DIALOG",
        WindowKind::Utility => "_NET_WM_WINDOW_TYPE_UTILITY",
        WindowKind::Notification => "_NET_WM_WINDOW_TYPE_NOTIFICATION",
    };
    set("_NET_WM_WINDOW_TYPE", XA_ATOM, &[atom(kind) as c_long]);

    let mut states = Vec::new();
    for (on, name) in [
        (s.above, "_NET_WM_STATE_ABOVE"),
        (s.below, "_NET_WM_STATE_BELOW"),
        (s.sticky, "_NET_WM_STATE_STICKY"),
        (s.skip_taskbar, "_NET_WM_STATE_SKIP_TASKBAR"),
        (s.skip_taskbar, "_NET_WM_STATE_SKIP_PAGER"),
    ] {
        if on {
            states.push(atom(name) as c_long);
        }
    }
    if !states.is_empty() {
        set("_NET_WM_STATE", XA_ATOM, &states);
    }
    if s.sticky {
        set("_NET_WM_DESKTOP", XA_CARDINAL, &[0xFFFF_FFFF]);
    }

    if let Some((edge, px)) = s.reserve {
        // left, right, top, bottom, then start/end pairs for each edge.
        let (x, y, ww, wh) = (win.x() as c_long, win.y() as c_long, win.w() as c_long, win.h() as c_long);
        let px = px as c_long;
        let mut p = [0 as c_long; 12];
        match edge {
            Edge::Left => (p[0], p[4], p[5]) = (px, y, y + wh - 1),
            Edge::Right => (p[1], p[6], p[7]) = (px, y, y + wh - 1),
            Edge::Top => (p[2], p[8], p[9]) = (px, x, x + ww - 1),
            Edge::Bottom => (p[3], p[10], p[11]) = (px, x, x + ww - 1),
        }
        set("_NET_WM_STRUT_PARTIAL", XA_CARDINAL, &p);
        set("_NET_WM_STRUT", XA_CARDINAL, &p[..4]);
    }

    unsafe {
        XMapWindow(d, w);
        XSync(d, 0);
    }
}
