//! Settings model: defaults, validation of changes coming from the windows, and migration of
//! settings written by earlier versions. Settings are plain JSON so the file stays readable.

use serde_json::{json, Map, Value};

const PUBLIC_KEYS: [&str; 16] = [
    "glass", "layout", "widget", "widgetSet", "lockPosition", "snapToEdges", "widgetOnTop", "widgetOpacity",
    "coveredMode", "placement", "taskbar", "showWidget", "launchAtLogin", "alertNotifications", "refreshMinutes", "zip",
];
const SHOW_KEYS: [&str; 17] = [
    "location", "updated", "refresh", "icon", "temp", "condition", "highLow", "feelsLike", "incoming", "timeline",
    "hourly", "daily", "alerts", "wind", "humidity", "uv", "sun",
];

/// Everything the desktop widget can show and how it is arranged. Presets are instances of this.
pub fn widget_defaults() -> Value {
    json!({
        "orientation": "vertical", "size": "medium", "align": "left", "ink": "light", "hours": 6, "days": 5,
        "show": {
            "location": true, "updated": true, "refresh": true, "icon": true, "temp": true, "condition": true,
            "highLow": true, "feelsLike": true, "incoming": true, "timeline": true, "hourly": true, "daily": true,
            "alerts": true, "wind": false, "humidity": false, "uv": false, "sun": false,
        },
    })
}

pub fn defaults() -> Value {
    json!({
        "version": 3,
        "locations": [], "activeZip": null,
        "glass": "none",            // widget style: none (fully transparent) | clear (tinted) | frosted
        "layout": "full",           // preset the widget started from, or "custom"
        "widget": widget_defaults(),
        "widgetSet": false,         // false until a widget config has been chosen (lets v1 layouts migrate)
        "placement": "desktop",     // where the weather lives: desktop widget | docked on the taskbar
        "taskbar": taskbar_defaults(), // what the docked taskbar strip shows
        "coveredMode": "tray",      // while apps cover the widget: tray temperature | taskbar pill
        "showWidget": true, "lockPosition": false, "snapToEdges": true, "widgetOnTop": false, "widgetOpacity": 100,
        "launchAtLogin": true, "alertNotifications": true, "refreshMinutes": 10,
        "widgetPos": null, "pillPos": null, "detailBounds": null, "notifiedAlerts": [],
    })
}

fn one_of(v: &Value, allowed: &[Value]) -> Option<Value> {
    allowed.iter().find(|a| *a == v).cloned()
}

pub fn sanitize_widget(input: &Value, base: &Value) -> Value {
    let mut out = if base.is_object() { base.clone() } else { widget_defaults() };
    let enums: [(&str, Vec<Value>); 6] = [
        ("orientation", vec![json!("vertical"), json!("horizontal")]),
        ("size", vec![json!("small"), json!("medium"), json!("large"), json!("xlarge")]),
        ("align", vec![json!("left"), json!("center"), json!("right")]),
        ("ink", vec![json!("light"), json!("dark")]),
        ("hours", vec![json!(4), json!(6), json!(8), json!(12)]),
        ("days", vec![json!(3), json!(5), json!(7)]),
    ];
    for (key, allowed) in enums {
        if let Some(v) = one_of(&input[key], &allowed) {
            out[key] = v;
        }
    }
    if !out["show"].is_object() {
        out["show"] = widget_defaults()["show"].clone();
    }
    for key in SHOW_KEYS {
        if let Some(b) = input["show"][key].as_bool() {
            out["show"][key] = json!(b);
        }
    }
    out
}

/// What the docked taskbar strip shows.
pub fn taskbar_defaults() -> Value {
    json!({ "condition": true, "highLow": true, "hint": true, "hours": 4 })
}

/// Merge a (possibly partial or hand-edited) taskbar strip config over `current`.
pub fn sanitize_taskbar(input: &Value, current: &Value) -> Value {
    let fallback = taskbar_defaults();
    let mut out = Map::new();
    for key in ["condition", "highLow", "hint"] {
        let v = input[key].as_bool().or(current[key].as_bool()).or(fallback[key].as_bool()).unwrap_or(true);
        out.insert(key.to_string(), json!(v));
    }
    let hours = [&input["hours"], &current["hours"]].into_iter().find_map(|v| v.as_i64().filter(|n| [0, 2, 4, 6].contains(n))).unwrap_or(4);
    out.insert("hours".into(), json!(hours));
    Value::Object(out)
}

/// Keep only the valid parts of a settings change sent by a window.
pub fn sanitize_patch(patch: &Value, current: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    let mut pick = |key: &str, allowed: &[Value]| {
        if let Some(v) = one_of(&patch[key], allowed) {
            out.insert(key.to_string(), v);
        }
    };
    pick("glass", &[json!("none"), json!("clear"), json!("frosted")]);
    pick("coveredMode", &[json!("taskbar"), json!("tray")]);
    pick("placement", &[json!("desktop"), json!("taskbar")]);
    pick("refreshMinutes", &[json!(5), json!(10), json!(15), json!(30)]);
    for key in ["showWidget", "launchAtLogin", "alertNotifications", "lockPosition", "snapToEdges", "widgetOnTop"] {
        if let Some(b) = patch[key].as_bool() {
            out.insert(key.to_string(), json!(b));
        }
    }
    if let Some(n) = patch["widgetOpacity"].as_f64().filter(|n| n.is_finite()) {
        out.insert("widgetOpacity".into(), json!(n.round().clamp(30.0, 100.0) as i64));
    }
    if let Some(s) = patch["layout"].as_str() {
        if !s.is_empty() && s.len() <= 24 && s.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
            out.insert("layout".into(), json!(s));
        }
    }
    if patch["taskbar"].is_object() {
        out.insert("taskbar".into(), sanitize_taskbar(&patch["taskbar"], &current["taskbar"]));
    }
    if patch["widget"].is_object() {
        out.insert("widget".into(), sanitize_widget(&patch["widget"], &current["widget"]));
        out.insert("widgetSet".into(), json!(true));
    }
    out
}

