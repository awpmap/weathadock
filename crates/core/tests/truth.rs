//! What the app shows must never contradict itself, must follow the clock, and must survive
//! whatever the services send. Each test here is one of those promises.
use serde_json::{json, Value};
use weathadock_core::weather::{advance, alert_notice, alerts_to_announce, build_state, feels_like, icon_from_text, NwsData};

const HOUR: i64 = 3_600_000;
const OFFSET: i64 = -6 * HOUR; // Central Standard Time
/// Wednesday 18 November 2026, 2:07 PM in Chicago.
const NOW: i64 = 1_795_032_420_000;

fn place() -> Value {
    json!({ "zip": "60601", "city": "Chicago", "state": "IL", "lat": 41.8858, "lon": -87.6181 })
}

/// "2026-11-18T14:00:00-06:00" for a time in milliseconds.
fn local_iso(ms: i64) -> String {
    let s = chrono::DateTime::from_timestamp_millis(ms + OFFSET).unwrap().format("%Y-%m-%dT%H:%M:%S").to_string();
    format!("{s}-06:00")
}

fn hour_start(ms: i64) -> i64 {
    ms.div_euclid(HOUR) * HOUR
}

/// An Open-Meteo answer: 55°F and clear now, 72 dry hours, 48 dry 15-minute steps.
fn open_meteo(now: i64) -> Value {
    let h0 = hour_start(now) / 1000;
    let hours: Vec<i64> = (0..72).map(|i| h0 + i * 3600).collect();
    let steps: Vec<i64> = (0..48).map(|i| now.div_euclid(900_000) * 900 + i * 900).collect();
    let midnight = ((now + OFFSET).div_euclid(24 * HOUR) * 24 * HOUR - OFFSET) / 1000;
    let days: Vec<i64> = (0..8).map(|i| midnight + i * 86_400).collect();
    json!({
        "timezone": "America/Chicago", "utc_offset_seconds": -21600, "hourly_units": { "visibility": "ft" },
        "current": { "temperature_2m": 55.2, "relative_humidity_2m": 40, "is_day": 1, "precipitation": 0, "weather_code": 0,
            "pressure_msl": 1016, "wind_speed_10m": 6.0, "wind_direction_10m": 270, "wind_gusts_10m": 12 },
        "minutely_15": { "time": steps, "precipitation": vec![0.0; 48], "snowfall": vec![0.0; 48], "weather_code": vec![0; 48] },
        "hourly": { "time": hours, "temperature_2m": vec![54.0; 72], "relative_humidity_2m": vec![40; 72], "dew_point_2m": vec![31.0; 72],
            "precipitation_probability": vec![0; 72], "precipitation": vec![0.0; 72], "weather_code": vec![0; 72],
            "wind_speed_10m": vec![6.0; 72], "wind_direction_10m": vec![270; 72], "visibility": vec![52800; 72], "uv_index": vec![2.0; 72],
            "is_day": (0..72).map(|i| { let h = ((h0 + i * 3600) * 1000 + OFFSET).div_euclid(HOUR).rem_euclid(24); i64::from((7..17).contains(&h)) }).collect::<Vec<_>>() },
        "daily": { "time": days, "weather_code": vec![0; 8], "temperature_2m_max": vec![57.0; 8], "temperature_2m_min": vec![39.0; 8],
            "sunrise": days.iter().map(|d| d + 7 * 3600).collect::<Vec<_>>(), "sunset": days.iter().map(|d| d + 17 * 3600).collect::<Vec<_>>(),
            "precipitation_probability_max": vec![0; 8] },
    })
}

fn period(start: i64, hours: i64, day: bool, temp: i64, words: &str) -> Value {
    json!({ "startTime": local_iso(start), "endTime": local_iso(start + hours * HOUR), "isDaytime": day, "temperature": temp,
        "probabilityOfPrecipitation": { "value": 10 }, "shortForecast": words,
        "detailedForecast": format!("{words}, with a {} around {temp}.", if day { "high" } else { "low" }) })
}

