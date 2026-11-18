//! Merges National Weather Service and Open-Meteo responses into the `WeatherState` JSON
//! described in docs/UI-CONTRACT.md. Pure functions over JSON values: no network, no clock.

use chrono::{DateTime, FixedOffset, TimeZone, Timelike, Utc};
use serde_json::{json, Map, Value};

pub const USER_AGENT: &str = "WeathaDockDesktop/1.1 (desktop weather widget)";
const WET_THRESHOLD: f64 = 0.004; // inches per 15 minutes (about 0.1 mm)
const HOUR_MS: i64 = 3_600_000;

/// What the NWS calls returned. `alerts` is `None` when the alerts call itself failed.
#[derive(Default, Debug, Clone)]
pub struct NwsData {
    pub time_zone: Option<String>,
    pub daily: Vec<Value>,
    pub hourly: Vec<Value>,
    pub alerts: Option<Vec<Value>>,
}

// ---------------------------------------------------------------- helpers

fn num(v: &Value) -> Option<f64> {
    v.as_f64().filter(|n| n.is_finite())
}
fn round(v: &Value) -> Value {
    num(v).map(|n| json!(n.round() as i64)).unwrap_or(Value::Null)
}
fn opt(v: Option<f64>) -> Value {
    v.map(|n| json!(n)).unwrap_or(Value::Null)
}
fn iso(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms).single().map(|d| d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()).unwrap_or_default()
}
fn parse_ms(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}
fn at(v: &Value, key: &str, i: usize) -> Value {
    v.get(key).and_then(|a| a.get(i)).cloned().unwrap_or(Value::Null)
}
fn compass(deg: &Value) -> Value {
    const NAMES: [&str; 16] = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"];
    num(deg).map(|d| json!(NAMES[((d / 22.5).round() as i64).rem_euclid(16) as usize])).unwrap_or(Value::Null)
}
fn local(ms: i64, offset: i32) -> DateTime<FixedOffset> {
    let tz = FixedOffset::east_opt(offset).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    tz.timestamp_millis_opt(ms).single().unwrap_or_else(|| tz.timestamp_millis_opt(0).unwrap())
}
fn local_date(ms: i64, offset: i32) -> String {
    local(ms, offset).format("%Y-%m-%d").to_string()
}
/// "4 PM", "4:15 PM"
fn clock(ms: i64, offset: i32) -> String {
    let d = local(ms, offset);
    let (pm, h) = d.hour12();
    let suffix = if pm { "PM" } else { "AM" };
    if d.minute() == 0 { format!("{h} {suffix}") } else { format!("{h}:{:02} {suffix}", d.minute()) }
}
fn noon_ms(date: &str) -> i64 {
    parse_ms(&format!("{date}T12:00:00Z")).unwrap_or(0)
}
fn weekday(date: &str) -> String {
    Utc.timestamp_millis_opt(noon_ms(date)).single().map(|d| d.format("%a").to_string()).unwrap_or_default()
}
fn add_days(date: &str, n: i64) -> String {
    iso(noon_ms(date) + n * 86_400_000)[..10].to_string()
}
/// "10 mph" -> 10, "5 to 10 mph" -> 10
fn wind_mph(v: &Value) -> Value {
    let nums: Vec<i64> = v.as_str().unwrap_or("").split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).collect();
    match nums.as_slice() {
        [] => Value::Null,
        [a] => json!(a),
        [_, b, ..] => json!(b),
    }
}
fn c_to_f(v: &Value) -> Value {
    num(v).map(|c| json!((c * 9.0 / 5.0 + 32.0).round() as i64)).unwrap_or(Value::Null)
}
fn round_to(n: f64, places: i32) -> f64 {
    let m = 10f64.powi(places);
    (n * m).round() / m
}

// ---------------------------------------------------------------- weather codes

