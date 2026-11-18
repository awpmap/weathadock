//! Windows-only window-stack helpers. They answer "is the desktop widget covered by other
//! windows?", keep the widget at desktop level, and locate the taskbar's tray area.
#![cfg(windows)]

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetClassNameW, GetForegroundWindow, GetTopWindow, GetWindow, GetWindowLongW,
    GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetWindowLongW, SetWindowPos, ShowWindow,
    GWL_EXSTYLE, GW_HWNDNEXT, HWND_BOTTOM, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SW_SHOWNOACTIVATE, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

pub type Handle = isize;

fn hwnd(h: Handle) -> HWND {
    HWND(h as *mut core::ffi::c_void)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Default)]
pub struct Snapshot {
    /// Share of the widget hidden by other windows, 0..1.
    pub coverage: f64,
    pub desktop_foreground: bool,
    pub foreground_is_ours: bool,
    pub foreground_rect: Option<Rect>,
}

fn rect_of(h: HWND, visible_bounds: bool) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe {
        // The DWM frame bounds leave out the invisible resize border that GetWindowRect includes.
        let dwm = visible_bounds
            && DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut _ as *mut _, core::mem::size_of::<RECT>() as u32).is_ok();
        if !dwm && GetWindowRect(h, &mut r).is_err() {
            return None;
        }
    }
    Some(Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom })
}

fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn pid_of(h: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
}

fn is_cloaked(h: HWND) -> bool {
    let mut cloaked = 0u32;
    unsafe { DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4).is_ok() && cloaked != 0 }
}

fn is_desktop_class(class: &str) -> bool {
    class == "Progman" || class == "WorkerW"
}

/// Look at the windows stacked above `target` and report how much of it they cover.
pub fn snapshot(target: Handle) -> Snapshot {
    let target = hwnd(target);
    let me = std::process::id();
    let mut covering: Vec<Rect> = Vec::new();
    let tr = rect_of(target, false).filter(|r| r.right > r.left && r.bottom > r.top);
    if let Some(tr) = tr {
        unsafe {
            let mut h = GetTopWindow(None).unwrap_or_default();
            let mut guard = 0;
            while !h.0.is_null() && h != target && guard < 2000 {
                guard += 1;
                let current = h;
                h = GetWindow(current, GW_HWNDNEXT).unwrap_or_default();
                if !IsWindowVisible(current).as_bool() || IsIconic(current).as_bool() {
                    continue;
                }
                let Some(r) = rect_of(current, true) else { continue };
                if r.right <= tr.left || r.left >= tr.right || r.bottom <= tr.top || r.top >= tr.bottom {
                    continue;
                }
                if r.right - r.left < 2 || r.bottom - r.top < 2 {
                    continue;
                }
                let ex = GetWindowLongW(current, GWL_EXSTYLE) as u32;
                if ex & WS_EX_TRANSPARENT.0 != 0 && ex & WS_EX_LAYERED.0 != 0 {
                    continue; // click-through overlays
                }
                if is_cloaked(current) || pid_of(current) == me {
                    continue; // hidden store apps and other virtual desktops; our own windows
                }
                let class = class_of(current);
                if is_desktop_class(&class) || class == "Shell_TrayWnd" || class == "Shell_SecondaryTrayWnd" {
                    continue;
                }
                covering.push(r);
            }
        }
    }
    let mut coverage = 0.0;
    if let (Some(tr), false) = (tr, covering.is_empty()) {
        const N: i32 = 8;
        let mut hit = 0;
        for iy in 0..N {
            for ix in 0..N {
                let x = tr.left as f64 + (ix as f64 + 0.5) / N as f64 * (tr.right - tr.left) as f64;
                let y = tr.top as f64 + (iy as f64 + 0.5) / N as f64 * (tr.bottom - tr.top) as f64;
                if covering.iter().any(|r| x >= r.left as f64 && x < r.right as f64 && y >= r.top as f64 && y < r.bottom as f64) {
                    hit += 1;
                }
            }
        }
        coverage = hit as f64 / (N * N) as f64;
    }
    let fg = unsafe { GetForegroundWindow() };
    let has_fg = !fg.0.is_null();
    Snapshot {
        coverage,
        desktop_foreground: has_fg && is_desktop_class(&class_of(fg)),
        foreground_is_ours: has_fg && pid_of(fg) == me,
        foreground_rect: if has_fg { rect_of(fg, true) } else { None },
    }
}

const KEEP: windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS =
    windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOACTIVATE.0);

pub fn send_to_bottom(h: Handle) {
    unsafe {
        let _ = SetWindowPos(hwnd(h), Some(HWND_BOTTOM), 0, 0, 0, 0, KEEP);
    }
}
pub fn keep_on_top(h: Handle) {
    unsafe {
        let _ = SetWindowPos(hwnd(h), Some(HWND_TOPMOST), 0, 0, 0, 0, KEEP);
    }
}
/// Bring to the front of the normal windows without taking focus or staying always-on-top.
pub fn raise(h: Handle) {
    unsafe {
        let _ = SetWindowPos(hwnd(h), Some(HWND_TOPMOST), 0, 0, 0, 0, KEEP);
        let _ = SetWindowPos(hwnd(h), Some(HWND_NOTOPMOST), 0, 0, 0, 0, KEEP);
    }
}

/// Make a window behave like a desktop widget: clicking it never takes focus from the app in
/// use, and it stays out of Alt+Tab.
pub fn make_widget_window(h: Handle) {
    unsafe {
        let ex = GetWindowLongW(hwnd(h), GWL_EXSTYLE) as u32;
        SetWindowLongW(hwnd(h), GWL_EXSTYLE, (ex | WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as i32);
    }
}

pub fn show_no_activate(h: Handle) {
    unsafe {
        let _ = ShowWindow(hwnd(h), SW_SHOWNOACTIVATE);
    }
}

/// Physical-pixel rectangles of the primary taskbar and of its notification area (tray).
pub fn taskbar_rects() -> Option<(Rect, Option<Rect>)> {
    unsafe {
        let bar = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok()?;
        let tray = FindWindowExW(Some(bar), None, w!("TrayNotifyWnd"), PCWSTR::null()).ok();
        Some((rect_of(bar, false)?, tray.and_then(|t| rect_of(t, false))))
    }
}

pub fn handle_of(window: &tauri::WebviewWindow) -> Option<Handle> {
    window.hwnd().ok().map(|h| h.0 as isize)
}
