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
/// How often the wallpaper behind the widget is looked at. A look takes about a millisecond
/// unless the wallpaper has changed.
const BACKDROP_EVERY: Duration = Duration::from_secs(3);
const BRIDGE: &str = include_str!("bridge.js");
/// WebView2 switches, identical for every window (they share one browser process). The first
/// group is Tauri's own default; the rest trims processes and background work a widget does not need.
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,msEdgeTranslate,OptimizationHints,Translate,MediaRouter \
--in-process-gpu --disable-extensions --disable-component-update --disable-background-networking --no-pings --disable-sync --disable-breakpad";
const TRAY_PLAIN: &[u8] = include_bytes!("../../build/tray.png");
const SECTIONS: [&str; 6] = ["overview", "hourly", "daily", "alerts", "widget", "settings"];
/// Between forecast refreshes the alerts alone are asked for this often.
const ALERTS_EVERY_MS: i64 = 2 * 60_000;
/// The narrowest free stretch of taskbar the docked strip still fits in (logical px): room for
/// the icon, the temperature and an alert's badge.
const DOCK_MIN: i32 = 140;

struct Shared {
    dir: PathBuf,
    settings: Mutex<Value>,
    weather: Mutex<Value>,
    /// zip -> last good state, for instant location switching and instant startup.
    cache: Mutex<HashMap<String, Value>>,
    refresh_lock: tokio::sync::Mutex<()>,
    /// One writer at a time for settings.json.
    settings_file: Mutex<()>,
    /// When the forecast and the alerts were last asked for.
    last_attempt: Mutex<i64>,
    last_alert_check: Mutex<i64>,
    /// Why the last forecast refresh failed, and whether the person had asked for that refresh
    /// (then it is reported at once). `None` after a refresh that worked.
    failure: Mutex<Option<(String, bool)>>,
    /// The last refresh brought only one of the two services, so the next one comes sooner.
    partial: AtomicBool,
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
    /// When the mouse last gave a good reading, the age last sent to the windows (minutes, -1 when
    /// fresh), and what the last checks found, for "Copy details" in Settings.
    battery_at: Mutex<i64>,
    battery_age_sent: Mutex<i64>,
    battery_log: Mutex<std::collections::VecDeque<String>>,
    battery_tier: Mutex<weathadock_core::battery::Bands>,
    /// The band the user was last notified about (low or urgent), so each is announced once.
    battery_warned: Mutex<Option<weathadock_core::battery::Tier>>,
    /// Room for the docked taskbar strip in logical px (0 when it cannot dock), and whether the taskbar is light.
    dock: Mutex<(u32, bool)>,
    /// Docked on a centered taskbar that has filled up: there is no room for the strip, so it
    /// steps aside and the tray icon carries the temperature until room returns.
    out_of_room: AtomicBool,
    taskbar_watch: Mutex<TaskbarWatch>,
    backdrop: Mutex<Backdrop>,
    pending_section: Mutex<Option<String>>,
    covered: Mutex<Covered>,
    quitting: AtomicBool,
}

/// What the docked strip last saw of the taskbar, to notice when its buttons may have moved.
/// What is behind the desktop widget, for text that chooses its own colour.
#[derive(Default)]
struct Backdrop {
    /// The wallpaper picture read last: what told it apart (file, size, date) and its thumbnail.
    #[cfg(windows)]
    picture: Option<(String, win32::Thumbnail)>,
    /// What the answer below was worked out for: the wallpaper's layout and the widget's place.
    seen: String,
    /// Whether the wallpaper behind the widget calls for dark text.
    dark: bool,
}

