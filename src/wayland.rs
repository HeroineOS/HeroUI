//! Wayland-specific startup: picking the backend for shell windows, and
//! wlr-layer-shell placement (feature `layer-shell`).
//!
//! Panels, desktop widgets and notifications need the wlr-layer-shell
//! protocol to be placed natively on Wayland. Before FLTK picks a backend,
//! [`prepare_backend`] asks the compositor whether it has that protocol
//! (one short connection, only for those window kinds). If it doesn't (e.g.
//! GNOME), or HeroUI was built without `layer-shell`, the app runs on
//! XWayland instead, where the X11 window hints apply.

use std::ffi::{c_char, c_int, c_void, CStr};

use crate::{Settings, WindowKind};

/// True if `settings` describes a desktop-shell window (layer-shell on Wayland).
pub(crate) fn is_shell_window(s: &Settings) -> bool {
    matches!(s.kind, WindowKind::Dock | WindowKind::Desktop | WindowKind::Notification)
}

/// Decides how a shell window is shown, before FLTK opens its display.
/// Returns true if it will be a native layer-shell surface; otherwise may
/// switch FLTK to X11 (XWayland) through `FLTK_BACKEND`.
pub(crate) fn prepare_backend(s: &Settings) -> bool {
    if !is_shell_window(s) || std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return false;
    }
    if let Some(b) = std::env::var_os("FLTK_BACKEND") {
        // The user chose; native only if it's Wayland and possible.
        return b == "wayland" && cfg!(feature = "layer-shell") && compositor_has_layer_shell();
    }
    let reason = if !compositor_has_layer_shell() {
        "the compositor has no wlr-layer-shell"
    } else if cfg!(feature = "layer-shell") {
        return true;
    } else {
        "HeroUI was built without the layer-shell feature"
    };
    if std::env::var_os("DISPLAY").is_some() {
        eprintln!("heroui: '{}' runs on XWayland: {reason}", s.title);
        // Single-threaded here: FLTK and our threads haven't started yet.
        std::env::set_var("FLTK_BACKEND", "x11");
    } else {
        eprintln!("heroui: '{}' is shown as a normal window: {reason}", s.title);
    }
    false
}

#[repr(C)]
struct WlInterface {
    _opaque: [u8; 0],
}

#[repr(C)]
struct RegistryListener {
    global: unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *const c_char, u32),
    global_remove: unsafe extern "C" fn(*mut c_void, *mut c_void, u32),
}

// libwayland-client, linked by fltk's Wayland backend.
extern "C" {
    static wl_registry_interface: WlInterface;
    fn wl_display_connect(name: *const c_char) -> *mut c_void;
    fn wl_display_disconnect(display: *mut c_void);
    fn wl_display_roundtrip(display: *mut c_void) -> c_int;
    fn wl_proxy_marshal_flags(
        proxy: *mut c_void,
        opcode: u32,
        interface: *const WlInterface,
        version: u32,
        flags: u32, ...
    ) -> *mut c_void;
    fn wl_proxy_get_version(proxy: *mut c_void) -> u32;
    fn wl_proxy_add_listener(proxy: *mut c_void, listener: *const c_void, data: *mut c_void) -> c_int;
    fn wl_proxy_destroy(proxy: *mut c_void);
}

unsafe extern "C" fn on_global(data: *mut c_void, _: *mut c_void, _: u32, interface: *const c_char, _: u32) {
    if CStr::from_ptr(interface).to_bytes() == b"zwlr_layer_shell_v1" {
        *(data as *mut bool) = true;
    }
}

unsafe extern "C" fn on_global_remove(_: *mut c_void, _: *mut c_void, _: u32) {}

static LISTENER: RegistryListener = RegistryListener { global: on_global, global_remove: on_global_remove };

/// Lists the compositor's globals over a short-lived connection.
fn compositor_has_layer_shell() -> bool {
    let mut found = false;
    unsafe {
        let display = wl_display_connect(std::ptr::null());
        if display.is_null() {
            return false;
        }
        // wl_display.get_registry (opcode 1), as the inline libwayland
        // helper does.
        let registry = wl_proxy_marshal_flags(
            display,
            1,
            &wl_registry_interface,
            wl_proxy_get_version(display),
            0,
            std::ptr::null_mut::<c_void>(),
        );
        if !registry.is_null() {
            wl_proxy_add_listener(registry, &LISTENER as *const _ as *const c_void, &mut found as *mut bool as *mut c_void);
            wl_display_roundtrip(display);
            wl_proxy_destroy(registry);
        }
        wl_display_disconnect(display);
    }
    found
}

/// Makes `win` a layer-shell surface according to `s` (before it's shown).
#[cfg(feature = "layer-shell")]
pub(crate) fn apply_layer(win: &fltk::window::Window, s: &Settings) {
    use fltk::prelude::*;
    use fltk_sys::window::{Fl_Window, Fl_Window_wl_layer_margins, Fl_Window_wl_layer_window};

    // FL/wayland.H: Fl_Wl_Layer, Fl_Wl_Anchor, Fl_Wl_Keyboard
    const BOTTOM: c_int = 1;
    const TOP: c_int = 2;
    const A_TOP: c_int = 1;
    const A_BOTTOM: c_int = 2;
    const A_LEFT: c_int = 4;
    const A_RIGHT: c_int = 8;
    const KEYBOARD_NONE: c_int = 0;
    const KEYBOARD_ON_DEMAND: c_int = 2;

    let (x, y) = s.position.unwrap_or((0, 0));
    // (layer, anchor, exclusive zone, keyboard, margins top/right/bottom/left)
    let (layer, anchor, zone, keyboard, margins) = match s.kind {
        WindowKind::Dock => {
            let (edge, px) = s.reserve.unwrap_or((crate::Edge::Top, 0));
            let anchor = match edge {
                crate::Edge::Top => A_TOP | A_LEFT | A_RIGHT,
                crate::Edge::Bottom => A_BOTTOM | A_LEFT | A_RIGHT,
                crate::Edge::Left => A_LEFT | A_TOP | A_BOTTOM,
                crate::Edge::Right => A_RIGHT | A_TOP | A_BOTTOM,
            };
            (TOP, anchor, px, KEYBOARD_ON_DEMAND, (0, 0, 0, 0))
        }
        // Positioned like on X11, from the screen's top-left corner,
        // ignoring space reserved by panels (-1).
        WindowKind::Desktop => (BOTTOM, A_TOP | A_LEFT, -1, KEYBOARD_NONE, (y, 0, 0, x)),
        _ => (TOP, A_TOP | A_LEFT, 0, KEYBOARD_NONE, (y, 0, 0, x)),
    };
    let name = s.class.as_deref().unwrap_or(&s.title);
    let name = std::ffi::CString::new(name.replace('\0', "")).unwrap_or_default();
    let ptr = win.as_widget_ptr() as *mut Fl_Window;
    unsafe {
        Fl_Window_wl_layer_window(ptr, layer, anchor, zone, keyboard, -1, name.as_ptr());
        Fl_Window_wl_layer_margins(ptr, margins.0, margins.1, margins.2, margins.3);
    }
}
