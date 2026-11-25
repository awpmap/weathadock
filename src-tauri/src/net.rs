//! Network access: zip lookup, National Weather Service and Open-Meteo. The merging of the
//! responses lives in `weathadock_core::weather`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use weathadock_core::weather::{self, NwsData};

pub struct Net {
    client: reqwest::Client,
    /// zip -> (forecast url, hourly url, time zone); the NWS grid for a place never changes.
    points: Mutex<HashMap<String, (String, String, Option<String>)>>,
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
        Net { client, points: Mutex::new(HashMap::new()) }
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

    async fn fetch_nws(&self, loc: &Value) -> Option<NwsData> {
        const GEO: &str = "application/geo+json";
        let zip = loc["zip"].as_str()?.to_string();
        let key = format!("{},{}", loc["lat"].as_f64()?, loc["lon"].as_f64()?);
        let cached = self.points.lock().ok()?.get(&zip).cloned();
        let (forecast, hourly, time_zone) = match cached {
            Some(p) => p,
            None => {
                let j = self.get_json(&format!("https://api.weather.gov/points/{key}"), GEO, 1).await.ok()?;
                let p = &j["properties"];
                let point = (p["forecast"].as_str()?.to_string(), p["forecastHourly"].as_str()?.to_string(), p["timeZone"].as_str().map(String::from));
                self.points.lock().ok()?.insert(zip, point.clone());
                point
            }
        };
        let alerts_url = format!("https://api.weather.gov/alerts/active?point={key}");
        let (daily, hours, alerts) = tokio::join!(
            self.get_json(&forecast, GEO, 1),
            self.get_json(&hourly, GEO, 1),
            self.get_json(&alerts_url, GEO, 1),
        );
        if daily.is_err() && hours.is_err() && alerts.is_err() {
            return None;
        }
        let periods = |r: Result<Value, Failure>| r.ok().and_then(|v| v["properties"]["periods"].as_array().cloned()).unwrap_or_default();
        Some(NwsData {
            time_zone,
            daily: periods(daily),
            hourly: periods(hours),
            alerts: alerts.ok().map(|v| v["features"].as_array().cloned().unwrap_or_default()),
        })
    }

    /// Fetch everything for a place. `Err` only when neither source answered.
    pub async fn fetch_weather(&self, loc: &Value, previous_alerts: &Value) -> Result<Value, String> {
        let (lat, lon) = (loc["lat"].as_f64().unwrap_or(0.0), loc["lon"].as_f64().unwrap_or(0.0));
        let om_url = weather::open_meteo_url(lat, lon);
        let (nws, om) = tokio::join!(self.fetch_nws(loc), self.get_json(&om_url, "application/json", 1));
        let om = om.ok();
        let now = chrono_now_ms();
        weather::build_state(loc, nws.as_ref(), om.as_ref(), now, previous_alerts)
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
