//! Date Builtin Methods
//!
//! This module contains all built-in date manipulation methods for the language.
//! Dates are represented as BigInt timestamps (milliseconds since Unix epoch).

use super::super::super::interpreter::err;
use crate::parsing::ast::Value;
use chrono::Datelike;
use num_traits::ToPrimitive;

/// Implements all date builtin methods
pub fn call_date_method(obj: &Value, method_name: &str, _args: &[Value]) -> Result<Value, String> {
    match obj {
        Value::BigInt(ts) => match method_name {
            "getTime" => Ok(Value::Number(ts.to_f64().unwrap_or(0.0))),
            "toISOString" => {
                let ms = ts.to_i64().unwrap_or(0);
                let secs = (ms / 1000) as i64;
                let nanos = ((ms % 1000) * 1_000_000) as i32;
                let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos as u32)
                    .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
                let iso = dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                Ok(Value::Str(iso))
            }
            "toString" => {
                let ms = ts.to_i64().unwrap_or(0);
                let secs = (ms / 1000) as i64;
                let nanos = ((ms % 1000) * 1_000_000) as i32;
                let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos as u32)
                    .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
                Ok(Value::Str(dt.to_string()))
            }
            "getFullYear" => {
                let ms = ts.to_i64().unwrap_or(0);
                let secs = (ms / 1000) as i64;
                let nanos = ((ms % 1000) * 1_000_000) as i32;
                let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos as u32)
                    .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
                Ok(Value::Number(dt.year() as f64))
            }
            "getMonth" => {
                let ms = ts.to_i64().unwrap_or(0);
                let secs = (ms / 1000) as i64;
                let nanos = ((ms % 1000) * 1_000_000) as i32;
                let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos as u32)
                    .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
                Ok(Value::Number((dt.month() - 1) as f64))
            }
            _ => Err(err(format!("Unimplemented date method: '{}'", method_name))),
        },
        _ => Err(err("Date methods require BigInt timestamp".to_string())),
    }
}