/// 48 NWS hourly rows from the hour of `now`, 54°F, the given words and chance of precipitation.
fn nws_hours(now: i64, words: &str, chance: i64) -> Vec<Value> {
    (0..48)
        .map(|i| json!({ "startTime": local_iso(hour_start(now) + i * HOUR), "isDaytime": true, "temperature": 54, "windSpeed": "5 mph",
            "windDirection": "W", "shortForecast": words, "probabilityOfPrecipitation": { "value": chance }, "relativeHumidity": { "value": 45 }, "dewpoint": { "value": 0 } }))
        .collect()
}

/// A Weather Service answer at 2 PM: this afternoon, tonight, tomorrow and tomorrow night.
fn nws(now: i64) -> NwsData {
    let two_pm = hour_start(now);
    NwsData {
        time_zone: Some("America/Chicago".into()),
        daily: vec![
            period(two_pm, 4, true, 57, "Sunny"),
            period(two_pm + 4 * HOUR, 12, false, 41, "Mostly Clear"),
            period(two_pm + 16 * HOUR, 12, true, 60, "Partly Sunny"),
            period(two_pm + 28 * HOUR, 12, false, 44, "Mostly Cloudy"),
        ],
        hourly: nws_hours(now, "Sunny", 5),
        alerts: Some(vec![]),
    }
}

fn alert(id: &str, event: &str, severity: &str, ends: i64) -> Value {
    json!({ "properties": { "id": id, "status": "Actual", "messageType": "Alert", "event": event, "severity": severity, "urgency": "Expected",
        "certainty": "Likely", "headline": format!("{event} in effect"), "onset": local_iso(NOW - HOUR), "ends": local_iso(ends), "expires": local_iso(ends) } })
}

fn state(nws: Option<&NwsData>, om: Option<&Value>, now: i64) -> Value {
    build_state(&place(), nws, om, now, &Value::Null).expect("a state")
}

fn assert_now_row_matches(s: &Value) {
    for key in ["temp", "feelsLike", "icon", "isDay", "condition", "windSpeed", "windDir", "humidity"] {
        assert_eq!(s["hourly"][0][key], s["current"][key], "the Now column and the current conditions disagree on {key}");
    }
}

// ---------------------------------------------------------------- one source per thing shown

#[test]
fn the_now_column_says_what_the_current_conditions_say() {
    let (n, o) = (nws(NOW), open_meteo(NOW));
    for s in [state(Some(&n), Some(&o), NOW), state(None, Some(&o), NOW), state(Some(&n), None, NOW)] {
        assert_now_row_matches(&s);
        assert_eq!(s["hourly"][0]["time"], "2026-11-18T20:00:00.000Z");
    }
}

#[test]
fn the_current_picture_and_words_come_from_the_same_reading() {
    let (n, mut o) = (nws(NOW), open_meteo(NOW));
    o["current"]["weather_code"] = json!(3);
    let s = state(Some(&n), Some(&o), NOW);
    assert_eq!((s["current"]["icon"].as_str(), s["current"]["condition"].as_str()), (Some("cloudy"), Some("Cloudy")));
    // A code with no words: fall back to the forecast for this hour for both, not for one.
    o["current"]["weather_code"] = json!(4);
    let s = state(Some(&n), Some(&o), NOW);
    assert_eq!((s["current"]["icon"].as_str(), s["current"]["condition"].as_str()), (Some("clear"), Some("Sunny")));
}

#[test]
fn forecast_pictures_follow_the_forecast_words() {
    let (mut n, mut o) = (nws(NOW), open_meteo(NOW));
    // Open-Meteo calls every hour and day rain; the Weather Service's words say otherwise.
    o["hourly"]["weather_code"] = json!(vec![63; 72]);
    o["daily"]["weather_code"] = json!(vec![63; 8]);
    n.hourly[3]["shortForecast"] = json!("Mostly Cloudy");
    let s = state(Some(&n), Some(&o), NOW);
    assert_eq!((s["hourly"][3]["icon"].as_str(), s["hourly"][3]["condition"].as_str()), (Some("cloudy"), Some("Mostly Cloudy")));
    assert_eq!((s["daily"][0]["icon"].as_str(), s["daily"][0]["condition"].as_str()), (Some("clear"), Some("Sunny")));
    assert_eq!((s["daily"][1]["icon"].as_str(), s["daily"][1]["condition"].as_str()), (Some("partly-cloudy"), Some("Partly Sunny")));
    // Days the Weather Service does not cover take both from Open-Meteo.
    let last = s["daily"].as_array().unwrap().last().unwrap();
    assert_eq!((last["icon"].as_str(), last["condition"].as_str()), (Some("rain"), Some("Rain")));
}

