//! Exact MiB input and destructive-change policy shared by native client presentations.
use super::{MediaCacheConfig, MIB};

fn decimal(text: &str) -> Option<(u128, i32)> {
    let text = text.strip_prefix('+').unwrap_or(text);
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().ok()?),
        None => (text, 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let value = whole
        .bytes()
        .chain(fraction.bytes())
        .try_fold(0u128, |value, digit| {
            digit.is_ascii_digit().then_some(())?;
            value.checked_mul(10)?.checked_add(u128::from(digit - b'0'))
        })?;
    Some((
        value,
        exponent.checked_sub(i32::try_from(fraction.len()).ok()?)?,
    ))
}

fn scale(value: u128, power: i32) -> Option<u128> {
    let factor = 10u128.checked_pow(power.unsigned_abs())?;
    if power >= 0 {
        value.checked_mul(factor)
    } else {
        value.is_multiple_of(factor).then_some(value / factor)
    }
}

/// Parse a finite decimal MiB quantity into an exact integral-byte budget.
///
/// Supports decimal exponents and surrounding shared input whitespace. Rejects fractions of a
/// byte, overflow and values outside the existing cache range; no floating-point rounding.
#[must_use]
pub fn parse_mib(text: &str) -> Option<u64> {
    let (value, power) = decimal(dyapp_profile::text_input::trim(text))?;
    let bytes = u64::try_from(scale(value.checked_mul(u128::from(MIB))?, power)?).ok()?;
    MediaCacheConfig {
        enabled: true,
        limit_bytes: bytes,
    }
    .validate()
    .ok()
    .map(|config| config.limit_bytes)
}

/// Exact decimal MiB text, including a non-whole-MiB persisted budget.
#[must_use]
pub fn mib_text(bytes: u64) -> String {
    let whole = bytes / MIB;
    // 2^20 divides 10^20, so every integral-byte remainder has an exact finite expansion.
    let fraction = u128::from(bytes % MIB) * 10u128.pow(20) / u128::from(MIB);
    if fraction == 0 {
        return whole.to_string();
    }
    format!("{whole}.{fraction:020}")
        .trim_end_matches('0')
        .to_owned()
}

/// Whether applying this change can erase retained originals or lowers an active budget.
#[must_use]
pub fn requires_confirmation(
    current: MediaCacheConfig,
    next: MediaCacheConfig,
    used_bytes: u64,
) -> bool {
    (current.enabled && (!next.enabled || next.limit_bytes < current.limit_bytes))
        || (used_bytes > 0 && (!next.enabled || next.limit_bytes < used_bytes))
}

#[cfg(test)]
mod tests;
