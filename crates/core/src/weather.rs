//! Merges National Weather Service and Open-Meteo responses into the `WeatherState` JSON
//! described in docs/UI-CONTRACT.md. Pure functions over JSON values: no network, no clock.

use chrono::{DateTime, FixedOffset, TimeZone, Timelike, Utc};
use serde_json::{json, Value};

pub const USER_AGENT: &str = "WeathaDockDesktop/1.1 (desktop weather widget)";
const WET_THRESHOLD: f64 = 0.004; // inches per 15 minutes (about 0.1 mm)
const HOUR_MS: i64 = 3_600_000;
const STEP_MS: i64 = 15 * 60_000;
const DAY_MS: i64 = 86_400_000;

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
/// A time nowhere near `now` is garbage from a broken response, not weather.
fn near(ms: i64, now: i64) -> bool {
    ms.checked_sub(now).map_or(false, |d| d.unsigned_abs() < 40 * DAY_MS as u64)
}
/// A time the services send as seconds since 1970, in milliseconds. Never overflows.
fn stamp_ms(v: &Value, now: i64) -> Option<i64> {
    Some((num(v)? as i64).saturating_mul(1000)).filter(|ms| near(*ms, now))
}
fn time_of(v: &Value, key: &str) -> Option<i64> {
    v[key].as_str().and_then(parse_ms)
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
    iso(noon_ms(date) + n * DAY_MS).get(..10).unwrap_or(date).to_string()
}
fn weekday_of(ms: i64, offset: i32) -> String {
    local(ms, offset).format("%a").to_string()
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

/// Icon for one part of an NWS short forecast ("Chance Rain Showers", already in lower case).
fn clause_icon(t: &str) -> &'static str {
    let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
    let word = |w: &str| t.split(|c: char| !c.is_ascii_alphabetic()).any(|part| part == w);
    if has(&["thunder", "t-storm", "tstorm"]) {
        "thunderstorm"
    } else if has(&["sleet", "wintry", "freezing rain", "freezing drizzle"]) || word("ice") || (t.contains("rain") && t.contains("snow")) {
        "sleet"
    } else if has(&["snow", "flurr", "blizzard"]) {
        "snow"
    } else if t.contains("heavy rain") {
        "heavy-rain"
    } else if t.contains("drizzle") {
        "drizzle"
    } else if has(&["rain", "shower"]) {
        "rain"
    } else if has(&["fog", "haze", "mist", "smoke", "dust"]) || word("sand") || word("ash") {
        "fog"
    } else if has(&["wind", "breezy", "blustery"]) && !has(&["cloud", "sun", "clear"]) {
        "wind"
    } else if t.contains("mostly cloudy") {
        "cloudy"
    } else if has(&["partly sunny", "partly cloudy", "increasing clouds", "decreasing clouds", "becoming", "clearing"]) {
        "partly-cloudy"
    } else if has(&["mostly sunny", "mostly clear"]) {
        "mostly-clear"
    } else if has(&["cloud", "overcast"]) {
        "cloudy"
    } else {
        "clear"
    }
}

/// How much an icon says about the weather; the more telling one stands for a whole period.
fn icon_rank(icon: &str) -> u8 {
    match icon {
        "thunderstorm" => 11,
        "sleet" => 10,
        "snow" => 9,
        "heavy-rain" => 8,
        "rain" => 7,
        "drizzle" => 6,
        "fog" => 5,
        "wind" => 4,
        "cloudy" => 3,
        "partly-cloudy" => 2,
        "mostly-clear" => 1,
        _ => 0,
    }
}

/// Icon for an NWS short forecast, so the picture always agrees with the words beside it.
/// "Mostly Sunny then Slight Chance Showers" has two parts: the most telling part wins, but
/// a slight chance of rain or snow never outranks what the rest of the period is like.
pub fn icon_from_text(text: &str) -> &'static str {
    let t = text.to_lowercase();
    let mut best: Option<(&'static str, bool)> = None;
    for clause in t.split(" then ") {
        let icon = clause_icon(clause);
        let slight = icon_rank(icon) >= 6 && (clause.contains("slight chance") || clause.contains("isolated"));
        best = Some(match best {
            Some((held, held_slight)) if held_slight == slight && icon_rank(held) >= icon_rank(icon) => (held, held_slight),
            Some((held, false)) if slight => (held, false),
            _ => (icon, slight),
        });
    }
    best.map_or("clear", |b| b.0)
}