#[derive(Default)]
struct TaskbarWatch {
    seen: (Option<(i32, i32, i32, i32)>, u32),
    ticks: u64,
    due: Option<u64>,
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
/// The docked strip has stepped aside because the taskbar has no room for it.
fn out_of_room(app: &AppHandle) -> bool {
    docked(app) && shared(app).out_of_room.load(Ordering::Relaxed)
}
fn set_out_of_room(app: &AppHandle, on: bool) {
    if shared(app).out_of_room.swap(on, Ordering::Relaxed) != on {
        trace(format!("docked strip out of room -> {on}"));
        apply_tray_image(app);
    }
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

/// Apply changes to the stored settings and write them to disk. One writer at a time, and the
/// file is only ever replaced by a complete copy that has reached the disk, so neither two
/// changes at once nor a crash or power cut can leave half a file behind.
fn store(app: &AppHandle, change: impl FnOnce(&mut Value)) {
    let st = shared(app);
    let _writer = st.settings_file.lock().unwrap();
    let text = {
        let mut data = st.settings.lock().unwrap();
        change(&mut data);
        serde_json::to_string_pretty(&*data).unwrap_or_default()
    };
    if text.is_empty() {
        return;
    }
    let _ = std::fs::create_dir_all(&st.dir);
    let _ = write_whole(&st.dir.join("settings.json"), &text);
}

/// Write `text` beside `path`, make sure it is on the disk, then swap it into place.
fn write_whole(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

/// Read the settings without ever losing them: a file that is busy is tried again, and a
/// damaged one is set aside (never overwritten with defaults) while the copy kept from the last
/// good start takes its place.
fn load_settings(dir: &PathBuf, roaming: Option<PathBuf>) -> Value {
    use std::io::ErrorKind::NotFound;
    let (path, backup) = (dir.join("settings.json"), dir.join("settings.backup.json"));
    let parse = |text: String| serde_json::from_str::<Value>(&text).ok().filter(Value::is_object);
    let read = |p: PathBuf| std::fs::read_to_string(p).ok().and_then(parse);
    let mut text = std::fs::read_to_string(&path).map_err(|e| e.kind());
    for _ in 0..4 {
        if !matches!(text, Err(kind) if kind != NotFound) {
            break;
        }
        // Another program (a backup, a sync tool, a virus scanner) has it open: wait a moment.
        std::thread::sleep(Duration::from_millis(150));
        text = std::fs::read_to_string(&path).map_err(|e| e.kind());
    }
    let stored = match text.map(parse) {
        Ok(Some(good)) => {
            let _ = std::fs::copy(&path, &backup);
            Some(good)
        }
        Ok(None) => {
            let _ = std::fs::rename(&path, dir.join("settings.damaged.json"));
            read(backup)
        }
        // First start here. Versions 1.0.x (built on Electron) kept their settings in %APPDATA%\WeathaDock.
        Err(NotFound) => roaming.and_then(|r| read(r.join("WeathaDock").join("settings.json"))),
        Err(_) => read(backup),
    };
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
    let _ = write_whole(&st.dir.join("forecast-cache.json"), &text);
}

fn updated_ms(state: &Value) -> Option<i64> {
    state["updatedAt"].as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp_millis())
}

fn refresh_ms(app: &AppHandle) -> i64 {
    setting(app, "refreshMinutes").as_i64().unwrap_or(10) * 60_000
}

/// Forecasts from the last run, so the app has something to show the moment it starts. A day
/// old at most: what is shown is brought up to the present before it reaches a window.
fn load_cache(dir: &PathBuf, data: &Value) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    let Some(Value::Object(map)) = std::fs::read_to_string(dir.join("forecast-cache.json")).ok().and_then(|t| serde_json::from_str(&t).ok()) else {
        return out;
    };
    let known: Vec<&str> = data["locations"].as_array().into_iter().flatten().filter_map(|l| l["zip"].as_str()).collect();
    for (zip, state) in map {
        let age = updated_ms(&state).map(|at| now_ms() - at);
        // Only states this version can bring up to the present are kept for longer than an hour.
        let limit = if state["utcOffset"].is_number() { 24 } else { 1 } * 3_600_000;
        if known.contains(&zip.as_str()) && state["status"] == "ok" && age.is_some_and(|a| (0..limit).contains(&a)) {
            out.insert(zip, state);
        }
    }
    out
}

/// What the windows should show right now: the forecast last fetched for the selected place,
/// brought up to the present, with a note once a failed refresh starts to matter.
fn compose(app: &AppHandle) -> Value {
    let st = shared(app);
    let Some(loc) = active_location(app) else { return weather::empty_state("loading", None, None) };
    let failure = st.failure.lock().unwrap().clone();
    let base = st.cache.lock().unwrap().get(loc["zip"].as_str().unwrap_or("")).cloned();
    let Some(base) = base else {
        return weather::empty_state(if failure.is_some() { "error" } else { "loading" }, failure.as_ref().map(|f| f.0.as_str()), Some(&loc));
    };
    let now = now_ms();
    let mut shown = weather::advance(&base, now).unwrap_or(base);
    if let Some((message, asked)) = failure {
        // A refresh the person asked for reports its failure at once. Automatic ones stay quiet
        // while the forecast on screen is still recent: a short outage is nobody's concern.
        if asked || now - updated_ms(&shown).unwrap_or(0) >= 2 * refresh_ms(app) {
            shown["stale"] = json!(true);
            shown["error"] = json!(message);
        }
    }
    shown
}

/// Work out what to show and tell the windows when it differs from what they have.
fn present(app: &AppHandle) {
    let next = compose(app);
    let st = shared(app);
    let changed = {
        let mut shown = st.weather.lock().unwrap();
        let changed = *shown != next;
        if changed {
            *shown = next;
        }
        changed
    };
    if changed {
        publish(app);
    }
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

/// Fetch the forecast for the selected place. `asked` says the person asked for it: a failure
/// is then reported at once. Automatic refreshes first try again quietly, because right after
/// sleep or sign-in the network is often a few seconds behind.
async fn refresh(app: &AppHandle, asked: bool) {
    let st = shared(app);
    let _guard = match st.refresh_lock.try_lock() {
        Ok(g) => g,
        Err(_) => {
            // Another refresh is running. Wait for it, and fetch again unless it has just
            // brought the forecast for the place that is selected now.
            let g = st.refresh_lock.lock().await;
            let fetched = active_zip(app).and_then(|zip| st.cache.lock().unwrap().get(&zip).and_then(updated_ms));
            if fetched.is_some_and(|at| (0..30_000).contains(&(now_ms() - at))) {
                return;
            }
            g
        }
    };
    let Some(loc) = active_location(app) else { return };
    let zip = loc["zip"].as_str().unwrap_or("").to_string();
    let selected = || active_zip(app).as_deref() == Some(zip.as_str());
    let started = std::time::Instant::now();
    let mut result = Err(String::new());
    for pause in [0, 5, 15] {
        if pause > 0 {
            if asked || !selected() || started.elapsed() > Duration::from_secs(30) {
                break;
            }
            tokio::time::sleep(Duration::from_secs(pause)).await;
        }
        *st.last_attempt.lock().unwrap() = now_ms();
        let previous = st.cache.lock().unwrap().get(&zip).cloned().unwrap_or(Value::Null);
        result = app.state::<Net>().fetch_weather(&loc, &previous).await;
        if result.is_ok() {
            break;
        }
    }
    match result {
        Ok(next) => {
            st.cache.lock().unwrap().insert(zip.clone(), next.clone());
            save_cache(app);
            if !selected() {
                return; // the person switched places while this was loading
            }
            st.partial.store(next["sources"]["nws"] != true || next["sources"]["openMeteo"] != true, Ordering::Relaxed);
            if next["alertsAt"] == next["updatedAt"] {
                *st.last_alert_check.lock().unwrap() = now_ms();
            }
            *st.failure.lock().unwrap() = None;
            notify_alerts(app, &next);
        }
        Err(message) => {
            if !selected() {
                return;
            }
            let mut failure = st.failure.lock().unwrap();
            let asked = asked || failure.as_ref().is_some_and(|f| f.1);
            *failure = Some((message, asked));
        }
    }
    present(app);
}

fn spawn_refresh(app: &AppHandle, asked: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { refresh(&app, asked).await });
}

/// Between forecast refreshes, ask for the alerts alone: a warning should not have to wait
/// for the next refresh, which can be half an hour away.
async fn check_alerts(app: &AppHandle, loc: &Value) {
    let st = shared(app);
    // A refresh asks for the alerts too, and must not be overtaken by this smaller answer.
    let Ok(_guard) = st.refresh_lock.try_lock() else { return };
    *st.last_alert_check.lock().unwrap() = now_ms();
    let Some(features) = app.state::<Net>().fetch_alerts(loc).await else { return };
    let Some(zip) = loc["zip"].as_str().filter(|zip| active_zip(app).as_deref() == Some(*zip)) else { return };
    let (next, news) = {
        let mut cache = st.cache.lock().unwrap();
        let Some(base) = cache.get(zip) else { return };
        let next = weather::with_alerts(base, &features, now_ms());
        let news = next["alerts"] != base["alerts"];
        cache.insert(zip.to_string(), next.clone());
        (next, news)
    };
    if news {
        save_cache(app);
        notify_alerts(app, &next);
    }
    present(app);
}

/// Every 20 seconds: keep what is shown in step with the clock, refresh the forecast on the
/// chosen interval (sooner after a failure or a partial answer, and promptly after sleep), and
/// ask for the alerts in between.
fn start_scheduler(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(20)).await;
            check_battery(&app, false);
            let Some(loc) = active_location(&app) else { continue };
            present(&app);
            let st = shared(&app);
            let failed = st.failure.lock().unwrap().is_some();
            let wait = if failed {
                60_000
            } else if st.partial.load(Ordering::Relaxed) {
                120_000
            } else {
                refresh_ms(&app)
            };
            let now = now_ms();
            let since_refresh = now - *st.last_attempt.lock().unwrap();
            let since_alerts = now - *st.last_alert_check.lock().unwrap();
            // A negative wait means the PC clock was set back: that must not stop the refreshes.
            if !(0..wait).contains(&since_refresh) {
                refresh(&app, false).await;
            } else if !(0..ALERTS_EVERY_MS).contains(&since_alerts) {
                check_alerts(&app, &loc).await;
            }
        }
    });
}