#[test]
fn it_is_not_sunny_after_sunset() {
    let (n, o) = (nws(NOW), open_meteo(NOW)); // every NWS hour says "Sunny"; the sun sets at 5 PM
    let s = state(Some(&n), Some(&o), NOW);
    let five_pm = &s["hourly"][3];
    assert_eq!((five_pm["isDay"].as_bool(), five_pm["condition"].as_str(), five_pm["icon"].as_str()), (Some(false), Some("Clear"), Some("clear")));
    assert_eq!(s["hourly"][2]["condition"], "Sunny");
}

#[test]
fn pictures_for_weather_service_wording() {
    for (words, icon) in [
        ("Sunny", "clear"), ("Clear", "clear"), ("Mostly Sunny", "mostly-clear"), ("Mostly Clear", "mostly-clear"),
        ("Partly Sunny", "partly-cloudy"), ("Partly Cloudy", "partly-cloudy"), ("Mostly Cloudy", "cloudy"), ("Cloudy", "cloudy"),
        ("Increasing Clouds", "partly-cloudy"), ("Gradual Clearing", "partly-cloudy"), ("Patchy Fog", "fog"), ("Haze", "fog"),
        ("Breezy", "wind"), ("Sunny And Breezy", "clear"), ("Patchy Drizzle", "drizzle"), ("Light Rain", "rain"),
        ("Scattered Showers", "rain"), ("Heavy Rain", "heavy-rain"), ("Light Snow Likely", "snow"), ("Blowing Snow", "snow"),
        ("Rain And Snow", "sleet"), ("Freezing Rain", "sleet"), ("Wintry Mix", "sleet"), ("Showers And Thunderstorms Likely", "thunderstorm"),
        ("Slight Chance Rain Showers", "rain"), ("Isolated Showers", "rain"), ("Hot", "clear"), ("", "clear"),
        // Two parts: the more telling one stands for the period...
        ("Sunny then Chance Showers", "rain"), ("Chance Rain then Mostly Cloudy", "rain"), ("Patchy Fog then Mostly Sunny", "fog"),
        ("Rain Likely then Chance Snow", "snow"),
        // ...but a slight chance never outweighs what the rest of the period is like.
        ("Mostly Clear then Slight Chance Rain And Snow", "mostly-clear"), ("Mostly Sunny then Isolated Showers", "mostly-clear"),
        ("Slight Chance Showers then Mostly Sunny", "mostly-clear"), ("Rain Likely then Slight Chance Snow", "rain"),
        ("Slight Chance Snow then Slight Chance Rain", "snow"),
    ] {
        assert_eq!(icon_from_text(words), icon, "{words:?}");
    }
}

#[test]
fn feels_like_the_way_the_weather_service_reports_it() {
    let f = |t: f64, wind: f64, rh: f64| feels_like(t, Some(wind), Some(rh)).round() as i64;
    // Wind chill chart values.
    assert_eq!((f(30.0, 10.0, 50.0), f(0.0, 15.0, 50.0), f(40.0, 5.0, 50.0), f(-10.0, 30.0, 50.0)), (21, -19, 36, -39));
    // Calm air, or mild air, feels like what it is.
    assert_eq!((f(30.0, 3.0, 50.0), f(65.0, 25.0, 90.0), f(79.0, 0.0, 100.0)), (30, 65, 79));
    // Heat index chart values.
    assert_eq!((f(80.0, 0.0, 40.0), f(90.0, 5.0, 70.0), f(100.0, 5.0, 55.0)), (80, 106, 124));
    assert!(feels_like(95.0, None, Some(10.0)) < 95.0, "very dry heat feels a little cooler");
    assert_eq!(feels_like(20.0, None, None), 20.0);
    for (t, w, h) in [(f64::MAX, 1e9, 500.0), (-459.0, 300.0, -5.0), (120.0, 0.0, 100.0)] {
        assert!(feels_like(t, Some(w), Some(h)).is_finite());
    }
}