/// What the air feels like the way the National Weather Service reports it: wind chill in
/// the cold (50°F and below, wind above 3 mph), heat index in the heat (80°F and above),
/// and otherwise simply the temperature. All in °F, mph and percent.
pub fn feels_like(temp: f64, wind: Option<f64>, humidity: Option<f64>) -> f64 {
    if temp <= 50.0 {
        if let Some(v) = wind.filter(|v| *v > 3.0) {
            let p = v.powf(0.16);
            return (35.74 + 0.6215 * temp - 35.75 * p + 0.4275 * temp * p).min(temp);
        }
    } else if temp >= 80.0 {
        if let Some(rh) = humidity.filter(|h| (0.0..=100.0).contains(h)) {
            // The NWS method: a simple estimate first, the full Rothfusz regression when that reaches 80.
            let simple = (0.5 * (temp + 61.0 + (temp - 68.0) * 1.2 + rh * 0.094) + temp) / 2.0;
            if simple < 80.0 {
                return simple;
            }
            let (t, r) = (temp, rh);
            let mut hi = -42.379 + 2.04901523 * t + 10.14333127 * r - 0.22475541 * t * r - 0.00683783 * t * t - 0.05481717 * r * r
                + 0.00122874 * t * t * r
                + 0.00085282 * t * r * r
                - 0.00000199 * t * t * r * r;
            if r < 13.0 && t <= 112.0 {
                hi -= ((13.0 - r) / 4.0) * ((17.0 - (t - 95.0).abs()) / 17.0).max(0.0).sqrt();
            } else if r > 85.0 && t <= 87.0 {
                hi += ((r - 85.0) / 10.0) * ((87.0 - t) / 5.0);
            }
            return hi;
        }
    }
    temp
}