// Checked on every scheduler pass (20 s), so plugging the mouse in shows up quickly. One check is a single
// short exchange with the receiver on a background thread.
const BATTERY_EVERY_MS: i64 = 15_000;

/// A reading older than this is shown as old: the mouse is asleep, switched off or out of reach.
const BATTERY_STALE_MS: i64 = 3 * 60_000;

/// Minutes since the last good reading once it counts as old, otherwise -1.
fn battery_age(app: &AppHandle) -> i64 {
    let at = *shared(app).battery_at.lock().unwrap();
    let age = now_ms() - at;
    if at > 0 && age > BATTERY_STALE_MS { age / 60_000 } else { -1 }
}

fn age_text(minutes: i64) -> String {
    match minutes {
        m if m < 60 => format!("{m} min ago"),
        m if m < 48 * 60 => format!("{} h ago", m / 60),
        m => format!("{} d ago", m / 1440),
    }
}

fn battery_info(app: &AppHandle) -> Value {
    use weathadock_core::battery::Tier;
    let st = shared(app);
    let reading = *st.battery.lock().unwrap();
    let tier = st.battery_tier.lock().unwrap().shown();
    match (reading, tier) {
        (Some((_, charging)), Some(tier)) if flag(app, "mouseBattery") => {
            let age = battery_age(app);
            // An old reading says nothing about charging right now.
            let (charging, stale) = (charging && age < 0, age >= 0);
            json!({ "tier": tier.id(), "label": tier.label(), "charging": charging,
                    "low": tier == Tier::Low && !charging, "urgent": tier == Tier::Urgent && !charging,
                    "stale": stale, "age": if stale { Value::from(age_text(age)) } else { Value::Null } })
        }
        _ => Value::Null,
    }
}

