//! Date and time runtime builtin functions
//!
//! This module provides date/time operations including:
//! - Date constructor and static methods (Date.now, Date.parse)
//! - Date instance methods (getFullYear, getMonth, getDate, etc.)
//! - UTC variants (getUTCFullYear, getUTCMonth, etc.)
//! - Locale string conversions (toLocaleString, toLocaleDateString, etc.)
//! - Performance timing (clock)
//!
//! Date objects are represented as RuntimeValue::Object with a __date_ts field
//! containing the timestamp in milliseconds since Unix epoch.

use super::RuntimeValue;

// ============================================================================
// Date Static Methods
// ============================================================================

/// Returns the current time as milliseconds since Unix epoch
pub(crate) fn runtime_date_now(_args: &[RuntimeValue]) -> RuntimeValue {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    RuntimeValue::Int(now)
}

/// Parses an ISO 8601 date string and returns milliseconds since epoch
pub(crate) fn runtime_date_parse(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() != 1 {
        return RuntimeValue::Null;
    }
    let s = match &args[0] {
        RuntimeValue::String(x) => x.clone(),
        _ => return RuntimeValue::Null,
    };
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
        RuntimeValue::Int(dt.timestamp_millis())
    } else {
        RuntimeValue::Null
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Helper to extract timestamp from Date object
fn get_date_ts(args: &[RuntimeValue]) -> Option<i64> {
    if args.is_empty() {
        return None;
    }
    match &args[0] {
        RuntimeValue::Object(obj) => obj.get("__date_ts").and_then(|v| match v {
            RuntimeValue::Int(n) => Some(*n),
            RuntimeValue::Float(n) => Some(*n as i64),
            _ => None,
        }),
        _ => None,
    }
}

// ============================================================================
// Date Instance Methods - Conversions
// ============================================================================

/// Converts date to ISO 8601 string format
pub(crate) fn runtime_date_to_iso_string(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{SecondsFormat, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(dt) = Utc.timestamp_opt(secs, nanos).single() {
            return RuntimeValue::String(dt.to_rfc3339_opts(SecondsFormat::Millis, true));
        }
    }
    RuntimeValue::Null
}

/// Returns the numeric value (timestamp in milliseconds)
pub(crate) fn runtime_date_get_time(args: &[RuntimeValue]) -> RuntimeValue {
    if let Some(ms) = get_date_ts(args) {
        RuntimeValue::Int(ms)
    } else {
        RuntimeValue::Null
    }
}

/// Converts date to a string representation in local time
pub(crate) fn runtime_date_to_string(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Local, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(utc) = Utc.timestamp_opt(secs, nanos).single() {
            let local = utc.with_timezone(&Local);
            return RuntimeValue::String(local.format("%c").to_string());
        }
    }
    RuntimeValue::Null
}

// ============================================================================
// Date Instance Methods - Local Time Getters
// ============================================================================

/// Returns the year of the date in local time
pub(crate) fn runtime_date_get_full_year(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Datelike, Local, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(utc) = Utc.timestamp_opt(secs, nanos).single() {
            let local = utc.with_timezone(&Local);
            return RuntimeValue::Int(local.year() as i64);
        }
    }
    RuntimeValue::Null
}

/// Returns the month (0-11) of the date in local time
pub(crate) fn runtime_date_get_month(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Datelike, Local, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(utc) = Utc.timestamp_opt(secs, nanos).single() {
            let local = utc.with_timezone(&Local);
            return RuntimeValue::Int(local.month0() as i64);
        }
    }
    RuntimeValue::Null
}

/// Returns the day of the month (1-31) in local time
pub(crate) fn runtime_date_get_date(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Datelike, Local, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(utc) = Utc.timestamp_opt(secs, nanos).single() {
            let local = utc.with_timezone(&Local);
            return RuntimeValue::Int(local.day() as i64);
        }
    }
    RuntimeValue::Null
}

/// Returns the hours (0-23) in local time
pub(crate) fn runtime_date_get_hours(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Local, TimeZone, Timelike, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(utc) = Utc.timestamp_opt(secs, nanos).single() {
            let local = utc.with_timezone(&Local);
            return RuntimeValue::Int(local.hour() as i64);
        }
    }
    RuntimeValue::Null
}

/// Returns the minutes (0-59) in local time
pub(crate) fn runtime_date_get_minutes(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Local, TimeZone, Timelike, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(utc) = Utc.timestamp_opt(secs, nanos).single() {
            let local = utc.with_timezone(&Local);
            return RuntimeValue::Int(local.minute() as i64);
        }
    }
    RuntimeValue::Null
}

/// Returns the seconds (0-59) in local time
pub(crate) fn runtime_date_get_seconds(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Local, TimeZone, Timelike, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(utc) = Utc.timestamp_opt(secs, nanos).single() {
            let local = utc.with_timezone(&Local);
            return RuntimeValue::Int(local.second() as i64);
        }
    }
    RuntimeValue::Null
}

