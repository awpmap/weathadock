//! What the app shows must never contradict itself, must follow the clock, and must survive
//! whatever the services send. Each test here is one of those promises.
use serde_json::{json, Value};
use weathadock_core::weather::{
    advance, alert_notice, alerts_to_announce, build_state, feels_like, ground_elevation, icon_from_text, nearby_stations, read_station, with_alerts, NwsData, Station,
};

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
        elevation: Some(181.0),
    }
}

fn alert(id: &str, event: &str, severity: &str, ends: i64) -> Value {
    json!({ "properties": { "id": id, "status": "Actual", "messageType": "Alert", "event": event, "severity": severity, "urgency": "Expected",
        "certainty": "Likely", "headline": format!("{event} in effect"), "onset": local_iso(NOW - HOUR), "ends": local_iso(ends), "expires": local_iso(ends) } })
}

fn state(nws: Option<&NwsData>, om: Option<&Value>, now: i64) -> Value {
    build_state(&place(), nws, om, None, now, &Value::Null).expect("a state")
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
    let with_memory = build_state(&place(), Some(&later), Some(&om), None, evening, &afternoon).unwrap();
    assert_eq!((with_memory["today"]["high"].as_i64(), with_memory["today"]["low"].as_i64()), (Some(57), Some(41)));
    assert_eq!(with_memory["nwsHigh"], afternoon["nwsHigh"], "and it is remembered for the rest of the evening");
    assert_eq!(build_state(&place(), Some(&later), Some(&om), None, evening, &Value::Null).unwrap()["today"]["high"], 52);
    // The next day it is forgotten.
    let tomorrow = NOW + 29 * HOUR;
    let s = build_state(&place(), None, Some(&open_meteo(tomorrow)), None, tomorrow, &afternoon).unwrap();
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

    // The alerts call failed a few minutes later: what was known still counts as checked.
    let soon = NOW + 5 * 60_000;
    let without = NwsData { alerts: None, ..nws(soon) };
    let kept = build_state(&place(), Some(&without), Some(&open_meteo(soon)), None, soon, &s).unwrap();
    assert_eq!((kept["alerts"].as_array().unwrap().len(), kept["sources"]["alerts"].as_bool(), &kept["alertsAt"]), (3, Some(true), &s["updatedAt"]));
    assert_eq!(advance(&kept, soon + 16 * 60_000).unwrap()["sources"]["alerts"], false, "but not for ever");
    // Two hours of failures: keep what was known, minus what has ended since, and say so.
    let later = NOW + 2 * HOUR;
    let without_alerts = NwsData { alerts: None, ..nws(later) };
    let kept = build_state(&place(), Some(&without_alerts), Some(&open_meteo(later)), None, later, &s).unwrap();
    assert_eq!(kept["alerts"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap()).collect::<Vec<_>>(), ["w", "m"]);
    assert_eq!(kept["sources"]["alerts"], false);

    // The forecast calls failed but the alerts call worked: alerts still arrive.
    let only_alerts = NwsData { alerts: Some(vec![alert("x", "Wind Advisory", "Minor", NOW + HOUR)]), ..NwsData::default() };
    let s = state(Some(&only_alerts), Some(&o), NOW);
    assert_eq!(s["sources"], json!({ "nws": false, "openMeteo": true, "alerts": true }));
    assert_eq!(s["alerts"][0]["id"], "x");
    assert_eq!(s["error"], "National Weather Service did not respond. Showing what is available.");
    assert!(build_state(&place(), Some(&only_alerts), None, None, NOW, &Value::Null).is_none(), "alerts alone are not a forecast");

    // Between refreshes only the alerts are asked for, and put into the forecast already shown.
    let quiet = state(Some(&nws(NOW)), Some(&o), NOW);
    let warned = with_alerts(&quiet, &[alert("t", "Tornado Warning", "Extreme", NOW + HOUR)], NOW + 4 * 60_000);
    assert_eq!((warned["alerts"][0]["event"].as_str(), warned["alertsAt"].as_str()), (Some("Tornado Warning"), Some("2026-11-18T20:11:00.000Z")));
    assert_eq!((&warned["hourly"], &warned["updatedAt"]), (&quiet["hourly"], &quiet["updatedAt"]));
    let loading = weathadock_core::weather::empty_state("loading", None, None);
    assert_eq!(with_alerts(&loading, &[], NOW), loading);
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

// ---------------------------------------------------------------- measured where a station is near

/// The Weather Service's list of the stations around downtown Chicago, in the shape it sends
/// it: O'Hare airport just out of reach, a volunteer's station next door, Midway 16 km away.
fn station_list() -> Value {
    let station = |id: &str, name: &str, lon: f64, lat: f64, metres: f64| {
        json!({ "type": "Feature", "geometry": { "type": "Point", "coordinates": [lon, lat] },
            "properties": { "@type": "wx:ObservationStation", "stationIdentifier": id, "name": name, "timeZone": "America/Chicago",
                "elevation": { "unitCode": "wmoUnit:m", "value": metres } } })
    };
    json!({ "type": "FeatureCollection", "features": [
        station("KORD", "Chicago, Chicago-O'Hare International Airport", -87.9047, 41.9786, 204.0),
        station("E1234", "EW1234 Chicago", -87.62, 41.89, 182.0),
        station("KMDW", "Chicago, Chicago Midway Airport", -87.7522, 41.7861, 188.0),
    ] })
}

fn midway() -> Station {
    nearby_stations(&station_list(), 41.8858, -87.6181).remove(0)
}

fn value(unit: &str, value: Value, checked: &str) -> Value {
    json!({ "unitCode": format!("wmoUnit:{unit}"), "value": value, "qualityControl": checked })
}

/// A station report in the shape the Weather Service sends it: 3.9°C with a dew point of
/// 1.1°C, wind from 250° at 16.56 km/h without gusts, 101,660 Pa, and ten miles of visibility.
fn report(taken: i64) -> Value {
    let time = chrono::DateTime::from_timestamp_millis(taken).unwrap().format("%Y-%m-%dT%H:%M:%S+00:00").to_string();
    json!({ "type": "Feature", "properties": { "station": "https://api.weather.gov/stations/KMDW", "timestamp": time, "textDescription": "Light Rain",
        "temperature": value("degC", json!(3.9), "V"), "dewpoint": value("degC", json!(1.1), "V"),
        "windDirection": value("degree_(angle)", json!(250), "V"), "windSpeed": value("km_h-1", json!(16.56), "V"), "windGust": value("km_h-1", json!(null), "Z"),
        "barometricPressure": value("Pa", json!(101660), "V"), "seaLevelPressure": value("Pa", json!(101690), "V"),
        "visibility": value("m", json!(16093.44), "C"), "relativeHumidity": value("percent", json!(81.9), "V") } })
}

/// The state with the given report, taken 14 minutes ago, laid over the usual two answers.
fn measured(report: &Value) -> Value {
    let reading = read_station(report, &midway(), NOW).expect("a usable report");
    build_state(&place(), Some(&nws(NOW)), Some(&open_meteo(NOW)), Some(&reading), NOW, &Value::Null).unwrap()
}

#[test]
fn now_is_measured_where_an_official_station_is_near() {
    let near = nearby_stations(&station_list(), 41.8858, -87.6181);
    assert_eq!(near.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["KMDW"], "not the volunteer's station next door, not the airport 26 km away");
    assert!((near[0].km - 15.7).abs() < 0.3, "{} km", near[0].km);

    let s = measured(&report(NOW - 14 * 60_000));
    let c = &s["current"];
    assert_eq!((c["temp"].as_i64(), c["dewPoint"].as_i64(), c["humidity"].as_i64()), (Some(39), Some(34), Some(82)));
    assert_eq!((c["windSpeed"].as_i64(), c["windDir"].as_str(), c["windGust"].is_null()), (Some(10), Some("WSW"), true));
    assert_eq!((c["pressure"].as_f64(), c["visibility"].as_f64()), (Some(30.02), Some(10.0)));
    assert_eq!(c["feelsLike"], 32, "wind chill from the measured temperature and the measured wind");
    assert_eq!(c["feelsLike"], feels_like(39.02, Some(16.56 / 1.609344), Some(82.0)).round() as i64);
    assert_eq!((c["observedAt"].as_str(), c["station"]["id"].as_str(), c["station"]["name"].as_str()), (Some("2026-11-18T19:53:00.000Z"), Some("KMDW"), Some("Chicago, Chicago Midway Airport")));
    assert_eq!(c["station"]["miles"], 9.8);

    // Everything else on screen follows the measured temperature.
    assert_now_row_matches(&s);
    assert_eq!((s["today"]["high"].as_i64(), s["today"]["low"].as_i64()), (Some(57), Some(39)), "the day's range contains what is measured now");
    assert_eq!(advance(&s, NOW), None);
    assert_eq!(s["sources"], json!({ "nws": true, "openMeteo": true, "alerts": true }));
}

#[test]
fn the_sky_words_stay_with_the_source_that_times_the_rain() {
    // The station said "Light Rain" 14 minutes ago; the 15-minute steps say it is dry. Only one
    // of the two is shown, so the words can never argue with the rain heads-up beside them.
    let (modelled, s) = (state(Some(&nws(NOW)), Some(&open_meteo(NOW)), NOW), measured(&report(NOW - 14 * 60_000)));
    for key in ["condition", "icon", "isDay", "uvIndex"] {
        assert_eq!(s["current"][key], modelled["current"][key], "{key}");
    }
    for key in ["nextPrecip", "minutely", "alerts", "location", "nwsHigh"] {
        assert_eq!(s[key], modelled[key], "{key}");
    }
    assert_eq!(s["hourly"][1], modelled["hourly"][1], "the hours ahead are the forecast's, as before");
    assert!(modelled["current"]["observedAt"].is_null() && modelled["current"]["station"].is_null(), "nothing is called measured without a station");
}

#[test]
fn a_report_stands_for_now_only_while_it_is_fresh() {
    let station = midway();
    // Most stations report once an hour: one report stays until the next has had half an hour to arrive.
    assert!(read_station(&report(NOW - 90 * 60_000), &station, NOW).is_ok());
    assert_eq!(read_station(&report(NOW - 91 * 60_000), &station, NOW).unwrap_err(), "its report is 91 minutes old");
    assert!(read_station(&report(NOW - 400 * 24 * HOUR), &station, NOW).is_err());
    // A PC whose clock runs a few minutes behind is fine; a report from tomorrow is not.
    assert_eq!(read_station(&report(NOW + 3 * 60_000), &station, NOW).unwrap().at, NOW);
    assert!(read_station(&report(NOW + 11 * 60_000), &station, NOW).is_err());
    let mut undated = report(NOW);
    undated["properties"]["timestamp"] = json!(null);
    assert!(read_station(&undated, &station, NOW).is_err());

    // Ninety minutes without news and the forecast for the hour takes over, as it always has.
    // From then on nothing on screen is called measured.
    let s = measured(&report(NOW - 14 * 60_000));
    let later = advance(&s, NOW + 40 * 60_000).unwrap();
    assert_eq!((later["current"]["temp"].as_i64(), later["current"]["observedAt"].as_str()), (Some(39), Some("2026-11-18T19:53:00.000Z")));
    let later = advance(&s, NOW + 91 * 60_000).unwrap();
    assert_eq!(later["current"]["temp"], 54);
    assert!(later["current"]["observedAt"].is_null() && later["current"]["station"].is_null() && later["current"]["pressure"].is_null());
    assert_now_row_matches(&later);
}

#[test]
fn a_value_the_service_turned_down_is_not_shown() {
    let station = midway();
    // X: failed the service's quality control. Q: questioned. B: marked bad by a person.
    for flag in ["X", "Q", "B"] {
        let mut r = report(NOW);
        r["properties"]["temperature"]["qualityControl"] = json!(flag);
        assert_eq!(read_station(&r, &station, NOW).unwrap_err(), "its report has no temperature", "{flag}");
        let mut r = report(NOW);
        for key in ["dewpoint", "windSpeed", "barometricPressure", "visibility"] {
            r["properties"][key]["qualityControl"] = json!(flag);
        }
        let reading = read_station(&r, &station, NOW).unwrap();
        assert!(reading.dew.is_none() && reading.wind.is_none() && reading.pressure.is_none() && reading.visibility.is_none(), "{flag}");
    }
    // Verified, screened, passed a coarse check, or not checked yet: taken, within what is physically possible.
    for flag in ["V", "S", "C", "G", "Z"] {
        let mut r = report(NOW);
        r["properties"]["temperature"]["qualityControl"] = json!(flag);
        assert!(read_station(&r, &station, NOW).is_ok(), "{flag}");
    }
    for (key, absurd) in [("temperature", 99.0), ("windSpeed", 900.0), ("windSpeed", -4.0), ("barometricPressure", 10_166.0), ("visibility", -1.0), ("windDirection", 720.0)] {
        let mut r = report(NOW);
        r["properties"][key]["value"] = json!(absurd);
        match read_station(&r, &station, NOW) {
            Err(why) => assert_eq!((key, why.as_str()), ("temperature", "its report has no temperature")),
            Ok(reading) => assert!(
                match key {
                    "windSpeed" => reading.wind.is_none(),
                    "windDirection" => reading.wind.unwrap().from.is_none(),
                    "barometricPressure" => reading.pressure.is_none(),
                    _ => reading.visibility.is_none(),
                },
                "{key} {absurd}"
            ),
        }
    }
}

#[test]
fn a_unit_this_app_does_not_know_is_never_guessed() {
    let station = midway();
    // The service used to send the wind in metres per second: 4.6 m/s is 10 mph too.
    let mut r = report(NOW);
    r["properties"]["windSpeed"] = json!({ "unitCode": "unit:m_s-1", "value": 4.6, "qualityControl": "V" });
    assert_eq!(read_station(&r, &station, NOW).unwrap().wind.unwrap().speed.round(), 10.0);
    r["properties"]["windSpeed"] = value("kn", json!(9), "V");
    r["properties"]["barometricPressure"] = value("hPa", json!(1016.6), "V");
    r["properties"]["visibility"] = value("km", json!(16), "V");
    r["properties"]["dewpoint"] = value("K", json!(274.2), "V");
    let reading = read_station(&r, &station, NOW).unwrap();
    assert!(reading.wind.is_none() && reading.pressure.is_none() && reading.visibility.is_none() && reading.dew.is_none());
    r["properties"]["temperature"] = value("K", json!(277.0), "V");
    assert!(read_station(&r, &station, NOW).is_err());
    r["properties"]["temperature"] = value("degF", json!(39.0), "V");
    assert_eq!(read_station(&r, &station, NOW).unwrap().temp, 39.0);
}

#[test]
fn what_a_station_leaves_out_stays_in_step_with_what_it_reports() {
    // No dew point from the station: the model's stays (31°F), and the humidity is the one
    // that follows from the measured temperature and that dew point, not the model's 40%.
    let mut r = report(NOW - 14 * 60_000);
    r["properties"]["dewpoint"]["value"] = json!(null);
    let c = measured(&r)["current"].clone();
    assert_eq!((c["temp"].as_i64(), c["dewPoint"].as_i64(), c["humidity"].as_i64()), (Some(39), Some(31), Some(73)));
    // A modelled dew point above the measured temperature would be fog that is not there.
    let mut o = open_meteo(NOW);
    o["hourly"]["dew_point_2m"] = json!(vec![45.0; 72]);
    let reading = read_station(&r, &midway(), NOW).unwrap();
    let c = build_state(&place(), Some(&nws(NOW)), Some(&o), Some(&reading), NOW, &Value::Null).unwrap()["current"].clone();
    assert_eq!((c["dewPoint"].as_i64(), c["humidity"].as_i64()), (Some(39), Some(100)));
    // The same when the station itself reports a dew point a hair above its temperature.
    let mut r = report(NOW - 14 * 60_000);
    r["properties"]["dewpoint"]["value"] = json!(4.0);
    let c = measured(&r)["current"].clone();
    assert_eq!((c["temp"].as_i64(), c["dewPoint"].as_i64(), c["humidity"].as_i64()), (Some(39), Some(39), Some(100)));

    // No wind from the station: the model's wind stays as one set (speed, direction and gusts),
    // and the wind chill is worked out from the measured temperature and that wind.
    let mut r = report(NOW - 14 * 60_000);
    r["properties"]["windSpeed"]["value"] = json!(null);
    let c = measured(&r)["current"].clone();
    assert_eq!((c["windSpeed"].as_i64(), c["windDir"].as_str(), c["windGust"].as_i64()), (Some(6), Some("W"), Some(12)));
    assert_eq!(c["feelsLike"], feels_like(39.02, Some(6.0), None).round() as i64);
    // Calm air comes from nowhere and chills nothing.
    let mut r = report(NOW - 14 * 60_000);
    r["properties"]["windSpeed"]["value"] = json!(0);
    r["properties"]["windDirection"]["value"] = json!(0);
    let c = measured(&r)["current"].clone();
    assert_eq!((c["windSpeed"].as_i64(), c["windDir"].is_null(), c["windGust"].is_null(), c["feelsLike"].as_i64()), (Some(0), true, true, Some(39)));
    // Gusts are reported only when there are any.
    let mut r = report(NOW - 14 * 60_000);
    r["properties"]["windGust"] = value("km_h-1", json!(40.7), "V");
    assert_eq!(measured(&r)["current"]["windGust"], 25);

    // No pressure or visibility from the station: the model's stay.
    let mut r = report(NOW - 14 * 60_000);
    r["properties"]["barometricPressure"]["value"] = json!(null);
    r["properties"]["visibility"]["value"] = json!(null);
    let c = measured(&r)["current"].clone();
    assert_eq!((c["pressure"].as_f64(), c["visibility"].as_f64()), (Some(30.0), Some(10.0)));
    // Fog at the station: a quarter of a mile is shown as a quarter of a mile, not as 0.3.
    for (metres, miles) in [(402.336, 0.25), (2816.352, 1.75), (4828.032, 3.0), (16093.44, 10.0), (24140.16, 15.0)] {
        let mut r = report(NOW - 14 * 60_000);
        r["properties"]["visibility"]["value"] = json!(metres);
        assert_eq!(measured(&r)["current"]["visibility"], miles);
    }

    // Without Open-Meteo the station is laid over the forecast for the hour instead.
    let reading = read_station(&report(NOW - 14 * 60_000), &midway(), NOW).unwrap();
    let s = build_state(&place(), Some(&nws(NOW)), None, Some(&reading), NOW, &Value::Null).unwrap();
    assert_eq!((s["current"]["temp"].as_i64(), s["current"]["condition"].as_str(), s["current"]["uvIndex"].is_null()), (Some(39), Some("Sunny"), true));
    assert_now_row_matches(&s);
}

#[test]
fn a_station_up_the_mountain_does_not_speak_for_the_valley() {
    let station = |metres: Option<f64>| Station { id: "KXYZ".into(), name: "Pass".into(), km: 9.0, elevation: metres };
    // Air cools about 2°F for every 150 m of height.
    assert!(station(Some(1740.0)).level_with(Some(1650.0)));
    assert!(!station(Some(2400.0)).level_with(Some(1650.0)));
    assert!(!station(Some(1650.0)).level_with(Some(1801.0)));
    assert!(station(None).level_with(Some(1650.0)) && station(Some(1650.0)).level_with(None), "unknown heights rule nothing out");
    // Open-Meteo says how high the exact spot lies; the Weather Service knows its grid square.
    assert_eq!(ground_elevation(Some(&nws(NOW)), Some(&json!({ "elevation": 179.0 }))), Some(179.0));
    assert_eq!(ground_elevation(Some(&nws(NOW)), Some(&json!({}))), Some(181.0));
    assert_eq!(ground_elevation(None, None), None);
    // A height given in feet is not read as metres.
    let mut list = station_list();
    list["features"][2]["properties"]["elevation"] = json!({ "unitCode": "wmoUnit:ft", "value": 617 });
    assert_eq!(nearby_stations(&list, 41.8858, -87.6181)[0].elevation, None);
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
                let s = build_state(&place(), Some(&good.0), Some(&o), None, NOW, j);
                built += usize::from(s.as_ref().map_or(false, |s| advance(s, NOW + 5 * HOUR).is_some()));
                let _ = build_state(&place(), None, Some(&o), None, NOW, &Value::Null).map(|s| advance(&s, NOW + 30 * HOUR));
            }
            let mut o = good.1.clone();
            o[block] = j.clone();
            let _ = build_state(&place(), None, Some(&o), None, NOW, &Value::Null).map(|s| advance(&s, NOW + HOUR));
        }
        let mut o = good.1.clone();
        o["utc_offset_seconds"] = j.clone();
        let _ = build_state(&place(), Some(&good.0), Some(&o), None, NOW, &Value::Null);
        // Every field of the Weather Service periods, hours and alerts.
        for key in ["startTime", "endTime", "isDaytime", "temperature", "probabilityOfPrecipitation", "shortForecast", "detailedForecast", "windSpeed", "relativeHumidity"] {
            let mut n = good.0.clone();
            for p in n.daily.iter_mut().chain(n.hourly.iter_mut()) {
                p[key] = j.clone();
            }
            n.alerts = Some(vec![j.clone(), json!({ "properties": j }), json!({ "properties": { "status": "Actual", "id": j, "event": j, "ends": j, "onset": j, "references": j, "severity": j } })]);
            for om in [Some(&good.1), None] {
                if let Some(s) = build_state(&place(), Some(&n), om, None, NOW, &Value::Null) {
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
        // The station list and the station's report, whole and field by field.
        let station = midway();
        let lists = [j.clone(), json!({ "features": j }), json!({ "features": [j, { "properties": j, "geometry": j },
            { "properties": { "stationIdentifier": "KMDW", "name": j, "elevation": j }, "geometry": { "coordinates": [j, j] } },
            { "properties": { "stationIdentifier": j, "name": "x", "elevation": { "unitCode": j, "value": j } }, "geometry": { "coordinates": j } }] })];
        for list in &lists {
            for (lat, lon) in [(41.8858, -87.6181), (f64::MAX, f64::MIN), (0.0, 0.0)] {
                assert!(nearby_stations(list, lat, lon).len() <= 1);
            }
        }
        let _ = (read_station(j, &station, NOW), read_station(&json!({ "properties": j }), &station, NOW), ground_elevation(None, Some(j)));
        for key in ["timestamp", "temperature", "dewpoint", "windDirection", "windSpeed", "windGust", "barometricPressure", "visibility"] {
            for damaged in [j.clone(), json!({ "unitCode": j, "value": 3.9, "qualityControl": "V" }), json!({ "unitCode": "wmoUnit:degC", "value": j, "qualityControl": j })] {
                let mut r = report(NOW - 14 * 60_000);
                r["properties"][key] = damaged;
                for now in [NOW, i64::MAX, i64::MIN, 0] {
                    if let Ok(reading) = read_station(&r, &station, now) {
                        let s = build_state(&place(), Some(&good.0), Some(&good.1), Some(&reading), NOW, &Value::Null).unwrap();
                        assert_now_row_matches(&s);
                        let _ = advance(&s, NOW + 2 * HOUR);
                    }
                }
            }
        }
        for now in [0, -1, 4_102_444_800_000, -62_000_000_000_000] {
            let _ = build_state(&place(), Some(&good.0), Some(&good.1), None, now, j).map(|s| advance(&s, now));
            let _ = advance(&state(Some(&good.0), Some(&good.1), NOW), now);
        }
    }
    assert!(built > 100, "most damaged answers still produce a forecast ({built})");
}
