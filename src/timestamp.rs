//! Strict UTC timestamp parsing. Numeric values outside i64 cannot fall in the
//! challenge's date windows and return `None`.

pub fn parse_ms(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if !b.is_empty() && b.iter().all(u8::is_ascii_digit) {
        return b.iter().try_fold(0_i64, |n, c| n.checked_mul(10)?.checked_add((c - b'0') as i64));
    }
    if b.len() != 20 && b.len() != 25 { return None; }
    if b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' { return None; }
    fn number(b: &[u8]) -> Option<i64> {
        b.iter().try_fold(0_i64, |n, c| c.is_ascii_digit().then_some(n * 10 + (c.wrapping_sub(b'0')) as i64))
    }
    let year = number(&b[0..4])?;
    let month = number(&b[5..7])?;
    let day = number(&b[8..10])?;
    let hour = number(&b[11..13])?;
    let minute = number(&b[14..16])?;
    let second = number(&b[17..19])?;
    if !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 { return None; }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month { 2 => 28 + i64::from(leap), 4 | 6 | 9 | 11 => 30, _ => 31 };
    if day < 1 || day > days { return None; }
    let offset = if b.len() == 20 {
        if b[19] != b'Z' { return None; }
        0
    } else {
        if (b[19] != b'+' && b[19] != b'-') || b[22] != b':' { return None; }
        let h = number(&b[20..22])?; let m = number(&b[23..25])?;
        if h > 23 || m > 59 { return None; }
        (h * 60 + m) * 60 * if b[19] == b'+' { 1 } else { -1 }
    };
    // Proleptic Gregorian civil date to days since 1970-01-01.
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days_since_epoch = era * 146097 + doe - 719468;
    Some((days_since_epoch * 86400 + hour * 3600 + minute * 60 + second - offset) * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn valid_forms() {
        assert_eq!(parse_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_ms("2026-10-03T23:58:10Z"), parse_ms("2026-10-03T19:58:10-04:00"));
        assert_eq!(parse_ms("00086400000"), Some(86_400_000));
        assert!(parse_ms("2028-02-29T00:00:00Z").is_some());
        assert_eq!(parse_ms("1970-01-01T00:00:00-00:00"), Some(0));
    }
    #[test] fn rejects_bad_forms() {
        for value in ["", "-5", "1.0", "2026-02-29T00:00:00Z", "2026-01-01T24:00:00Z", "2026-01-01T00:00:60Z", "2026-01-01t00:00:00Z", "2026-01-01T00:00:00Z\r", "2026-01-01T00:00:00+24:00", "2026-01-01T00:00:00+05:60", "99999999999999999999999999"] {
            assert_eq!(parse_ms(value), None, "{value}");
        }
    }
}
