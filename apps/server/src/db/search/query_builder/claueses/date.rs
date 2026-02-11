use crate::db::search::escape::unescape_search_value;
use chrono::{DateTime, Duration, FixedOffset, NaiveDate, TimeZone, Utc};

use super::super::bind::push_text;
use super::super::{BindValue, ResolvedParam, SearchPrefix};

pub(in crate::db::search::query_builder) fn build_date_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let mut parts = Vec::new();
    for v in &resolved.values {
        let prefix = v.prefix.unwrap_or(SearchPrefix::Eq);
        let (start, end) = fhir_date_range(&v.raw).ok()?;

        let clause = match prefix {
            // eq: parameter range fully contains resource range (FHIR 3.2.1.5.6).
            SearchPrefix::Eq => {
                let s_idx = push_text(bind_params, start.to_rfc3339());
                let e_idx = push_text(bind_params, end.to_rfc3339());
                format!(
                    "(sp.start_date >= ${}::timestamptz AND sp.end_date <= ${}::timestamptz)",
                    s_idx, e_idx
                )
            }
            // ne: ranges do not overlap
            SearchPrefix::Ne => {
                let s_idx = push_text(bind_params, start.to_rfc3339());
                let e_idx = push_text(bind_params, end.to_rfc3339());
                format!(
                    "(sp.end_date <= ${}::timestamptz OR sp.start_date >= ${}::timestamptz)",
                    s_idx, e_idx
                )
            }
            // gt: resource has any part > search value (after search.end)
            SearchPrefix::Gt => {
                let e_idx = push_text(bind_params, end.to_rfc3339());
                format!("sp.end_date > ${}::timestamptz", e_idx)
            }
            // ge: resource has any part >= search value (overlaps with [search.start, ∞))
            // CRITICAL: Use > not >= because if sp.end == search.start and end is exclusive,
            // there's no overlap (period [..., T) does not overlap with [T, ∞))
            SearchPrefix::Ge => {
                let s_idx = push_text(bind_params, start.to_rfc3339());
                format!("sp.end_date > ${}::timestamptz", s_idx)
            }
            // lt: resource has any part < search value (before search.start)
            SearchPrefix::Lt => {
                let s_idx = push_text(bind_params, start.to_rfc3339());
                format!("sp.start_date < ${}::timestamptz", s_idx)
            }
            // le: resource has any part <= search value (overlaps with (-∞, search.end))
            // CRITICAL: Use < not <= because if sp.start == search.end and end is exclusive,
            // there's no overlap (period [T, ...) does not overlap with (-∞, T))
            SearchPrefix::Le => {
                let e_idx = push_text(bind_params, end.to_rfc3339());
                format!("sp.start_date < ${}::timestamptz", e_idx)
            }
            // sa (starts after): period starts after search value ends
            SearchPrefix::Sa => {
                let e_idx = push_text(bind_params, end.to_rfc3339());
                format!("sp.start_date >= ${}::timestamptz", e_idx)
            }
            // eb (ends before): period ends before or at search value starts
            SearchPrefix::Eb => {
                let s_idx = push_text(bind_params, start.to_rfc3339());
                format!("sp.end_date <= ${}::timestamptz", s_idx)
            }
            // ap (approximately): 10% tolerance around the value
            SearchPrefix::Ap => {
                let (a_start, a_end) = approximate_date_range(start, end);
                let s_idx = push_text(bind_params, a_start.to_rfc3339());
                let e_idx = push_text(bind_params, a_end.to_rfc3339());
                format!(
                    "(sp.start_date < ${}::timestamptz AND sp.end_date > ${}::timestamptz)",
                    e_idx, s_idx
                )
            }
        };

        parts.push(clause);
    }

    if parts.is_empty() {
        None
    } else if parts.len() == 1 {
        Some(parts.remove(0))
    } else {
        Some(format!("({})", parts.join(" OR ")))
    }
}