#[test]
fn todays_range_contains_the_temperature_right_now() {
    let (n, mut o) = (nws(NOW), open_meteo(NOW));
    o["current"]["temperature_2m"] = json!(61.4); // warmer than the forecast high of 57
    let s = state(Some(&n), Some(&o), NOW);
    assert_eq!((s["current"]["temp"].as_i64(), s["today"]["high"].as_i64(), s["daily"][0]["high"].as_i64()), (Some(61), Some(61), Some(61)));
    assert_eq!(s["nwsHigh"], json!({ "date": "2026-11-18", "value": 57 }));
    o["current"]["temperature_2m"] = json!(38.0); // colder than tonight's forecast low of 41
    assert_eq!(state(Some(&n), Some(&o), NOW)["today"]["low"], 38);
}

// ---------------------------------------------------------------- the rain heads-up

#[test]
fn never_dry_beside_an_hour_the_forecast_calls_likely_wet() {
    let (mut n, o) = (nws(NOW), open_meteo(NOW));
    let dry = state(Some(&n), Some(&o), NOW);
    assert_eq!(dry["nextPrecip"]["headline"], "Dry for the next 12 hours");
    for i in 3..6 {
        n.hourly[i]["probabilityOfPrecipitation"]["value"] = json!(70);
        n.hourly[i]["shortForecast"] = json!("Rain Likely");
    }
    let np = &state(Some(&n), Some(&o), NOW)["nextPrecip"];
    assert_eq!(np["headline"], "Rain likely around 5 PM (70%)");
    assert_eq!((np["kind"].as_str(), np["chance"].as_i64(), np["isNow"].as_bool()), (Some("rain"), Some(70), Some(false)));
}

#[test]
fn a_lone_trace_of_rain_is_not_announced() {
    let (n, mut o) = (nws(NOW), open_meteo(NOW));
    o["minutely_15"]["precipitation"][5] = json!(0.006);
    o["minutely_15"]["precipitation"][20] = json!(0.002);
    let s = state(Some(&n), Some(&o), NOW);
    assert_eq!(s["nextPrecip"]["headline"], "Dry for the next 12 hours");
    assert!(s["minutely"].as_array().unwrap().iter().all(|m| m["precip"] == 0.0 && m["kind"].is_null()), "no bar may be drawn under a dry headline");
    // Two wet steps, or one with real rain, are weather.
    o["minutely_15"]["precipitation"][6] = json!(0.006);
    assert_eq!(state(Some(&n), Some(&o), NOW)["nextPrecip"]["kind"], "rain");
    o["minutely_15"]["precipitation"][6] = json!(0.0);
    o["minutely_15"]["precipitation"][5] = json!(0.04);
    assert_eq!(state(Some(&n), Some(&o), NOW)["nextPrecip"]["headline"], "Rain around 3:15 PM");
}

#[test]
fn one_hour_is_singular() {
    let (n, mut o) = (nws(NOW), open_meteo(NOW));
    for key in ["time", "precipitation", "snowfall", "weather_code"] {
        o["minutely_15"][key].as_array_mut().unwrap().truncate(4);
    }
    assert_eq!(state(Some(&n), Some(&o), NOW)["nextPrecip"]["headline"], "Dry for the next hour");
}

// ---------------------------------------------------------------- today's high and low

#[test]
fn after_midnight_todays_low_is_the_low_of_the_night_in_progress() {
    let now = NOW + 11 * HOUR; // 1:07 AM on Thursday
    let one_am = hour_start(now);
    let days = |first: Value| NwsData {
        daily: vec![first, period(one_am + 5 * HOUR, 12, true, 60, "Sunny"), period(one_am + 17 * HOUR, 12, false, 46, "Mostly Cloudy")],
        hourly: nws_hours(now, "Mostly Clear", 5),
        ..nws(now)
    };
    let o = open_meteo(now);
    // Whether the service lists it as "Overnight" or still as last evening's "Tonight".
    for first in [period(one_am, 5, false, 42, "Mostly Clear"), period(one_am - 7 * HOUR, 12, false, 42, "Mostly Clear")] {
        let s = state(Some(&days(first)), Some(&o), now);
        assert_eq!(s["daily"][0]["date"], "2026-11-19");
        assert_eq!((s["today"]["low"].as_i64(), s["today"]["high"].as_i64()), (Some(42), Some(60)));
        assert_eq!(s["daily"][0]["nightText"], "Mostly Clear, with a low around 42.");
        assert_eq!(s["today"]["summary"], "Mostly Clear, with a low around 42.");
    }
    // Without end times the start hour decides.
    let mut n = days(period(one_am, 5, false, 42, "Mostly Clear"));
    for p in &mut n.daily {
        p.as_object_mut().unwrap().remove("endTime");
    }
    assert_eq!(state(Some(&n), Some(&o), now)["today"]["low"], 42);
}

