//! Time and Date
//!
//! High-resolution monotonic timers and epoch timestamps:
//! - `clock`: seconds since program start
//! - `time.now*`: nanos/micros/millis/seconds using monotonic clock
//! - `time.epoch*`: wall-clock epoch timestamps
//! - `Date(ms?)`: construct a simple date object wrapper
use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::stdlib::registry::BuiltinRegistry;
use once_cell::sync::Lazy;
use rustc_hash::FxHashMap as HashMap;
use std::time::Instant;

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "time",
        "system",
        "Time namespace",
        |_env: &mut dyn BuiltinEnv, _args: Vec<Value>| {
            let mut methods = HashMap::default();
            methods.insert(
                "now".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(builtin_time_now))),
            );
            methods.insert(
                "nowMs".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(builtin_time_now_ms))),
            );
            methods.insert(
                "nowUs".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(builtin_time_now_us))),
            );
            methods.insert(
                "nowSecs".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(builtin_time_now_secs))),
            );
            methods.insert(
                "epoch".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(builtin_time_epoch))),
            );
            methods.insert(
                "epochNanos".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(builtin_time_epoch_nanos))),
            );
            methods.insert(
                "systemNow".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(builtin_time_system_now))),
            );
            methods.insert(
                "monotonicNow".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(builtin_time_monotonic_now))),
            );
            methods.insert(
                "sleepNanos".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(builtin_time_sleep_nanos))),
            );
            methods.insert(
                "localOffsetSecs".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(
                    builtin_time_local_offset_secs,
                ))),
            );
            methods.insert(
                "timezoneOffsetSecs".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(
                    builtin_time_timezone_offset_secs,
                ))),
            );
            methods.insert(
                "format".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(builtin_time_format))),
            );
            methods.insert(
                "parse".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(builtin_time_parse))),
            );
            methods.insert(
                "parseRfc3339".to_string(),
                Value::Function(NativeFn(std::sync::Arc::new(builtin_time_parse_rfc3339))),
            );
            Ok(Value::Object(std::sync::Arc::new(methods)))
        },
    );
    registry.register(
        "Date",
        "system",
        "Construct a Date-like object",
        builtin_date,
    );
    registry.register(
        "clock",
        "system",
        "High-resolution monotonic time in seconds",
        builtin_clock,
    );
    // High-resolution nanosecond time functions
    registry.register(
        "time.now",
        "system",
        "Current time in nanoseconds (monotonic)",
        builtin_time_now,
    );
    registry.register(
        "time.nowMs",
        "system",
        "Current time in milliseconds (monotonic)",
        builtin_time_now_ms,
    );
    registry.register(
        "time.nowUs",
        "system",
        "Current time in microseconds (monotonic)",
        builtin_time_now_us,
    );
    registry.register(
        "time.nowSecs",
        "system",
        "Current time in seconds with high precision (monotonic)",
        builtin_time_now_secs,
    );
    registry.register(
        "time.epoch",
        "system",
        "Unix epoch timestamp in milliseconds",
        builtin_time_epoch,
    );
    registry.register(
        "time.epochNanos",
        "system",
        "Unix epoch timestamp in nanoseconds",
        builtin_time_epoch_nanos,
    );
    registry.register(
        "time.systemNow",
        "system",
        "Unix epoch timestamp in nanoseconds",
        builtin_time_system_now,
    );
    registry.register(
        "time.monotonicNow",
        "system",
        "Monotonic timestamp in nanoseconds",
        builtin_time_monotonic_now,
    );
    registry.register(
        "time.sleepNanos",
        "system",
        "Sleep for nanoseconds",
        builtin_time_sleep_nanos,
    );
    registry.register(
        "time.localOffsetSecs",
        "system",
        "UTC offset of local timezone at epoch",
        builtin_time_local_offset_secs,
    );
    registry.register(
        "time.timezoneOffsetSecs",
        "system",
        "UTC offset of named timezone at epoch",
        builtin_time_timezone_offset_secs,
    );
    registry.register(
        "time.format",
        "system",
        "Format epoch time with pattern",
        builtin_time_format,
    );
    registry.register(
        "time.parse",
        "system",
        "Parse date time string with pattern",
        builtin_time_parse,
    );
    registry.register(
        "time.parseRfc3339",
        "system",
        "Parse RFC3339 datetime string",
        builtin_time_parse_rfc3339,
    );
    registry.register(
        "time.isoString",
        "system",
        "Current local ISO date time string",
        |_env: &mut dyn BuiltinEnv, _args: Vec<Value>| {
            let now = chrono::Local::now();
            Ok(Value::Str(now.format("%Y-%m-%d %H:%M:%S").to_string()))
        },
    );
}

