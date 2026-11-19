#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! WeathaDock: a transparent desktop weather widget with tray and taskbar presence.
//! This file owns the windows, tray, settings, refresh schedule and desktop/taskbar switching.

#[cfg(windows)]
mod hidpp;
mod net;
mod updater;
#[cfg(windows)]
mod win32;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::menu::{CheckMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Wry};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_notification::NotificationExt as _;
use weathadock_core::{settings, weather};

use net::{chrono_now_ms as now_ms, Net};
use updater::Updater;

const APP_NAME: &str = "WeathaDock";
const COVERED_AT: f64 = 0.35; // the widget counts as covered when this share of it is hidden
const TICK: Duration = Duration::from_millis(500);
const BRIDGE: &str = include_str!("bridge.js");
/// WebView2 switches, identical for every window (they share one browser process). The first
/// group is Tauri's own default; the rest trims processes and background work a widget does not need.
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,msEdgeTranslate,OptimizationHints,Translate,MediaRouter \
--in-process-gpu --disable-extensions --disable-component-update --disable-background-networking --no-pings --disable-sync --disable-breakpad";
const TRAY_PLAIN: &[u8] = include_bytes!("../../build/tray.png");
const SECTIONS: [&str; 5] = ["overview", "hourly", "daily", "alerts", "settings"];

struct Shared {
    dir: PathBuf,
    settings: Mutex<Value>,
    weather: Mutex<Value>,
    /// zip -> last good state, for instant location switching and instant startup.
    cache: Mutex<HashMap<String, Value>>,
    refresh_lock: tokio::sync::Mutex<()>,
    last_attempt: Mutex<i64>,
    last_failed: AtomicBool,
    /// window label -> accumulated drag position (physical px)
    drags: Mutex<HashMap<String, (f64, f64)>>,
    /// window label -> intended size (physical px)
    sizes: Mutex<HashMap<String, (u32, u32)>>,
    tray_number: Mutex<Option<Vec<u8>>>,
    /// Mouse battery: last reading as (percent, charging), when it was last tried, and whether the
    /// low-battery notice has been shown for this discharge.
    battery: Mutex<Option<(u8, bool)>>,
    battery_smooth: Mutex<weathadock_core::battery::Smoother>,
    battery_tried: Mutex<i64>,
    battery_warned: AtomicBool,
    /// Room for the docked taskbar strip in logical px (0 when it cannot dock), and whether the taskbar is light.
    dock: Mutex<(u32, bool)>,
    pending_section: Mutex<Option<String>>,
    covered: Mutex<Covered>,
    quitting: AtomicBool,
}

#[derive(Default)]
struct Covered {
    covered: bool,
    votes: u8,
    desktop_was_foreground: bool,
    fullscreen: bool,
}

// ---------------------------------------------------------------- settings

fn shared(app: &AppHandle) -> tauri::State<'_, Shared> {
    app.state::<Shared>()
}
fn setting(app: &AppHandle, key: &str) -> Value {
    shared(app).settings.lock().unwrap()[key].clone()
}
fn flag(app: &AppHandle, key: &str) -> bool {
    setting(app, key).as_bool().unwrap_or(false)
}
fn settings_snapshot(app: &AppHandle) -> Value {
    shared(app).settings.lock().unwrap().clone()
}
/// The weather is docked on the taskbar instead of sitting on the desktop.
fn docked(app: &AppHandle) -> bool {
    setting(app, "placement") == "taskbar"
}

/// The desktop widget is switched on and not replaced by the taskbar dock.
fn widget_enabled(app: &AppHandle) -> bool {
    flag(app, "showWidget") && !docked(app)
}

fn active_location(app: &AppHandle) -> Option<Value> {
    settings::active_location(&shared(app).settings.lock().unwrap())
}
fn active_zip(app: &AppHandle) -> Option<String> {
    settings::active_zip(&shared(app).settings.lock().unwrap())
}

/// Apply changes to the stored settings and write them to disk.
fn store(app: &AppHandle, change: impl FnOnce(&mut Value)) {
    let st = shared(app);
    let text = {
        let mut data = st.settings.lock().unwrap();
        change(&mut data);
        serde_json::to_string_pretty(&*data).unwrap_or_default()
    };
    let _ = std::fs::create_dir_all(&st.dir);
    let tmp = st.dir.join("settings.json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, st.dir.join("settings.json"));
    }
}

fn load_settings(dir: &PathBuf, roaming: Option<PathBuf>) -> Value {
    let read = |p: PathBuf| std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let stored = read(dir.join("settings.json"))
        // Versions 1.0.x (built on Electron) kept their settings in %APPDATA%\WeathaDock.
        .or_else(|| roaming.and_then(|r| read(r.join("WeathaDock").join("settings.json"))));
    settings::load(stored)
}

fn glass(app: &AppHandle) -> String {
    match setting(app, "glass").as_str() {
        Some("clear") => "clear",
        Some("frosted") if cfg!(windows) => "frosted",
        _ => "none",
    }
    .to_string()
}

// ---------------------------------------------------------------- weather

fn save_cache(app: &AppHandle) {
    let st = shared(app);
    let text = serde_json::to_string(&*st.cache.lock().unwrap()).unwrap_or_default();
    let _ = std::fs::write(st.dir.join("forecast-cache.json"), text);
}

fn load_cache(dir: &PathBuf, data: &Value) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    let Some(Value::Object(map)) = std::fs::read_to_string(dir.join("forecast-cache.json")).ok().and_then(|t| serde_json::from_str(&t).ok()) else {
        return out;
    };
    let known: Vec<&str> = data["locations"].as_array().into_iter().flatten().filter_map(|l| l["zip"].as_str()).collect();
    for (zip, state) in map {
        let age = state["updatedAt"].as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d| now_ms() - d.timestamp_millis());
        if known.contains(&zip.as_str()) && state["status"] == "ok" && age.map_or(false, |a| (0..6 * 3_600_000).contains(&a)) {
            out.insert(zip, state);
        }
    }
    out
}

fn publish(app: &AppHandle) {
    let state = shared(app).weather.lock().unwrap().clone();
    let _ = app.emit("sky:state", state);
    update_tray(app);
}

