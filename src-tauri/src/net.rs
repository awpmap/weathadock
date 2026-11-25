//! Network access: zip lookup, National Weather Service and Open-Meteo. The merging of the
//! responses, and the rules for which station may speak for a place, live in
//! `weathadock_core::weather`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use weathadock_core::weather::{self, NwsData, Reading, Station};

const GEO: &str = "application/geo+json";

/// What the Weather Service's grid lookup says about a place.
#[derive(Clone)]
struct Point {
    forecast: String,
    hourly: String,
    time_zone: Option<String>,
    /// Where the list of the observing stations around the place is found.
    stations: Option<String>,
}

pub struct Net {
    client: reqwest::Client,
    /// zip -> the grid lookup; the NWS grid for a place never changes.
    points: Mutex<HashMap<String, Point>>,
    /// zip -> the official stations within reach of the place, nearest first. Asked for once.
    stations: Mutex<HashMap<String, Vec<Station>>>,
}

pub enum Failure {
    NotFound,
    #[allow(dead_code)]
    Other(String),
}

impl Net {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(weather::USER_AGENT)
            .timeout(Duration::from_secs(12))
            .build()
            .unwrap_or_default();
        Net { client, points: Mutex::new(HashMap::new()), stations: Mutex::new(HashMap::new()) }
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn get_once(&self, url: &str, accept: &str) -> Result<Value, Failure> {
        let res = self.client.get(url).header("Accept", accept).send().await.map_err(|e| Failure::Other(e.to_string()))?;
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(Failure::NotFound);
        }
        if !res.status().is_success() {
            return Err(Failure::Other(format!("HTTP {}", res.status().as_u16())));
        }
        res.json::<Value>().await.map_err(|e| Failure::Other(e.to_string()))
    }

    /// GET JSON with one retry (the NWS API fails transiently fairly often).
    pub async fn get_json(&self, url: &str, accept: &str, retries: u32) -> Result<Value, Failure> {
        let mut last = Failure::Other("no attempt".into());
        for attempt in 0..=retries {
            match self.get_once(url, accept).await {
                Ok(v) => return Ok(v),
                Err(Failure::NotFound) => return Err(Failure::NotFound),
                Err(e) => last = e,
            }
            if attempt < retries {
                tokio::time::sleep(Duration::from_millis(800)).await;
            }
        }
        Err(last)
    }

    /// Active alerts for a place, or `None` when the call failed. They only need the coordinates.
    pub async fn fetch_alerts(&self, loc: &Value) -> Option<Vec<Value>> {
        let url = format!("https://api.weather.gov/alerts/active?point={},{}", loc["lat"].as_f64()?, loc["lon"].as_f64()?);
        self.get_json(&url, GEO, 1).await.ok()?["features"].as_array().cloned()
    }

    /// The Weather Service's daily and hourly forecasts for a place, with its time zone and how
    /// high its grid square lies, found through the grid lookup (remembered per place).
    async fn fetch_forecasts(&self, loc: &Value) -> Option<NwsData> {
        let zip = loc["zip"].as_str()?.to_string();
        let cached = self.points.lock().ok()?.get(&zip).cloned();
        let point = match cached {
            Some(p) => p,
            None => {
                let key = format!("{},{}", loc["lat"].as_f64()?, loc["lon"].as_f64()?);
                let j = self.get_json(&format!("https://api.weather.gov/points/{key}"), GEO, 1).await.ok()?;
                let p = &j["properties"];
                let point = Point {
                    forecast: p["forecast"].as_str()?.to_string(),
                    hourly: p["forecastHourly"].as_str()?.to_string(),
                    time_zone: p["timeZone"].as_str().map(String::from),
                    stations: p["observationStations"].as_str().map(String::from),
                };
                self.points.lock().ok()?.insert(zip.clone(), point.clone());
                point
            }
        };
        let (daily, hours) = tokio::join!(self.get_json(&point.forecast, GEO, 1), self.get_json(&point.hourly, GEO, 1));
        // The service moves a place to another grid now and then: look it up afresh next time.
        if matches!(daily, Err(Failure::NotFound)) || matches!(hours, Err(Failure::NotFound)) {
            self.points.lock().ok()?.remove(&zip);
        }
        let elevation = [&daily, &hours].into_iter().find_map(|r| r.as_ref().ok()?["properties"]["elevation"]["value"].as_f64());
        let periods = |r: Result<Value, Failure>| r.ok().and_then(|v| v["properties"]["periods"].as_array().cloned()).unwrap_or_default();
        Some(NwsData { time_zone: point.time_zone, daily: periods(daily), hourly: periods(hours), alerts: None, elevation })
    }

    /// Everything the Weather Service forecasts for a place. The alerts are asked for on their
    /// own, so they arrive even when the grid lookup or the forecasts fail.
    async fn fetch_nws(&self, loc: &Value) -> Option<NwsData> {
        let (forecasts, alerts) = tokio::join!(self.fetch_forecasts(loc), self.fetch_alerts(loc));
        let nws = NwsData { alerts, ..forecasts.unwrap_or_default() };
        (nws.alerts.is_some() || !nws.daily.is_empty() || !nws.hourly.is_empty()).then_some(nws)
    }

    /// The latest report of an official station that can speak for a place: the nearest one
    /// that stands about as high as the place (`ground`, in metres) and has reported lately.
    /// `None` when there is no such station; the modelled conditions are shown then.
    async fn fetch_reading(&self, loc: &Value, ground: Option<f64>) -> Option<Reading> {
        let zip = loc["zip"].as_str()?;
        let known = self.stations.lock().ok()?.get(zip).cloned();
        let stations = match known {
            Some(list) => list,
            None => {
                let url = self.points.lock().ok()?.get(zip)?.stations.clone()?;
                let list = self.get_json(&url, GEO, 0).await.ok()?;
                let near = weather::nearby_stations(&list, loc["lat"].as_f64()?, loc["lon"].as_f64()?);
                let names: Vec<String> = near.iter().map(|s| format!("{} {:.0} km", s.id, s.km)).collect();
                crate::trace(format!("stations in reach: {}", if names.is_empty() { "none".into() } else { names.join(", ") }));
                self.stations.lock().ok()?.insert(zip.to_string(), near.clone());
                near
            }
        };
        // The nearest first; a neighbour only while the nearer ones have nothing fresh to say.
        for station in stations.iter().filter(|s| s.level_with(ground)).take(3) {
            let url = format!("https://api.weather.gov/stations/{}/observations/latest", station.id);
            let why = match self.get_json(&url, GEO, 0).await {
                Ok(report) => match weather::read_station(&report, station, chrono_now_ms()) {
                    Ok(reading) => {
                        crate::trace(format!("now: measured at {} ({}), {}", station.id, station.name, describe(&reading)));
                        return Some(reading);
                    }
                    Err(why) => why,
                },
                Err(Failure::NotFound) => {
                    // The service lists stations that have stopped reporting for good.
                    if let Some(list) = self.stations.lock().ok()?.get_mut(zip) {
                        list.retain(|s| s.id != station.id);
                    }
                    "it has no reports".into()
                }
                Err(Failure::Other(why)) => format!("it did not answer ({why})"),
            };
            crate::trace(format!("now: not from {}, {why}", station.id));
        }
        None
    }

    /// Fetch everything for a place. `Err` only when neither forecast source answered.
    /// `previous` is the state built last time for this place (see `weather::build_state`).
    pub async fn fetch_weather(&self, loc: &Value, previous: &Value) -> Result<Value, String> {
        let (lat, lon) = (loc["lat"].as_f64().unwrap_or(0.0), loc["lon"].as_f64().unwrap_or(0.0));
        let om_url = weather::open_meteo_url(lat, lon);
        let (nws, om) = tokio::join!(self.fetch_nws(loc), self.get_json(&om_url, "application/json", 1));
        let om = om.ok();
        // The station is asked last: which one may speak for the place depends on how high the
        // place lies, and the forecasts say that. It is one small request, and it gets a few
        // seconds at most, so a silent station never holds the forecast back.
        let mut reading = None;
        if nws.is_some() || om.is_some() {
            let ground = weather::ground_elevation(nws.as_ref(), om.as_ref());
            reading = tokio::time::timeout(Duration::from_secs(8), self.fetch_reading(loc, ground)).await.ok().flatten();
        }
        let now = chrono_now_ms();
        weather::build_state(loc, nws.as_ref(), om.as_ref(), reading.as_ref(), now, previous)
            .ok_or_else(|| "Could not reach the National Weather Service or Open-Meteo. Check your internet connection.".to_string())
    }

    /// Resolve a US zip code to `{ ok, zip, city, state, lat, lon }` or `{ ok: false, error }`.
    pub async fn lookup_zip(&self, input: &str) -> Value {
        let zip = input.trim();
        if zip.len() != 5 || !zip.chars().all(|c| c.is_ascii_digit()) {
            return json!({ "ok": false, "error": "Enter a 5-digit US zip code." });
        }
        let mut network_error = false;
        match self.get_json(&format!("https://api.zippopotam.us/us/{zip}"), "application/json", 0).await {
            Ok(body) => {
                if let Some(mut place) = weather::place_from_zippopotam(zip, &body) {
                    place["ok"] = json!(true);
                    return place;
                }
            }
            Err(Failure::NotFound) => {}
            Err(Failure::Other(_)) => network_error = true,
        }
        // Fallback: Open-Meteo's geocoder understands postal codes too.
        let url = format!("https://geocoding-api.open-meteo.com/v1/search?name={zip}&count=10&language=en&format=json");
        match self.get_json(&url, "application/json", 0).await {
            Ok(body) => {
                let hit = body["results"].as_array().into_iter().flatten().find(|r| {
                    r["country_code"] == "US" && r["postcodes"].as_array().map_or(false, |p| p.iter().any(|z| z == zip))
                });
                if let Some(r) = hit {
                    let round4 = |v: &Value| (v.as_f64().unwrap_or(0.0) * 10_000.0).round() / 10_000.0;
                    let admin = r["admin1"].as_str().unwrap_or("");
                    return json!({
                        "ok": true, "zip": zip, "city": r["name"], "state": state_abbreviation(admin).unwrap_or(admin),
                        "lat": round4(&r["latitude"]), "lon": round4(&r["longitude"]),
                    });
                }
            }
            Err(_) => network_error = true,
        }
        if network_error {
            json!({ "ok": false, "error": "Could not reach the zip lookup service. Check your connection and try again." })
        } else {
            json!({ "ok": false, "error": format!("No US location found for {zip}.") })
        }
    }
}

