use chrono::{FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

use crate::value::{DatePrecision, DateTimePrecision, TimePrecision, Value};

pub(crate) fn parse_date_value(input: &str) -> Option<Value> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }

    match s.len() {
        4 => {
            let date = NaiveDate::parse_from_str(&format!("{}-01-01", s), "%Y-%m-%d").ok()?;
            Some(Value::date_with_precision(date, DatePrecision::Year))
        }
        7 => {
            let date = NaiveDate::parse_from_str(&format!("{}-01", s), "%Y-%m-%d").ok()?;
            Some(Value::date_with_precision(date, DatePrecision::Month))
        }
        10 => {
            let date = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
            Some(Value::date_with_precision(date, DatePrecision::Day))
        }
        _ => None,
    }
}

pub(crate) fn parse_time_value(input: &str) -> Option<Value> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }

    let precision = if s.contains('.') {
        TimePrecision::Millisecond
    } else if s.matches(':').count() >= 2 {
        TimePrecision::Second
    } else if s.contains(':') {
        TimePrecision::Minute
    } else {
        TimePrecision::Hour
    };

    let value = NaiveTime::parse_from_str(s, "%H:%M:%S%.f")
        .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M:%S"))
        .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M"))
        .or_else(|_| NaiveTime::parse_from_str(s, "%H"))
        .ok()?;

    Some(Value::time_with_precision(value, precision))
}

pub(crate) fn parse_datetime_value_lenient(input: &str) -> Option<Value> {
    let raw = input.trim();
    if raw.is_empty() {
        return None;
    }

    // Date-only values are treated as dateTime with date precision.
    let Some((date_part, rest)) = raw.split_once('T') else {
        let date_value = parse_date_value(raw)?;
        let (date, date_prec) = match date_value.data() {
            crate::value::ValueData::Date { value, precision } => (*value, *precision),
            _ => return None,
        };
        let dt_naive = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0)?);
        let dt_utc = chrono::DateTime::<Utc>::from_naive_utc_and_offset(dt_naive, Utc);
        let dt_prec = match date_prec {
            DatePrecision::Year => DateTimePrecision::Year,
            DatePrecision::Month => DateTimePrecision::Month,
            DatePrecision::Day => DateTimePrecision::Day,
        };
        return Some(Value::datetime_with_precision_and_offset(
            dt_utc, dt_prec, None,
        ));
    };

    // Parse date part.
    let date_value = parse_date_value(date_part)?;
    let date = match date_value.data() {
        crate::value::ValueData::Date { value, .. } => *value,
        _ => return None,
    };

    let (time_part, tz_offset) = parse_timezone(rest)?;
    let (time, precision) = parse_datetime_time(time_part)?;

    let local = NaiveDateTime::new(date, time);
    let dt_utc = if let Some(offset_secs) = tz_offset {
        let offset = FixedOffset::east_opt(offset_secs)?;
        offset
            .from_local_datetime(&local)
            .single()?
            .with_timezone(&Utc)
    } else {
        chrono::DateTime::<Utc>::from_naive_utc_and_offset(local, Utc)
    };

    Some(Value::datetime_with_precision_and_offset(
        dt_utc, precision, tz_offset,
    ))
}

pub(crate) fn parse_temporal_pair(left: &str, right: &str) -> Option<(Value, Value)> {
    let left = left.trim();
    let right = right.trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }

    let left_has_t = left.contains('T');
    let right_has_t = right.contains('T');
    let left_has_colon = left.contains(':');
    let right_has_colon = right.contains(':');

    if left_has_t || right_has_t {
        return Some((
            parse_datetime_value_lenient(left)?,
            parse_datetime_value_lenient(right)?,
        ));
    }

    if left_has_colon || right_has_colon {
        return Some((parse_time_value(left)?, parse_time_value(right)?));
    }

    // Only treat as date if it matches common FHIR date patterns (YYYY, YYYY-MM, YYYY-MM-DD).
    let looks_like_date =
        |s: &str| (s.len() == 4 && s.chars().all(|c| c.is_ascii_digit())) || s.contains('-');

    if looks_like_date(left) && looks_like_date(right) {
        return Some((parse_date_value(left)?, parse_date_value(right)?));
    }

    None
}

fn parse_timezone(rest: &str) -> Option<(&str, Option<i32>)> {
    if let Some(stripped) = rest.strip_suffix('Z') {
        return Some((stripped, Some(0)));
    }

    if let Some(pos) = rest.rfind(['+', '-']) {
        let (time, tz) = rest.split_at(pos);
        let tz = tz.trim();
        if tz.len() >= 6 && tz.as_bytes().get(3) == Some(&b':') {
            let sign = if tz.starts_with('-') { -1 } else { 1 };
            let hours: i32 = tz[1..3].parse().ok()?;
            let minutes: i32 = tz[4..6].parse().ok()?;
            return Some((time, Some(sign * (hours * 3600 + minutes * 60))));
        }
        if tz.len() == 5 {
            let sign = if tz.starts_with('-') { -1 } else { 1 };
            let hours: i32 = tz[1..3].parse().ok()?;
            let minutes: i32 = tz[3..5].parse().ok()?;
            return Some((time, Some(sign * (hours * 3600 + minutes * 60))));
        }
    }

    Some((rest, None))
}

fn parse_datetime_time(time_part: &str) -> Option<(NaiveTime, DateTimePrecision)> {
    let time_part = time_part.trim();
    if time_part.is_empty() {
        return Some((NaiveTime::from_hms_opt(0, 0, 0)?, DateTimePrecision::Day));
    }

    let (main, frac) = time_part
        .split_once('.')
        .map(|(a, b)| (a, Some(b)))
        .unwrap_or((time_part, None));

    let parts: Vec<&str> = main.split(':').collect();
    let (hour_str, minute_str, second_str, precision) = match parts.as_slice() {
        [hh] => (hh.trim(), "0", "0", DateTimePrecision::Minute),
        [hh, mm] => (hh.trim(), mm.trim(), "0", DateTimePrecision::Minute),
        [hh, mm, ss] => (
            hh.trim(),
            mm.trim(),
            ss.trim(),
            if frac.is_some() {
                DateTimePrecision::Millisecond
            } else {
                DateTimePrecision::Second
            },
        ),
        _ => return None,
    };

    let hour: u32 = hour_str.parse().ok()?;
    let minute: u32 = minute_str.parse().ok()?;
    let second: u32 = second_str.parse().ok()?;

    let nanos: u32 = if let Some(frac) = frac {
        let digits: String = frac.chars().take(3).collect();
        let padded = format!("{:0<3}", digits);
        let ms: u32 = padded.parse().ok()?;
        ms * 1_000_000
    } else {
        0
    };

    let time = NaiveTime::from_hms_nano_opt(hour, minute, second, nanos)?;
    Some((time, precision))
}