fn publish_settings(app: &AppHandle) {
    let data = settings_snapshot(app);
    let _ = app.emit("sky:settings", settings::public_settings(&data));
    let _ = app.emit("sky:locations", settings::public_locations(&data));
    update_tray(app);
}

async fn refresh(app: &AppHandle) {
    let st = shared(app);
    let _guard = match st.refresh_lock.try_lock() {
        Ok(g) => g,
        Err(_) => {
            // Another refresh is running. Wait for it, and only fetch again if it was for a
            // different place than the one now selected.
            let g = st.refresh_lock.lock().await;
            let shown = st.weather.lock().unwrap()["location"]["zip"].as_str().map(String::from);
            let fresh = st.weather.lock().unwrap()["updatedAt"].is_string() && !st.last_failed.load(Ordering::Relaxed);
            if shown == active_zip(app) && fresh {
                return;
            }
            g
        }
    };
    let Some(loc) = active_location(app) else { return };
    let zip = loc["zip"].as_str().unwrap_or("").to_string();
    *st.last_attempt.lock().unwrap() = now_ms();
    let previous_alerts = st.weather.lock().unwrap()["alerts"].clone();
    let result = app.state::<Net>().fetch_weather(&loc, &previous_alerts).await;
    let still_active = active_zip(app).as_deref() == Some(zip.as_str());
    match result {
        Ok(next) => {
            st.cache.lock().unwrap().insert(zip, next.clone());
            save_cache(app);
            if !still_active {
                return; // the person switched places while this was loading
            }
            st.last_failed.store(false, Ordering::Relaxed);
            notify_alerts(app, &next["alerts"]);
            *st.weather.lock().unwrap() = next;
        }
        Err(message) => {
            if !still_active {
                return;
            }
            st.last_failed.store(true, Ordering::Relaxed);
            let mut current = st.weather.lock().unwrap();
            if current["current"].is_object() {
                current["stale"] = json!(true);
                current["error"] = json!(message);
            } else {
                *current = weather::empty_state("error", Some(&message), Some(&loc));
            }
        }
    }
    publish(app);
}

fn spawn_refresh(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { refresh(&app).await });
}

/// Refreshes on the chosen interval, a minute after a failure, and promptly after sleep.
fn start_scheduler(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(20)).await;
            check_battery(&app, false);
            if active_location(&app).is_none() {
                continue;
            }
            let st = shared(&app);
            let minutes = setting(&app, "refreshMinutes").as_i64().unwrap_or(10);
            let wait = if st.last_failed.load(Ordering::Relaxed) { 60_000 } else { minutes * 60_000 };
            let due = now_ms() - *st.last_attempt.lock().unwrap() >= wait;
            if due {
                refresh(&app).await;
            }
        }
    });
}

// Checked on every scheduler pass (20 s), so plugging the mouse in shows up quickly. One check is a single
// short exchange with the receiver on a background thread.
const BATTERY_EVERY_MS: i64 = 15_000;
const BATTERY_LOW: u8 = 15;

fn battery_info(app: &AppHandle) -> Value {
    match *shared(app).battery.lock().unwrap() {
        Some((percent, charging)) if flag(app, "mouseBattery") => json!({ "percent": percent, "charging": charging, "low": percent <= BATTERY_LOW && !charging }),
        _ => Value::Null,
    }
}

/// Read the mouse battery when the option is on: every few minutes, or right away with `now`.
/// The last reading is kept while the mouse is asleep, so the number does not come and go.
fn check_battery(app: &AppHandle, now: bool) {
    let st = shared(app);
    if !flag(app, "mouseBattery") {
        return;
    }
    {
        let mut tried = st.battery_tried.lock().unwrap();
        if !now && now_ms() - *tried < BATTERY_EVERY_MS {
            return;
        }
        *tried = now_ms();
    }
    let app = app.clone();
    std::thread::spawn(move || {
        #[cfg(windows)]
        let reading = hidpp::mouse_battery().map(|b| {
            trace(format!("mouse battery raw feature={:04x} device={:02x} bytes={:02x?}", b.feature, b.device, b.raw));
            (b.percent, b.charging)
        });
        #[cfg(not(windows))]
        let reading: Option<(u8, bool)> = None;
        let st = shared(&app);
        let Some((raw_percent, charging)) = reading else {
            trace("mouse battery -> None".to_string());
            return;
        };
        // Single readings wander with what the mouse is doing; show the steadied value.
        let raw_charging = charging;
        let (percent, charging) = st.battery_smooth.lock().unwrap().push(raw_percent, raw_charging);
        trace(format!("mouse battery -> read {raw_percent}% charging {raw_charging}, shown {percent}% charging {charging}"));
        let changed = st.battery.lock().unwrap().replace((percent, charging)) != Some((percent, charging));
        if charging || percent > BATTERY_LOW + 5 {
            st.battery_warned.store(false, Ordering::Relaxed);
        } else if percent <= BATTERY_LOW && !st.battery_warned.swap(true, Ordering::Relaxed) {
            let _ = app.notification().builder().title("Mouse battery low").body(format!("Your mouse is at {percent}%. Time to charge it.")).show();
        }
        if changed {
            let _ = app.emit("sky:battery", battery_info(&app));
            update_tray(&app);
        }
    });
}

fn notify_alerts(app: &AppHandle, alerts: &Value) {
    let seen: Vec<Value> = setting(app, "notifiedAlerts").as_array().cloned().unwrap_or_default();
    let fresh: Vec<&Value> = alerts.as_array().into_iter().flatten().filter(|a| !seen.contains(&a["id"])).collect();
    if fresh.is_empty() {
        return;
    }
    let mut ids: Vec<Value> = fresh.iter().map(|a| a["id"].clone()).collect();
    ids.extend(seen);
    ids.truncate(80);
    store(app, |d| d["notifiedAlerts"] = json!(ids));
    if !flag(app, "alertNotifications") {
        return;
    }
    for a in fresh.iter().take(3) {
        let body = a["headline"].as_str().filter(|s| !s.is_empty()).or(a["areaDesc"].as_str()).unwrap_or("");
        let _ = app.notification().builder().title(a["event"].as_str().unwrap_or("Weather alert")).body(body).show();
    }
}