/// Keep the last checks for "Copy details" in Settings: what each device answered, nothing else.
fn battery_note(app: &AppHandle, line: String) {
    trace(format!("mouse battery: {line}"));
    let st = shared(app);
    let mut log = st.battery_log.lock().unwrap();
    if log.len() == 60 {
        log.pop_front();
    }
    log.push_back(format!("{} {line}", chrono::Local::now().format("%H:%M:%S")));
}

/// Read the mouse battery when the option is on: on every scheduler pass, or right away with `now`.
/// The last reading is kept while the mouse is asleep, and marked as old after a few minutes.
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
        let (reading, notes) = hidpp::mouse_battery();
        #[cfg(windows)]
        let reading = reading.map(|b| (b.percent, b.charging));
        #[cfg(not(windows))]
        let (reading, notes): (Option<(u8, bool)>, Vec<String>) = (None, Vec::new());
        let st = shared(&app);
        let mut changed = false;
        let mut line = notes.join("; ");
        if let Some((raw_percent, raw_charging)) = reading {
            // Single readings wander with what the mouse is doing; show the steadied value.
            let (percent, charging) = st.battery_smooth.lock().unwrap().push(raw_percent, raw_charging);
            use weathadock_core::battery::Tier;
            // The number wanders by a few points, so what is shown is the band it falls in.
            // Low and Charge now only appear once several checks in a row agree.
            let band = {
                let mut bands = st.battery_tier.lock().unwrap();
                let before = bands.shown();
                let band = bands.push(percent);
                changed = band != before;
                band
            };
            line.push_str(&format!(" -> steadied {percent}%, shown {}{}", band.map_or("nothing yet", |b| b.label()), if charging { " charging" } else { "" }));
            *st.battery_at.lock().unwrap() = now_ms();
            changed |= st.battery.lock().unwrap().replace((percent, charging)).map(|b| b.1) != Some(charging);
            // One notice when it gets low and one when it must be charged; charging or a good level resets both.
            let mut warned = st.battery_warned.lock().unwrap();
            if charging || band.map_or(true, |b| b >= Tier::Mid) {
                *warned = None;
            } else if let Some(band) = band.filter(|b| warned.map_or(true, |w| *b < w)) {
                *warned = Some(band);
                let (title, body) = if band == Tier::Urgent {
                    ("Charge your mouse now", "The mouse battery is almost empty.")
                } else {
                    ("Mouse battery low", "Charge your mouse soon.")
                };
                let _ = app.notification().builder().title(title).body(body).show();
            }
            drop(warned);
        }
        battery_note(&app, line);
        // Tell the windows when the reading changes, goes old, or its age moves on.
        let age = battery_age(&app);
        if std::mem::replace(&mut *st.battery_age_sent.lock().unwrap(), age) != age {
            changed = true;
        }
        if changed {
            let _ = app.emit("sky:battery", battery_info(&app));
            update_tray(&app);
        }
    });
}

