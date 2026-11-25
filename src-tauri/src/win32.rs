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
    SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
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
    /// The monitor a game or video is filling, if any. `ALL_MONITORS` during a presentation.
    pub fullscreen: Option<isize>,
}

/// Stands for every monitor in `Snapshot::fullscreen`.
pub const ALL_MONITORS: isize = 0;

/// The monitor a window is on, as a value to compare with `Snapshot::fullscreen`.
pub fn monitor_of(h: Handle) -> isize {
    use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
    unsafe { MonitorFromWindow(hwnd(h), MONITOR_DEFAULTTONEAREST).0 as isize }
}

/// Whether `fullscreen` (from a snapshot) means the monitor this window is on is taken.
pub fn fullscreen_on(fullscreen: Option<isize>, h: Handle) -> bool {
    fullscreen.is_some_and(|monitor| monitor == ALL_MONITORS || monitor == monitor_of(h))
}

/// Which monitor something is using the whole of, so nothing of ours sits on top of it there.
/// Only that monitor counts: a game on one screen leaves the weather on another alone.
/// Asks Windows first (the same signal it uses to hold back notifications during games and
/// presentations), then checks whether the window in use covers its entire monitor, which
/// catches borderless-window games and full-screen video.
fn fullscreen_now(fg: HWND, desktop: bool, ours: bool) -> Option<isize> {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN};
    unsafe {
        let state = SHQueryUserNotificationState().ok();
        if state == Some(QUNS_PRESENTATION_MODE) {
            return Some(ALL_MONITORS); // a setting for the whole PC, not one window
        }
        if fg.0.is_null() || desktop || ours {
            return None;
        }
        // Windows reports a game or other full-screen app without saying where it is; it is
        // on the monitor of the window in use.
        let monitor = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        if state == Some(QUNS_RUNNING_D3D_FULL_SCREEN) || state == Some(QUNS_BUSY) {
            return Some(monitor.0 as isize);
        }
        let mut info = MONITORINFO { cbSize: core::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let m = info.rcMonitor;
        // Either measure of the window covering the monitor counts.
        [rect_of(fg, true), rect_of(fg, false)]
            .into_iter()
            .flatten()
            .any(|r| r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom)
            .then_some(monitor.0 as isize)
    }
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
    let desktop_foreground = has_fg && is_desktop_class(&class_of(fg));
    let foreground_is_ours = has_fg && pid_of(fg) == me;
    Snapshot {
        coverage,
        desktop_foreground,
        foreground_is_ours,
        fullscreen: fullscreen_now(fg, desktop_foreground, foreground_is_ours),
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

pub fn hide(h: Handle) {
    unsafe {
        let _ = ShowWindow(hwnd(h), SW_HIDE);
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

/// The empty stretch of the primary taskbar to the left of the Start button, as physical
/// `(left, right)` x coordinates. With centered taskbar icons this is the wide free area a
/// docked weather strip can use; with left-aligned icons it is empty or a few pixels.
/// Windows 11 draws the taskbar without child windows, so the buttons are located through
/// UI Automation. Called only when the strip is placed, never on a timer.
pub fn taskbar_free_left() -> Option<(i32, i32)> {
    use windows::core::BSTR;
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, TreeScope_Descendants, UIA_AutomationIdPropertyId};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let bar = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok()?;
        let bar_rect = rect_of(bar, false)?;
        let root = uia.ElementFromHandle(bar).ok()?;
        let find = |id: &str| {
            let condition = uia.CreatePropertyCondition(UIA_AutomationIdPropertyId, &VARIANT::from(BSTR::from(id))).ok()?;
            root.FindFirst(TreeScope_Descendants, &condition).ok()?.CurrentBoundingRectangle().ok()
        };
        let start = find("StartButton")?;
        // The Windows Widgets button, when switched on, takes the far left of a centered taskbar.
        let left = find("WidgetsButton").filter(|w| w.right > w.left && w.left < start.left).map_or(bar_rect.left, |w| w.right);
        Some((left, start.left))
    }
}

/// Whether Windows is set to a light taskbar ("Light" Windows mode).
pub fn taskbar_is_light() -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value = 1u32;
    let mut size = 4u32;
    unsafe {
        let _ = RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut core::ffi::c_void),
            Some(&mut size),
        )
        .is_ok();
    }
    // Windows is light unless the value says otherwise (a missing value means the default, light).
    value == 1
}