// ---------------------------------------------------------------- tray

fn tray_tooltip(app: &AppHandle) -> String {
    let s = shared(app).weather.lock().unwrap().clone();
    let Some(temp) = s["current"]["temp"].as_i64() else {
        return match s["error"].as_str() {
            Some(e) if s["status"] == "error" => format!("{APP_NAME}: {e}").chars().take(120).collect(),
            _ => APP_NAME.to_string(),
        };
    };
    let deg = |v: &Value| v.as_i64().map(|n| n.to_string()).unwrap_or_else(|| "--".into());
    // Lead with what this is, so the number is never taken for a hardware temperature.
    let place = active_location(app).map(|l| place_label(&l));
    let mut parts = vec![
        place.map_or_else(|| "Weather".to_string(), |p| format!("Weather in {p}")),
        format!("{temp}°F {}", s["current"]["condition"].as_str().unwrap_or("")),
    ];
    if s["today"].is_object() {
        parts.push(format!("H {}°  L {}°", deg(&s["today"]["high"]), deg(&s["today"]["low"])));
    }
    if let Some(h) = s["nextPrecip"]["headline"].as_str().filter(|h| !h.is_empty()) {
        parts.push(h.to_string());
    }
    if let Some(alerts) = s["alerts"].as_array().filter(|a| !a.is_empty()) {
        let more = if alerts.len() > 1 { format!(" +{}", alerts.len() - 1) } else { String::new() };
        parts.push(format!("⚠ {}{more}", alerts[0]["event"].as_str().unwrap_or("Alert")));
    }
    if let Some(percent) = battery_info(app)["percent"].as_u64() {
        parts.push(format!("Mouse battery {percent}%"));
    }
    // Windows limits tray tooltips to 127 characters.
    parts.join("\n").chars().take(127).collect()
}

fn place_label(l: &Value) -> String {
    format!("{}, {}", l["city"].as_str().unwrap_or(""), l["state"].as_str().unwrap_or(""))
}

fn build_menu(app: &AppHandle, for_widget: bool) -> tauri::Result<Menu<Wry>> {
    let data = settings_snapshot(app);
    let places = data["locations"].as_array().cloned().unwrap_or_default();
    let active = settings::active_zip(&data);
    let mut menu = MenuBuilder::new(app);
    if !for_widget {
        let title = settings::active_location(&data).map(|l| place_label(&l)).unwrap_or_else(|| APP_NAME.into());
        menu = menu.item(&MenuItemBuilder::with_id("title", title).enabled(false).build(app)?).separator();
    }
    menu = menu
        .item(&MenuItemBuilder::with_id("open", "Open WeathaDock").build(app)?)
        .item(&MenuItemBuilder::with_id("refresh", "Refresh now").build(app)?);
    if places.len() > 1 {
        let mut sub = SubmenuBuilder::new(app, "Location");
        for l in &places {
            let zip = l["zip"].as_str().unwrap_or("");
            let item = CheckMenuItemBuilder::with_id(format!("loc:{zip}"), format!("{}  {zip}", place_label(l)))
                .checked(active.as_deref() == Some(zip))
                .build(app)?;
            sub = sub.item(&item);
        }
        menu = menu.item(&sub.build()?);
    }
    let alerts = shared(app).weather.lock().unwrap()["alerts"].as_array().map_or(0, Vec::len);
    if alerts > 0 && !for_widget {
        menu = menu.item(&MenuItemBuilder::with_id("alerts", format!("Alerts ({alerts})")).build(app)?);
    }
    let check = |id: &str, label: &str, key: &str| CheckMenuItemBuilder::with_id(id, label).checked(data[key].as_bool().unwrap_or(false)).build(app);
    menu = menu.separator();
    if for_widget {
        menu = menu
            .item(&MenuItemBuilder::with_id("settings", "Customize widget…").build(app)?)
            .item(&check("lock", "Lock position", "lockPosition")?)
            .item(&check("ontop", "Keep above other windows", "widgetOnTop")?)
            .item(&MenuItemBuilder::with_id("hide", "Hide widget").build(app)?);
    } else {
        menu = if docked(app) {
            menu.item(&MenuItemBuilder::with_id("undock", "Move weather to the desktop").build(app)?)
        } else {
            menu.item(&check("show", "Show desktop widget", "showWidget")?)
        };
        menu = menu
            .item(&MenuItemBuilder::with_id("reset", "Reset widget and taskbar positions").build(app)?)
            .item(&MenuItemBuilder::with_id("settings", "Settings").build(app)?);
        if let Some(version) = app.state::<Updater>().is_ready() {
            menu = menu.item(&MenuItemBuilder::with_id("update", format!("Restart to update to {version}")).build(app)?);
        }
    }
    menu.separator().item(&MenuItemBuilder::with_id("quit", "Quit WeathaDock").build(app)?).build()
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => open_detail(app, "overview"),
        "alerts" => open_detail(app, "alerts"),
        "settings" => open_detail(app, "settings"),
        "refresh" => spawn_refresh(app),
        "show" => apply_settings(app, &json!({ "showWidget": !flag(app, "showWidget") })),
        "hide" => apply_settings(app, &json!({ "showWidget": false })),
        "undock" => apply_settings(app, &json!({ "placement": "desktop", "showWidget": true })),
        "lock" => apply_settings(app, &json!({ "lockPosition": !flag(app, "lockPosition") })),
        "ontop" => apply_settings(app, &json!({ "widgetOnTop": !flag(app, "widgetOnTop") })),
        "reset" => {
            store(app, |d| {
                d["widgetPos"] = Value::Null;
                d["pillPos"] = Value::Null;
            });
            rebuild_widget(app);
            place_pill(app);
        }
        "update" => app.state::<Updater>().install(app),
        "quit" => quit(app),
        other => {
            if let Some(zip) = other.strip_prefix("loc:") {
                set_active(app, zip);
            }
        }
    }
}

