//! Wall-clock helpers with WebAssembly fallbacks.
//!
//! The browser runtime (wasm32-unknown-unknown) is built without chrono's
//! `clock` feature: that feature pulls `iana-time-zone`, which requires the
//! wasm-bindgen JS glue, and the editor loads through a glue-free C-ABI
//! loader. Sandboxed WebAssembly also has no real system clock, so these
//! helpers return the Unix epoch there. Native builds behave normally.

use chrono::{DateTime, FixedOffset, TimeZone, Utc};

/// Nanoseconds since the Unix epoch. Deterministic zero in browser builds:
/// wasm32-unknown-unknown has no wall clock (`SystemTime::now` panics there).
pub fn epoch_nanos() -> u128 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

/// Milliseconds since the Unix epoch (zero in browser builds).
pub fn epoch_millis() -> u64 {
    (epoch_nanos() / 1_000_000) as u64
}

/// Current UTC time. Unix epoch in browser builds.
pub fn utc_now() -> DateTime<Utc> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Utc::now()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Utc.timestamp_opt(0, 0).unwrap()
    }
}

/// Current local time (native) / epoch UTC (browser builds).
pub fn local_now() -> DateTime<FixedOffset> {
    utc_to_local(&utc_now())
}

/// Render a UTC instant in the machine's local timezone (native) or as UTC
/// (browser builds).
pub fn utc_to_local(dt: &DateTime<Utc>) -> DateTime<FixedOffset> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use chrono::Offset;
        let local = dt.with_timezone(&chrono::Local);
        local.with_timezone(&local.offset().fix())
    }
    #[cfg(target_arch = "wasm32")]
    {
        dt.with_timezone(&FixedOffset::east(0))
    }
}