pub(in crate::db::search::query_builder) fn build_last_updated_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> Option<String> {
    let mut parts = Vec::new();
    for v in &resolved.values {
        let prefix = v.prefix.unwrap_or(SearchPrefix::Eq);
        let (start, end) = fhir_date_range(&v.raw).ok()?;

        let clause = match prefix {
            SearchPrefix::Eq => {
                let s_idx = push_text(bind_params, start.to_rfc3339());
                let e_idx = push_text(bind_params, end.to_rfc3339());
                format!(
                    "({a}.last_updated >= ${}::timestamptz AND {a}.last_updated < ${}::timestamptz)",
                    s_idx,
                    e_idx,
                    a = resource_alias
                )
            }
            SearchPrefix::Ne => {
                let s_idx = push_text(bind_params, start.to_rfc3339());
                let e_idx = push_text(bind_params, end.to_rfc3339());
                format!(
                    "NOT ({a}.last_updated >= ${}::timestamptz AND {a}.last_updated < ${}::timestamptz)",
                    s_idx,
                    e_idx,
                    a = resource_alias
                )
            }
            SearchPrefix::Gt | SearchPrefix::Sa => {
                let e_idx = push_text(bind_params, end.to_rfc3339());
                format!(
                    "{a}.last_updated >= ${}::timestamptz",
                    e_idx,
                    a = resource_alias
                )
            }
            SearchPrefix::Ge => {
                let s_idx = push_text(bind_params, start.to_rfc3339());
                format!(
                    "{a}.last_updated >= ${}::timestamptz",
                    s_idx,
                    a = resource_alias
                )
            }
            SearchPrefix::Lt | SearchPrefix::Eb => {
                let s_idx = push_text(bind_params, start.to_rfc3339());
                format!(
                    "{a}.last_updated < ${}::timestamptz",
                    s_idx,
                    a = resource_alias
                )
            }
            SearchPrefix::Le => {
                let e_idx = push_text(bind_params, end.to_rfc3339());
                format!(
                    "{a}.last_updated < ${}::timestamptz",
                    e_idx,
                    a = resource_alias
                )
            }
            SearchPrefix::Ap => {
                let (a_start, a_end) = approximate_date_range(start, end);
                let s_idx = push_text(bind_params, a_start.to_rfc3339());
                let e_idx = push_text(bind_params, a_end.to_rfc3339());
                format!(
                    "({a}.last_updated >= ${}::timestamptz AND {a}.last_updated < ${}::timestamptz)",
                    s_idx,
                    e_idx,
                    a = resource_alias
                )
            }
        };

        parts.push(clause);
    }

    if parts.is_empty() {
        None
    } else if parts.len() == 1 {
        Some(parts.remove(0))
    } else {
        Some(format!("({})", parts.join(" OR ")))
    }
}

pub(in crate::db::search::query_builder) fn build_date_json_clause(
    idx: usize,
    raw_value: &str,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let v = unescape_search_value(raw_value).ok()?;
    let (prefix, rest) = SearchPrefix::parse_prefix(v.as_str());
    let prefix = prefix.unwrap_or(SearchPrefix::Eq);
    let (start, end) = fhir_date_range(rest).ok()?;

    let start_expr = format!("(sc.components->{}->>'start')::timestamptz", idx);
    let end_expr = format!("(sc.components->{}->>'end')::timestamptz", idx);

    let clause = match prefix {
        SearchPrefix::Eq => {
            let s_idx = push_text(bind_params, start.to_rfc3339());
            let e_idx = push_text(bind_params, end.to_rfc3339());
            format!(
                "({} >= ${}::timestamptz AND {} <= ${}::timestamptz)",
                start_expr, s_idx, end_expr, e_idx
            )
        }
        SearchPrefix::Ne => {
            let s_idx = push_text(bind_params, start.to_rfc3339());
            let e_idx = push_text(bind_params, end.to_rfc3339());
            format!(
                "({} <= ${}::timestamptz OR {} >= ${}::timestamptz)",
                end_expr, s_idx, start_expr, e_idx
            )
        }
        SearchPrefix::Gt => {
            let e_idx = push_text(bind_params, end.to_rfc3339());
            format!("{} > ${}::timestamptz", end_expr, e_idx)
        }
        SearchPrefix::Ge => {
            let s_idx = push_text(bind_params, start.to_rfc3339());
            format!("{} > ${}::timestamptz", end_expr, s_idx)
        }
        SearchPrefix::Lt => {
            let s_idx = push_text(bind_params, start.to_rfc3339());
            format!("{} < ${}::timestamptz", start_expr, s_idx)
        }
        SearchPrefix::Le => {
            let e_idx = push_text(bind_params, end.to_rfc3339());
            format!("{} < ${}::timestamptz", start_expr, e_idx)
        }
        SearchPrefix::Sa => {
            let e_idx = push_text(bind_params, end.to_rfc3339());
            format!("{} >= ${}::timestamptz", start_expr, e_idx)
        }
        SearchPrefix::Eb => {
            let s_idx = push_text(bind_params, start.to_rfc3339());
            format!("{} <= ${}::timestamptz", end_expr, s_idx)
        }
        SearchPrefix::Ap => {
            let (a_start, a_end) = approximate_date_range(start, end);
            let s_idx = push_text(bind_params, a_start.to_rfc3339());
            let e_idx = push_text(bind_params, a_end.to_rfc3339());
            format!(
                "({} < ${}::timestamptz AND {} > ${}::timestamptz)",
                start_expr, e_idx, end_expr, s_idx
            )
        }
    };

    Some(clause)
}