fn wmo(code: i64) -> (&'static str, &'static str) {
    match code {
        0 => ("clear", "Clear"),
        1 => ("mostly-clear", "Mostly Clear"),
        2 => ("partly-cloudy", "Partly Cloudy"),
        3 => ("cloudy", "Cloudy"),
        45 => ("fog", "Fog"),
        48 => ("fog", "Freezing Fog"),
        51 => ("drizzle", "Light Drizzle"),
        53 => ("drizzle", "Drizzle"),
        55 => ("drizzle", "Heavy Drizzle"),
        56 | 57 => ("sleet", "Freezing Drizzle"),
        61 => ("rain", "Light Rain"),
        63 => ("rain", "Rain"),
        65 => ("heavy-rain", "Heavy Rain"),
        66 | 67 => ("sleet", "Freezing Rain"),
        71 => ("snow", "Light Snow"),
        73 => ("snow", "Snow"),
        75 => ("snow", "Heavy Snow"),
        77 => ("snow", "Snow Grains"),
        80 => ("rain", "Light Showers"),
        81 => ("rain", "Showers"),
        82 => ("heavy-rain", "Heavy Showers"),
        85 => ("snow", "Snow Showers"),
        86 => ("snow", "Heavy Snow Showers"),
        95 => ("thunderstorm", "Thunderstorms"),
        96 | 99 => ("thunderstorm", "Thunderstorms with Hail"),
        _ => ("cloudy", ""),
    }
}
fn code_of(v: &Value) -> i64 {
    num(v).map(|n| n as i64).unwrap_or(-1)
}

fn precip_kind(code: i64, precip: f64, snowfall: f64) -> &'static str {
    if code >= 95 {
        "storm"
    } else if matches!(code, 56 | 57 | 66 | 67) {
        "mix"
    } else if matches!(code, 71 | 73 | 75 | 77 | 85 | 86) {
        "snow"
    } else if snowfall > 0.0 {
        if precip > 0.0 && (51..=67).contains(&code) { "mix" } else { "snow" }
    } else {
        "rain"
    }
}

/// Icon from an NWS short forecast; used only when Open-Meteo is unavailable.
pub fn icon_from_text(text: &str) -> &'static str {
    let t = text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if has(&["thunder", "t-storm", "tstorm"]) {
        "thunderstorm"
    } else if has(&["sleet", "freezing", "ice", "wintry"]) || (t.contains("rain") && t.contains("snow")) {
        "sleet"
    } else if has(&["snow", "flurr", "blizzard"]) {
        "snow"
    } else if t.contains("heavy rain") {
        "heavy-rain"
    } else if t.contains("drizzle") {
        "drizzle"
    } else if has(&["rain", "shower"]) {
        "rain"
    } else if has(&["fog", "haze", "mist", "smoke"]) {
        "fog"
    } else if has(&["wind", "breezy", "blustery"]) && !has(&["cloud", "sun", "clear"]) {
        "wind"
    } else if t.contains("mostly cloudy") {
        "cloudy"
    } else if has(&["partly sunny", "partly cloudy"]) {
        "partly-cloudy"
    } else if has(&["mostly sunny", "mostly clear"]) {
        "mostly-clear"
    } else if has(&["cloud", "overcast"]) {
        "cloudy"
    } else {
        "clear"
    }
}

fn kind_word(kind: &str) -> &'static str {
    match kind {
        "snow" => "Snow",
        "mix" => "Wintry mix",
        "storm" => "Thunderstorms",
        _ => "Rain",
    }
}

// ---------------------------------------------------------------- requests

pub fn open_meteo_url(lat: f64, lon: f64) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}\
&current=temperature_2m,relative_humidity_2m,apparent_temperature,is_day,precipitation,weather_code,pressure_msl,wind_speed_10m,wind_direction_10m,wind_gusts_10m\
&minutely_15=precipitation,snowfall,weather_code\
&hourly=temperature_2m,apparent_temperature,relative_humidity_2m,dew_point_2m,precipitation_probability,precipitation,weather_code,wind_speed_10m,wind_direction_10m,visibility,uv_index,is_day\
&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,precipitation_probability_max\
&temperature_unit=fahrenheit&wind_speed_unit=mph&precipitation_unit=inch&timezone=auto&timeformat=unixtime&forecast_days=8&forecast_minutely_15=52&past_minutely_15=0"
    )
}