fn update_tray(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("main") else { return };
    let _ = tray.set_tooltip(Some(tray_tooltip(app)));
    if let Ok(menu) = build_menu(app, false) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Either/or: in "tray" mode the tray icon is the temperature; in "taskbar" mode the pill
/// carries the weather and the tray shows the plain app glyph.
fn apply_tray_image(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("main") else { return };
    let number = shared(app).tray_number.lock().unwrap().clone();
    let bytes = match (setting(app, "coveredMode").as_str(), number) {
        (Some("tray"), Some(png)) if !docked(app) => png,
        _ => TRAY_PLAIN.to_vec(),
    };
    if let Ok(image) = tauri::image::Image::from_bytes(&bytes) {
        let _ = tray.set_icon(Some(image));
    }
}

fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip(APP_NAME)
        .menu(&build_menu(app, false)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                open_detail(tray.app_handle(), "overview");
            }
        });
    if let Ok(image) = tauri::image::Image::from_bytes(TRAY_PLAIN) {
        builder = builder.icon(image);
    }
    builder.build(app)?;
    Ok(())
}

// ---------------------------------------------------------------- windows

/// Windows must not be created on the main thread while the event loop is busy with the call
/// that asked for them (menu clicks, tray clicks, commands), so creation happens on a worker.
fn off_main(app: &AppHandle, work: impl FnOnce(&AppHandle) + Send + 'static) {
    let app = app.clone();
    std::thread::spawn(move || work(&app));
}

fn page(app: &AppHandle, file: &str) -> WebviewUrl {
    WebviewUrl::App(format!("{file}?glass={}&platform=win32", glass(app)).into())
}

fn window(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
    app.get_webview_window(label)
}

/// Hide a window that `show_inactive` showed. Those windows are shown with Win32 directly, so
/// the window library still believes they are hidden and its own `hide()` would do nothing.
fn hide_window(w: &WebviewWindow) {
    #[cfg(windows)]
    if let Some(h) = win32::handle_of(w) {
        win32::hide(h);
        return;
    }
    let _ = w.hide();
}

fn show_inactive(w: &WebviewWindow) {
    #[cfg(windows)]
    if let Some(h) = win32::handle_of(w) {
        win32::show_no_activate(h);
        return;
    }
    let _ = w.show();
}