pub(crate) fn fhir_date_range(raw: &str) -> Result<(DateTime<Utc>, DateTime<Utc>), ()> {
    // Supported formats:
    // - YYYY
    // - YYYY-MM
    // - YYYY-MM-DD
    // - YYYY-MM-DDThh:mm(:ss(.s{1,6}))?(Z|±hh:mm)?
    let s = raw.trim();
    if s.len() == 4 && s.chars().all(|c| c.is_ascii_digit()) {
        let year: i32 = s.parse().map_err(|_| ())?;
        let start = Utc
            .with_ymd_and_hms(year, 1, 1, 0, 0, 0)
            .single()
            .ok_or(())?;
        let end = Utc
            .with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0)
            .single()
            .ok_or(())?;
        return Ok((start, end));
    }

    if s.len() == 7 && s.chars().nth(4) == Some('-') {
        let year: i32 = s[0..4].parse().map_err(|_| ())?;
        let month: u32 = s[5..7].parse().map_err(|_| ())?;
        let start = Utc
            .with_ymd_and_hms(year, month, 1, 0, 0, 0)
            .single()
            .ok_or(())?;
        let (ny, nm) = if month == 12 {
            (year + 1, 1)
        } else {
            (year, month + 1)
        };
        let end = Utc
            .with_ymd_and_hms(ny, nm, 1, 0, 0, 0)
            .single()
            .ok_or(())?;
        return Ok((start, end));
    }

    if s.len() == 10 && s.chars().nth(4) == Some('-') && s.chars().nth(7) == Some('-') {
        let date = NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| ())?;
        let start =
            DateTime::<Utc>::from_naive_utc_and_offset(date.and_hms_opt(0, 0, 0).ok_or(())?, Utc);
        let end = start + Duration::days(1);
        return Ok((start, end));
    }

    // DateTime parsing with optional seconds/fraction and timezone.
    let (dt_part, tz_part) = split_datetime_timezone(s);
    let (naive, unit) = parse_datetime_with_precision(dt_part)?;
    let offset = parse_tz_offset(tz_part)?;
    let dt = offset.from_local_datetime(&naive).single().ok_or(())?;
    let start = dt.with_timezone(&Utc);
    let end = (dt + unit).with_timezone(&Utc);
    Ok((start, end))
}

fn split_datetime_timezone(s: &str) -> (&str, &str) {
    if let Some(pos) = s.rfind('Z') {
        if pos == s.len() - 1 {
            return (&s[..pos], "Z");
        }
    }
    if let Some(pos) = s.rfind('+') {
        // timezone offset
        return (&s[..pos], &s[pos..]);
    }
    if let Some(pos) = s.rfind('-') {
        // could also be date separator; timezone offset has pattern ...T..-hh:mm
        if s[..pos].contains('T') && s[pos..].len() >= 6 {
            return (&s[..pos], &s[pos..]);
        }
    }
    (s, "")
}

fn parse_tz_offset(tz: &str) -> Result<FixedOffset, ()> {
    if tz.is_empty() || tz == "Z" {
        return FixedOffset::east_opt(0).ok_or(());
    }
    let sign = if tz.starts_with('+') { 1 } else { -1 };
    let t = tz.trim_start_matches(['+', '-']);
    let (h, m) = t.split_once(':').ok_or(())?;
    let hours: i32 = h.parse().map_err(|_| ())?;
    let mins: i32 = m.parse().map_err(|_| ())?;
    FixedOffset::east_opt(sign * (hours * 3600 + mins * 60)).ok_or(())
}

fn parse_datetime_with_precision(dt: &str) -> Result<(chrono::NaiveDateTime, Duration), ()> {
    // Expect `YYYY-MM-DDThh:mm` or `YYYY-MM-DDThh:mm:ss(.fraction)?`
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(dt, "%Y-%m-%dT%H:%M") {
        return Ok((naive, Duration::minutes(1)));
    }

    if let Some((base, frac)) = dt.split_once('.') {
        let naive =
            chrono::NaiveDateTime::parse_from_str(base, "%Y-%m-%dT%H:%M:%S").map_err(|_| ())?;
        let digits = frac
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .count()
            .min(6);
        if digits == 0 {
            return Ok((naive, Duration::seconds(1)));
        }
        let micros = 10_i64.pow((6 - digits) as u32);
        return Ok((naive, Duration::microseconds(micros)));
    }

    let naive = chrono::NaiveDateTime::parse_from_str(dt, "%Y-%m-%dT%H:%M:%S").map_err(|_| ())?;
    Ok((naive, Duration::seconds(1)))
}

fn approximate_date_range(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let duration = end - start;
    let min_delta = Duration::days(1);
    let approx = if duration > Duration::zero() {
        let ten_percent =
            Duration::microseconds((duration.num_microseconds().unwrap_or(0) as f64 * 0.1) as i64);
        if ten_percent > min_delta {
            ten_percent
        } else {
            min_delta
        }
    } else {
        min_delta
    };
    (start - approx, end + approx)
}