/// Parse a zippopotam.us response into a place.
pub fn place_from_zippopotam(zip: &str, body: &Value) -> Option<Value> {
    let p = body.get("places")?.get(0)?;
    let coord = |k: &str| p.get(k)?.as_str()?.parse::<f64>().ok().map(|n| round_to(n, 4));
    Some(json!({
        "zip": zip,
        "city": p.get("place name")?.as_str()?,
        "state": p.get("state abbreviation")?.as_str()?,
        "lat": coord("latitude")?,
        "lon": coord("longitude")?,
    }))
}

// ---------------------------------------------------------------- normalisation

struct Step {
    ms: i64,
    precip: f64,
    kind: Option<&'static str>,
}

fn build_minutely(om: Option<&Value>, now: i64) -> Vec<Step> {
    let Some(m) = om.and_then(|o| o.get("minutely_15")) else { return vec![] };
    let Some(times) = m.get("time").and_then(Value::as_array) else { return vec![] };
    let mut out = Vec::new();
    for (i, t) in times.iter().enumerate() {
        let Some(t) = num(t) else { continue };
        let ms = t as i64 * 1000;
        if ms + 15 * 60_000 <= now {
            continue;
        }
        let precip = num(&at(m, "precipitation", i)).unwrap_or(0.0);
        let kind = (precip >= WET_THRESHOLD).then(|| {
            let code = m.get("weather_code").map(|_| code_of(&at(m, "weather_code", i))).unwrap_or(61);
            precip_kind(code, precip, num(&at(m, "snowfall", i)).unwrap_or(0.0))
        });
        out.push(Step { ms, precip: round_to(precip, 3), kind });
        if out.len() >= 48 {
            break;
        }
    }
    out
}

fn next_precip(kind: Value, is_now: bool, starts: Value, ends: Value, minutes: Value, headline: String) -> Value {
    json!({ "kind": kind, "isNow": is_now, "startsAt": starts, "endsAt": ends, "minutesUntil": minutes, "headline": headline })
}

fn build_next_precip(steps: &[Step], hourly: &[Value], now: i64, offset: i32) -> Value {
    if let Some(last) = steps.last() {
        let Some(start) = steps.iter().position(|s| s.kind.is_some()) else {
            let hours = ((last.ms + 15 * 60_000 - now) as f64 / HOUR_MS as f64).round().max(1.0) as i64;
            return next_precip(Value::Null, false, Value::Null, Value::Null, Value::Null, format!("Dry for the next {hours} hours"));
        };
        // The spell ends at the first gap of two dry steps (30 minutes).
        let end = (start + 1..steps.len())
            .find(|&i| steps[i].kind.is_none() && steps.get(i + 1).map_or(true, |n| n.kind.is_none()));
        let spell = &steps[start..end.unwrap_or(steps.len())];
        let has = |k: &str| spell.iter().any(|s| s.kind == Some(k));
        let kind = if has("storm") {
            "storm"
        } else if has("mix") || (has("snow") && has("rain")) {
            "mix"
        } else if has("snow") {
            "snow"
        } else {
            "rain"
        };
        let word = kind_word(kind);
        let start_ms = steps[start].ms;
        let end_ms = end.map(|i| steps[i].ms);
        let is_now = start_ms <= now;
        let minutes = if is_now { 0 } else { (((start_ms - now) as f64) / 60_000.0).round().max(1.0) as i64 };
        let headline = if is_now {
            match end_ms {
                Some(e) => format!("{word} until {}", clock(e, offset)),
                None => format!("{word} continuing for hours"),
            }
        } else if minutes < 60 {
            format!("{word} starting in {minutes} min")
        } else {
            format!("{word} around {}", clock(start_ms, offset))
        };
        return next_precip(json!(kind), is_now, json!(iso(start_ms)), end_ms.map(|e| json!(iso(e))).unwrap_or(Value::Null), json!(minutes), headline);
    }
    // No 15-minute data: fall back to the hourly chance of precipitation.
    let likely = hourly.iter().take(12).find(|h| num(&h["precipProb"]).unwrap_or(0.0) >= 50.0);
    if let Some(h) = likely {
        let kind = match h["icon"].as_str().unwrap_or("") {
            "snow" => "snow",
            "sleet" => "mix",
            "thunderstorm" => "storm",
            _ => "rain",
        };
        let ms = h["time"].as_str().and_then(parse_ms).unwrap_or(now);
        let prob = num(&h["precipProb"]).unwrap_or(0.0) as i64;
        let is_now = ms <= now;
        let headline = if is_now {
            format!("{} likely now ({prob}%)", kind_word(kind))
        } else {
            format!("{} likely around {} ({prob}%)", kind_word(kind), clock(ms, offset))
        };
        let minutes = if is_now { 0 } else { ((ms - now) as f64 / 60_000.0).round() as i64 };
        return next_precip(json!(kind), is_now, h["time"].clone(), Value::Null, json!(minutes), headline);
    }
    let headline = if hourly.is_empty() { "Precipitation timing unavailable" } else { "No precipitation expected in the next 12 hours" };
    next_precip(Value::Null, false, Value::Null, Value::Null, Value::Null, headline.to_string())
}

