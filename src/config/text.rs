//! The text forms of setting values, shared by the file format and the
//! environment variables.

use std::time::Duration;

/// Why a duration could not be read. It never repeats the text, so callers
/// can report it without exposing the value.
pub(crate) const DURATION_FORMAT: &str = "write a whole number and ms, s, m, or h";

/// Reads a whole number followed by `ms`, `s`, `m`, or `h`.
pub(crate) fn parse_duration(text: &str) -> Result<Duration, &'static str> {
    let mut digits = 0;
    for byte in text.bytes() {
        if !byte.is_ascii_digit() {
            break;
        }
        digits += 1;
    }
    let (number, unit) = text.split_at(digits);
    let Ok(number) = number.parse::<u64>() else {
        return Err(DURATION_FORMAT);
    };
    let seconds_per_unit = match unit {
        "ms" => return Ok(Duration::from_millis(number)),
        "s" => 1,
        "m" => 60,
        "h" => 3_600,
        _ => return Err(DURATION_FORMAT),
    };
    match number.checked_mul(seconds_per_unit) {
        Some(seconds) => Ok(Duration::from_secs(seconds)),
        None => Err(DURATION_FORMAT),
    }
}

/// Writes a duration in the largest unit that represents it exactly.
///
/// Fails for a duration with a fraction of a millisecond.
#[cfg_attr(
    not(feature = "serde"),
    expect(dead_code, reason = "only the file format writes durations")
)]
pub(crate) fn format_duration(value: Duration) -> Result<String, String> {
    if value.subsec_nanos() % 1_000_000 != 0 {
        return Err(format!(
            "{value:?} has a fraction of a millisecond, which the text form cannot hold"
        ));
    }
    if value.subsec_millis() != 0 {
        return Ok(format!("{}ms", value.as_millis()));
    }
    Ok(match value.as_secs() {
        0 => "0s".to_owned(),
        s if s % 3_600 == 0 => format!("{}h", s / 3_600),
        s if s % 60 == 0 => format!("{}m", s / 60),
        s => format!("{s}s"),
    })
}
