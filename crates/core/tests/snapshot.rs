//! Three recorded sets of service responses and the state the merge builds from each
//! (tests/fixtures/cases.json). Any change to what the app would show appears as a diff here.
//! After an intended change, run with WEATHADOCK_BLESS=1 to rewrite the file, and read the diff.
use serde_json::{json, Value};
use weathadock_core::weather::{build_state, NwsData};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cases.json");

fn diff(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() > 1e-9 {
                out.push(format!("{path}: {x} != {y}"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                diff(&format!("{path}.{k}"), x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), out);
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.push(format!("{path}: length {} != {}", x.len(), y.len()));
            }
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                diff(&format!("{path}[{i}]"), p, q, out);
            }
        }
        _ => {
            if a != b {
                out.push(format!("{path}: {a} != {b}"));
            }
        }
    }
}

#[test]
fn builds_the_recorded_states() {
    let mut cases: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURES).unwrap()).unwrap();
    let bless = std::env::var_os("WEATHADOCK_BLESS").is_some();
    for case in cases.as_array_mut().unwrap() {
        let nws = case["nws"].as_object().map(|n| NwsData {
            time_zone: n["timeZone"].as_str().map(String::from),
            daily: n["daily"].as_array().unwrap().clone(),
            hourly: n["hourly"].as_array().unwrap().clone(),
            alerts: Some(n["alerts"].as_array().unwrap().clone()),
        });
        let om = (!case["om"].is_null()).then(|| &case["om"]);
        let previous = json!({ "alerts": case["previousAlerts"] });
        let got = build_state(&case["loc"], nws.as_ref(), om, case["now"].as_i64().unwrap(), &previous).unwrap();
        if bless {
            case["expected"] = got;
            continue;
        }
        let mut out = Vec::new();
        diff("state", &got, &case["expected"], &mut out);
        out.sort();
        out.dedup();
        assert!(out.is_empty(), "case {}:\n{}", case["name"], out.join("\n"));
    }
    if bless {
        std::fs::write(FIXTURES, serde_json::to_string(&cases).unwrap() + "\n").unwrap();
    }
}