fn build_hourly(nws: Option<&NwsData>, om: Option<&Value>, now: i64) -> Vec<Value> {
    let h = om.and_then(|o| o.get("hourly")).filter(|h| h.get("time").map_or(false, Value::is_array));
    let hour_start = now.div_euclid(HOUR_MS) * HOUR_MS;
    let om_index = |ms: i64| -> Option<usize> {
        let key = ms.div_euclid(HOUR_MS) * 3600;
        h?.get("time")?.as_array()?.iter().position(|t| num(t).map(|n| n as i64) == Some(key))
    };
    let add_om = |row: &mut Map<String, Value>, h: &Value, i: usize| {
        row.insert("feelsLike".into(), round(&at(h, "apparent_temperature", i)));
        row.insert("icon".into(), json!(wmo(code_of(&at(h, "weather_code", i))).0));
        row.insert("isDay".into(), json!(num(&at(h, "is_day", i)) == Some(1.0)));
        row.insert("precipAmount".into(), opt(num(&at(h, "precipitation", i))));
    };
    let mut out = Vec::new();
    if let Some(n) = nws.filter(|n| !n.hourly.is_empty()) {
        for p in &n.hourly {
            let Some(t) = p["startTime"].as_str().and_then(parse_ms) else { continue };
            if t < hour_start {
                continue;
            }
            let short = p["shortForecast"].as_str().unwrap_or("");
            let mut row = json!({
                "time": iso(t), "temp": round(&p["temperature"]), "feelsLike": null, "icon": icon_from_text(short),
                "isDay": p["isDaytime"].as_bool().unwrap_or(false), "condition": short,
                "precipProb": opt(num(&p["probabilityOfPrecipitation"]["value"])), "precipAmount": null,
                "windSpeed": wind_mph(&p["windSpeed"]), "windDir": p["windDirection"].as_str(),
                "humidity": opt(num(&p["relativeHumidity"]["value"])), "dewPoint": c_to_f(&p["dewpoint"]["value"]),
            });
            if let (Some(h), Some(i)) = (h, om_index(t)) {
                add_om(row.as_object_mut().unwrap(), h, i);
            }
            out.push(row);
            if out.len() >= 48 {
                break;
            }
        }
        return out;
    }
    if let Some(h) = h {
        for (i, t) in h["time"].as_array().into_iter().flatten().enumerate() {
            let Some(t) = num(t) else { continue };
            let ms = t as i64 * 1000;
            if ms < hour_start {
                continue;
            }
            let mut row = json!({
                "time": iso(ms), "temp": round(&at(h, "temperature_2m", i)),
                "condition": wmo(code_of(&at(h, "weather_code", i))).1,
                "precipProb": opt(num(&at(h, "precipitation_probability", i))),
                "windSpeed": round(&at(h, "wind_speed_10m", i)), "windDir": compass(&at(h, "wind_direction_10m", i)),
                "humidity": opt(num(&at(h, "relative_humidity_2m", i))), "dewPoint": round(&at(h, "dew_point_2m", i)),
            });
            add_om(row.as_object_mut().unwrap(), h, i);
            out.push(row);
            if out.len() >= 48 {
                break;
            }
        }
    }
    out
}