/// Merge a stored settings file over the defaults and bring older formats up to date.
pub fn load(stored: Option<Value>) -> Value {
    let mut data = defaults();
    let Some(Value::Object(stored)) = stored else { return data };
    let had_single_location = stored.contains_key("location"); // the very first preview build
    let had_widget = stored.contains_key("widget");
    for (k, v) in &stored {
        data[k.as_str()] = v.clone();
    }
    if !data["locations"].is_array() {
        data["locations"] = json!([]);
    }
    if had_single_location {
        if data["locations"].as_array().map_or(true, Vec::is_empty) && data["location"].is_object() {
            data["activeZip"] = data["location"]["zip"].clone();
            data["locations"] = json!([data["location"].clone()]);
        }
        data["glass"] = json!("none");
        data["coveredMode"] = json!("tray");
    }
    if let Some(o) = data.as_object_mut() {
        o.remove("location");
        o.remove("zip");
        o.remove("lastSeenVersion");
    }
    data["widget"] = sanitize_widget(&data["widget"].clone(), &widget_defaults());
    data["taskbar"] = sanitize_taskbar(&data["taskbar"].clone(), &taskbar_defaults());
    if !had_widget {
        data["widgetSet"] = json!(false);
    }
    // Re-validate everything else so a hand-edited file cannot put the app in a bad state.
    let clean = sanitize_patch(&data.clone(), &data);
    let fallback = defaults();
    for key in ["glass", "coveredMode", "placement", "refreshMinutes", "showWidget", "launchAtLogin", "alertNotifications",
        "lockPosition", "snapToEdges", "widgetOnTop", "widgetOpacity", "layout"] {
        data[key] = clean.get(key).cloned().unwrap_or_else(|| fallback[key].clone());
    }
    if !data["notifiedAlerts"].is_array() {
        data["notifiedAlerts"] = json!([]);
    }
    data["version"] = json!(3);
    data
}

pub fn active_location(data: &Value) -> Option<Value> {
    let list = data["locations"].as_array()?;
    list.iter().find(|l| l["zip"] == data["activeZip"]).or(list.first()).cloned()
}

pub fn active_zip(data: &Value) -> Option<String> {
    active_location(data).and_then(|l| l["zip"].as_str().map(String::from))
}

pub fn public_settings(data: &Value) -> Value {
    let mut out = Map::new();
    for key in PUBLIC_KEYS {
        out.insert(key.to_string(), if key == "zip" { json!(active_zip(data)) } else { data[key].clone() });
    }
    Value::Object(out)
}

pub fn public_locations(data: &Value) -> Value {
    let places: Vec<Value> = data["locations"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|l| json!({ "zip": l["zip"], "city": l["city"], "state": l["state"] }))
        .collect();
    json!({ "locations": places, "activeZip": active_zip(data) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_first_preview_settings() {
        let old = json!({ "zip": "62701", "location": { "zip": "62701", "city": "Springfield", "state": "IL", "lat": 1.0, "lon": 2.0 },
            "glass": "clear", "coveredMode": "taskbar", "layout": "wide" });
        let d = load(Some(old));
        assert_eq!(d["glass"], "none");
        assert_eq!(d["coveredMode"], "tray");
        assert_eq!(active_zip(&d).as_deref(), Some("62701"));
        assert_eq!(d["layout"], "wide");
        assert_eq!(d["widgetSet"], false);
        assert!(d.get("location").is_none());
        assert_eq!(public_locations(&d)["locations"][0]["city"], "Springfield");
    }

    #[test]
    fn rejects_bad_values() {
        let d = load(Some(json!({ "glass": "neon", "refreshMinutes": 1, "widgetOpacity": 5, "widget": { "size": "huge", "show": { "daily": false, "evil": true } } })));
        assert_eq!(d["glass"], "none");
        assert_eq!(d["refreshMinutes"], 10);
        assert_eq!(d["widgetOpacity"], 30);
        assert_eq!(d["widget"]["size"], "medium");
        assert_eq!(d["widget"]["show"]["daily"], false);
        assert!(d["widget"]["show"].get("evil").is_none());
    }

    #[test]
    fn widget_patch_merges_over_current() {
        let d = load(None);
        let p = sanitize_patch(&json!({ "layout": "custom", "widget": { "orientation": "horizontal", "show": { "hourly": false } } }), &d);
        assert_eq!(p["widget"]["orientation"], "horizontal");
        assert_eq!(p["widget"]["show"]["hourly"], false);
        assert_eq!(p["widget"]["show"]["temp"], true);
        assert_eq!(p["widgetSet"], true);
        assert_eq!(p["layout"], "custom");
    }

    #[test]
    fn taskbar_strip_config_is_merged_and_validated() {
        let d = load(Some(json!({ "taskbar": { "hours": 9, "hint": false } })));
        assert_eq!(d["taskbar"], json!({ "condition": true, "highLow": true, "hint": false, "hours": 4 }));
        let p = sanitize_patch(&json!({ "taskbar": { "hours": 6 } }), &d);
        assert_eq!(p["taskbar"], json!({ "condition": true, "highLow": true, "hint": false, "hours": 6 }));
    }
}
