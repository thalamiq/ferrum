/// Search test helpers for manually populating search index tables
///
/// Since background workers are disabled in tests, we need to manually populate
/// the search index tables (search_string, search_token, search_date, etc.)
/// after creating resources.
use sqlx::PgPool;

/// Populates search_membership_in table (for `_in` searches).
pub async fn index_membership_in(
    pool: &PgPool,
    collection_type: &str,
    collection_id: &str,
    member_type: &str,
    member_id: &str,
    member_inactive: bool,
    period_start: Option<chrono::DateTime<chrono::Utc>>,
    period_end: Option<chrono::DateTime<chrono::Utc>>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO search_membership_in (
            collection_type, collection_id,
            member_type, member_id,
            member_inactive, period_start, period_end
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (collection_type, collection_id, member_type, member_id)
        DO UPDATE SET
            member_inactive = EXCLUDED.member_inactive,
            period_start = EXCLUDED.period_start,
            period_end = EXCLUDED.period_end
        "#,
    )
    .bind(collection_type)
    .bind(collection_id)
    .bind(member_type)
    .bind(member_id)
    .bind(member_inactive)
    .bind(period_start)
    .bind(period_end)
    .execute(pool)
    .await?;

    Ok(())
}

/// Populates search_membership_list table (for `_list` searches).
pub async fn index_membership_list(
    pool: &PgPool,
    list_id: &str,
    member_type: &str,
    member_id: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO search_membership_list (list_id, member_type, member_id)
        VALUES ($1, $2, $3)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(list_id)
    .bind(member_type)
    .bind(member_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Populates search_token table for a resource
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `resource_type` - Resource type (e.g., "Patient")
/// * `resource_id` - Resource ID
/// * `version_id` - Version ID (usually 1)
/// * `parameter_name` - Search parameter code (e.g., "identifier")
/// * `system` - Code system URL (can be None)
/// * `code` - Code value
/// * `display` - Display text (optional)
pub async fn index_token(
    pool: &PgPool,
    resource_type: &str,
    resource_id: &str,
    version_id: i32,
    parameter_name: &str,
    system: Option<&str>,
    code: &str,
    display: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO search_token (
            resource_type, resource_id, version_id, parameter_name,
            system, code, code_ci, display, entry_hash
        )
        VALUES (
            $1, $2, $3, $4,
            $5, $6, $7, $8,
            MD5($1 || $2 || $3::text || $4 || COALESCE($5, '') || $6)
        )
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(version_id)
    .bind(parameter_name)
    .bind(system)
    .bind(code)
    .bind(code.to_lowercase())
    .bind(display)
    .execute(pool)
    .await?;

    Ok(())
}

/// Populates search_token_identifier table for :of-type modifier
///
/// Used for Identifier type with type.coding matching
pub async fn index_token_identifier(
    pool: &PgPool,
    resource_type: &str,
    resource_id: &str,
    version_id: i32,
    parameter_name: &str,
    type_system: Option<&str>,
    type_code: &str,
    value: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO search_token_identifier (
            resource_type, resource_id, version_id, parameter_name,
            type_system, type_code, type_code_ci, value, value_ci, entry_hash
        )
        VALUES (
            $1, $2, $3, $4,
            $5, $6, $7, $8, $9,
            MD5($1 || $2 || $3::text || $4 || COALESCE($5, '') || $6 || $8)
        )
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(version_id)
    .bind(parameter_name)
    .bind(type_system)
    .bind(type_code)
    .bind(type_code.to_lowercase())
    .bind(value)
    .bind(value.to_lowercase())
    .execute(pool)
    .await?;

    Ok(())
}

/// Populates search_string table for a resource
pub async fn index_string(
    pool: &PgPool,
    resource_type: &str,
    resource_id: &str,
    version_id: i32,
    parameter_name: &str,
    value: &str,
    value_normalized: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO search_string (
            resource_type, resource_id, version_id, parameter_name,
            value, value_normalized, entry_hash
        )
        VALUES (
            $1, $2, $3, $4, $5, $6,
            MD5($1 || $2 || $3::text || $4 || $5)
        )
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(version_id)
    .bind(parameter_name)
    .bind(value)
    .bind(value_normalized)
    .execute(pool)
    .await?;

    Ok(())
}

/// Populates search_reference table for a resource
pub async fn index_reference(
    pool: &PgPool,
    resource_type: &str,
    resource_id: &str,
    version_id: i32,
    parameter_name: &str,
    target_type: &str,
    target_id: &str,
    display: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO search_reference (
            resource_type, resource_id, version_id, parameter_name,
            reference_kind, target_type, target_id, target_version_id,
            target_url, canonical_url, canonical_version, display, entry_hash
        )
        VALUES (
            $1, $2, $3, $4,
            'relative', $5, $6, '', '', '', '', $7,
            MD5($1 || $2 || $3::text || $4 || 'relative' || $5 || $6 || '' || '' || '' || '')
        )
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(version_id)
    .bind(parameter_name)
    .bind(target_type)
    .bind(target_id)
    .bind(display)
    .execute(pool)
    .await?;

    Ok(())
}

/// Populates search_reference table for an absolute reference URL.
///
/// Useful for canonical/profile URLs such as `http://hl7.org/fhir/StructureDefinition/bp`.
pub async fn index_reference_absolute_url(
    pool: &PgPool,
    resource_type: &str,
    resource_id: &str,
    version_id: i32,
    parameter_name: &str,
    target_type: &str,
    target_id: &str,
    target_url: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO search_reference (
            resource_type, resource_id, version_id, parameter_name,
            reference_kind, target_type, target_id, target_version_id,
            target_url, canonical_url, canonical_version, display, entry_hash
        )
        VALUES (
            $1, $2, $3, $4,
            'absolute', $5, $6, '', $7, '', '', NULL,
            MD5($1 || $2 || $3::text || $4 || 'absolute' || $5 || $6 || '' || $7 || '' || '')
        )
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(version_id)
    .bind(parameter_name)
    .bind(target_type)
    .bind(target_id)
    .bind(target_url)
    .execute(pool)
    .await?;

    Ok(())
}

/// Populates search_date table for a resource
pub async fn index_date(
    pool: &PgPool,
    resource_type: &str,
    resource_id: &str,
    version_id: i32,
    parameter_name: &str,
    start_date: chrono::DateTime<chrono::Utc>,
    end_date: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO search_date (
            resource_type, resource_id, version_id, parameter_name,
            start_date, end_date, entry_hash
        )
        VALUES (
            $1, $2, $3, $4, $5, $6,
            MD5($1 || $2 || $3::text || $4 || $5::text || $6::text)
        )
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(version_id)
    .bind(parameter_name)
    .bind(start_date)
    .bind(end_date)
    .execute(pool)
    .await?;

    Ok(())
}

/// Populates search_number table for a resource
pub async fn index_number(
    pool: &PgPool,
    resource_type: &str,
    resource_id: &str,
    version_id: i32,
    parameter_name: &str,
    value: f64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO search_number (
            resource_type, resource_id, version_id, parameter_name, value, entry_hash
        )
        VALUES (
            $1, $2, $3, $4, $5,
            MD5($1 || $2 || $3::text || $4 || $5::text)
        )
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(version_id)
    .bind(parameter_name)
    .bind(value)
    .execute(pool)
    .await?;

    Ok(())
}

/// Populates search_quantity table for a resource
pub async fn index_quantity(
    pool: &PgPool,
    resource_type: &str,
    resource_id: &str,
    version_id: i32,
    parameter_name: &str,
    value: f64,
    system: Option<&str>,
    code: &str,
    unit: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO search_quantity (
            resource_type, resource_id, version_id, parameter_name,
            value, system, code, unit, entry_hash
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8,
            MD5($1 || $2 || $3::text || $4 || $5::text || COALESCE($6, '') || $7)
        )
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(version_id)
    .bind(parameter_name)
    .bind(value)
    .bind(system)
    .bind(code)
    .bind(unit)
    .execute(pool)
    .await?;

    Ok(())
}

/// Populates search_uri table for a resource
pub async fn index_uri(
    pool: &PgPool,
    resource_type: &str,
    resource_id: &str,
    version_id: i32,
    parameter_name: &str,
    value: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO search_uri (
            resource_type, resource_id, version_id, parameter_name, value, entry_hash
        )
        VALUES (
            $1, $2, $3, $4, $5,
            MD5($1 || $2 || $3::text || $4 || $5)
        )
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(version_id)
    .bind(parameter_name)
    .bind(value)
    .execute(pool)
    .await?;

    Ok(())
}

/// Helper to register a search parameter for testing
///
/// This ensures the search parameter exists in search_parameters table
/// so that searches can be performed on it.
pub async fn register_search_parameter(
    pool: &PgPool,
    code: &str,
    resource_type: &str,
    param_type: &str,
    expression: &str,
    modifiers: &[&str],
) -> anyhow::Result<()> {
    let modifiers_array: Vec<String> = modifiers.iter().map(|s| s.to_string()).collect();

    sqlx::query(
        r#"
        INSERT INTO search_parameters (
            code, resource_type, type, expression, description,
            modifiers, multiple_or, multiple_and
        )
        VALUES ($1, $2, $3, $4, $5, $6, TRUE, TRUE)
        ON CONFLICT (code, resource_type) DO UPDATE
        SET type = EXCLUDED.type,
            expression = EXCLUDED.expression,
            description = EXCLUDED.description,
            modifiers = EXCLUDED.modifiers,
            multiple_or = EXCLUDED.multiple_or,
            multiple_and = EXCLUDED.multiple_and,
            active = TRUE
        "#,
    )
    .bind(code)
    .bind(resource_type)
    .bind(param_type)
    .bind(expression)
    .bind(format!("Test search parameter: {}", code))
    .bind(&modifiers_array)
    .execute(pool)
    .await?;

    Ok(())
}
