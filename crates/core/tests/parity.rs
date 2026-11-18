//! Checks the Rust weather merge against states produced by the original JavaScript
//! implementation for the same inputs (tests/fixtures/cases.json).
use serde_json::Value;
use weathadock_core::weather::{build_state, NwsData};

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
fn matches_the_javascript_implementation() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/cases.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let nws = case["nws"].as_object().map(|n| NwsData {
            time_zone: n["timeZone"].as_str().map(String::from),
            daily: n["daily"].as_array().unwrap().clone(),
            hourly: n["hourly"].as_array().unwrap().clone(),
            alerts: Some(n["alerts"].as_array().unwrap().clone()),
        });
        let om = (!case["om"].is_null()).then(|| &case["om"]);
        let got = build_state(&case["loc"], nws.as_ref(), om, case["now"].as_i64().unwrap(), &case["previousAlerts"]).unwrap();
        let mut out = Vec::new();
        diff("state", &got, &case["expected"], &mut out);
        out.sort();
        out.dedup();
        assert!(out.is_empty(), "case {}:\n{}", case["name"], out.join("\n"));
    }
}