fn build_daily(nws: Option<&NwsData>, om: Option<&Value>, now: i64, offset: i32) -> Vec<Value> {
    let today = local_date(now, offset);
    let mut days: Vec<Value> = (0..7)
        .map(|i| {
            let date = add_days(&today, i);
            json!({
                "date": date, "name": if i == 0 { "Today".to_string() } else { weekday(&date) },
                "high": null, "low": null, "icon": null, "condition": "", "precipProb": null,
                "dayText": "", "nightText": "", "sunrise": null, "sunset": null,
            })
        })
        .collect();
    let find = |days: &mut Vec<Value>, date: &str| days.iter().position(|d| d["date"] == date);

    if let Some(n) = nws {
        for p in &n.daily {
            let start = p["startTime"].as_str().unwrap_or("");
            if start.len() < 13 {
                continue;
            }
            // NWS start times carry the local offset, so the first 10 characters are the local
            // date. An overnight period that started after midnight belongs to the previous day.
            let mut date = start[..10].to_string();
            let is_day = p["isDaytime"].as_bool().unwrap_or(false);
            if !is_day && start[11..13].parse::<u32>().map_or(false, |h| h < 6) {
                date = add_days(&date, -1);
            }
            let Some(i) = find(&mut days, &date) else { continue };
            let d = &mut days[i];
            if let Some(pop) = num(&p["probabilityOfPrecipitation"]["value"]) {
                d["precipProb"] = json!(num(&d["precipProb"]).unwrap_or(0.0).max(pop));
            }
            let short = p["shortForecast"].as_str().unwrap_or("");
            let detail = p["detailedForecast"].as_str().unwrap_or("");
            if is_day {
                d["high"] = round(&p["temperature"]);
                d["dayText"] = json!(detail);
                if !short.is_empty() {
                    d["condition"] = json!(short);
                }
                d["icon"] = json!(icon_from_text(short));
            } else {
                d["low"] = round(&p["temperature"]);
                d["nightText"] = json!(detail);
                if d["condition"].as_str().unwrap_or("").is_empty() {
                    d["condition"] = json!(short);
                    d["icon"] = json!(icon_from_text(short));
                }
            }
        }
    }
    if let Some(o) = om.and_then(|o| o.get("daily")).filter(|o| o.get("time").map_or(false, Value::is_array)) {
        let om_offset = om.and_then(|o| num(&o["utc_offset_seconds"])).unwrap_or(0.0) as i64 * 1000;
        for (i, t) in o["time"].as_array().into_iter().flatten().enumerate() {
            let Some(t) = num(t) else { continue };
            let date = iso(t as i64 * 1000 + om_offset)[..10].to_string();
            let Some(di) = find(&mut days, &date) else { continue };
            let d = &mut days[di];
            if d["high"].is_null() {
                d["high"] = round(&at(o, "temperature_2m_max", i));
            }
            if d["low"].is_null() {
                d["low"] = round(&at(o, "temperature_2m_min", i));
            }
            if d["precipProb"].is_null() {
                d["precipProb"] = opt(num(&at(o, "precipitation_probability_max", i)));
            }
            let code = code_of(&at(o, "weather_code", i));
            if d["condition"].as_str().unwrap_or("").is_empty() {
                d["condition"] = json!(wmo(code).1);
            }
            d["icon"] = json!(wmo(code).0); // one consistent icon source when available
            let stamp = |v: Value| num(&v).filter(|n| *n > 0.0).map(|n| json!(iso(n as i64 * 1000))).unwrap_or(Value::Null);
            d["sunrise"] = stamp(at(o, "sunrise", i));
            d["sunset"] = stamp(at(o, "sunset", i));
        }
    }
    days.retain(|d| !d["high"].is_null() || !d["low"].is_null());
    for d in &mut days {
        if d["icon"].is_null() {
            d["icon"] = json!("cloudy");
        }
    }
    days
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "Extreme" => 0,
        "Severe" => 1,
        "Moderate" => 2,
        "Minor" => 3,
        _ => 4,
    }
}