#[test]
fn the_evening_keeps_the_high_the_day_was_given() {
    let (n, o) = (nws(NOW), open_meteo(NOW));
    let afternoon = state(Some(&n), Some(&o), NOW);
    assert_eq!(afternoon["today"]["high"], 57);
    // 7:07 PM: the daytime period has left the forecast, and Open-Meteo's figure for the day differs.
    let evening = NOW + 5 * HOUR;
    let mut later = nws(evening);
    later.daily.remove(0);
    later.daily[0] = period(hour_start(NOW) + 4 * HOUR, 12, false, 41, "Mostly Clear");
    let mut om = open_meteo(evening);
    om["daily"]["temperature_2m_max"] = json!(vec![52.0; 8]);
    om["current"]["temperature_2m"] = json!(45.0);
    let with_memory = build_state(&place(), Some(&later), Some(&om), evening, &afternoon).unwrap();
    assert_eq!((with_memory["today"]["high"].as_i64(), with_memory["today"]["low"].as_i64()), (Some(57), Some(41)));
    assert_eq!(with_memory["nwsHigh"], afternoon["nwsHigh"], "and it is remembered for the rest of the evening");
    assert_eq!(build_state(&place(), Some(&later), Some(&om), evening, &Value::Null).unwrap()["today"]["high"], 52);
    // The next day it is forgotten.
    let tomorrow = NOW + 29 * HOUR;
    let s = build_state(&place(), None, Some(&open_meteo(tomorrow)), tomorrow, &afternoon).unwrap();
    assert_eq!(s["today"]["high"], 57); // Open-Meteo's own figure, not a memory
    assert!(s["nwsHigh"].is_null());
}

// ---------------------------------------------------------------- following the clock

#[test]
fn a_state_just_built_needs_no_advancing() {
    let (mut n, mut o) = (nws(NOW), open_meteo(NOW));
    n.alerts = Some(vec![alert("a1", "Wind Advisory", "Minor", NOW + 3 * HOUR)]);
    o["minutely_15"]["precipitation"][9] = json!(0.05);
    o["minutely_15"]["precipitation"][10] = json!(0.05);
    for s in [state(Some(&n), Some(&o), NOW), state(None, Some(&o), NOW), state(Some(&n), None, NOW)] {
        assert_eq!(advance(&s, NOW), None);
    }
}

