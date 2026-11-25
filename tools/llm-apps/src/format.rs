//! Number formatting that matches the JavaScript the reports were first written in, so
//! regenerated reports and JSON stay byte-compatible with earlier ones.

use serde_json::Value;

/// `Number.prototype.toFixed`. Both languages round the exact binary value to the
/// nearest decimal, but on an exact tie Rust rounds to even and JavaScript rounds up.
pub fn to_fixed(x: f64, digits: usize) -> String {
    if x.is_nan() {
        return "NaN".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let abs = x.abs();
    // Every f64 has a finite decimal expansion of at most 1074 fractional digits.
    let exact = format!("{abs:.1074}");
    let (int, frac) = exact.split_once('.').expect("fractional digits");
    let rest = &frac[digits..];
    let is_tie = rest.starts_with('5') && rest[1..].bytes().all(|b| b == b'0');
    if !is_tie {
        return format!("{sign}{abs:.digits$}");
    }
    // Round the truncated digits up by one unit in the last place.
    let mut kept: Vec<u8> = format!("{int}{}", &frac[..digits]).into_bytes();
    let mut i = kept.len();
    loop {
        if i == 0 {
            kept.insert(0, b'1');
            break;
        }
        i -= 1;
        if kept[i] == b'9' {
            kept[i] = b'0';
        } else {
            kept[i] += 1;
            break;
        }
    }
    let kept = String::from_utf8(kept).expect("ascii digits");
    let (int, frac) = kept.split_at(kept.len() - digits);
    if digits == 0 {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}.{frac}")
    }
}

/// `Number.prototype.toLocaleString()` in en-US, for the values reports print with it.
pub fn grouped(x: f64) -> String {
    if !x.is_finite() {
        return to_fixed(x, 0);
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let fixed = to_fixed(x.abs(), 3);
    let (int, frac) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let frac = frac.trim_end_matches('0');
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if frac.is_empty() {
        format!("{sign}{out}")
    } else {
        format!("{sign}{out}.{frac}")
    }
}

/// How JavaScript prints a number in a template string: `36`, `34.5`.
pub fn js_number(x: f64) -> String {
    if x.is_nan() {
        "NaN".to_owned()
    } else {
        format!("{x}")
    }
}

/// A JSON number the way `JSON.stringify` writes it: whole numbers without a fraction,
/// and `null` for values JSON cannot hold.
pub fn json_number(x: f64) -> Value {
    if x.is_finite() && x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
        Value::from(x as i64)
    } else {
        serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_fixed_rounds_ties_up_like_javascript() {
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(1.005, 2), "1.00"); // 1.005 is just below the tie in binary
        assert_eq!(to_fixed(-0.25, 1), "-0.3");
        assert_eq!(to_fixed(91.666_666_666_666_67, 1), "91.7");
        assert_eq!(to_fixed(0.0, 4), "0.0000");
        assert_eq!(to_fixed(9.5, 0), "10");
        assert_eq!(to_fixed(0.995, 2), "0.99"); // just below the tie
                                                // Multiplying by 10^4 would make this an exact tie; its true value is below one.
        assert_eq!(to_fixed(0.79455, 4), "0.7945");
    }

    #[test]
    fn grouped_matches_en_us_locale_string() {
        assert_eq!(grouped(0.0), "0");
        assert_eq!(grouped(999.0), "999");
        assert_eq!(grouped(1_234_567.0), "1,234,567");
        assert_eq!(grouped(1234.5), "1,234.5");
        assert_eq!(grouped(-1000.0), "-1,000");
    }

    #[test]
    fn numbers_print_like_javascript() {
        assert_eq!(js_number(36.0), "36");
        assert_eq!(js_number(34.5), "34.5");
        assert_eq!(json_number(36.0), Value::from(36));
        assert_eq!(json_number(91.666_666_666_666_67).to_string(), "91.66666666666667");
        assert_eq!(json_number(f64::NAN), Value::Null);
    }
}