/// Announce the alerts in `state` that are news, once each (see `weather::alerts_to_announce`).
fn notify_alerts(app: &AppHandle, state: &Value) {
    let alerts = state["alerts"].as_array().cloned().unwrap_or_default();
    let seen: Vec<Value> = setting(app, "notifiedAlerts").as_array().cloned().unwrap_or_default();
    let (news, record) = weather::alerts_to_announce(&alerts, &seen);
    if record != seen {
        store(app, |d| d["notifiedAlerts"] = json!(record));
    }
    if !flag(app, "alertNotifications") {
        return;
    }
    let place = active_location(app).map(|l| place_label(&l)).unwrap_or_default();
    let offset = state["utcOffset"].as_i64().unwrap_or(0) as i32;
    for alert in news.iter().take(3) {
        let (title, body) = weather::alert_notice(alert, &place, now_ms(), offset);
        let _ = app.notification().builder().title(title).body(body).show();
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
    let place = active_location(app).map(|l| place_label(&l)).filter(|p| !p.is_empty());
    let offline = if s["stale"] == true { " (offline)" } else { "" };
    // Most important first: Windows shows 127 characters.
    let mut parts = vec![
        place.map_or_else(|| "Weather".to_string(), |p| format!("Weather in {p}")),
        format!("{temp}°F {}", s["current"]["condition"].as_str().unwrap_or("")).trim_end().to_string() + offline,
    ];
    if let Some(alerts) = s["alerts"].as_array().filter(|a| !a.is_empty()) {
        let more = if alerts.len() > 1 { format!(" +{}", alerts.len() - 1) } else { String::new() };
        parts.push(format!("⚠ {}{more}", alerts[0]["event"].as_str().unwrap_or("Alert")));
    }
    if s["today"].is_object() {
        parts.push(format!("H {}°  L {}°", deg(&s["today"]["high"]), deg(&s["today"]["low"])));
    }
    if let Some(h) = s["nextPrecip"]["headline"].as_str().filter(|h| !h.is_empty()) {
        parts.push(h.to_string());
    }
    let battery = battery_info(app);
    if let Some(tier) = battery["tier"].as_str() {
        let level = match tier {
            "urgent" => "almost empty",
            "low" => "low",
            "mid" => "mid",
            _ => "good",
        };
        let note = match (battery["age"].as_str(), battery["charging"].as_bool()) {
            (Some(age), _) => format!(" (read {age})"),
            (None, Some(true)) => ", charging".to_string(),
            (None, _) if tier == "urgent" => ", charge it now".to_string(),
            _ => String::new(),
        };
        parts.push(format!("Mouse battery {level}{note}"));
    }
    // A line that does not fit is left out whole instead of being cut in the middle.
    let mut text = String::new();
    for part in parts {
        if text.encode_utf16().count() + part.encode_utf16().count() < 127 {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&part);
        }
    }
    text
}

/// "Springfield, IL", or whichever of the two parts is known.
fn place_label(l: &Value) -> String {
    let part = |key: &str| l[key].as_str().map(str::trim).filter(|s| !s.is_empty());
    [part("city"), part("state")].into_iter().flatten().collect::<Vec<_>>().join(", ")
}