fn feels(temp: Option<f64>, wind: Option<f64>, humidity: Option<f64>) -> Value {
    temp.map(|t| json!(feels_like(t, wind, humidity).round() as i64)).unwrap_or(Value::Null)
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
&current=temperature_2m,relative_humidity_2m,is_day,precipitation,weather_code,pressure_msl,wind_speed_10m,wind_direction_10m,wind_gusts_10m\
&minutely_15=precipitation,snowfall,weather_code\
&hourly=temperature_2m,relative_humidity_2m,dew_point_2m,precipitation_probability,precipitation,weather_code,wind_speed_10m,wind_direction_10m,visibility,uv_index,is_day\
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

#[derive(Clone)]
struct Step {
    ms: i64,
    precip: f64,
    kind: Option<&'static str>,
}

const KINDS: [&str; 4] = ["rain", "snow", "mix", "storm"];
/// What the first hourly row (the one every surface labels "Now") and the current conditions share.
const NOW_KEYS: [&str; 8] = ["temp", "feelsLike", "icon", "isDay", "condition", "windSpeed", "windDir", "humidity"];
/// What only a reading can tell; left blank once the reading is too old to stand for "now".
const READING_KEYS: [&str; 5] = ["windGust", "pressure", "visibility", "uvIndex", "dewPoint"];
const SEVERITY: [&str; 4] = ["Extreme", "Severe", "Moderate", "Minor"];
const URGENCY: [&str; 4] = ["Immediate", "Expected", "Future", "Past"];
const CERTAINTY: [&str; 4] = ["Observed", "Likely", "Possible", "Unlikely"];

fn rank(value: &Value, order: &[&str]) -> usize {
    order.iter().position(|o| value.as_str() == Some(*o)).unwrap_or(order.len())
}

/// Where the wet spell that starts at `start` ends: the first of two dry steps in a row.
fn spell_end(steps: &[Step], start: usize) -> Option<usize> {
    (start + 1..steps.len()).find(|&i| steps[i].kind.is_none() && steps.get(i + 1).map_or(true, |n| n.kind.is_none()))
}

/// A lone 15-minute step with a trace of precipitation is model noise, not weather worth a
/// heads-up. It counts as dry, so the headline and the timeline bars always agree.
fn drop_traces(steps: &mut [Step]) {
    let mut i = 0;
    while i < steps.len() {
        if steps[i].kind.is_none() {
            i += 1;
            continue;
        }
        let end = spell_end(steps, i).unwrap_or(steps.len());
        if steps[i..end].iter().filter(|s| s.kind.is_some()).count() < 2 && steps[i].precip < 0.01 {
            steps[i].kind = None;
            steps[i].precip = 0.0;
        }
        i = end.max(i + 1);
    }
}

fn build_minutely(om: Option<&Value>, now: i64) -> Vec<Step> {
    let Some(m) = om.and_then(|o| o.get("minutely_15")) else { return vec![] };
    let Some(times) = m.get("time").and_then(Value::as_array) else { return vec![] };
    let mut out: Vec<Step> = Vec::new();
    for (i, t) in times.iter().enumerate() {
        let Some(ms) = stamp_ms(t, now) else { continue };
        if ms + STEP_MS <= now || out.last().map_or(false, |l| ms <= l.ms) {
            continue;
        }
        let precip = num(&at(m, "precipitation", i)).unwrap_or(0.0);
        let kind = (precip >= WET_THRESHOLD).then(|| {
            let code = m.get("weather_code").map(|_| code_of(&at(m, "weather_code", i))).unwrap_or(61);
            precip_kind(code, precip, num(&at(m, "snowfall", i)).unwrap_or(0.0))
        });
        out.push(Step { ms, precip: if kind.is_some() { round_to(precip, 3) } else { 0.0 }, kind });
        if out.len() >= 48 {
            break;
        }
    }
    drop_traces(&mut out);
    out
}

fn steps_json(steps: &[Step]) -> Value {
    steps.iter().map(|s| json!({ "time": iso(s.ms), "precip": s.precip, "kind": s.kind })).collect()
}

fn steps_of(state: &Value) -> Vec<Step> {
    let step = |m: &Value| {
        Some(Step { ms: time_of(m, "time")?, precip: num(&m["precip"]).unwrap_or(0.0), kind: KINDS.into_iter().find(|k| m["kind"].as_str() == Some(*k)) })
    };
    state["minutely"].as_array().into_iter().flatten().filter_map(step).collect()
}

fn next_precip(kind: Value, is_now: bool, starts: Value, ends: Value, minutes: Value, chance: Value, headline: String) -> Value {
    json!({ "kind": kind, "isNow": is_now, "startsAt": starts, "endsAt": ends, "minutesUntil": minutes, "chance": chance, "headline": headline })
}

fn build_next_precip(steps: &[Step], hourly: &[Value], now: i64, offset: i32) -> Value {
    if let Some(start) = steps.iter().position(|s| s.kind.is_some()) {
        let end = spell_end(steps, start);
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
        let minutes = if is_now { 0 } else { ((start_ms - now) as f64 / 60_000.0).round().max(1.0) as i64 };
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
        let ends = end_ms.map(|e| json!(iso(e))).unwrap_or(Value::Null);
        return next_precip(json!(kind), is_now, json!(iso(start_ms)), ends, json!(minutes), Value::Null, headline);
    }
    // Nothing in the 15-minute data, or no such data: the hourly chance of precipitation speaks,
    // so the headline never says "dry" beside an hour the forecast calls likely wet.
    let horizon = steps.last().map(|l| l.ms + STEP_MS);
    let until = horizon.unwrap_or(now + 12 * HOUR_MS);
    let likely = hourly
        .iter()
        .find(|h| num(&h["precipProb"]).unwrap_or(0.0) >= 50.0 && time_of(h, "time").map_or(false, |t| t + HOUR_MS > now && t < until));
    if let Some(h) = likely {
        let kind = match h["icon"].as_str().unwrap_or("") {
            "snow" => "snow",
            "sleet" => "mix",
            "thunderstorm" => "storm",
            "rain" | "heavy-rain" | "drizzle" => "rain",
            _ if num(&h["temp"]).map_or(false, |t| t <= 32.0) => "snow",
            _ => "rain",
        };
        let word = kind_word(kind);
        let ms = time_of(h, "time").unwrap_or(now);
        let prob = num(&h["precipProb"]).unwrap_or(0.0).round() as i64;
        let (minutes, headline) = if ms <= now {
            (0, format!("{word} likely this hour ({prob}%)"))
        } else {
            (((ms - now) as f64 / 60_000.0).round() as i64, format!("{word} likely around {} ({prob}%)", clock(ms, offset)))
        };
        return next_precip(json!(kind), false, h["time"].clone(), Value::Null, json!(minutes), json!(prob), headline);
    }
    let headline = match horizon {
        Some(end) => match ((end - now) as f64 / HOUR_MS as f64).round() as i64 {
            hours if hours > 1 => format!("Dry for the next {hours} hours"),
            _ => "Dry for the next hour".to_string(),
        },
        None if hourly.is_empty() => "Precipitation timing unavailable".to_string(),
        None => "No precipitation expected in the next 12 hours".to_string(),
    };
    next_precip(Value::Null, false, Value::Null, Value::Null, Value::Null, Value::Null, headline)
}

/// The Weather Service says "Sunny" by the clock (6 AM to 6 PM); once the sun is down the
/// same sky is "Clear", and the picture beside the words is a moon.
fn night_words(text: &str) -> String {
    text.replace("Partly Sunny", "Partly Cloudy").replace("Sunny", "Clear")
}

fn build_hourly(nws: Option<&NwsData>, om: Option<&Value>, now: i64) -> Vec<Value> {
    let h = om.and_then(|o| o.get("hourly")).filter(|h| h.get("time").map_or(false, Value::is_array));
    let hour_start = now.div_euclid(HOUR_MS) * HOUR_MS;
    let om_index = |ms: i64| -> Option<usize> {
        let key = ms.div_euclid(HOUR_MS) * 3600;
        h?.get("time")?.as_array()?.iter().position(|t| num(t).map(|n| n as i64) == Some(key))
    };
    let mut out = Vec::new();
    if let Some(n) = nws.filter(|n| !n.hourly.is_empty()) {
        // Forecast rows are the Weather Service's: temperature, words, and the picture that goes
        // with those words. Open-Meteo only adds what the Weather Service does not say.
        for p in &n.hourly {
            let Some(t) = time_of(p, "startTime").filter(|t| near(*t, now) && *t >= hour_start) else { continue };
            let from_om = h.zip(om_index(t));
            let mut is_day = p["isDaytime"].as_bool().unwrap_or(false);
            let mut amount = Value::Null;
            if let Some((h, i)) = from_om {
                is_day = num(&at(h, "is_day", i)).map_or(is_day, |d| d == 1.0);
                amount = opt(num(&at(h, "precipitation", i)));
            }
            let mut words = p["shortForecast"].as_str().unwrap_or("").to_string();
            if !is_day {
                words = night_words(&words);
            }
            let mut icon = icon_from_text(&words);
            if words.is_empty() {
                (icon, words) = from_om.map_or(("cloudy", String::new()), |(h, i)| {
                    let (icon, words) = wmo(code_of(&at(h, "weather_code", i)));
                    (icon, words.to_string())
                });
            }
            let wind = wind_mph(&p["windSpeed"]);
            let humidity = num(&p["relativeHumidity"]["value"]);
            out.push(json!({
                "time": iso(t), "temp": round(&p["temperature"]), "feelsLike": feels(num(&p["temperature"]), num(&wind), humidity),
                "icon": icon, "isDay": is_day, "condition": words,
                "precipProb": opt(num(&p["probabilityOfPrecipitation"]["value"])), "precipAmount": amount,
                "windSpeed": wind, "windDir": p["windDirection"].as_str(),
                "humidity": opt(humidity), "dewPoint": c_to_f(&p["dewpoint"]["value"]),
            }));
            if out.len() >= 48 {
                break;
            }
        }
        return out;
    }
    if let Some(h) = h {
        for (i, t) in h["time"].as_array().into_iter().flatten().enumerate() {
            let Some(ms) = stamp_ms(t, now).filter(|ms| *ms >= hour_start) else { continue };
            let (icon, words) = wmo(code_of(&at(h, "weather_code", i)));
            let humidity = num(&at(h, "relative_humidity_2m", i));
            out.push(json!({
                "time": iso(ms), "temp": round(&at(h, "temperature_2m", i)),
                "feelsLike": feels(num(&at(h, "temperature_2m", i)), num(&at(h, "wind_speed_10m", i)), humidity),
                "icon": icon, "isDay": num(&at(h, "is_day", i)) == Some(1.0), "condition": words,
                "precipProb": opt(num(&at(h, "precipitation_probability", i))), "precipAmount": opt(num(&at(h, "precipitation", i))),
                "windSpeed": round(&at(h, "wind_speed_10m", i)), "windDir": compass(&at(h, "wind_direction_10m", i)),
                "humidity": opt(humidity), "dewPoint": round(&at(h, "dew_point_2m", i)),
            }));
            if out.len() >= 48 {
                break;
            }
        }
    }
    out
}

/// The days ahead, and the Weather Service's high for today (kept so the evening still shows it).
fn build_daily(nws: Option<&NwsData>, om: Option<&Value>, now: i64, offset: i32, previous: &Value) -> (Vec<Value>, Value) {
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
    let find = |days: &[Value], date: &str| days.iter().position(|d| d["date"] == date);
    let mut nws_high = Value::Null;
    let mut night_in_progress = false;

    if let Some(n) = nws {
        for p in &n.daily {
            // NWS start times carry the local offset, so the first 10 characters are the local date.
            let start = p["startTime"].as_str().unwrap_or("");
            let (Some(day), Some(hour)) = (start.get(..10), start.get(11..13).and_then(|h| h.parse::<u32>().ok())) else { continue };
            let is_day = p["isDaytime"].as_bool().unwrap_or(false);
            // After midnight the night still in progress ("Overnight", or last evening's
            // "Tonight") is the rest of this night: its low is the one the hours ahead reach,
            // so it is today's low until morning. Any other period that starts before 6 AM
            // belongs to the evening before.
            let this_night = !is_day
                && match time_of(p, "endTime") {
                    Some(end) => end > now && local_date(end, offset) == today && local(end, offset).hour() <= 12,
                    None => day == today && hour < 6,
                };
            let date = if this_night {
                today.clone()
            } else if !is_day && hour < 6 {
                add_days(day, -1)
            } else {
                day.to_string()
            };
            let Some(i) = find(&days, &date) else { continue };
            let d = &mut days[i];
            if let Some(pop) = num(&p["probabilityOfPrecipitation"]["value"]) {
                d["precipProb"] = json!(num(&d["precipProb"]).unwrap_or(0.0).max(pop));
            }
            let words = p["shortForecast"].as_str().unwrap_or("");
            let detail = p["detailedForecast"].as_str().unwrap_or("");
            if is_day {
                d["high"] = round(&p["temperature"]);
                d["dayText"] = json!(detail);
                if !words.is_empty() {
                    d["condition"] = json!(words);
                    d["icon"] = json!(icon_from_text(words));
                }
                if i == 0 {
                    nws_high = d["high"].clone();
                }
            } else if !(i == 0 && night_in_progress) {
                d["low"] = round(&p["temperature"]);
                d["nightText"] = json!(detail);
                if d["condition"].as_str().unwrap_or("").is_empty() && !words.is_empty() {
                    d["condition"] = json!(words);
                    d["icon"] = json!(icon_from_text(words));
                }
                night_in_progress |= this_night;
            }
        }
    }
    // In the evening the daytime period has left the forecast. Keep the high it gave for
    // today instead of switching to another model's figure for the calendar day.
    if !nws_high.is_number() && previous["nwsHigh"]["date"] == today.as_str() && previous["nwsHigh"]["value"].is_number() {
        nws_high = previous["nwsHigh"]["value"].clone();
        if days[0]["high"].is_null() {
            days[0]["high"] = nws_high.clone();
        }
    }
    if let Some(o) = om.and_then(|o| o.get("daily")).filter(|o| o.get("time").map_or(false, Value::is_array)) {
        let om_offset = om.and_then(|o| num(&o["utc_offset_seconds"])).unwrap_or(0.0).clamp(-86_400.0, 86_400.0) as i64 * 1000;
        for (i, t) in o["time"].as_array().into_iter().flatten().enumerate() {
            let Some(ms) = stamp_ms(t, now) else { continue };
            let Some(di) = find(&days, iso(ms + om_offset).get(..10).unwrap_or("")) else { continue };
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
            // Picture and words always come from the same place: Open-Meteo's only when the
            // Weather Service said nothing about this day.
            if d["condition"].as_str().unwrap_or("").is_empty() {
                let (icon, words) = wmo(code_of(&at(o, "weather_code", i)));
                d["condition"] = json!(words);
                d["icon"] = json!(icon);
            }
            let stamp = |v: Value| stamp_ms(&v, now).filter(|ms| *ms > 0).map(|ms| json!(iso(ms))).unwrap_or(Value::Null);
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
    let kept = if nws_high.is_number() { json!({ "date": today, "value": nws_high }) } else { Value::Null };
    (days, kept)
}

/// How long after the last successful check the alerts shown still count as checked.
const ALERTS_CHECKED_MS: i64 = 20 * 60_000;

fn alerts_checked(at: &Value, now: i64) -> bool {
    at.as_str().and_then(parse_ms).map_or(false, |t| now - t < ALERTS_CHECKED_MS)
}

fn alert_over(alert: &Value, now: i64) -> bool {
    time_of(alert, "ends").or(time_of(alert, "expires")).map_or(false, |end| end <= now)
}

fn build_alerts(features: &[Value], now: i64) -> Vec<Value> {
    let text = |p: &Value, k: &str| p[k].as_str().unwrap_or("").to_string();
    let mut out: Vec<Value> = features
        .iter()
        .map(|f| &f["properties"])
        .filter(|p| p["status"] == "Actual" && p["messageType"] != "Cancel" && !alert_over(p, now))
        .map(|p| {
            let event = if text(p, "event").is_empty() { "Weather Alert".to_string() } else { text(p, "event") };
            let or = |a: String, b: &str| if a.is_empty() { b.to_string() } else { a };
            // An update arrives under a new id and names the messages it takes the place of.
            let replaces: Vec<&str> =
                p["references"].as_array().into_iter().flatten().flat_map(|r| [r["identifier"].as_str(), r["@id"].as_str()]).flatten().take(40).collect();
            json!({
                "id": or(or(text(p, "id"), &text(p, "@id")), &format!("{event}-{}", text(p, "sent"))),
                "event": event,
                "severity": if rank(&p["severity"], &SEVERITY) < SEVERITY.len() { text(p, "severity") } else { "Unknown".to_string() },
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
                "replaces": replaces,
            })
        })
        .collect();
    // Most severe first; among equals, what is happening now before what may happen later.
    out.sort_by_key(|a| (rank(&a["severity"], &SEVERITY), rank(&a["urgency"], &URGENCY), rank(&a["certainty"], &CERTAINTY)));
    out
}

fn build_current(om: Option<&Value>, hourly: &[Value], om_hour: Option<usize>) -> Value {
    let h0 = hourly.first();
    if let Some((om, c)) = om.and_then(|o| Some((o, o.get("current")?))).filter(|(_, c)| num(&c["temperature_2m"]).is_some()) {
        let oh = &om["hourly"];
        let mut visibility = Value::Null;
        if let Some(v) = om_hour.and_then(|i| num(&at(oh, "visibility", i))) {
            let feet = om["hourly_units"]["visibility"].as_str() == Some("ft");
            let miles = round_to(v / if feet { 5280.0 } else { 1609.344 }, 1);
            visibility = json!(if miles > 10.0 { miles.round() } else { miles });
        }
        // The picture and the words beside it describe the same reading.
        let (icon, words) = wmo(code_of(&c["weather_code"]));
        let (icon, condition) = match h0.filter(|_| words.is_empty()) {
            Some(h) => (h["icon"].clone(), h["condition"].clone()),
            None => (json!(icon), json!(words)),
        };
        return json!({
            "temp": round(&c["temperature_2m"]),
            "feelsLike": feels(num(&c["temperature_2m"]), num(&c["wind_speed_10m"]), num(&c["relative_humidity_2m"])),
            "condition": condition, "icon": icon, "isDay": num(&c["is_day"]) == Some(1.0),
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

/// Make the first hourly row say exactly what the current conditions say, and make sure
/// there is a row for the hour we are in.
fn sync_now_row(hourly: &mut Vec<Value>, current: &Value, now: i64) {
    let hour_start = now.div_euclid(HOUR_MS) * HOUR_MS;
    let Some(first) = hourly.first().and_then(|h| time_of(h, "time")) else { return };
    if !current["temp"].is_number() {
        return;
    }
    if first > hour_start {
        hourly.insert(0, json!({ "time": iso(hour_start), "precipProb": null, "precipAmount": null }));
        hourly.truncate(48);
    }
    for key in NOW_KEYS {
        hourly[0][key] = current[key].clone();
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

fn today_of(day: Option<&Value>, summary: &str) -> Value {
    day.map(|d| json!({ "high": d["high"], "low": d["low"], "sunrise": d["sunrise"], "sunset": d["sunset"], "summary": summary })).unwrap_or(Value::Null)
}

/// Build the `WeatherState` for a place. Returns `None` when neither forecast source answered.
/// `loc` is `{ zip, city, state, lat, lon }`. `previous` is the state built last time for the
/// same place (or null): its alerts are kept when the alerts call failed, and so is the high
/// the Weather Service gave for today.
pub fn build_state(loc: &Value, nws: Option<&NwsData>, om: Option<&Value>, now: i64, previous: &Value) -> Option<Value> {
    // The alerts call stands on its own: it can succeed when the forecast calls did not.
    let forecast = nws.filter(|n| !n.daily.is_empty() || !n.hourly.is_empty());
    if forecast.is_none() && om.is_none() {
        return None;
    }
    let offset = utc_offset(forecast, om);
    let time_zone = forecast
        .and_then(|n| n.time_zone.clone())
        .or_else(|| om.and_then(|o| o["timezone"].as_str().map(String::from)))
        .unwrap_or_else(|| "America/Chicago".into());
    let mut hourly = build_hourly(forecast, om, now);
    let (mut daily, nws_high) = build_daily(forecast, om, now, offset, previous);
    let steps = build_minutely(om, now);
    let om_hour = om.and_then(|o| {
        let key = now.div_euclid(HOUR_MS) * 3600;
        o["hourly"]["time"].as_array()?.iter().position(|t| num(t).map(|n| n as i64) == Some(key))
    });
    let current = build_current(om, &hourly, om_hour);
    for h in &mut hourly {
        if let Some(o) = h.as_object_mut() {
            o.remove("dewPoint");
        }
    }
    sync_now_row(&mut hourly, &current, now);
    let next = build_next_precip(&steps, &hourly, now, offset);

    // Today's range always contains the temperature it is right now.
    let today_date = local_date(now, offset);
    let today_row = daily.iter().position(|d| d["date"] == today_date.as_str());
    if let (Some(i), Some(temp)) = (today_row, num(&current["temp"])) {
        if num(&daily[i]["high"]).map_or(false, |high| temp > high) {
            daily[i]["high"] = current["temp"].clone();
        }
        if num(&daily[i]["low"]).map_or(false, |low| temp < low) {
            daily[i]["low"] = current["temp"].clone();
        }
    }
    let summary = forecast.and_then(|n| n.daily.first()).and_then(|p| p["detailedForecast"].as_str()).unwrap_or("");
    let today = today_of(today_row.map(|i| &daily[i]), summary);

    let mut missing = Vec::new();
    if forecast.is_none() {
        missing.push("National Weather Service");
    }
    if om.is_none() {
        missing.push("Open-Meteo");
    }
    let mut location = loc.clone();
    if let Some(o) = location.as_object_mut() {
        o.insert("timeZone".into(), json!(time_zone));
    }
    // When the alerts call failed, what was known is kept (minus what has ended since) together
    // with the time it was learned, so the windows can say when alerts could not be checked.
    let (alerts, alerts_at) = match nws.and_then(|n| n.alerts.as_ref()) {
        Some(features) => (build_alerts(features, now), json!(iso(now))),
        None => (previous["alerts"].as_array().into_iter().flatten().filter(|a| !alert_over(a, now)).cloned().collect(), previous["alertsAt"].clone()),
    };
    Some(json!({
        "status": "ok",
        "error": if missing.is_empty() { Value::Null } else { json!(format!("{} did not respond. Showing what is available.", missing.join(" and "))) },
        "stale": false,
        "updatedAt": iso(now),
        "utcOffset": offset,
        "sources": { "nws": forecast.is_some(), "openMeteo": om.is_some(), "alerts": alerts_checked(&alerts_at, now) },
        "location": location,
        "current": current,
        "today": today,
        "nextPrecip": next,
        "minutely": steps_json(&steps),
        "hourly": hourly,
        "daily": daily,
        "alerts": alerts,
        "alertsAt": alerts_at,
        "nwsHigh": nws_high,
    }))
}

/// The same state with the answer of an alerts-only check put in: alerts are asked for more
/// often than the forecast, because a warning should not wait for the next refresh.
pub fn with_alerts(state: &Value, features: &[Value], now: i64) -> Value {
    let mut next = state.clone();
    if next["status"] == "ok" {
        next["alerts"] = json!(build_alerts(features, now));
        next["alertsAt"] = json!(iso(now));
        next["sources"]["alerts"] = json!(true);
    }
    next
}

/// Bring a state built earlier up to `now` without new data, so the display follows the clock
/// and not the last fetch: hours that have passed are dropped, the "Now" row and the rain
/// heads-up are worked out again, alerts that have ended are removed and the days roll over at
/// local midnight. Always called on the state as it was built, never on its own result.
/// Returns `None` when nothing needs to change.
pub fn advance(state: &Value, now: i64) -> Option<Value> {
    if state["status"] != "ok" || !state["current"].is_object() {
        return None;
    }
    let offset = num(&state["utcOffset"])? as i32;
    let list = |key: &str| state[key].as_array().cloned().unwrap_or_default();
    let mut next = state.clone();

    let mut hourly = list("hourly");
    hourly.retain(|h| time_of(h, "time").map_or(false, |t| t + HOUR_MS > now));
    let mut current = state["current"].clone();
    if now - time_of(state, "updatedAt").unwrap_or(now) >= 90 * 60_000 {
        // No news for a good while: the forecast for this hour is a better account of "now"
        // than a reading from hours ago (and it knows whether the sun is up).
        if let Some(h) = hourly.first().filter(|h| time_of(h, "time").map_or(false, |t| t <= now)) {
            for key in NOW_KEYS {
                current[key] = h[key].clone();
            }
            for key in READING_KEYS {
                current[key] = Value::Null;
            }
        }
    }
    sync_now_row(&mut hourly, &current, now);

    let mut steps = steps_of(state);
    steps.retain(|s| s.ms + STEP_MS > now);
    let mut alerts = list("alerts");
    alerts.retain(|a| !alert_over(a, now));

    let today = local_date(now, offset);
    let mut daily = list("daily");
    daily.retain(|d| d["date"].as_str().map_or(false, |date| date >= today.as_str()));
    if state["daily"][0]["date"] != today.as_str() {
        let row = daily.first_mut().filter(|d| d["date"] == today.as_str());
        next["today"] = today_of(row.as_deref(), "");
        if let Some(d) = row {
            let text = |k: &str| d[k].as_str().filter(|s| !s.is_empty()).map(String::from);
            next["today"]["summary"] = json!(text("dayText").or(text("nightText")).unwrap_or_default());
            d["name"] = json!("Today");
        }
    }

    next["nextPrecip"] = build_next_precip(&steps, &hourly, now, offset);
    next["minutely"] = steps_json(&steps);
    next["current"] = current;
    next["hourly"] = json!(hourly);
    next["daily"] = json!(daily);
    next["alerts"] = json!(alerts);
    next["sources"]["alerts"] = json!(alerts_checked(&state["alertsAt"], now));
    (next != *state).then_some(next)
}

/// Which of `alerts` deserve a notification, and the record to keep so that none is announced
/// twice. `seen` is that record from last time: `{ id, event, severity }` entries (plain id
/// strings written by earlier versions are understood). The Weather Service re-issues an alert
/// under a new id whenever it updates it; a re-issue is only news when the event has a new
/// name or has become more severe.
pub fn alerts_to_announce(alerts: &[Value], seen: &[Value]) -> (Vec<Value>, Vec<Value>) {
    let id_of = |s: &Value| s.as_str().or(s["id"].as_str()).map(String::from);
    let known = |id: &str| seen.iter().find(|s| id_of(s).as_deref() == Some(id));
    let mut news = Vec::new();
    let mut record: Vec<Value> = Vec::new();
    for a in alerts {
        let Some(id) = a["id"].as_str() else { continue };
        if known(id).is_none() {
            let mut earlier = a["replaces"].as_array().into_iter().flatten().filter_map(|r| known(r.as_str()?));
            if earlier.all(|e| e["event"].is_string() && (e["event"] != a["event"] || rank(&a["severity"], &SEVERITY) < rank(&e["severity"], &SEVERITY))) {
                news.push(a.clone());
            }
        }
        record.push(json!({ "id": id, "event": a["event"], "severity": a["severity"] }));
    }
    // Earlier entries stay (another place's alerts, and the ones the next re-issue will name),
    // behind the current ones, without growing for ever.
    for s in seen {
        if let Some(id) = id_of(s).filter(|id| !record.iter().any(|r| r["id"] == id.as_str())) {
            record.push(if s.is_object() { s.clone() } else { json!({ "id": id }) });
        }
    }
    record.truncate(80);
    (news, record)
}

/// Title and text of the notification for an alert:
/// "Winter Storm Warning" / "Until Wed 6 AM · Springfield, IL".
pub fn alert_notice(alert: &Value, place: &str, now: i64, offset: i32) -> (String, String) {
    let when = |ms: i64| {
        let day = if local_date(ms, offset) == local_date(now, offset) { String::new() } else { format!("{} ", weekday_of(ms, offset)) };
        format!("{day}{}", clock(ms, offset))
    };
    let onset = time_of(alert, "onset").filter(|t| *t > now + 10 * 60_000);
    let ends = time_of(alert, "ends").filter(|t| *t > now);
    let timing = match (onset, ends) {
        (Some(o), Some(e)) => format!("From {} until {}", when(o), when(e)),
        (Some(o), None) => format!("From {}", when(o)),
        (None, Some(e)) => format!("Until {}", when(e)),
        (None, None) => "In effect now".to_string(),
    };
    let title = alert["event"].as_str().filter(|s| !s.is_empty()).unwrap_or("Weather Alert").to_string();
    (title, if place.is_empty() { timing } else { format!("{timing} · {place}") })
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
        "sources": { "nws": false, "openMeteo": false, "alerts": false },
        "location": location, "current": null, "today": null,
        "nextPrecip": { "kind": null, "isNow": false, "startsAt": null, "endsAt": null, "minutesUntil": null, "chance": null, "headline": "" },
        "minutely": [], "hourly": [], "daily": [], "alerts": [],
    })
}