fn value_to_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => Some(*n as i64),
        Value::BigInt(b) => num_traits::ToPrimitive::to_i64(b),
        Value::I8(n) => Some(*n as i64),
        Value::I16(n) => Some(*n as i64),
        Value::I32(n) => Some(*n as i64),
        Value::I64(n) => Some(*n),
        Value::I128(n) => num_traits::ToPrimitive::to_i64(n),
        Value::U8(n) => Some(*n as i64),
        Value::U16(n) => Some(*n as i64),
        Value::U32(n) => Some(*n as i64),
        Value::U64(n) => num_traits::ToPrimitive::to_i64(n),
        Value::U128(n) => num_traits::ToPrimitive::to_i64(n),
        Value::F32(n) => Some(*n as i64),
        Value::F64(n) => Some(*n as i64),
        _ => None,
    }
}

fn value_to_i128(v: &Value) -> Option<i128> {
    match v {
        Value::Number(n) => Some(*n as i128),
        Value::BigInt(b) => num_traits::ToPrimitive::to_i128(b),
        Value::I8(n) => Some(*n as i128),
        Value::I16(n) => Some(*n as i128),
        Value::I32(n) => Some(*n as i128),
        Value::I64(n) => Some(*n as i128),
        Value::I128(n) => Some(*n),
        Value::U8(n) => Some(*n as i128),
        Value::U16(n) => Some(*n as i128),
        Value::U32(n) => Some(*n as i128),
        Value::U64(n) => Some(*n as i128),
        Value::U128(n) => Some(*n as i128),
        Value::F32(n) => Some(*n as i128),
        Value::F64(n) => Some(*n as i128),
        _ => None,
    }
}

fn builtin_date(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let ts_ms: i128 = if args.is_empty() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "system time error".to_string())?;
        now.as_millis() as i128
    } else {
        value_to_i128(&args[0]).ok_or_else(|| "Date(ms) expects number or bigint".to_string())?
    };
    let mut m: HashMap<String, Value> = HashMap::default();
    m.insert(
        "__date_ts".into(),
        Value::BigInt(num_bigint::BigInt::from(ts_ms)),
    );
    Ok(Value::Object(m.into()))
}

static START: Lazy<Instant> = Lazy::new(|| Instant::now());

fn builtin_clock(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let secs = START.elapsed().as_secs_f64();
    Ok(Value::Number(secs))
}

/// High-resolution time in nanoseconds (monotonic clock)
fn builtin_time_now(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let nanos = START.elapsed().as_nanos();
    // Return as BigInt for full precision
    Ok(Value::BigInt(num_bigint::BigInt::from(nanos)))
}

/// High-resolution time in milliseconds (monotonic clock)
fn builtin_time_now_ms(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let millis = START.elapsed().as_millis() as f64;
    Ok(Value::Number(millis))
}

/// High-resolution time in microseconds (monotonic clock)
fn builtin_time_now_us(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let micros = START.elapsed().as_micros() as f64;
    Ok(Value::Number(micros))
}

/// High-resolution time in seconds with fractional precision (monotonic clock)
fn builtin_time_now_secs(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let secs = START.elapsed().as_secs_f64();
    Ok(Value::Number(secs))
}

/// Unix epoch timestamp in milliseconds
fn builtin_time_epoch(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system time error".to_string())?;
    Ok(Value::Number(now.as_millis() as f64))
}

/// Unix epoch timestamp in nanoseconds
fn builtin_time_epoch_nanos(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system time error".to_string())?;
    // Return as BigInt for full precision
    Ok(Value::BigInt(num_bigint::BigInt::from(now.as_nanos())))
}

fn builtin_time_system_now(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system time error".to_string())?;
    Ok(Value::BigInt(num_bigint::BigInt::from(now.as_nanos())))
}

fn builtin_time_monotonic_now(
    _env: &mut dyn BuiltinEnv,
    _args: Vec<Value>,
) -> Result<Value, String> {
    let nanos = START.elapsed().as_nanos();
    Ok(Value::BigInt(num_bigint::BigInt::from(nanos)))
}

fn builtin_time_sleep_nanos(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("time.sleepNanos expects 1 argument".to_string());
    }
    let nanos = value_to_i64(&args[0])
        .map(|n| n as u64)
        .ok_or_else(|| "time.sleepNanos expects number or bigint".to_string())?;
    std::thread::sleep(std::time::Duration::from_nanos(nanos));
    Ok(Value::Null)
}