/// Returns the milliseconds (0-999) of the date
pub(crate) fn runtime_date_get_milliseconds(args: &[RuntimeValue]) -> RuntimeValue {
    if let Some(ms) = get_date_ts(args) {
        RuntimeValue::Int(ms % 1000)
    } else {
        RuntimeValue::Null
    }
}

/// Returns the day of the week (0=Sunday, 6=Saturday) in local time
pub(crate) fn runtime_date_get_day(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Datelike, Local, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(utc) = Utc.timestamp_opt(secs, nanos).single() {
            let local = utc.with_timezone(&Local);
            return RuntimeValue::Int(local.weekday().num_days_from_sunday() as i64);
        }
    }
    RuntimeValue::Null
}

// ============================================================================
// Date Instance Methods - UTC Time Getters
// ============================================================================

/// Returns the year of the date in UTC
pub(crate) fn runtime_date_get_utc_full_year(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Datelike, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(dt) = Utc.timestamp_opt(secs, nanos).single() {
            return RuntimeValue::Int(dt.year() as i64);
        }
    }
    RuntimeValue::Null
}

/// Returns the month (0-11) of the date in UTC
pub(crate) fn runtime_date_get_utc_month(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Datelike, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(dt) = Utc.timestamp_opt(secs, nanos).single() {
            return RuntimeValue::Int(dt.month0() as i64);
        }
    }
    RuntimeValue::Null
}

/// Returns the day of the month (1-31) in UTC
pub(crate) fn runtime_date_get_utc_date(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Datelike, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(dt) = Utc.timestamp_opt(secs, nanos).single() {
            return RuntimeValue::Int(dt.day() as i64);
        }
    }
    RuntimeValue::Null
}

/// Returns the hours (0-23) in UTC
pub(crate) fn runtime_date_get_utc_hours(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{TimeZone, Timelike, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(dt) = Utc.timestamp_opt(secs, nanos).single() {
            return RuntimeValue::Int(dt.hour() as i64);
        }
    }
    RuntimeValue::Null
}

/// Returns the minutes (0-59) in UTC
pub(crate) fn runtime_date_get_utc_minutes(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{TimeZone, Timelike, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(dt) = Utc.timestamp_opt(secs, nanos).single() {
            return RuntimeValue::Int(dt.minute() as i64);
        }
    }
    RuntimeValue::Null
}

/// Returns the seconds (0-59) in UTC
pub(crate) fn runtime_date_get_utc_seconds(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{TimeZone, Timelike, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(dt) = Utc.timestamp_opt(secs, nanos).single() {
            return RuntimeValue::Int(dt.second() as i64);
        }
    }
    RuntimeValue::Null
}

/// Returns the milliseconds (0-999) of the date in UTC
pub(crate) fn runtime_date_get_utc_milliseconds(args: &[RuntimeValue]) -> RuntimeValue {
    if let Some(ms) = get_date_ts(args) {
        RuntimeValue::Int(ms % 1000)
    } else {
        RuntimeValue::Null
    }
}

// ============================================================================
// Date Instance Methods - Timezone and Locale
// ============================================================================

/// Returns the timezone offset in minutes
pub(crate) fn runtime_date_get_timezone_offset(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Local, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(utc) = Utc.timestamp_opt(secs, nanos).single() {
            let local = utc.with_timezone(&Local);
            let offset_secs = local.offset().local_minus_utc();
            // Return offset in minutes (JS convention)
            return RuntimeValue::Float(offset_secs as f64 / 60.0);
        }
    }
    RuntimeValue::Null
}

/// Converts date to a locale-specific string representation
pub(crate) fn runtime_date_to_locale_string(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Local, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(utc) = Utc.timestamp_opt(secs, nanos).single() {
            let local = utc.with_timezone(&Local);
            return RuntimeValue::String(local.format("%c").to_string());
        }
    }
    RuntimeValue::Null
}

/// Converts date portion to a locale-specific string
pub(crate) fn runtime_date_to_locale_date_string(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Local, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(utc) = Utc.timestamp_opt(secs, nanos).single() {
            let local = utc.with_timezone(&Local);
            return RuntimeValue::String(local.format("%x").to_string());
        }
    }
    RuntimeValue::Null
}

/// Converts time portion to a locale-specific string
pub(crate) fn runtime_date_to_locale_time_string(args: &[RuntimeValue]) -> RuntimeValue {
    use chrono::{Local, TimeZone, Utc};
    if let Some(ms) = get_date_ts(args) {
        let secs = ms / 1000;
        let nanos = ((ms % 1000) * 1_000_000) as u32;
        if let Some(utc) = Utc.timestamp_opt(secs, nanos).single() {
            let local = utc.with_timezone(&Local);
            return RuntimeValue::String(local.format("%X").to_string());
        }
    }
    RuntimeValue::Null
}

// ============================================================================
// Performance Timing
// ============================================================================

/// Returns seconds elapsed since program start (high-resolution timer)
pub(crate) fn runtime_clock(_args: &[RuntimeValue]) -> RuntimeValue {
    use once_cell::sync::Lazy;
    use std::time::Instant;

    static START: Lazy<Instant> = Lazy::new(|| Instant::now());
    let secs = START.elapsed().as_secs_f64();
    RuntimeValue::Float(secs)
}