#[test]
fn the_display_follows_the_clock_without_new_data() {
    let (mut n, mut o) = (nws(NOW), open_meteo(NOW));
    n.alerts = Some(vec![alert("short", "Dense Fog Advisory", "Minor", NOW + 2 * HOUR), alert("long", "Wind Advisory", "Minor", NOW + 9 * HOUR)]);
    for i in 0..48 {
        n.hourly[i]["temperature"] = json!(54 - i as i64);
    }
    o["minutely_15"]["precipitation"][14] = json!(0.05); // rain from 5:30 PM
    o["minutely_15"]["precipitation"][15] = json!(0.05);
    let built = state(Some(&n), Some(&o), NOW);
    assert_eq!(built["nextPrecip"]["headline"], "Rain around 5:30 PM");

    // 40 minutes on: the 2 PM hour is still the present one and the reading still stands.
    let s = advance(&built, NOW + 40 * 60_000).expect("the countdown moved");
    assert_eq!((s["hourly"][0]["time"].as_str(), s["current"]["temp"].as_i64()), (Some("2026-11-18T20:00:00.000Z"), Some(55)));
    assert_eq!(s["nextPrecip"]["headline"], "Rain around 5:30 PM");
    assert_now_row_matches(&s);

    // 3 hours on: 5:07 PM. Passed hours are gone, "Now" is the 5 PM forecast, the sun is down.
    let later = NOW + 3 * HOUR;
    let s = advance(&built, later).unwrap();
    assert_eq!(s["hourly"][0]["time"], "2026-11-18T23:00:00.000Z");
    assert_eq!((s["current"]["temp"].as_i64(), s["current"]["isDay"].as_bool(), s["current"]["condition"].as_str()), (Some(51), Some(false), Some("Clear")));
    assert!(s["current"]["uvIndex"].is_null() && s["current"]["pressure"].is_null(), "old readings are not passed off as current");
    assert_now_row_matches(&s);
    assert_eq!(s["hourly"].as_array().unwrap().len(), 45);
    assert_eq!(s["nextPrecip"]["headline"], "Rain starting in 23 min");
    assert!(s["minutely"].as_array().unwrap().iter().all(|m| m["time"].as_str().unwrap() >= "2026-11-18T23:00:00.000Z"));
    assert_eq!(s["alerts"].as_array().unwrap().len(), 1, "the advisory that ended at 4:07 PM is gone");
    assert_eq!(s["alerts"][0]["id"], "long");
    assert_eq!((s["updatedAt"].clone(), s["stale"].clone()), (built["updatedAt"].clone(), built["stale"].clone()));

    // Past midnight: Thursday is "Today".
    let s = advance(&built, NOW + 11 * HOUR).unwrap();
    assert_eq!((s["daily"][0]["date"].as_str(), s["daily"][0]["name"].as_str()), (Some("2026-11-19"), Some("Today")));
    assert_eq!((s["today"]["high"].as_i64(), s["today"]["low"].as_i64()), (Some(60), Some(44)));
    assert_eq!(s["today"]["summary"], "Partly Sunny, with a high around 60.");
    assert!(s["alerts"].as_array().unwrap().is_empty());

    // Days later nothing is left to show as a forecast, and nothing breaks.
    let s = advance(&built, NOW + 30 * 24 * HOUR).unwrap();
    assert!(s["hourly"].as_array().unwrap().is_empty() && s["daily"].as_array().unwrap().is_empty() && s["today"].is_null());
}

#[test]
fn only_a_real_forecast_is_advanced() {
    let loading = weathadock_core::weather::empty_state("loading", None, Some(&place()));
    assert_eq!(advance(&loading, NOW), None);
    assert_eq!(advance(&json!({ "status": "ok", "current": {} }), NOW), None, "states cached by earlier versions have no offset");
    assert_eq!(advance(&Value::Null, NOW), None);
}

// ---------------------------------------------------------------- alerts

#[test]
fn alerts_are_ordered_and_say_when_they_could_not_be_checked() {
    let (mut n, o) = (nws(NOW), open_meteo(NOW));
    let mut watch = alert("w", "Tornado Watch", "Severe", NOW + 5 * HOUR);
    watch["properties"]["urgency"] = json!("Future");
    let mut over = alert("old", "Flood Advisory", "Minor", NOW - 60_000);
    over["properties"]["references"] = json!([{ "identifier": "older" }]);
    let mut update = alert("u2", "Tornado Warning", "Severe", NOW + HOUR);
    update["properties"]["urgency"] = json!("Immediate");
    update["properties"]["references"] = json!([{ "identifier": "u1", "@id": "https://api.weather.gov/alerts/u1" }]);
    n.alerts = Some(vec![alert("m", "Wind Advisory", "Minor", NOW + 5 * HOUR), watch, over, update]);
    let s = state(Some(&n), Some(&o), NOW);
    let ids: Vec<&str> = s["alerts"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["u2", "w", "m"], "most severe first, happening now before possible later, ended ones dropped");
    assert_eq!(s["alerts"][0]["replaces"], json!(["u1", "https://api.weather.gov/alerts/u1"]));
    assert_eq!(s["sources"], json!({ "nws": true, "openMeteo": true, "alerts": true }));

    // The alerts call failed: keep what was known, minus what has ended since, and say so.
    let later = NOW + 2 * HOUR;
    let without_alerts = NwsData { alerts: None, ..nws(later) };
    let kept = build_state(&place(), Some(&without_alerts), Some(&open_meteo(later)), later, &s).unwrap();
    assert_eq!(kept["alerts"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap()).collect::<Vec<_>>(), ["w", "m"]);
    assert_eq!(kept["sources"]["alerts"], false);

    // The forecast calls failed but the alerts call worked: alerts still arrive.
    let only_alerts = NwsData { time_zone: None, daily: vec![], hourly: vec![], alerts: Some(vec![alert("x", "Wind Advisory", "Minor", NOW + HOUR)]) };
    let s = state(Some(&only_alerts), Some(&o), NOW);
    assert_eq!(s["sources"], json!({ "nws": false, "openMeteo": true, "alerts": true }));
    assert_eq!(s["alerts"][0]["id"], "x");
    assert_eq!(s["error"], "National Weather Service did not respond. Showing what is available.");
    assert!(build_state(&place(), Some(&only_alerts), None, NOW, &Value::Null).is_none(), "alerts alone are not a forecast");
}

