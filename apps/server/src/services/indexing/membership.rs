use crate::{models::Resource, Result};
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::Value;

use super::extract::extract_reference_values;

fn is_truthy_bool(v: Option<&Value>) -> bool {
    matches!(v.and_then(|v| v.as_bool()), Some(true))
}

fn is_string_value(v: Option<&Value>, expected: &str) -> bool {
    v.and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn parse_fhir_datetime(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&Utc));
    }

    // Common non-timezoned forms (treat as UTC).
    if let Ok(dt) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S") {
        return Some(Utc.from_utc_datetime(&dt));
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M") {
        return Some(Utc.from_utc_datetime(&dt));
    }

    // Date-only (treat as start of day UTC).
    if let Ok(d) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0)?));
    }

    None
}

fn parse_period_bounds(period: &Value) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    let Some(obj) = period.as_object() else {
        return (None, None);
    };

    let start = obj
        .get("start")
        .and_then(|v| v.as_str())
        .and_then(parse_fhir_datetime);
    let end = obj
        .get("end")
        .and_then(|v| v.as_str())
        .and_then(parse_fhir_datetime);

    (start, end)
}

#[derive(Debug, Clone)]
struct MembershipInRow {
    member_type: String,
    member_id: String,
    member_inactive: bool,
    period_start: Option<DateTime<Utc>>,
    period_end: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct MembershipListRow {
    member_type: String,
    member_id: String,
}

fn extract_group_members(resource: &Resource) -> Vec<MembershipInRow> {
    let mut out = Vec::new();
    let Some(obj) = resource.resource.as_object() else {
        return out;
    };

    // Conservative: an inactive Group has no active members.
    if matches!(obj.get("active").and_then(|v| v.as_bool()), Some(false)) {
        return out;
    }

    let Some(members) = obj.get("member").and_then(|v| v.as_array()) else {
        return out;
    };

    for member in members {
        let Some(member_obj) = member.as_object() else {
            continue;
        };

        let member_inactive = matches!(
            member_obj.get("inactive").and_then(|v| v.as_bool()),
            Some(true)
        );
        let (period_start, period_end) = member_obj
            .get("period")
            .map(parse_period_bounds)
            .unwrap_or((None, None));

        let Some(entity) = member_obj.get("entity") else {
            continue;
        };

        for r in extract_reference_values(entity) {
            if r.target_type.is_empty() || r.target_id.is_empty() {
                continue;
            }
            out.push(MembershipInRow {
                member_type: r.target_type,
                member_id: r.target_id,
                member_inactive,
                period_start,
                period_end,
            });
        }
    }

    out
}

fn extract_careteam_members(resource: &Resource) -> Vec<MembershipInRow> {
    let mut out = Vec::new();
    let Some(obj) = resource.resource.as_object() else {
        return out;
    };

    // Conservative: only index active CareTeams for `_in`.
    if !is_string_value(obj.get("status"), "active") {
        return out;
    }

    let Some(participants) = obj.get("participant").and_then(|v| v.as_array()) else {
        return out;
    };

    for participant in participants {
        let Some(participant_obj) = participant.as_object() else {
            continue;
        };

        let (period_start, period_end) = participant_obj
            .get("period")
            .map(parse_period_bounds)
            .unwrap_or((None, None));

        let Some(member) = participant_obj.get("member") else {
            continue;
        };

        for r in extract_reference_values(member) {
            if r.target_type.is_empty() || r.target_id.is_empty() {
                continue;
            }
            out.push(MembershipInRow {
                member_type: r.target_type,
                member_id: r.target_id,
                member_inactive: false,
                period_start,
                period_end,
            });
        }
    }

    out
}

fn extract_list_members(resource: &Resource) -> (Vec<MembershipListRow>, Vec<MembershipInRow>) {
    let mut list_rows = Vec::new();
    let mut in_rows = Vec::new();

    let Some(obj) = resource.resource.as_object() else {
        return (list_rows, in_rows);
    };

    let Some(entries) = obj.get("entry").and_then(|v| v.as_array()) else {
        return (list_rows, in_rows);
    };

    let list_is_current = is_string_value(obj.get("status"), "current");

    for entry in entries {
        let Some(entry_obj) = entry.as_object() else {
            continue;
        };
        if is_truthy_bool(entry_obj.get("deleted")) {
            continue;
        }

        let Some(item) = entry_obj.get("item") else {
            continue;
        };

        for r in extract_reference_values(item) {
            if r.target_type.is_empty() || r.target_id.is_empty() {
                continue;
            }

            list_rows.push(MembershipListRow {
                member_type: r.target_type.clone(),
                member_id: r.target_id.clone(),
            });

            if list_is_current {
                in_rows.push(MembershipInRow {
                    member_type: r.target_type,
                    member_id: r.target_id,
                    member_inactive: false,
                    period_start: None,
                    period_end: None,
                });
            }
        }
    }

    (list_rows, in_rows)
}

pub(super) async fn rebuild_memberships_for_resource(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    resource: &Resource,
) -> Result<()> {
    let collection_type = resource.resource_type.as_str();
    if !matches!(collection_type, "Group" | "List" | "CareTeam") {
        return Ok(());
    }

    // Always clear existing membership rows for this collection (this is the authoritative source).
    sqlx::query(
        "DELETE FROM search_membership_in WHERE collection_type = $1 AND collection_id = $2",
    )
    .bind(collection_type)
    .bind(&resource.id)
    .execute(&mut **tx)
    .await
    .map_err(crate::Error::Database)?;

    if collection_type == "List" {
        sqlx::query("DELETE FROM search_membership_list WHERE list_id = $1")
            .bind(&resource.id)
            .execute(&mut **tx)
            .await
            .map_err(crate::Error::Database)?;
    }

    // Deleted collections have no members.
    if resource.deleted {
        return Ok(());
    }

    let mut membership_in_rows: Vec<MembershipInRow> = Vec::new();
    let mut membership_list_rows: Vec<MembershipListRow> = Vec::new();

    match collection_type {
        "Group" => membership_in_rows = extract_group_members(resource),
        "CareTeam" => membership_in_rows = extract_careteam_members(resource),
        "List" => {
            let (list_rows, in_rows) = extract_list_members(resource);
            membership_list_rows = list_rows;
            membership_in_rows = in_rows;
        }
        _ => {}
    }

    // Insert `_in` membership rows.
    if !membership_in_rows.is_empty() {
        let mut member_types: Vec<String> = Vec::with_capacity(membership_in_rows.len());
        let mut member_ids: Vec<String> = Vec::with_capacity(membership_in_rows.len());
        let mut member_inactives: Vec<bool> = Vec::with_capacity(membership_in_rows.len());
        let mut period_starts: Vec<Option<DateTime<Utc>>> =
            Vec::with_capacity(membership_in_rows.len());
        let mut period_ends: Vec<Option<DateTime<Utc>>> =
            Vec::with_capacity(membership_in_rows.len());

        for row in membership_in_rows {
            member_types.push(row.member_type);
            member_ids.push(row.member_id);
            member_inactives.push(row.member_inactive);
            period_starts.push(row.period_start);
            period_ends.push(row.period_end);
        }

        sqlx::query(
            r#"
            INSERT INTO search_membership_in (
                collection_type, collection_id,
                member_type, member_id,
                member_inactive, period_start, period_end
            )
            SELECT
                $1, $2,
                t.member_type, t.member_id,
                t.member_inactive, t.period_start, t.period_end
            FROM UNNEST(
                $3::text[], $4::text[],
                $5::bool[], $6::timestamptz[], $7::timestamptz[]
            ) AS t(member_type, member_id, member_inactive, period_start, period_end)
            ON CONFLICT (collection_type, collection_id, member_type, member_id)
            DO UPDATE SET
                member_inactive = EXCLUDED.member_inactive,
                period_start = EXCLUDED.period_start,
                period_end = EXCLUDED.period_end
            "#,
        )
        .bind(collection_type)
        .bind(&resource.id)
        .bind(&member_types)
        .bind(&member_ids)
        .bind(&member_inactives)
        .bind(&period_starts)
        .bind(&period_ends)
        .execute(&mut **tx)
        .await
        .map_err(crate::Error::Database)?;
    }

    // Insert `_list` membership rows.
    if collection_type == "List" && !membership_list_rows.is_empty() {
        let mut member_types: Vec<String> = Vec::with_capacity(membership_list_rows.len());
        let mut member_ids: Vec<String> = Vec::with_capacity(membership_list_rows.len());
        for row in membership_list_rows {
            member_types.push(row.member_type);
            member_ids.push(row.member_id);
        }

        sqlx::query(
            r#"
            INSERT INTO search_membership_list (list_id, member_type, member_id)
            SELECT $1, t.member_type, t.member_id
            FROM UNNEST($2::text[], $3::text[]) AS t(member_type, member_id)
            ON CONFLICT (list_id, member_type, member_id)
            DO NOTHING
            "#,
        )
        .bind(&resource.id)
        .bind(&member_types)
        .bind(&member_ids)
        .execute(&mut **tx)
        .await
        .map_err(crate::Error::Database)?;
    }

    Ok(())
}