pub(crate) fn runtime_time_system_now(_args: &[RuntimeValue]) -> RuntimeValue {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    RuntimeValue::BigInt(num_bigint::BigInt::from(now))
}

pub(crate) fn runtime_time_monotonic_now(_args: &[RuntimeValue]) -> RuntimeValue {
    use once_cell::sync::Lazy;
    use std::time::Instant;
    static MONOTONIC_START: Lazy<Instant> = Lazy::new(|| Instant::now());
    let nanos = MONOTONIC_START.elapsed().as_nanos();
    RuntimeValue::BigInt(num_bigint::BigInt::from(nanos))
}

pub(crate) fn runtime_time_sleep_nanos(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let nanos = args[0].as_int().unwrap_or(0) as u64;
    std::thread::sleep(std::time::Duration::from_nanos(nanos));
    RuntimeValue::Null
}

pub(crate) fn runtime_time_local_offset_secs(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let epoch_sec = args[0].as_int().unwrap_or(0);
    use chrono::{Local, Offset, TimeZone};
    let local_time = Local
        .timestamp_opt(epoch_sec, 0)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).unwrap());
    let offset_secs = local_time.offset().fix().local_minus_utc();
    RuntimeValue::Int(offset_secs as i64)
}

pub(crate) fn runtime_time_timezone_offset_secs(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let tz_name = args[0].as_string();
    let epoch_sec = args[1].as_int().unwrap_or(0);
    use chrono::TimeZone;
    use chrono_tz::Tz;
    if let Ok(tz) = tz_name.parse::<Tz>() {
        if let Some(datetime) = tz.timestamp_opt(epoch_sec, 0).single() {
            use chrono::Offset;
            let offset_secs = datetime.offset().fix().local_minus_utc();
            return RuntimeValue::Int(offset_secs as i64);
        }
    }
    RuntimeValue::Null
}

fn translate_pattern(pat: &str) -> String {
    pat.replace("yyyy", "%Y")
        .replace("MMMM", "%B")
        .replace("MMM", "%b")
        .replace("MM", "%m")
        .replace("dd", "%d")
        .replace("EEEE", "%A")
        .replace("EEE", "%a")
        .replace("HH", "%H")
        .replace("hh", "%I")
        .replace("mm", "%M")
        .replace("ss", "%S")
        .replace("a", "%p")
        .replace("XXX", "%:z")
        .replace("XX", "%z")
}

pub(crate) fn runtime_time_format(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 4 {
        return RuntimeValue::Null;
    }
    let epoch_sec = args[0].as_int().unwrap_or(0);
    let nanos = args[1].as_int().unwrap_or(0) as u32;
    let offset_sec = args[2].as_int().unwrap_or(0) as i32;
    let pattern = args[3].as_string();

    use chrono::{DateTime, FixedOffset};
    if let Some(utc_dt) = DateTime::from_timestamp(epoch_sec, nanos) {
        if let Some(offset) = FixedOffset::east_opt(offset_sec) {
            let naive = utc_dt.naive_utc();
            let datetime = DateTime::<FixedOffset>::from_naive_utc_and_offset(naive, offset);
            let translated = translate_pattern(&pattern);
            return RuntimeValue::String(datetime.format(&translated).to_string());
        }
    }
    RuntimeValue::Null
}

pub(crate) fn runtime_time_parse(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let input = args[0].as_string();
    let pattern = args[1].as_string();

    use chrono::DateTime;
    let translated = translate_pattern(&pattern);
    if let Some(ndt) =
        DateTime::parse_from_str(&format!("{} +0000", input), &format!("{} %z", translated))
            .ok()
            .or_else(|| {
                chrono::NaiveDateTime::parse_from_str(&input, &translated)
                    .ok()
                    .and_then(|n| Some(n.and_utc().fixed_offset()))
            })
    {
        let mut obj = crate::utils::collections::FastMap::default();
        obj.insert(
            "epoch_sec".to_string(),
            RuntimeValue::BigInt(num_bigint::BigInt::from(ndt.timestamp())),
        );
        obj.insert(
            "nanos".to_string(),
            RuntimeValue::Float(ndt.timestamp_subsec_nanos() as f64),
        );
        return RuntimeValue::Object(obj);
    }
    RuntimeValue::Null
}

pub(crate) fn runtime_time_parse_rfc3339(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let input = args[0].as_string();

    use chrono::{DateTime, Offset};
    if let Ok(dt) = DateTime::parse_from_rfc3339(&input) {
        let mut obj = crate::utils::collections::FastMap::default();
        obj.insert(
            "epoch_sec".to_string(),
            RuntimeValue::BigInt(num_bigint::BigInt::from(dt.timestamp())),
        );
        obj.insert(
            "nanos".to_string(),
            RuntimeValue::Float(dt.timestamp_subsec_nanos() as f64),
        );
        obj.insert(
            "offset_sec".to_string(),
            RuntimeValue::Float(dt.offset().fix().local_minus_utc() as f64),
        );
        return RuntimeValue::Object(obj);
    }
    RuntimeValue::Null
}