#[test]
fn an_alert_is_announced_once_however_often_it_is_reissued() {
    let a = |id: &str, event: &str, severity: &str, replaces: Value| json!({ "id": id, "event": event, "severity": severity, "replaces": replaces });
    let ids = |list: &[Value]| list.iter().map(|a| a["id"].as_str().unwrap().to_string()).collect::<Vec<_>>();

    let first = [a("w1", "Winter Storm Watch", "Moderate", json!([]))];
    let (news, seen) = alerts_to_announce(&first, &[]);
    assert_eq!(ids(&news), ["w1"]);
    assert_eq!(seen, [json!({ "id": "w1", "event": "Winter Storm Watch", "severity": "Moderate" })]);
    assert!(alerts_to_announce(&first, &seen).0.is_empty(), "the same alert again");

    // Re-issued under a new id with nothing new to say.
    let (news, seen) = alerts_to_announce(&[a("w2", "Winter Storm Watch", "Moderate", json!(["w1"]))], &seen);
    assert!(news.is_empty());
    assert_eq!(ids(&seen), ["w2", "w1"]);
    // Upgraded to a warning: news. More severe under the same name: news.
    let (news, seen) = alerts_to_announce(&[a("w3", "Winter Storm Warning", "Moderate", json!(["w1", "w2"]))], &seen);
    assert_eq!(ids(&news), ["w3"]);
    let (news, seen) = alerts_to_announce(&[a("w4", "Winter Storm Warning", "Severe", json!(["w1", "w2", "w3"]))], &seen);
    assert_eq!(ids(&news), ["w4"]);
    // Downgraded: not worth a notification.
    let (news, seen) = alerts_to_announce(&[a("w5", "Winter Storm Warning", "Moderate", json!(["w4"]))], &seen);
    assert!(news.is_empty());
    // An unrelated alert is always news, and another place's alerts are not forgotten meanwhile.
    let (news, seen) = alerts_to_announce(&[a("f1", "Flood Warning", "Severe", json!([]))], &seen);
    assert_eq!(ids(&news), ["f1"]);
    assert!(alerts_to_announce(&[a("w5", "Winter Storm Warning", "Moderate", json!(["w4"]))], &seen).0.is_empty());

    // Records written by earlier versions were plain ids.
    let old = [json!("w1")];
    assert!(alerts_to_announce(&first, &old).0.is_empty());
    assert!(alerts_to_announce(&[a("w2", "Winter Storm Watch", "Moderate", json!(["w1"]))], &old).0.is_empty());
    // The record does not grow for ever.
    let many: Vec<Value> = (0..200).map(|i| json!({ "id": format!("x{i}"), "event": "E", "severity": "Minor" })).collect();
    assert_eq!(alerts_to_announce(&first, &many).1.len(), 80);
}