fn build_alerts(features: &[Value], now: i64) -> Vec<Value> {
    let text = |p: &Value, k: &str| p[k].as_str().unwrap_or("").to_string();
    let mut out: Vec<Value> = features
        .iter()
        .map(|f| &f["properties"])
        .filter(|p| p["status"] == "Actual" && p["messageType"] != "Cancel")
        .filter(|p| {
            let end = p["ends"].as_str().or(p["expires"].as_str()).and_then(parse_ms);
            end.map_or(true, |e| e > now)
        })
        .map(|p| {
            let event = if text(p, "event").is_empty() { "Weather Alert".to_string() } else { text(p, "event") };
            let severity = text(p, "severity");
            let or = |a: String, b: &str| if a.is_empty() { b.to_string() } else { a };
            json!({
                "id": or(or(text(p, "id"), &text(p, "@id")), &format!("{event}-{}", text(p, "sent"))),
                "event": event,
                "severity": if severity_rank(&severity) == 4 { "Unknown".to_string() } else { severity },
                "urgency": or(text(p, "urgency"), "Unknown"),
                "certainty": or(text(p, "certainty"), "Unknown"),
                "headline": or(text(p, "headline"), &event),
                "description": text(p, "description"),
                "instruction": text(p, "instruction"),
                "onset": p["onset"].as_str().or(p["effective"].as_str()),
                "ends": p["ends"].as_str(),
                "expires": p["expires"].as_str(),
                "sender": or(text(p, "senderName"), "National Weather Service"),
                "areaDesc": text(p, "areaDesc"),
            })
        })
        .collect();
    out.sort_by_key(|a| severity_rank(a["severity"].as_str().unwrap_or("")));
    out
}

fn build_current(om: Option<&Value>, hourly: &[Value], om_hour: Option<usize>) -> Value {
    let h0 = hourly.first();
    if let Some(c) = om.and_then(|o| o.get("current")).filter(|c| c.is_object()) {
        let om = om.unwrap();
        let oh = &om["hourly"];
        let mut visibility = Value::Null;
        if let Some(v) = om_hour.and_then(|i| num(&at(oh, "visibility", i))) {
            let feet = om["hourly_units"]["visibility"].as_str() == Some("ft");
            let miles = round_to(v / if feet { 5280.0 } else { 1609.344 }, 1);
            visibility = json!(if miles > 10.0 { miles.round() } else { miles });
        }
        let code = code_of(&c["weather_code"]);
        let condition = h0.and_then(|h| h["condition"].as_str()).filter(|s| !s.is_empty()).unwrap_or(wmo(code).1);
        return json!({
            "temp": round(&c["temperature_2m"]), "feelsLike": round(&c["apparent_temperature"]),
            "condition": condition, "icon": wmo(code).0, "isDay": num(&c["is_day"]) == Some(1.0),
            "humidity": opt(num(&c["relative_humidity_2m"])),
            "dewPoint": match om_hour { Some(i) => round(&at(oh, "dew_point_2m", i)), None => h0.map(|h| h["dewPoint"].clone()).unwrap_or(Value::Null) },
            "windSpeed": round(&c["wind_speed_10m"]), "windGust": round(&c["wind_gusts_10m"]),
            "windDir": compass(&c["wind_direction_10m"]),
            "pressure": opt(num(&c["pressure_msl"]).map(|p| round_to(p * 0.02953, 2))),
            "visibility": visibility,
            "uvIndex": om_hour.map(|i| round(&at(oh, "uv_index", i))).unwrap_or(Value::Null),
        });
    }
    match h0 {
        Some(h) => json!({
            "temp": h["temp"], "feelsLike": h["feelsLike"], "condition": h["condition"], "icon": h["icon"],
            "isDay": h["isDay"], "humidity": h["humidity"], "dewPoint": h["dewPoint"], "windSpeed": h["windSpeed"],
            "windGust": null, "windDir": h["windDir"], "pressure": null, "visibility": null, "uvIndex": null,
        }),
        None => Value::Null,
    }
}