fn build_menu(app: &AppHandle, for_widget: bool) -> tauri::Result<Menu<Wry>> {
    let data = settings_snapshot(app);
    let places = data["locations"].as_array().cloned().unwrap_or_default();
    let active = settings::active_zip(&data);
    let mut menu = MenuBuilder::new(app);
    if !for_widget {
        let title = settings::active_location(&data).map(|l| place_label(&l)).filter(|t| !t.is_empty()).unwrap_or_else(|| APP_NAME.into());
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
            .item(&MenuItemBuilder::with_id("customize", "Customize widget…").build(app)?)
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
        "customize" => open_detail(app, "widget"),
        "refresh" => spawn_refresh(app, true),
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
/// carries the weather and the tray shows the plain app glyph. Docked, the strip carries it,
/// and the tray icon steps in only while the taskbar has no room for the strip.
fn apply_tray_image(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("main") else { return };
    let number = shared(app).tray_number.lock().unwrap().clone();
    let number = match setting(app, "coveredMode").as_str() {
        Some("tray") if !docked(app) => number,
        _ if out_of_room(app) => number,
        _ => None,
    };
    trace(format!("tray icon -> {}", if number.is_some() { "temperature" } else { "app glyph" }));
    let bytes = number.unwrap_or_else(|| TRAY_PLAIN.to_vec());
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
    // Docked on the taskbar, the strip carries the weather: no desktop widget, and none of the
    // memory its page would take.
    if docked(app) || window(app, "widget").is_some() {
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
        .focusable(false)
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

/// With the text colour on "Auto", the widget's text follows the wallpaper behind it: work
/// out whether that is bright enough to call for dark text, and tell the widget when the
/// answer changes. The answer is kept current whatever the text colour is set to, so that
/// choosing "Auto" never shows the wrong colour first. Next to free unless the wallpaper or
/// the widget's place has changed, and the picture itself is read only when it is a new one;
/// that read can take a moment, so this is never called on the main thread.
#[cfg(windows)]
fn check_backdrop_now(app: &AppHandle) {
    use weathadock_core::backdrop::{self, Area, Picture};
    // No widget on the desktop (switched off, or docked on the taskbar): nothing to colour.
    if !widget_enabled(app) {
        return;
    }
    let Some(place) = window(app, "widget").and_then(|w| win32::handle_of(&w)).and_then(win32::rect) else { return };
    let Some(wallpaper) = win32::wallpaper_at(place) else { return };
    let file = std::fs::metadata(&wallpaper.path).ok().map(|m| (m.len(), m.modified().ok()));
    let picture_key = format!("{} {file:?}", wallpaper.path);
    let seen = format!("{wallpaper:?} {file:?} {place:?}");
    let st = shared(app);
    let known = {
        let b = st.backdrop.lock().unwrap();
        if b.seen == seen {
            return;
        }
        b.picture.as_ref().filter(|p| p.0 == picture_key).map(|p| p.1.clone())
    };
    let thumbnail = if wallpaper.path.is_empty() {
        None // a plain colour, no picture
    } else {
        let Some(read) = known.or_else(|| win32::thumbnail(&wallpaper.path)) else {
            // The text stays as it is. Not tried again until something changes: a file that
            // was only half written is a different file (size, date) once it is whole.
            trace("wallpaper behind the widget: the picture could not be read".into());
            st.backdrop.lock().unwrap().seen = seen;
            return;
        };
        Some(read)
    };
    let area = |r: win32::Rect| Area { left: r.left as f64, top: r.top as f64, right: r.right as f64, bottom: r.bottom as f64 };
    let picture = thumbnail.as_ref().map(|t| Picture { pixels: &t.pixels, width: t.width, height: t.height, natural: t.natural });
    let brightness = backdrop::brightness_behind(picture.as_ref(), wallpaper.fit, wallpaper.colour, area(wallpaper.screen), area(place));
    let (dark, changed) = {
        let mut b = st.backdrop.lock().unwrap();
        let dark = backdrop::dark_text(brightness, b.dark);
        let changed = dark != b.dark;
        b.seen = seen;
        b.dark = dark;
        b.picture = thumbnail.map(|t| (picture_key, t));
        (dark, changed)
    };
    trace(format!("wallpaper behind the widget: {:?}, brightness {brightness:.2} -> {} text", wallpaper.fit, if dark { "dark" } else { "light" }));
    if changed {
        let _ = app.emit("sky:backdrop", backdrop_info(app));
    }
}

#[cfg(not(windows))]
fn check_backdrop_now(_app: &AppHandle) {}

fn check_backdrop(app: &AppHandle) {
    off_main(app, check_backdrop_now);
}

/// A wallpaper can change at any time (a slideshow, a new picture, another desktop colour),
/// and nothing announces it: look every few seconds.
fn start_backdrop_watch(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        // Kept for the life of the thread, so each look does not start the machinery again.
        #[cfg(windows)]
        let _com = win32::Com::start();
        loop {
            std::thread::sleep(BACKDROP_EVERY);
            check_backdrop_now(&app);
        }
    });
}

fn backdrop_info(app: &AppHandle) -> Value {
    json!({ "dark": shared(app).backdrop.lock().unwrap().dark })
}

fn show_widget(app: &AppHandle) {
    let Some(w) = window(app, "widget") else { return };
    let on_top = flag(app, "widgetOnTop");
    show_inactive(&w);
    // Above or below other windows is set with Win32 directly, never through the window
    // library: it believes this window is hidden (it is shown with Win32 too) and would hide
    // it again, and rewrite its styles, whenever the setting changed.
    #[cfg(windows)]
    if let Some(h) = win32::handle_of(&w) {
        win32::set_topmost(h, on_top);
        if !on_top {
            win32::send_to_bottom(h);
        }
    }
    #[cfg(not(windows))]
    let _ = w.set_always_on_top(on_top);
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
            let mut crowded = false;
            if docked(app) {
                // Docked: the empty stretch left of the Start button on a centered taskbar.
                let margin = (height as f64 * 0.25) as i32;
                if let Some((left, right)) = win32::taskbar_free_left() {
                    let room = ((right - left - 2 * margin) as f64 / scale) as i32;
                    if room >= DOCK_MIN {
                        dock.0 = room.min(900) as u32;
                        spot = Some((left + margin, bar.top, height));
                    } else {
                        // A centered taskbar fills from the middle outwards: with no room left of
                        // Start there is none by the tray either, only app buttons to cover.
                        crowded = win32::taskbar_is_centered();
                    }
                }
            }
            trace(format!("pill_geometry docked={} dock={dock:?} spot={spot:?} crowded={crowded} scale={scale}", docked(app)));
            set_out_of_room(app, crowded);
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
    set_out_of_room(app, false);
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
    if out_of_room(app) {
        // Nowhere to put it: it waits out of sight until `follow_taskbar` finds room again.
        hide_window(&w);
        return;
    }
    let room = st.dock.lock().unwrap().0;
    let (x, y, width) = if room > 0 {
        // The dock has one fixed home, and never reaches past the room the taskbar has for it.
        let scale = w.scale_factor().unwrap_or(1.0);
        (x, y, width.min((room as f64 * scale) as i32))
    } else {
        // Only the floating pill remembers where it was dragged, as long as that is still on a screen.
        let saved = saved_point(&setting(app, "pillPos")).filter(|&(x, y)| app.monitor_from_point(x as f64 + 20.0, y as f64 + 10.0).ok().flatten().is_some());
        let (x, y) = saved.unwrap_or((x, y));
        (x, y, width)
    };
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
                .focusable(false)
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
        if !out_of_room(app) {
            show_inactive(&w);
        }
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
    let stale = updated_ms(&shared(app).weather.lock().unwrap()).map_or(true, |at| now_ms() - at > 60_000);
    if stale {
        spawn_refresh(app, false);
    }
    if let Some(w) = window(app, "detail") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("sky:navigate", section);
        return;
    }
    *shared(app).pending_section.lock().unwrap() = Some(section.to_string());
    // Never larger than the screen it opens on (small or heavily scaled displays).
    let (_, _, area_w, area_h, scale) = work_area(app, None);
    let fit = |want: f64, have: i32| want.min(have as f64 / scale - 32.0).max(420.0);
    let (width, height) = (fit(1000.0, area_w), fit(700.0, area_h));
    let mut builder = WebviewWindowBuilder::new(app, "detail", page(app, "detail.html"))
        .title(APP_NAME)
        .inner_size(width, height)
        .min_inner_size(width.min(780.0), height.min(560.0))
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
        let (_, _, area_w, area_h, _) = work_area(app, saved_point(&saved));
        let _ = w.set_size(PhysicalSize::new((width as u32).min(area_w as u32), (height as u32).min(area_h as u32)));
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
    spawn_refresh(app, false);
}

fn quit(app: &AppHandle) {
    shared(app).quitting.store(true, Ordering::Relaxed);
    app.exit(0);
}

// ---------------------------------------------------------------- desktop / taskbar switching

#[cfg(windows)]
fn tick(app: &AppHandle) {
    let widget = window(app, "widget").and_then(|w| Some((win32::handle_of(&w)?, w)));
    let pill = window(app, "pill").and_then(|w| win32::handle_of(&w));
    // Docked, there is no widget: the strip is then the window the checks are about.
    let Some(target) = widget.as_ref().map(|w| w.0).or(pill) else { return };
    let snap = win32::snapshot(target);
    let mut covered = true;
    if let Some((handle, widget)) = widget {
        let widget_on = widget_enabled(app);
        let visible = widget.is_visible().unwrap_or(false);
        let on_top = flag(app, "widgetOnTop");
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
        covered = c.covered;
        // A widget kept above other windows steps back while a game, video or presentation has the
        // whole screen, and returns afterwards.
        // Only the monitor each window is on matters: a game on another screen changes nothing.
        let widget_fullscreen = win32::fullscreen_on(snap.fullscreen, handle);
        if widget_fullscreen != c.fullscreen {
            c.fullscreen = widget_fullscreen;
            trace(format!("fullscreen on the widget's monitor -> {widget_fullscreen}"));
            if widget_on && on_top {
                // With Win32 directly, for the reason given in `show_widget`.
                win32::set_topmost(handle, !widget_fullscreen);
                if widget_fullscreen {
                    win32::send_to_bottom(handle);
                }
            }
        }
    }

    let Some(pill) = pill else { return };
    let wanted = covered && !win32::fullscreen_on(snap.fullscreen, pill);
    let show = wanted && !out_of_room(app);
    set_pill_visible(app, show);
    if show {
        win32::keep_on_top(pill); // the taskbar likes to climb back over it
    }
    if wanted {
        // Also while the strip waits for room, which is how it learns that room has returned.
        follow_taskbar(app);
    }
}

/// Keep the docked strip where the taskbar has room for it. Asking the taskbar where its
/// buttons are is slow, so it is only asked when something suggests they moved: the taskbar
/// itself changed (another resolution, scale or screen), an app window opened or closed (a
/// centered taskbar then slides its Start button), or half a minute has passed.
#[cfg(windows)]
fn follow_taskbar(app: &AppHandle) {
    if !docked(app) {
        return;
    }
    let seen = (win32::taskbar_rects().map(|(bar, _)| (bar.left, bar.top, bar.right, bar.bottom)), win32::app_window_count());
    let st = shared(app);
    {
        let mut watch = st.taskbar_watch.lock().unwrap();
        watch.ticks += 1;
        if seen != watch.seen {
            watch.seen = seen;
            watch.due = Some(watch.ticks + 2); // let the taskbar finish moving its buttons
        }
        if !(watch.due.is_some_and(|at| watch.ticks >= at) || watch.ticks % 60 == 0) {
            return;
        }
        watch.due = None;
    }
    place_pill(app);
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
    // A hidden widget picks the setting up when it is shown again.
    if changed("widgetOnTop") && !changed("glass") && widget_enabled(app) {
        show_widget(app);
    }
    if changed("showWidget") || changed("placement") {
        match window(app, "widget") {
            // Docked, the strip carries the weather: the widget and its page are let go.
            Some(w) if docked(app) => {
                let _ = w.destroy();
            }
            Some(w) if !widget_enabled(app) => hide_window(&w),
            Some(_) => show_widget(app),
            None => create_widget(app),
        }
    }
    if changed("coveredMode") || changed("glass") || changed("placement") {
        if changed("placement") {
            // Whether there is room is measured afresh when the strip is placed.
            shared(app).out_of_room.store(false, Ordering::Relaxed);
        }
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
    if changed("showWidget") {
        check_backdrop(app); // nobody looked at the wallpaper while the widget was off
    }
    publish_settings(app);
}

fn set_active(app: &AppHandle, zip: &str) {
    if !settings_snapshot(app)["locations"].as_array().into_iter().flatten().any(|l| l["zip"] == zip) {
        return;
    }
    store(app, |d| d["activeZip"] = json!(zip));
    // Show what is already known for this place straight away, then fetch fresh data.
    let st = shared(app);
    *st.failure.lock().unwrap() = None;
    st.partial.store(false, Ordering::Relaxed);
    present(app);
    publish_settings(app);
    spawn_refresh(app, true);
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
    // Read, change and write in one step, so two additions at once cannot lose one.
    let mut full = false;
    store(&app, |d| {
        let mut list: Vec<Value> = d["locations"].as_array().cloned().unwrap_or_default();
        list.retain(|l| l["zip"] != place["zip"]);
        full = list.len() >= 12;
        if !full {
            list.push(place.clone());
            d["locations"] = json!(list);
        }
    });
    if full {
        return json!({ "ok": false, "error": "You can keep up to 12 locations. Remove one first." });
    }
    set_active(&app, place["zip"].as_str().unwrap_or(""));
    json!({ "ok": true, "place": { "zip": place["zip"], "city": place["city"], "state": place["state"] } })
}

#[tauri::command]
fn remove_location(app: AppHandle, zip: String) {
    let was_active = active_zip(&app).as_deref() == Some(zip.as_str());
    let mut first = None;
    store(&app, |d| {
        let list: Vec<Value> = d["locations"].as_array().cloned().unwrap_or_default();
        if list.len() <= 1 || !list.iter().any(|l| l["zip"] == zip.as_str()) {
            return; // the last place stays, and so does one that is not in the list
        }
        let rest: Vec<Value> = list.into_iter().filter(|l| l["zip"] != zip.as_str()).collect();
        first = rest[0]["zip"].as_str().map(String::from);
        d["locations"] = json!(rest);
    });
    let Some(first) = first else { return };
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
    *shared(&app).failure.lock().unwrap() = None;
    present(&app);
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
    refresh(&app, true).await;
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
    if label == "widget" {
        check_backdrop(&app); // another part of the wallpaper is behind it now
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
            check_backdrop(&app);
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
    // The widget draws it. Docked, there is no widget and the strip's page does.
    if window.label() != "widget" && window.label() != "pill" {
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

/// The release build stops dead on an internal error, which would otherwise leave no trace.
/// Note the time, the version and where it happened in a file that never leaves this PC.
fn log_crashes(dir: &PathBuf, version: String) {
    let file = dir.join("crash.log");
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        let start_over = std::fs::metadata(&file).is_ok_and(|m| m.len() > 64_000);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).write(true).append(!start_over).truncate(start_over).open(&file) {
            let _ = writeln!(f, "{} WeathaDock {version}: {info}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%SZ"));
        }
    }));
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

/// What the last mouse battery checks found, as text the user can copy and send. It holds only
/// the app version, Logitech product ids and the battery replies.
#[tauri::command]
fn get_battery_log(app: AppHandle) -> String {
    let st = shared(&app);
    let log = st.battery_log.lock().unwrap();
    let mut text = format!("WeathaDock {} mouse battery details\n", app.package_info().version);
    for line in log.iter() {
        text.push_str(line);
        text.push('\n');
    }
    text
}

/// Whether the wallpaper behind the widget calls for dark text (text colour "Auto"). The
/// widget's page asks before it first draws: look now, off the main thread.
#[tauri::command(async)]
fn get_backdrop(app: AppHandle) -> Value {
    check_backdrop_now(&app);
    backdrop_info(&app)
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
            window_drag, move_by, drag_end, set_content_size, set_tray_icon, show_context_menu, get_dock, get_backdrop, get_battery, get_battery_log, get_update_state,
            check_for_updates, install_update, quit_cmd
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let dir = app.path().app_config_dir()?;
            let _ = std::fs::create_dir_all(&dir);
            log_crashes(&dir, app.package_info().version.to_string());
            let data = load_settings(&dir, app.path().data_dir().ok());
            let cache = load_cache(&dir, &data);
            let loc = settings::active_location(&data);
            app.manage(Shared {
                dir,
                settings: Mutex::new(data),
                weather: Mutex::new(Value::Null),
                cache: Mutex::new(cache),
                refresh_lock: tokio::sync::Mutex::new(()),
                settings_file: Mutex::new(()),
                last_attempt: Mutex::new(0),
                last_alert_check: Mutex::new(0),
                failure: Mutex::new(None),
                partial: AtomicBool::new(false),
                drags: Mutex::new(HashMap::new()),
                sizes: Mutex::new(HashMap::new()),
                tray_number: Mutex::new(None),
                dock: Mutex::new((0, false)),
                out_of_room: AtomicBool::new(false),
                taskbar_watch: Mutex::new(TaskbarWatch::default()),
                backdrop: Mutex::new(Backdrop::default()),
                battery: Mutex::new(None),
                battery_smooth: Mutex::new(Default::default()),
                battery_tried: Mutex::new(0),
                battery_at: Mutex::new(0),
                battery_age_sent: Mutex::new(-1),
                battery_log: Mutex::new(Default::default()),
                battery_tier: Mutex::new(Default::default()),
                battery_warned: Mutex::new(None),
                pending_section: Mutex::new(None),
                covered: Mutex::new(Covered::default()),
                quitting: AtomicBool::new(false),
            });
            app.manage(Net::new());
            app.manage(Updater::default());
            *shared(&handle).weather.lock().unwrap() = compose(&handle);
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
            start_backdrop_watch(&handle);
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