#[test]
fn a_notification_says_what_and_until_when() {
    let a = json!({ "event": "Winter Storm Warning", "onset": local_iso(NOW - HOUR), "ends": local_iso(hour_start(NOW) + 16 * HOUR), "expires": local_iso(NOW + 2 * HOUR) });
    assert_eq!(alert_notice(&a, "Chicago, IL", NOW, -21600), ("Winter Storm Warning".into(), "Until Thu 6 AM · Chicago, IL".into()));
    let later = json!({ "event": "Wind Advisory", "onset": local_iso(hour_start(NOW) + 4 * HOUR), "ends": local_iso(hour_start(NOW) + 8 * HOUR) });
    assert_eq!(alert_notice(&later, "Chicago, IL", NOW, -21600).1, "From 6 PM until 10 PM · Chicago, IL");
    // No stated end: do not present the message's own expiry as the end of the weather.
    let open = json!({ "event": "Flood Warning", "onset": local_iso(NOW - HOUR), "ends": null, "expires": local_iso(NOW + 2 * HOUR) });
    assert_eq!(alert_notice(&open, "", NOW, -21600), ("Flood Warning".into(), "In effect now".into()));
    assert_eq!(alert_notice(&json!({}), "Chicago, IL", NOW, -21600).0, "Weather Alert");
}

// ---------------------------------------------------------------- whatever the services send

#[test]
fn nothing_the_services_send_can_crash_the_merge() {
    let junk = [
        json!(null), json!("x"), json!(1e300), json!(-1e300), json!(f64::MAX), json!(i64::MAX), json!(i64::MIN), json!(-1), json!(0),
        json!([]), json!({}), json!([null, "é", 9e18]), json!("2026-11-18"), json!("日本語日本語日本語日本語"), json!("9999-99-99T99:99:99Z"), json!(true),
    ];
    let good = (nws(NOW), open_meteo(NOW));
    let mut built = 0;
    for j in &junk {
        // Every field of every Open-Meteo block replaced in turn, then every block itself.
        for block in ["current", "minutely_15", "hourly", "daily"] {
            let keys: Vec<String> = good.1[block].as_object().unwrap().keys().cloned().collect();
            for key in keys {
                let mut o = good.1.clone();
                o[block][key.as_str()] = if o[block][key.as_str()].is_array() && !j.is_array() { json!(vec![j.clone(); 72]) } else { j.clone() };
                let s = build_state(&place(), Some(&good.0), Some(&o), NOW, j);
                built += usize::from(s.as_ref().map_or(false, |s| advance(s, NOW + 5 * HOUR).is_some()));
                let _ = build_state(&place(), None, Some(&o), NOW, &Value::Null).map(|s| advance(&s, NOW + 30 * HOUR));
            }
            let mut o = good.1.clone();
            o[block] = j.clone();
            let _ = build_state(&place(), None, Some(&o), NOW, &Value::Null).map(|s| advance(&s, NOW + HOUR));
        }
        let mut o = good.1.clone();
        o["utc_offset_seconds"] = j.clone();
        let _ = build_state(&place(), Some(&good.0), Some(&o), NOW, &Value::Null);
        // Every field of the Weather Service periods, hours and alerts.
        for key in ["startTime", "endTime", "isDaytime", "temperature", "probabilityOfPrecipitation", "shortForecast", "detailedForecast", "windSpeed", "relativeHumidity"] {
            let mut n = good.0.clone();
            for p in n.daily.iter_mut().chain(n.hourly.iter_mut()) {
                p[key] = j.clone();
            }
            n.alerts = Some(vec![j.clone(), json!({ "properties": j }), json!({ "properties": { "status": "Actual", "id": j, "event": j, "ends": j, "onset": j, "references": j, "severity": j } })]);
            for om in [Some(&good.1), None] {
                if let Some(s) = build_state(&place(), Some(&n), om, NOW, &Value::Null) {
                    let _ = advance(&s, NOW + 7 * HOUR);
                    let list = s["alerts"].as_array().unwrap();
                    let _ = alerts_to_announce(list, &junk);
                    for a in list {
                        let _ = alert_notice(a, "Chicago, IL", NOW, -21600);
                    }
                }
            }
        }
        let _ = (alert_notice(j, "", NOW, i32::MAX), alerts_to_announce(&junk, &junk), advance(j, NOW), icon_from_text(j.as_str().unwrap_or("")));
        for now in [0, -1, 4_102_444_800_000, -62_000_000_000_000] {
            let _ = build_state(&place(), Some(&good.0), Some(&good.1), now, j).map(|s| advance(&s, now));
            let _ = advance(&state(Some(&good.0), Some(&good.1), NOW), now);
        }
    }
    assert!(built > 100, "most damaged answers still produce a forecast ({built})");
}
