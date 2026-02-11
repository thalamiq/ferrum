//! Temporal functions for FHIRPath.
//!
//! This module implements temporal-related functions. Note that `now()`, `today()`, and
//! `timeOfDay()` are implemented in the utility module.

// Parse partial dateTime strings allowed by FHIRPath (e.g., YYYY, YYYY-MM, YYYY-MM-DDThh, YYYY-MM-DDThh:mm, YYYY-MM-DDThh:mm:ss(.fff)(zzz))
pub(super) fn parse_partial_datetime(input: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    // Try RFC3339 first (already handled elsewhere)
    // Then try progressively more specific formats
    const FORMATS: &[&str] = &[
        "%Y-%m-%dT%H:%M:%S%.f%:z",
        "%Y-%m-%dT%H:%M:%S%:z",
        "%Y-%m-%dT%H:%M%:z",
        "%Y-%m-%dT%H%:z",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%dT%H",
        "%Y-%m-%d",
        "%Y-%m",
        "%Y",
    ];

    for fmt in FORMATS {
        if let Ok(dt) = chrono::DateTime::parse_from_str(input, fmt) {
            return Some(dt.with_timezone(&chrono::Utc));
        }
    }
    // Handle timezone without colon (e.g., +0000)
    if let Ok(dt) = chrono::DateTime::parse_from_str(input, "%Y-%m-%dT%H:%M:%S%z") {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    None
}

/// Check if a string is a valid partial or full date (YYYY, YYYY-MM, YYYY-MM-DD)
pub(super) fn is_valid_date_string(s: &str) -> bool {
    // Pattern: YYYY
    if s.len() == 4 && s.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(year) = s.parse::<i32>() {
            return (1..=9999).contains(&year);
        }
    }

    // Pattern: YYYY-MM
    if s.len() == 7 && s.as_bytes()[4] == b'-' {
        let year = &s[0..4];
        let month = &s[5..7];
        if year.chars().all(|c| c.is_ascii_digit()) && month.chars().all(|c| c.is_ascii_digit()) {
            if let (Ok(y), Ok(m)) = (year.parse::<i32>(), month.parse::<u32>()) {
                return (1..=9999).contains(&y) && (1..=12).contains(&m);
            }
        }
    }

    // Pattern: YYYY-MM-DD
    if s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' {
        let year = &s[0..4];
        let month = &s[5..7];
        let day = &s[8..10];
        if year.chars().all(|c| c.is_ascii_digit())
            && month.chars().all(|c| c.is_ascii_digit())
            && day.chars().all(|c| c.is_ascii_digit())
        {
            if let (Ok(y), Ok(m), Ok(d)) = (
                year.parse::<i32>(),
                month.parse::<u32>(),
                day.parse::<u32>(),
            ) {
                if (1..=9999).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d) {
                    // Verify it's an actual valid date
                    return chrono::NaiveDate::from_ymd_opt(y, m, d).is_some();
                }
            }
        }
    }

    false
}

/// Check if a string is a valid partial or full datetime
pub(super) fn is_valid_datetime_string(s: &str) -> bool {
    // First check if it's just a date
    if is_valid_date_string(s) {
        return true;
    }

    // Check for datetime patterns: YYYY-MM-DDThh, YYYY-MM-DDThh:mm, YYYY-MM-DDThh:mm:ss, YYYY-MM-DDThh:mm:ss.fff
    // With optional timezone
    if s.len() < 13 {
        return false; // Minimum is YYYY-MM-DDThh
    }

    // Check if we have a 'T' separator at position 10
    if s.len() >= 13 && s.as_bytes()[10] == b'T' {
        let date_part = &s[0..10];
        if !is_valid_date_string(date_part) {
            return false;
        }

        let time_tz_part = &s[11..];

        // Check for timezone markers
        let (time_part, _tz_part) = if let Some(pos) = time_tz_part.find('+') {
            time_tz_part.split_at(pos)
        } else if let Some(pos) = time_tz_part.find('-') {
            time_tz_part.split_at(pos)
        } else if let Some(stripped) = time_tz_part.strip_suffix('Z') {
            (stripped, "Z")
        } else {
            (time_tz_part, "")
        };

        // Validate time part: hh, hh:mm, hh:mm:ss, hh:mm:ss.fff
        return is_valid_time_string(time_part);
    }

    false
}

/// Check if a string is a valid partial or full time (HH, HH:MM, HH:MM:SS, HH:MM:SS.fff)
pub(super) fn is_valid_time_string(s: &str) -> bool {
    // Pattern: HH
    if s.len() == 2 && s.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(h) = s.parse::<u32>() {
            return h <= 23;
        }
    }

    // Pattern: HH:MM
    if s.len() == 5 && s.as_bytes()[2] == b':' {
        let hour = &s[0..2];
        let min = &s[3..5];
        if hour.chars().all(|c| c.is_ascii_digit()) && min.chars().all(|c| c.is_ascii_digit()) {
            if let (Ok(h), Ok(m)) = (hour.parse::<u32>(), min.parse::<u32>()) {
                return h <= 23 && m <= 59;
            }
        }
    }

    // Pattern: HH:MM:SS or HH:MM:SS.fff
    if s.len() >= 8 && s.as_bytes()[2] == b':' && s.as_bytes()[5] == b':' {
        let hour = &s[0..2];
        let min = &s[3..5];
        let sec_and_frac = &s[6..];

        if !hour.chars().all(|c| c.is_ascii_digit()) || !min.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }

        // Check seconds (and optional fractional part)
        let sec = if let Some(dot_pos) = sec_and_frac.find('.') {
            let sec_part = &sec_and_frac[0..dot_pos];
            let frac_part = &sec_and_frac[dot_pos + 1..];
            if !frac_part.chars().all(|c| c.is_ascii_digit()) {
                return false;
            }
            sec_part
        } else {
            sec_and_frac
        };

        if !sec.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }

        if let (Ok(h), Ok(m), Ok(s)) = (hour.parse::<u32>(), min.parse::<u32>(), sec.parse::<u32>())
        {
            return h <= 23 && m <= 59 && s <= 59;
        }
    }

    false
}