/// Work area (x, y, width, height in physical px) and scale of the monitor a window is on.
fn work_area(app: &AppHandle, near: Option<(i32, i32)>) -> (i32, i32, i32, i32, f64) {
    let monitor = near
        .and_then(|(x, y)| app.monitor_from_point(x as f64, y as f64).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    match monitor {
        Some(m) => {
            let a = m.work_area();
            (a.position.x, a.position.y, a.size.width as i32, a.size.height as i32, m.scale_factor())
        }
        None => (0, 0, 1920, 1080, 1.0),
    }
}

fn saved_point(v: &Value) -> Option<(i32, i32)> {
    Some((v["x"].as_i64()? as i32, v["y"].as_i64()? as i32))
}

fn create_widget(app: &AppHandle) {
    off_main(app, create_widget_now);
}

fn create_widget_now(app: &AppHandle) {
    if window(app, "widget").is_some() {
        return;
    }
    let frosted = glass(app) == "frosted";
    let mut builder = WebviewWindowBuilder::new(app, "widget", page(app, "widget.html"))
        .title(APP_NAME)
        .inner_size(380.0, 420.0)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .shadow(false)
        .focused(false)
        .visible(false)
        .always_on_top(flag(app, "widgetOnTop"))
        .initialization_script(BRIDGE)
        .additional_browser_args(BROWSER_ARGS);
    if frosted {
        builder = builder.effects(tauri::window::EffectsBuilder::new().effect(tauri::window::Effect::Acrylic).build());
    }
    let Ok(w) = builder.build() else { return };
    // Park it where it was left, or in the top-right corner of the main screen.
    let saved = saved_point(&setting(app, "widgetPos")).filter(|&(x, y)| {
        let (ax, ay, aw, ah, _) = work_area(app, Some((x, y)));
        x + 60 > ax && x < ax + aw - 40 && y + 20 > ay && y < ay + ah - 20
    });
    let (ax, ay, aw, _, scale) = work_area(app, None);
    let (x, y) = saved.unwrap_or((ax + aw - ((380.0 + 28.0) * scale) as i32, ay + (28.0 * scale) as i32));
    let _ = w.set_position(PhysicalPosition::new(x, y));
    #[cfg(windows)]
    if let Some(h) = win32::handle_of(&w) {
        win32::make_widget_window(h);
    }
    if widget_enabled(app) {
        show_widget(app);
    }
}

fn show_widget(app: &AppHandle) {
    let Some(w) = window(app, "widget") else { return };
    let on_top = flag(app, "widgetOnTop");
    let _ = w.set_always_on_top(on_top);
    show_inactive(&w);
    #[cfg(windows)]
    if !on_top {
        if let Some(h) = win32::handle_of(&w) {
            win32::send_to_bottom(h);
        }
    }
}

fn rebuild_widget(app: &AppHandle) {
    // The glass style is fixed when a window is created, so make a new one.
    if let Some(w) = window(app, "widget") {
        let _ = w.destroy();
    }
    off_main(app, |app| {
        std::thread::sleep(Duration::from_millis(150));
        create_widget_now(app);
    });
}

/// Where the pill belongs: over the taskbar, just left of the tray icons (physical px).
fn pill_geometry(app: &AppHandle, width: i32) -> (i32, i32, i32) {
    #[cfg(windows)]
    if let Some((bar, tray)) = win32::taskbar_rects() {
        let height = bar.bottom - bar.top;
        let horizontal = bar.right - bar.left > height * 3;
        if horizontal && height >= 30 {
            let scale = window(app, "pill").and_then(|w| w.scale_factor().ok()).unwrap_or(1.0);
            let mut dock = (0u32, win32::taskbar_is_light());
            let mut spot = None;
            if docked(app) {
                // Docked: the empty stretch left of the Start button on a centered taskbar.
                let margin = (height as f64 * 0.25) as i32;
                if let Some((left, right)) = win32::taskbar_free_left() {
                    let room = ((right - left - 2 * margin) as f64 / scale) as i32;
                    if room >= 150 {
                        dock.0 = room.min(900) as u32;
                        spot = Some((left + margin, bar.top, height));
                    }
                }
            }
            trace(format!("pill_geometry docked={} dock={dock:?} spot={spot:?} scale={scale}", docked(app)));
            let changed = std::mem::replace(&mut *shared(app).dock.lock().unwrap(), dock) != dock;
            if changed {
                let _ = app.emit("sky:dock", dock_info(app));
            }
            if let Some(spot) = spot {
                return spot;
            }
            // Otherwise (no free stretch, for example left-aligned icons) it sits by the tray.
            let right = tray.map(|t| t.left).filter(|&l| l > (bar.left + bar.right) / 2).unwrap_or(bar.right - 340) - 8;
            return (right - width, bar.top, height);
        }
    }
    // No usable taskbar: float in the bottom-right corner of the work area instead.
    let (ax, ay, aw, ah, scale) = work_area(app, None);
    let height = (44.0 * scale) as i32;
    (ax + aw - width - (16.0 * scale) as i32, ay + ah - height - (12.0 * scale) as i32, height)
}

fn place_pill(app: &AppHandle) {
    let Some(w) = window(app, "pill") else { return };
    let st = shared(app);
    let width = st.sizes.lock().unwrap().get("pill").map(|s| s.0 as i32).unwrap_or(180);
    let (x, y, height) = pill_geometry(app, width);
    st.sizes.lock().unwrap().insert("pill".into(), (width as u32, height as u32));
    // The dock has one fixed home; only the floating pill remembers where it was dragged.
    let (x, y) = if shared(app).dock.lock().unwrap().0 > 0 { (x, y) } else { saved_point(&setting(app, "pillPos")).unwrap_or((x, y)) };
    let _ = w.set_size(PhysicalSize::new(width as u32, height as u32));
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

/// The taskbar pill has its own webview, so it only exists in "taskbar" mode.
fn sync_pill(app: &AppHandle) {
    off_main(app, sync_pill_now);
}

fn sync_pill_now(app: &AppHandle) {
    let wanted = docked(app) || setting(app, "coveredMode") == "taskbar";
    match (wanted, window(app, "pill")) {
        (true, None) => {
            let built = WebviewWindowBuilder::new(app, "pill", page(app, "pill.html"))
                .title("WeathaDock taskbar")
                .inner_size(180.0, 48.0)
                .decorations(false)
                .transparent(true)
                .resizable(false)
                .maximizable(false)
                .minimizable(false)
                .skip_taskbar(true)
                .shadow(false)
                .focused(false)
                .visible(false)
                .always_on_top(true)
                .initialization_script(BRIDGE)
        .additional_browser_args(BROWSER_ARGS)
                .build();
            if let Ok(_w) = built {
                #[cfg(windows)]
                if let Some(h) = win32::handle_of(&_w) {
                    win32::make_widget_window(h);
                }
                place_pill(app);
            }
        }
        (false, Some(w)) => {
            let _ = w.destroy();
        }
        _ => {}
    }
}

fn set_pill_visible(app: &AppHandle, visible: bool) {
    let Some(w) = window(app, "pill") else { return };
    if w.is_visible().unwrap_or(false) == visible {
        return;
    }
    if visible {
        place_pill(app);
        show_inactive(&w);
    } else {
        hide_window(&w);
    }
}

fn open_detail(app: &AppHandle, section: &str) {
    let section = section.to_string();
    off_main(app, move |app| open_detail_now(app, &section));
}

fn open_detail_now(app: &AppHandle, section: &str) {
    if active_location(app).is_none() {
        return open_setup_now(app);
    }
    let section = if SECTIONS.contains(&section) { section } else { "overview" };
    // Opening the app is a good moment to make sure the numbers are current.
    let stale = shared(app).weather.lock().unwrap()["updatedAt"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map_or(true, |d| now_ms() - d.timestamp_millis() > 60_000);
    if stale {
        spawn_refresh(app);
    }
    if let Some(w) = window(app, "detail") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("sky:navigate", section);
        return;
    }
    *shared(app).pending_section.lock().unwrap() = Some(section.to_string());
    let mut builder = WebviewWindowBuilder::new(app, "detail", page(app, "detail.html"))
        .title(APP_NAME)
        .inner_size(1000.0, 700.0)
        .min_inner_size(780.0, 560.0)
        .decorations(false)
        .background_color(tauri::window::Color(18, 23, 34, 255))
        .initialization_script(BRIDGE)
        .additional_browser_args(BROWSER_ARGS);
    let saved = setting(app, "detailBounds");
    let size = saved["width"].as_u64().zip(saved["height"].as_u64()).filter(|&(w, h)| w >= 600 && h >= 400);
    if size.is_none() {
        builder = builder.center();
    }
    let Ok(w) = builder.build() else { return };
    if let Some((width, height)) = size {
        let _ = w.set_size(PhysicalSize::new(width as u32, height as u32));
        match saved_point(&saved).filter(|&(x, y)| {
            let (ax, ay, aw, ah, _) = work_area(app, Some((x, y)));
            x + 200 > ax && x < ax + aw - 100 && y >= ay - 10 && y < ay + ah - 100
        }) {
            Some((x, y)) => {
                let _ = w.set_position(PhysicalPosition::new(x, y));
            }
            None => {
                let _ = w.center();
            }
        }
    }
    let handle = app.clone();
    w.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { .. } = event {
            let Some(w) = window(&handle, "detail") else { return };
            if w.is_maximized().unwrap_or(false) || w.is_minimized().unwrap_or(false) {
                return;
            }
            if let (Ok(p), Ok(s)) = (w.outer_position(), w.inner_size()) {
                store(&handle, |d| d["detailBounds"] = json!({ "x": p.x, "y": p.y, "width": s.width, "height": s.height }));
            }
        }
    });
}

fn open_setup(app: &AppHandle) {
    off_main(app, open_setup_now);
}

fn open_setup_now(app: &AppHandle) {
    if let Some(w) = window(app, "setup") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, "setup", page(app, "setup.html"))
        .title("Welcome to WeathaDock")
        .inner_size(520.0, 640.0)
        .resizable(false)
        .maximizable(false)
        .decorations(false)
        .background_color(tauri::window::Color(18, 23, 34, 255))
        .center()
        .initialization_script(BRIDGE)
        .additional_browser_args(BROWSER_ARGS)
        .build();
}