/// UTC offset of the location in seconds: Open-Meteo states it; otherwise read it off an NWS time.
fn utc_offset(nws: Option<&NwsData>, om: Option<&Value>) -> i32 {
    if let Some(o) = om.and_then(|o| num(&o["utc_offset_seconds"])) {
        return o as i32;
    }
    nws.and_then(|n| n.daily.first().or(n.hourly.first()))
        .and_then(|p| p["startTime"].as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.offset().local_minus_utc())
        .unwrap_or(0)
}

/// Build the `WeatherState` for a place. Returns `None` when neither source answered.
/// `loc` is `{ zip, city, state, lat, lon }`; `previous_alerts` is kept when the alerts call failed.
pub fn build_state(loc: &Value, nws: Option<&NwsData>, om: Option<&Value>, now: i64, previous_alerts: &Value) -> Option<Value> {
    if nws.is_none() && om.is_none() {
        return None;
    }
    let offset = utc_offset(nws, om);
    let time_zone = nws
        .and_then(|n| n.time_zone.clone())
        .or_else(|| om.and_then(|o| o["timezone"].as_str().map(String::from)))
        .unwrap_or_else(|| "America/Chicago".into());
    let mut hourly = build_hourly(nws, om, now);
    let daily = build_daily(nws, om, now, offset);
    let steps = build_minutely(om, now);
    let next = build_next_precip(&steps, &hourly, now, offset);
    let om_hour = om.and_then(|o| {
        let key = now.div_euclid(HOUR_MS) * 3600;
        o["hourly"]["time"].as_array()?.iter().position(|t| num(t).map(|n| n as i64) == Some(key))
    });
    let current = build_current(om, &hourly, om_hour);
    let today = daily.first().map(|d| {
        json!({
            "high": d["high"], "low": d["low"], "sunrise": d["sunrise"], "sunset": d["sunset"],
            "summary": nws.and_then(|n| n.daily.first()).and_then(|p| p["detailedForecast"].as_str()).unwrap_or(""),
        })
    });
    let mut missing = Vec::new();
    if nws.is_none() {
        missing.push("National Weather Service");
    }
    if om.is_none() {
        missing.push("Open-Meteo");
    }
    for h in &mut hourly {
        if let Some(o) = h.as_object_mut() {
            o.remove("dewPoint");
        }
    }
    let mut location = loc.clone();
    if let Some(o) = location.as_object_mut() {
        o.insert("timeZone".into(), json!(time_zone));
    }
    let alerts = match nws.and_then(|n| n.alerts.as_ref()) {
        Some(features) => json!(build_alerts(features, now)),
        None if previous_alerts.is_array() => previous_alerts.clone(),
        None => json!([]),
    };
    Some(json!({
        "status": "ok",
        "error": if missing.is_empty() { Value::Null } else { json!(format!("{} did not respond. Showing what is available.", missing.join(" and "))) },
        "stale": false,
        "updatedAt": iso(now),
        "sources": { "nws": nws.is_some(), "openMeteo": om.is_some() },
        "location": location,
        "current": current,
        "today": today,
        "nextPrecip": next,
        "minutely": steps.iter().take(24).map(|s| json!({ "time": iso(s.ms), "precip": s.precip, "kind": s.kind })).collect::<Vec<_>>(),
        "hourly": hourly,
        "daily": daily,
        "alerts": alerts,
    }))
}

/// The state shown before any data exists (`status` is "loading" or "error").
pub fn empty_state(status: &str, error: Option<&str>, loc: Option<&Value>) -> Value {
    let location = loc.map(|l| {
        let mut l = l.clone();
        if let Some(o) = l.as_object_mut() {
            o.entry("timeZone").or_insert(json!("UTC"));
        }
        l
    });
    json!({
        "status": status, "error": error, "stale": false, "updatedAt": null,
        "sources": { "nws": false, "openMeteo": false },
        "location": location, "current": null, "today": null,
        "nextPrecip": { "kind": null, "isNow": false, "startsAt": null, "endsAt": null, "minutesUntil": null, "headline": "" },
        "minutely": [], "hourly": [], "daily": [], "alerts": [],
    })
}
