use crate::huobi::errors::*;
use chrono::Utc;
use lazy_static::lazy_static;
use serde_json::Value;
use serde_qs as qs;
use std::collections::BTreeMap;

pub fn build_request(parameters: &BTreeMap<String, String>) -> String {
    let mut request = String::new();
    for (key, value) in parameters {
        let param = format!("{}={}&", key, value);
        request.push_str(param.as_ref());
    }
    request.pop(); // remove last &

    request
}

pub fn build_request_p<S>(payload: S) -> Result<String>
where
    S: serde::Serialize,
{
    Ok(qs::to_string(&payload)?)
}

pub fn build_signed_request(mut parameters: BTreeMap<String, String>) -> Result<String> {
    if let Ok(timestamp) = get_timestamp() {
        parameters.insert("timestamp".into(), timestamp.to_string());

        let mut request = String::new();
        for (key, value) in &parameters {
            let param = format!("{}={}&", key, value);
            request.push_str(param.as_ref());
        }
        request.pop();

        Ok(request)
    } else {
        Err(Error::Msg("Failed to get timestamp".to_string()))
    }
}

pub fn build_signed_request_p<S>(payload: S) -> Result<String>
where
    S: serde::Serialize,
{
    let query_string = qs::to_string(&payload)?;
    let mut parameters: BTreeMap<String, String> = BTreeMap::new();

    if let Ok(timestamp) = get_timestamp() {
        parameters.insert("timestamp".into(), timestamp.to_string());

        let mut request = query_string;
        for (key, value) in &parameters {
            let param = format!("{}={}&", key, value);
            request.push_str(param.as_ref());
        }
        if let Some('&') = request.chars().last() {
            request.pop();
        }

        Ok(request)
    } else {
        Err(Error::Msg("Failed to get timestamp".to_string()))
    }
}

pub fn to_i64(v: &Value) -> i64 {
    v.as_i64().unwrap()
}

pub fn to_f64(v: &Value) -> f64 {
    v.as_str().unwrap().parse().unwrap()
}

fn get_timestamp() -> Result<u64> {
    Ok(Utc::now().timestamp_millis() as u64)
}

lazy_static! {
    static ref TRUE: String = "TRUE".to_string();
}
lazy_static! {
    static ref FALSE: String = "FALSE".to_string();
}

pub fn bool_to_string(b: bool) -> String {
    match b {
        true => TRUE.to_string(),
        false => FALSE.to_string(),
    }
}