fn start_app(app: &AppHandle) {
    create_widget(app);
    sync_pill(app);
    spawn_refresh(app);
}

fn quit(app: &AppHandle) {
    shared(app).quitting.store(true, Ordering::Relaxed);
    app.exit(0);
}

// ---------------------------------------------------------------- desktop / taskbar switching

#[cfg(windows)]
fn tick(app: &AppHandle) {
    let Some(widget) = window(app, "widget") else { return };
    let Some(handle) = win32::handle_of(&widget) else { return };
    let widget_on = widget_enabled(app);
    let visible = widget.is_visible().unwrap_or(false);
    let on_top = flag(app, "widgetOnTop");
    let snap = win32::snapshot(handle);
    let st = shared(app);
    let mut c = st.covered.lock().unwrap();

    // Keep the widget at desktop level: bring it forward when the desktop itself is in front
    // (for example after Win+D), and tuck it under other apps as soon as one is used.
    if widget_on && visible && !on_top {
        if snap.desktop_foreground && !c.desktop_was_foreground {
            win32::raise(handle);
        }
        if !snap.desktop_foreground && c.desktop_was_foreground && !snap.foreground_is_ours {
            win32::send_to_bottom(handle);
        }
    }
    c.desktop_was_foreground = snap.desktop_foreground;

    let now_covered = !widget_on || !visible || (!on_top && !snap.desktop_foreground && snap.coverage >= COVERED_AT);
    // Two matching readings in a row before switching, so a window dragged past does not flicker.
    c.votes = if now_covered == c.covered { 0 } else { c.votes + 1 };
    if c.votes >= 2 {
        c.covered = now_covered;
        c.votes = 0;
    }
    let covered = c.covered;
    // A widget kept above other windows steps back while a game, video or presentation has the
    // whole screen, and returns afterwards.
    if snap.fullscreen != c.fullscreen {
        c.fullscreen = snap.fullscreen;
        trace(format!("fullscreen -> {}", snap.fullscreen));
        if widget_on && on_top {
            let _ = widget.set_always_on_top(!snap.fullscreen);
            if snap.fullscreen {
                win32::send_to_bottom(handle);
            }
        }
    }
    drop(c);

    let Some(pill) = window(app, "pill") else { return };
    let fullscreen = snap.fullscreen;
    let show = covered && !fullscreen;
    set_pill_visible(app, show);
    if show {
        if let Some(h) = win32::handle_of(&pill) {
            win32::keep_on_top(h); // the taskbar likes to climb back over it
        }
    }
}

fn start_ticking(app: &AppHandle) {
    #[cfg(windows)]
    {
        let app = app.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(TICK);
            tick(&app);
        });
    }
    #[cfg(not(windows))]
    let _ = (app, TICK, COVERED_AT);
}

// ---------------------------------------------------------------- settings changes

fn apply_login_item(app: &AppHandle) {
    // Only the installed app registers itself, never a development build.
    if cfg!(debug_assertions) {
        return;
    }
    let launcher = app.autolaunch();
    let _ = if flag(app, "launchAtLogin") { launcher.enable() } else { launcher.disable() };
}

fn apply_settings(app: &AppHandle, patch: &Value) {
    let before = settings_snapshot(app);
    let clean = settings::sanitize_patch(patch, &before);
    store(app, |d| {
        for (k, v) in &clean {
            d[k.as_str()] = v.clone();
        }
    });
    let after = settings_snapshot(app);
    let changed = |key: &str| before[key] != after[key];
    if changed("launchAtLogin") {
        apply_login_item(app);
    }
    if changed("glass") {
        rebuild_widget(app);
        if let Some(pill) = window(app, "pill") {
            let _ = pill.destroy();
        }
    }
    if changed("widgetOnTop") && !changed("glass") {
        if widget_enabled(app) {
            show_widget(app);
        } else if let Some(w) = window(app, "widget") {
            let _ = w.set_always_on_top(flag(app, "widgetOnTop"));
        }
    }
    if changed("showWidget") || changed("placement") {
        if widget_enabled(app) {
            show_widget(app);
        } else if let Some(w) = window(app, "widget") {
            hide_window(&w);
        }
    }
    if changed("coveredMode") || changed("glass") || changed("placement") {
        apply_tray_image(app);
        off_main(app, |app| {
            std::thread::sleep(Duration::from_millis(200));
            sync_pill_now(app);
            place_pill(app);
        });
    }
    if changed("mouseBattery") {
        let _ = app.emit("sky:battery", battery_info(app));
        check_battery(app, true);
    }
    publish_settings(app);
}

fn set_active(app: &AppHandle, zip: &str) {
    let Some(loc) = settings_snapshot(app)["locations"].as_array().into_iter().flatten().find(|l| l["zip"] == zip).cloned() else {
        return;
    };
    store(app, |d| d["activeZip"] = json!(zip));
    // Show what is already known for this place straight away, then fetch fresh data.
    let st = shared(app);
    let cached = st.cache.lock().unwrap().get(zip).cloned();
    *st.weather.lock().unwrap() = cached.unwrap_or_else(|| weather::empty_state("loading", None, Some(&loc)));
    publish(app);
    publish_settings(app);
    spawn_refresh(app);
}

// ---------------------------------------------------------------- commands