/// "16 km away, taken 14 min ago: 39.0°F, dew point 34.0°F, wind 10.3 mph from 250°, 30.02 inHg, 10.00 mi"
fn describe(r: &Reading) -> String {
    let mut parts = vec![format!("{:.1}°F", r.temp)];
    if let Some(dew) = r.dew {
        parts.push(format!("dew point {dew:.1}°F"));
    }
    if let Some(w) = &r.wind {
        let from = w.from.map_or(String::new(), |d| format!(" from {d:.0}°"));
        let gust = w.gust.map_or(String::new(), |g| format!(", gusts {g:.1}"));
        parts.push(format!("wind {:.1} mph{from}{gust}", w.speed));
    }
    if let Some(p) = r.pressure {
        parts.push(format!("{p:.2} inHg"));
    }
    if let Some(v) = r.visibility {
        parts.push(format!("{v:.2} mi"));
    }
    format!("{:.0} km away, taken {} min ago: {}", r.station.km, (chrono_now_ms() - r.at) / 60_000, parts.join(", "))
}

pub fn chrono_now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn state_abbreviation(name: &str) -> Option<&'static str> {
    const STATES: [(&str, &str); 52] = [
        ("Alabama", "AL"), ("Alaska", "AK"), ("Arizona", "AZ"), ("Arkansas", "AR"), ("California", "CA"), ("Colorado", "CO"),
        ("Connecticut", "CT"), ("Delaware", "DE"), ("District of Columbia", "DC"), ("Florida", "FL"), ("Georgia", "GA"),
        ("Hawaii", "HI"), ("Idaho", "ID"), ("Illinois", "IL"), ("Indiana", "IN"), ("Iowa", "IA"), ("Kansas", "KS"),
        ("Kentucky", "KY"), ("Louisiana", "LA"), ("Maine", "ME"), ("Maryland", "MD"), ("Massachusetts", "MA"),
        ("Michigan", "MI"), ("Minnesota", "MN"), ("Mississippi", "MS"), ("Missouri", "MO"), ("Montana", "MT"),
        ("Nebraska", "NE"), ("Nevada", "NV"), ("New Hampshire", "NH"), ("New Jersey", "NJ"), ("New Mexico", "NM"),
        ("New York", "NY"), ("North Carolina", "NC"), ("North Dakota", "ND"), ("Ohio", "OH"), ("Oklahoma", "OK"),
        ("Oregon", "OR"), ("Pennsylvania", "PA"), ("Puerto Rico", "PR"), ("Rhode Island", "RI"), ("South Carolina", "SC"),
        ("South Dakota", "SD"), ("Tennessee", "TN"), ("Texas", "TX"), ("Utah", "UT"), ("Vermont", "VT"), ("Virginia", "VA"),
        ("Washington", "WA"), ("West Virginia", "WV"), ("Wisconsin", "WI"), ("Wyoming", "WY"),
    ];
    STATES.iter().find(|(n, _)| *n == name).map(|(_, a)| *a)
}