fn builtin_time_local_offset_secs(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("time.localOffsetSecs expects 1 argument".to_string());
    }
    let epoch_sec = value_to_i64(&args[0])
        .ok_or_else(|| "time.localOffsetSecs expects number or bigint".to_string())?;
    use chrono::{Local, Offset, TimeZone};
    let local_time = Local
        .timestamp_opt(epoch_sec, 0)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).unwrap());
    let offset_secs = local_time.offset().fix().local_minus_utc();
    Ok(Value::Number(offset_secs as f64))
}

fn builtin_time_timezone_offset_secs(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("time.timezoneOffsetSecs expects 2 arguments".to_string());
    }
    let tz_name = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("time.timezoneOffsetSecs first argument must be string".to_string()),
    };
    let epoch_sec = value_to_i64(&args[1]).ok_or_else(|| {
        "time.timezoneOffsetSecs second argument must be number or bigint".to_string()
    })?;
    use chrono::TimeZone;
    use chrono_tz::Tz;
    let tz: Tz = tz_name
        .parse()
        .map_err(|_| format!("Unknown timezone: {}", tz_name))?;
    let datetime = tz
        .timestamp_opt(epoch_sec, 0)
        .single()
        .ok_or_else(|| format!("Invalid timestamp for timezone: {}", epoch_sec))?;
    use chrono::Offset;
    let offset_secs = datetime.offset().fix().local_minus_utc();
    Ok(Value::Number(offset_secs as f64))
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

fn builtin_time_format(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 4 {
        return Err("time.format expects 4 arguments".to_string());
    }
    let epoch_sec = value_to_i64(&args[0])
        .ok_or_else(|| "time.format epoch_sec must be number or bigint".to_string())?;
    let nanos = value_to_i64(&args[1])
        .map(|n| n as u32)
        .ok_or_else(|| "time.format nanos must be number or bigint".to_string())?;
    let offset_sec = value_to_i64(&args[2])
        .map(|n| n as i32)
        .ok_or_else(|| "time.format offset_sec must be number or bigint".to_string())?;
    let pattern = match &args[3] {
        Value::Str(s) => s.as_str(),
        _ => return Err("time.format pattern must be string".to_string()),
    };

    use chrono::{DateTime, FixedOffset};
    let utc_dt = DateTime::from_timestamp(epoch_sec, nanos)
        .ok_or_else(|| "Invalid timestamp".to_string())?;
    let naive = utc_dt.naive_utc();
    let offset = FixedOffset::east_opt(offset_sec).ok_or_else(|| "Invalid offset".to_string())?;
    let datetime = DateTime::<FixedOffset>::from_naive_utc_and_offset(naive, offset);
    let translated = translate_pattern(pattern);
    Ok(Value::Str(datetime.format(&translated).to_string()))
}

fn builtin_time_parse(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("time.parse expects 2 arguments".to_string());
    }
    let input = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("time.parse input must be string".to_string()),
    };
    let pattern = match &args[1] {
        Value::Str(s) => s.as_str(),
        _ => return Err("time.parse pattern must be string".to_string()),
    };

    use chrono::NaiveDateTime;
    let translated = translate_pattern(pattern);
    let ndt = NaiveDateTime::parse_from_str(input, &translated)
        .map_err(|e| format!("Parse error: {}", e))?;

    let mut methods = HashMap::default();
    methods.insert(
        "epoch_sec".to_string(),
        Value::BigInt(num_bigint::BigInt::from(ndt.and_utc().timestamp())),
    );
    methods.insert(
        "nanos".to_string(),
        Value::Number(ndt.and_utc().timestamp_subsec_nanos() as f64),
    );
    Ok(Value::Object(std::sync::Arc::new(methods)))
}

fn builtin_time_parse_rfc3339(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("time.parseRfc3339 expects 1 argument".to_string());
    }
    let input = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("time.parseRfc3339 input must be string".to_string()),
    };

    use chrono::{DateTime, Offset};
    let dt = DateTime::parse_from_rfc3339(input).map_err(|e| format!("Parse error: {}", e))?;

    let mut methods = HashMap::default();
    methods.insert(
        "epoch_sec".to_string(),
        Value::BigInt(num_bigint::BigInt::from(dt.timestamp())),
    );
    methods.insert(
        "nanos".to_string(),
        Value::Number(dt.timestamp_subsec_nanos() as f64),
    );
    methods.insert(
        "offset_sec".to_string(),
        Value::Number(dt.offset().fix().local_minus_utc() as f64),
    );
    Ok(Value::Object(std::sync::Arc::new(methods)))
}