#[tauri::command]
fn get_state(app: AppHandle) -> Value {
    shared(&app).weather.lock().unwrap().clone()
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Value {
    settings::public_settings(&settings_snapshot(&app))
}

#[tauri::command]
fn set_settings(app: AppHandle, patch: Value) -> Value {
    apply_settings(&app, &patch);
    settings::public_settings(&settings_snapshot(&app))
}

#[tauri::command]
fn get_locations(app: AppHandle) -> Value {
    settings::public_locations(&settings_snapshot(&app))
}

#[tauri::command]
async fn lookup_zip(app: AppHandle, zip: String) -> Value {
    app.state::<Net>().lookup_zip(&zip).await
}

fn place_of(found: &Value) -> Value {
    json!({ "zip": found["zip"], "city": found["city"], "state": found["state"], "lat": found["lat"], "lon": found["lon"] })
}

#[tauri::command]
async fn add_location(app: AppHandle, zip: String) -> Value {
    let found = app.state::<Net>().lookup_zip(&zip).await;
    if found["ok"] != true {
        return found;
    }
    let place = place_of(&found);
    let mut list: Vec<Value> = settings_snapshot(&app)["locations"].as_array().cloned().unwrap_or_default();
    list.retain(|l| l["zip"] != place["zip"]);
    if list.len() >= 12 {
        return json!({ "ok": false, "error": "You can keep up to 12 locations. Remove one first." });
    }
    list.push(place.clone());
    store(&app, |d| d["locations"] = json!(list));
    set_active(&app, place["zip"].as_str().unwrap_or(""));
    json!({ "ok": true, "place": { "zip": place["zip"], "city": place["city"], "state": place["state"] } })
}

#[tauri::command]
fn remove_location(app: AppHandle, zip: String) {
    let list: Vec<Value> = settings_snapshot(&app)["locations"].as_array().cloned().unwrap_or_default();
    if list.len() <= 1 || !list.iter().any(|l| l["zip"] == zip.as_str()) {
        return;
    }
    let was_active = active_zip(&app).as_deref() == Some(zip.as_str());
    let rest: Vec<Value> = list.into_iter().filter(|l| l["zip"] != zip.as_str()).collect();
    let first = rest[0]["zip"].as_str().unwrap_or("").to_string();
    store(&app, |d| d["locations"] = json!(rest));
    shared(&app).cache.lock().unwrap().remove(&zip);
    if was_active {
        set_active(&app, &first);
    } else {
        publish_settings(&app);
    }
}

#[tauri::command]
fn set_active_location(app: AppHandle, zip: String) {
    set_active(&app, &zip);
}

#[tauri::command]
async fn complete_setup(app: AppHandle, zip: String, glass: String) -> Value {
    let found = app.state::<Net>().lookup_zip(&zip).await;
    if found["ok"] != true {
        return json!({ "ok": false, "error": found["error"] });
    }
    let place = place_of(&found);
    let style = if ["none", "clear", "frosted"].contains(&glass.as_str()) { glass } else { "none".into() };
    store(&app, |d| {
        d["activeZip"] = place["zip"].clone();
        d["locations"] = json!([place.clone()]);
        d["glass"] = json!(style);
    });
    *shared(&app).weather.lock().unwrap() = weather::empty_state("loading", None, Some(&place));
    apply_login_item(&app);
    start_app(&app);
    if let Some(w) = window(&app, "setup") {
        let _ = w.close();
    }
    publish_settings(&app);
    json!({ "ok": true })
}

#[tauri::command]
async fn refresh_now(app: AppHandle) {
    refresh(&app).await;
}

#[tauri::command]
fn open_detail_cmd(app: AppHandle, section: Option<String>) {
    open_detail(&app, section.as_deref().unwrap_or("overview"));
}

#[tauri::command]
fn take_pending_section(app: AppHandle, window: WebviewWindow) -> Option<String> {
    if window.label() != "detail" {
        return None;
    }
    shared(&app).pending_section.lock().unwrap().take()
}

#[tauri::command]
fn window_control(app: AppHandle, window: WebviewWindow, action: String) {
    let label = window.label().to_string();
    if label != "detail" && label != "setup" {
        return;
    }
    match action.as_str() {
        // Closing setup before finishing it leaves nothing running, so quit.
        "close" if label == "setup" && active_location(&app).is_none() => quit(&app),
        "close" => {
            let _ = window.close();
        }
        "minimize" => {
            let _ = window.minimize();
        }
        "maximize" => {
            let _ = if window.is_maximized().unwrap_or(false) { window.unmaximize() } else { window.maximize() };
        }
        _ => {}
    }
}

#[tauri::command]
fn window_drag(window: WebviewWindow) {
    if window.label() == "detail" || window.label() == "setup" {
        let _ = window.start_dragging();
    }
}

fn snap(value: i32, target: i32, range: i32) -> i32 {
    if (value - target).abs() < range { target } else { value }
}

#[tauri::command]
fn move_by(app: AppHandle, window: WebviewWindow, dx: f64, dy: f64) {
    let label = window.label().to_string();
    if (label != "widget" && label != "pill") || !dx.is_finite() || !dy.is_finite() {
        return;
    }
    if (label == "widget" && flag(&app, "lockPosition")) || (label == "pill" && docked(&app)) {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let st = shared(&app);
    let mut drags = st.drags.lock().unwrap();
    let start = window.outer_position().map(|p| (p.x as f64, p.y as f64)).unwrap_or((0.0, 0.0));
    let d = drags.entry(label.clone()).or_insert(start);
    d.0 += dx * scale;
    d.1 += dy * scale;
    let (mut x, mut y) = (d.0.round() as i32, d.1.round() as i32);
    if label == "widget" && flag(&app, "snapToEdges") {
        if let Ok(size) = window.outer_size() {
            let (ax, ay, aw, ah, s) = work_area(&app, Some((x + size.width as i32 / 2, y + size.height as i32 / 2)));
            let (range, margin) = ((14.0 * s) as i32, (12.0 * s) as i32);
            x = snap(x, ax + margin, range);
            x = snap(x, ax + aw - margin - size.width as i32, range);
            y = snap(y, ay + margin, range);
            y = snap(y, ay + ah - margin - size.height as i32, range);
        }
    }
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

#[tauri::command]
fn drag_end(app: AppHandle, window: WebviewWindow) {
    let label = window.label().to_string();
    if shared(&app).drags.lock().unwrap().remove(&label).is_none() {
        return;
    }
    if let Ok(p) = window.outer_position() {
        let key = if label == "widget" { "widgetPos" } else { "pillPos" };
        store(&app, |d| d[key] = json!({ "x": p.x, "y": p.y }));
    }
}

#[tauri::command]
fn set_content_size(app: AppHandle, window: WebviewWindow, width: f64, height: f64) {
    if !width.is_finite() || !height.is_finite() {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let px = |v: f64, lo: f64, hi: f64| (v.ceil().clamp(lo, hi) * scale).round() as u32;
    let st = shared(&app);
    match window.label() {
        "widget" => {
            let next = (px(width, 140.0, 900.0), px(height, 60.0, 900.0));
            let Ok(current) = window.inner_size() else { return };
            if (current.width, current.height) == next {
                return;
            }
            st.sizes.lock().unwrap().insert("widget".into(), next);
            // A widget parked on the right half of the screen keeps its right edge when its width changes.
            if let Ok(p) = window.outer_position() {
                let (ax, _, aw, _, _) = work_area(&app, Some((p.x + current.width as i32 / 2, p.y)));
                let right_side = p.x + current.width as i32 / 2 > ax + aw / 2;
                if right_side && next.0 != current.width && !st.drags.lock().unwrap().contains_key("widget") {
                    let _ = window.set_position(PhysicalPosition::new(p.x + current.width as i32 - next.0 as i32, p.y));
                }
            }
            let _ = window.set_size(PhysicalSize::new(next.0, next.1));
        }
        "pill" => {
            let room = st.dock.lock().unwrap().0;
            let width = px(width, 80.0, if room > 0 { room as f64 } else { 360.0 });
            let previous = st.sizes.lock().unwrap().get("pill").copied().unwrap_or((0, (48.0 * scale) as u32));
            if previous.0 == width {
                return;
            }
            st.sizes.lock().unwrap().insert("pill".into(), (width, previous.1));
            place_pill(&app);
        }
        _ => {}
    }
}

#[tauri::command]
fn set_tray_icon(app: AppHandle, window: WebviewWindow, data_url: String) {
    use base64::Engine as _;
    if window.label() != "widget" {
        return;
    }
    let Some(encoded) = data_url.strip_prefix("data:image/png;base64,") else { return };
    let Ok(png) = base64::engine::general_purpose::STANDARD.decode(encoded) else { return };
    if png.len() > 200_000 {
        return;
    }
    *shared(&app).tray_number.lock().unwrap() = Some(png);
    apply_tray_image(&app);
}

#[tauri::command]
fn show_context_menu(app: AppHandle, window: WebviewWindow) {
    if window.label() != "widget" && window.label() != "pill" {
        return;
    }
    if let Ok(menu) = build_menu(&app, true) {
        let _ = window.popup_menu(&menu);
    }
}

/// Troubleshooting aid: when WEATHADOCK_LOG names a file, placement decisions are appended to it.
fn trace(line: String) {
    if let Ok(path) = std::env::var("WEATHADOCK_LOG") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

fn dock_info(app: &AppHandle) -> Value {
    let (room, light) = *shared(app).dock.lock().unwrap();
    json!({ "room": room, "light": light })
}

#[tauri::command]
fn get_battery(app: AppHandle) -> Value {
    battery_info(&app)
}

#[tauri::command]
fn get_dock(app: AppHandle) -> Value {
    // Measure now: the strip's page can ask before the window has been placed for the first time.
    let _ = pill_geometry(&app, 0);
    trace(format!("get_dock -> {}", dock_info(&app)));
    dock_info(&app)
}

#[tauri::command]
fn get_update_state(app: AppHandle) -> Value {
    app.state::<Updater>().public(&app)
}

#[tauri::command]
async fn check_for_updates(app: AppHandle) -> Value {
    let result = app.state::<Updater>().check(&app).await;
    update_tray(&app);
    result
}

#[tauri::command]
fn install_update(app: AppHandle) {
    app.state::<Updater>().install(&app);
}

#[tauri::command]
fn quit_cmd(app: AppHandle) {
    quit(&app);
}

// ---------------------------------------------------------------- lifecycle

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| open_detail(app, "overview")))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .invoke_handler(tauri::generate_handler![
            get_state, get_settings, set_settings, get_locations, lookup_zip, add_location, remove_location,
            set_active_location, complete_setup, refresh_now, open_detail_cmd, take_pending_section, window_control,
            window_drag, move_by, drag_end, set_content_size, set_tray_icon, show_context_menu, get_dock, get_battery, get_update_state,
            check_for_updates, install_update, quit_cmd
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let dir = app.path().app_config_dir()?;
            let data = load_settings(&dir, app.path().data_dir().ok());
            let cache = load_cache(&dir, &data);
            let loc = settings::active_location(&data);
            let initial = loc
                .as_ref()
                .and_then(|l| cache.get(l["zip"].as_str().unwrap_or("")).cloned())
                .unwrap_or_else(|| weather::empty_state("loading", None, loc.as_ref()));
            app.manage(Shared {
                dir,
                settings: Mutex::new(data),
                weather: Mutex::new(initial),
                cache: Mutex::new(cache),
                refresh_lock: tokio::sync::Mutex::new(()),
                last_attempt: Mutex::new(0),
                last_failed: AtomicBool::new(false),
                drags: Mutex::new(HashMap::new()),
                sizes: Mutex::new(HashMap::new()),
                tray_number: Mutex::new(None),
                dock: Mutex::new((0, false)),
                battery: Mutex::new(None),
                battery_smooth: Mutex::new(Default::default()),
                battery_tried: Mutex::new(0),
                battery_warned: AtomicBool::new(false),
                pending_section: Mutex::new(None),
                covered: Mutex::new(Covered::default()),
                quitting: AtomicBool::new(false),
            });
            app.manage(Net::new());
            app.manage(Updater::default());
            store(&handle, |_| {}); // write migrated settings in the new location
            create_tray(&handle)?;
            // Register (or clear) the start-with-Windows entry on every launch, so the default applies
            // without the switch ever being touched and the entry follows the app if its folder changes.
            apply_login_item(&handle);
            if loc.is_some() {
                start_app(&handle);
            } else {
                open_setup(&handle);
            }
            start_scheduler(&handle);
            check_battery(&handle, true);
            start_ticking(&handle);
            // Look for updates shortly after launch and then every six hours.
            let updates = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(15)).await;
                loop {
                    updates.state::<Updater>().check(&updates).await;
                    update_tray(&updates);
                    tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while starting WeathaDock")
        .run(|app, event| {
            // A tray app keeps running when its windows close.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() && !shared(app).quitting.load(Ordering::Relaxed) {
                    api.prevent_exit();
                }
            }
        });
}
