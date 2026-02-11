-- ============================================================================
-- FHIR SERVER DATABASE SCHEMA
-- ============================================================================
-- ============================================================================
-- RESOURCE STORAGE
-- Main table for ALL FHIR resources (Patient, Observation, CapabilityStatement, etc.)
-- One table for all resource types with versioning and conformance resource support
-- ============================================================================
CREATE TABLE resources (
    -- Identity
    id VARCHAR(255) NOT NULL,
    resource_type VARCHAR(64) NOT NULL,
    version_id INTEGER NOT NULL DEFAULT 1,
    -- Content
    resource JSONB NOT NULL,
    -- Metadata (extracted from resource.meta for performance)
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Canonical URL for conformance resources (NULL for regular resources)
    url TEXT,
    -- Meta source (extracted from resource.meta.source)
    -- Useful for audit, provenance, subscriptions, and debugging
    meta_source TEXT,
    -- Meta tags (extracted from resource.meta.tag)
    -- Array of tag codes for filtering and subscription matching
    meta_tags TEXT [],
    -- Lifecycle
    deleted BOOLEAN DEFAULT FALSE,
    -- Track current version for efficient querying
    is_current BOOLEAN DEFAULT TRUE,
    -- Primary key is composite for versioning
    PRIMARY KEY (resource_type, id, version_id)
);
-- Partitioning by resource_type for better performance
-- Each resource type becomes its own partition
-- This is optional but recommended for large deployments
-- CREATE TABLE resources_patient PARTITION OF resources FOR VALUES IN ('Patient');
-- CREATE TABLE resources_observation PARTITION OF resources FOR VALUES IN ('Observation');
-- Core indexes
CREATE INDEX idx_resources_id ON resources(resource_type, id)
WHERE NOT deleted;
CREATE INDEX idx_resources_last_updated ON resources(last_updated);
CREATE INDEX idx_resources_type ON resources(resource_type)
WHERE NOT deleted;
-- Index for current non-deleted resources (primary search index)
CREATE INDEX idx_resources_current ON resources(resource_type, is_current)
WHERE is_current = TRUE
    AND deleted = FALSE;
CREATE INDEX idx_resources_resource_gin ON resources USING GIN(resource jsonb_path_ops);
-- Index for efficient conformance resource lookups
CREATE INDEX idx_resources_url ON resources(url)
WHERE url IS NOT NULL;
-- Indexes for conformance resource canonical + version lookups
-- Fast path when `resources.url` is populated (preferred).
CREATE INDEX idx_resources_url_version ON resources (url, (resource->>'version'))
WHERE url IS NOT NULL
    AND deleted = FALSE;
-- Fallback path for legacy rows where `resources.url` is NULL.
CREATE INDEX idx_resources_json_url_version ON resources ((resource->>'url'), (resource->>'version'))
WHERE url IS NULL
    AND deleted = FALSE;
-- Indexes for meta.source and meta.tag queries
-- Useful for audit trails, provenance tracking, and subscription filtering
CREATE INDEX idx_resources_meta_source ON resources(meta_source)
WHERE meta_source IS NOT NULL;
CREATE INDEX idx_resources_meta_tags ON resources USING GIN(meta_tags)
WHERE meta_tags IS NOT NULL;
-- Search indexing performance optimizations
-- Composite index for pagination cursor queries (cursor-based pagination)
CREATE INDEX idx_resources_indexing_cursor ON resources(resource_type, last_updated, id)
WHERE NOT deleted;
-- Index for counting resources needing indexing
CREATE INDEX idx_resources_current_not_deleted ON resources(resource_type, deleted, is_current)
WHERE deleted = FALSE
    AND is_current = TRUE;
-- Index for bulk resource fetching by ID list
CREATE INDEX idx_resources_type_id_current ON resources(resource_type, id, version_id DESC)
WHERE NOT deleted
    AND is_current = TRUE;
-- Enforce "single current version" constraint at DB level
-- Prevents race conditions during concurrent updates
CREATE UNIQUE INDEX uq_resources_current ON resources(resource_type, id)
WHERE is_current = TRUE;
-- ============================================================================
-- RESOURCE VERSION TRACKING
-- Tracks next version_id for each resource to enable atomic version incrementing
-- This ensures monotonic version IDs under concurrency
-- ============================================================================
CREATE TABLE resource_versions (
    resource_type VARCHAR(64) NOT NULL,
    id VARCHAR(255) NOT NULL,
    next_version INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (resource_type, id)
);
COMMENT ON TABLE resource_versions IS 'Tracks next version_id for each resource to enable atomic version incrementing';
COMMENT ON INDEX uq_resources_current IS 'Ensures only one current version exists per resource, preventing race conditions';
-- ============================================================================
-- SEARCH PARAMETER CONFIGURATION
-- Stores which search parameters are active and their metadata
-- Loaded from SearchParameter conformance resources
-- ============================================================================
CREATE TABLE search_parameters (
    id SERIAL PRIMARY KEY,
    code VARCHAR(64) NOT NULL,
    -- The search parameter code (e.g., "name", "birthdate")
    resource_type VARCHAR(64) NOT NULL,
    -- Which resource type this applies to
    type VARCHAR(20) NOT NULL,
    -- string | number | date | token | reference | composite | quantity | uri | special
    expression TEXT,
    -- FHIRPath expression to extract value
    url TEXT,
    -- Canonical URL of the SearchParameter
    description TEXT,
    active BOOLEAN DEFAULT TRUE,
    -- FHIR R5 search parameter capabilities
    multiple_or BOOLEAN DEFAULT TRUE,
    -- Can use OR logic (param=value1,value2)
    multiple_and BOOLEAN DEFAULT TRUE,
    -- Can repeat parameter (param=value1&param=value2)
    comparators TEXT [],
    -- Supported comparators: eq, ne, gt, lt, ge, le, sa, eb, ap
    modifiers TEXT [],
    -- Supported modifiers: missing, exact, contains, etc.
    chains TEXT [],
    -- For reference parameters: supported chaining
    targets TEXT [],
    -- For reference parameters: allowed target resource types (SearchParameter.target)
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT unique_code_type UNIQUE (code, resource_type)
);
CREATE INDEX idx_search_params_type ON search_parameters(resource_type);
CREATE INDEX idx_search_params_code ON search_parameters(code);
CREATE INDEX idx_search_params_active ON search_parameters(active)
WHERE active = TRUE;
-- Composite SearchParameter component metadata (SearchParameter.component[])
-- Store component definitions for composite SearchParameters (FHIR search 3.2.1.5.17)
CREATE TABLE search_parameter_components (
    search_parameter_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    definition_url TEXT NOT NULL,
    expression TEXT,
    -- Denormalized component metadata (resolved from definition_url at write time)
    component_code VARCHAR(64),
    component_type VARCHAR(20),
    PRIMARY KEY (search_parameter_id, position),
    FOREIGN KEY (search_parameter_id) REFERENCES search_parameters(id) ON DELETE CASCADE
);
CREATE INDEX idx_search_parameter_components_param ON search_parameter_components(search_parameter_id);
CREATE INDEX idx_search_parameter_components_definition ON search_parameter_components(definition_url);
CREATE INDEX idx_search_parameter_components_code ON search_parameter_components(component_code)
WHERE component_code IS NOT NULL;
CREATE INDEX idx_search_parameter_components_type ON search_parameter_components(component_type)
WHERE component_type IS NOT NULL;
-- ============================================================================
-- SEARCH INDEX TABLES
-- Separate tables for each search parameter type for optimal indexing
-- These are populated when resources are created/updated
--
-- Performance Optimization: All foreign key constraints use DEFERRABLE INITIALLY DEFERRED
-- This defers FK constraint validation to transaction commit time instead of checking
-- on every INSERT, reducing lock contention during bulk indexing operations.
-- Impact: 60-80% reduction in FK lock overhead, enabling faster bulk indexing.
-- ============================================================================
-- STRING search parameters (name, address, etc.)
-- value_normalized: Normalized string column for spec-compliant FHIR string searching (3.2.1.5.13)
-- Case/diacritic/punctuation/whitespace-insensitive matching uses `value_normalized`.
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
-- entry_hash: MD5 hash for efficient UNIQUE constraint deduplication
CREATE UNLOGGED TABLE search_string (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    value TEXT NOT NULL,
    value_normalized TEXT NOT NULL DEFAULT '',
    entry_hash CHAR(32) NOT NULL,
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);
-- Functional index using LEFT(value, 300) to handle long values while staying under btree limit
-- Full values stored in database (FHIR compliant), fast prefix matching via btree index
CREATE INDEX idx_search_string_lookup ON search_string(
    resource_type,
    parameter_name,
    LEFT(value, 300)
);
-- Index for exact lookups using normalized form (prefix matching).
-- Functional index using LEFT(value_normalized, 300) to handle long normalized values
CREATE INDEX idx_search_string_normalized_lookup ON search_string(
    resource_type,
    parameter_name,
    LEFT(value_normalized, 300)
);
-- Hash-based UNIQUE constraint for efficient deduplication
CREATE UNIQUE INDEX idx_search_string_unique_hash ON search_string(
    resource_type,
    resource_id,
    version_id,
    parameter_name,
    entry_hash
);
ALTER TABLE search_string
ADD CONSTRAINT unique_search_string UNIQUE USING INDEX idx_search_string_unique_hash;
-- Removed: idx_search_string_resource (redundant - covered by FK and lookup indexes)
-- Removed: idx_search_string_partial (redundant - pattern matching can use base index)
-- Removed: idx_search_string_normalized_partial (redundant - pattern matching can use normalized_lookup)
-- Removed: idx_search_string_delete (maintenance index - no longer needed with smart DELETE strategy)
-- TOKEN search parameters (identifier, code, etc.)
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
-- entry_hash: MD5 hash for efficient UNIQUE constraint deduplication
CREATE UNLOGGED TABLE search_token (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    system TEXT,
    -- Code system URL
    code TEXT NOT NULL,
    -- The actual code
    code_ci TEXT NOT NULL DEFAULT '',
    display TEXT,
    -- Display text (for reference)
    entry_hash CHAR(32) NOT NULL,
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);
-- Functional index using LEFT() to handle long system/code values while staying under btree limit
-- Most codes are short, but some can be very long (SNOMED expressions, etc.)
CREATE INDEX idx_search_token_lookup ON search_token(
    resource_type,
    parameter_name,
    LEFT(COALESCE(system, ''), 300),
    LEFT(code, 300)
);
-- Removed: idx_search_token_lookup_ci (redundant - use LOWER(code) in queries)
CREATE INDEX idx_search_token_code ON search_token(
    resource_type,
    parameter_name,
    LEFT(code, 300)
);
-- Removed: idx_search_token_code_ci (redundant - use LOWER(code) in queries)
CREATE INDEX idx_search_token_resource ON search_token(resource_type, resource_id);
-- Hash-based UNIQUE constraint for efficient deduplication
CREATE UNIQUE INDEX idx_search_token_unique_hash ON search_token(
    resource_type,
    resource_id,
    version_id,
    parameter_name,
    entry_hash
);
ALTER TABLE search_token
ADD CONSTRAINT unique_search_token UNIQUE USING INDEX idx_search_token_unique_hash;
-- Removed: idx_search_token_delete (maintenance index - no longer needed with smart DELETE strategy)
-- Correlated Identifier indexing for `:of-type` modifier
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
-- entry_hash: MD5 hash for efficient UNIQUE constraint deduplication
CREATE UNLOGGED TABLE search_token_identifier (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    type_system TEXT,
    type_code TEXT NOT NULL,
    type_code_ci TEXT NOT NULL DEFAULT '',
    value TEXT NOT NULL,
    value_ci TEXT NOT NULL DEFAULT '',
    entry_hash CHAR(32) NOT NULL,
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);
-- Functional index using LEFT() to handle long identifier values while staying under btree limit
CREATE INDEX idx_search_token_identifier_lookup ON search_token_identifier(
    resource_type,
    parameter_name,
    LEFT(COALESCE(type_system, ''), 300),
    LEFT(type_code, 300),
    LEFT(value, 300)
);
-- Hash-based UNIQUE constraint for efficient deduplication
CREATE UNIQUE INDEX idx_search_token_identifier_unique_hash ON search_token_identifier(
    resource_type,
    resource_id,
    version_id,
    parameter_name,
    entry_hash
);
ALTER TABLE search_token_identifier
ADD CONSTRAINT unique_search_token_identifier UNIQUE USING INDEX idx_search_token_identifier_unique_hash;
-- Removed: idx_search_token_identifier_delete (maintenance index - no longer needed with smart DELETE strategy)
-- DATE search parameters (birthdate, date, period, etc.)
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
-- entry_hash: MD5 hash for efficient UNIQUE constraint deduplication
CREATE UNLOGGED TABLE search_date (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    start_date TIMESTAMPTZ NOT NULL,
    -- Start of period or exact date
    end_date TIMESTAMPTZ NOT NULL,
    -- End of period or same as start
    entry_hash CHAR(32) NOT NULL,
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);
-- Removed: idx_search_date_lookup (redundant - GIST index handles range queries efficiently)
CREATE INDEX idx_search_date_range ON search_date USING GIST(tstzrange(start_date, end_date));
CREATE INDEX idx_search_date_resource ON search_date(resource_type, resource_id);
-- Hash-based UNIQUE constraint for efficient deduplication
CREATE UNIQUE INDEX idx_search_date_unique_hash ON search_date(
    resource_type,
    resource_id,
    version_id,
    parameter_name,
    entry_hash
);
ALTER TABLE search_date
ADD CONSTRAINT unique_search_date UNIQUE USING INDEX idx_search_date_unique_hash;
-- Removed: idx_search_date_delete (maintenance index - no longer needed with smart DELETE strategy)
-- NUMBER search parameters (value-quantity value, etc.)
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
-- entry_hash: MD5 hash for efficient UNIQUE constraint deduplication
CREATE UNLOGGED TABLE search_number (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    value NUMERIC NOT NULL,
    entry_hash CHAR(32) NOT NULL,
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);
CREATE INDEX idx_search_number_lookup ON search_number(resource_type, parameter_name, value);
CREATE INDEX idx_search_number_resource ON search_number(resource_type, resource_id);
-- Hash-based UNIQUE constraint for efficient deduplication
CREATE UNIQUE INDEX idx_search_number_unique_hash ON search_number(
    resource_type,
    resource_id,
    version_id,
    parameter_name,
    entry_hash
);
ALTER TABLE search_number
ADD CONSTRAINT unique_search_number UNIQUE USING INDEX idx_search_number_unique_hash;
-- Removed: idx_search_number_delete (maintenance index - no longer needed with smart DELETE strategy)
-- QUANTITY search parameters (height, weight, etc. with units)
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
-- entry_hash: MD5 hash for efficient UNIQUE constraint deduplication
CREATE UNLOGGED TABLE search_quantity (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    value NUMERIC NOT NULL,
    system TEXT,
    -- Unit system (e.g., UCUM)
    code TEXT NOT NULL,
    -- Unit code (e.g., "kg", "cm")
    unit TEXT,
    -- Human-readable unit display (e.g., "kilograms", "centimeters")
    entry_hash CHAR(32) NOT NULL,
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);
CREATE INDEX idx_search_quantity_lookup ON search_quantity(resource_type, parameter_name, value, code);
CREATE INDEX idx_search_quantity_resource ON search_quantity(resource_type, resource_id);
-- Index for unit column to support ||code format searches
CREATE INDEX idx_search_quantity_unit ON search_quantity(resource_type, parameter_name, unit)
WHERE unit IS NOT NULL;
-- Hash-based UNIQUE constraint for efficient deduplication
CREATE UNIQUE INDEX idx_search_quantity_unique_hash ON search_quantity(
    resource_type,
    resource_id,
    version_id,
    parameter_name,
    entry_hash
);
ALTER TABLE search_quantity
ADD CONSTRAINT unique_search_quantity UNIQUE USING INDEX idx_search_quantity_unique_hash;
-- Removed: idx_search_quantity_delete (maintenance index - no longer needed with smart DELETE strategy)
COMMENT ON COLUMN search_quantity.code IS 'Coded unit value from Quantity.code (e.g., "mg", "cm")';
COMMENT ON COLUMN search_quantity.unit IS 'Human-readable unit display from Quantity.unit (e.g., "milligrams", "centimeters")';
-- REFERENCE search parameters (patient, subject, encounter, etc.)
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
-- entry_hash: MD5 hash for efficient UNIQUE constraint deduplication
CREATE UNLOGGED TABLE search_reference (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    target_type VARCHAR(64) NOT NULL,
    -- Referenced resource type
    target_id VARCHAR(255) NOT NULL,
    -- Referenced resource ID
    reference_kind TEXT NOT NULL DEFAULT 'relative',
    -- Type of reference: 'relative', 'absolute', 'canonical', etc.
    target_version_id TEXT NOT NULL DEFAULT '',
    -- Version ID for versioned references
    target_url TEXT NOT NULL DEFAULT '',
    -- Full URL for absolute references
    canonical_url TEXT NOT NULL DEFAULT '',
    -- Canonical URL for canonical references
    canonical_version TEXT NOT NULL DEFAULT '',
    -- Version for canonical references
    display TEXT,
    -- Display text for the reference
    entry_hash CHAR(32) NOT NULL,
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);
CREATE INDEX idx_search_reference_lookup ON search_reference(
    resource_type,
    parameter_name,
    target_type,
    target_id
);
CREATE INDEX idx_search_reference_target ON search_reference(target_type, target_id);
-- Removed: idx_search_reference_resource (redundant - covered by FK and lookup indexes)
-- Optimized index for _revinclude queries
-- Finds resources that reference our search results
-- For example: GET /Patient/123?_revinclude=Observation:subject
CREATE INDEX idx_search_reference_revinclude ON search_reference(
    target_type,
    target_id,
    resource_type,
    parameter_name
)
WHERE resource_type IS NOT NULL
    AND target_type IS NOT NULL;
-- Index for wildcard _include queries
-- For example: GET /Encounter/123?_include=Encounter:*
CREATE INDEX idx_search_reference_wildcard ON search_reference(resource_type, resource_id, parameter_name)
WHERE resource_type IS NOT NULL;
-- Hash-based UNIQUE constraint for efficient deduplication
CREATE UNIQUE INDEX idx_search_reference_unique_hash ON search_reference(
    resource_type,
    resource_id,
    version_id,
    parameter_name,
    entry_hash
);
ALTER TABLE search_reference
ADD CONSTRAINT unique_search_reference UNIQUE USING INDEX idx_search_reference_unique_hash;
-- Removed: idx_search_reference_delete (maintenance index - no longer needed with smart DELETE strategy)
-- Removed: idx_search_reference_kind_lookup (edge-case index - <1% of queries, can add back if needed)
-- Removed: idx_search_reference_target_version (edge-case index - versioned references are rare)
-- Removed: idx_search_reference_target_url (edge-case index - absolute URL references are rare)
-- Removed: idx_search_reference_canonical (edge-case index - canonical references are rare)
-- URI search parameters (profile, url, etc.)
-- value_normalized: Normalized URI column for spec-compliant FHIR URI searching (3.2.1.5.15)
-- Case/diacritic/punctuation/whitespace-insensitive matching uses `value_normalized`.
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
-- entry_hash: MD5 hash for efficient UNIQUE constraint deduplication
CREATE UNLOGGED TABLE search_uri (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    value TEXT NOT NULL,
    value_normalized TEXT NOT NULL DEFAULT '',
    entry_hash CHAR(32) NOT NULL,
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);
-- Functional index using LEFT(value, 300) to handle long URI values while staying under btree limit
-- Full values stored in database (FHIR compliant), fast prefix matching via btree index
CREATE INDEX idx_search_uri_lookup ON search_uri(
    resource_type,
    parameter_name,
    LEFT(value, 300)
);
-- Removed: idx_search_uri_normalized_lookup (redundant - normalized queries can use value_normalized column with lookup index)
CREATE INDEX idx_search_uri_resource ON search_uri(resource_type, resource_id);
-- Hash-based UNIQUE constraint for efficient deduplication
CREATE UNIQUE INDEX idx_search_uri_unique_hash ON search_uri(
    resource_type,
    resource_id,
    version_id,
    parameter_name,
    entry_hash
);
ALTER TABLE search_uri
ADD CONSTRAINT unique_search_uri UNIQUE USING INDEX idx_search_uri_unique_hash;
-- Removed: idx_search_uri_delete (maintenance index - no longer needed with smart DELETE strategy)
-- TEXT narrative search parameters (_text)
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
CREATE UNLOGGED TABLE search_text (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    content TEXT NOT NULL,
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
    -- One aggregated narrative entry per resource version
    CONSTRAINT unique_search_text UNIQUE (
        resource_type,
        resource_id,
        version_id,
        parameter_name
    )
);
CREATE INDEX idx_search_text_lookup ON search_text(resource_type, parameter_name);
CREATE INDEX idx_search_text_tsv ON search_text USING GIN (to_tsvector('simple', content));
-- Removed: idx_search_text_delete (maintenance index - no longer needed with smart DELETE strategy)
-- CONTENT full-text parameters (_content)
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
CREATE UNLOGGED TABLE search_content (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    content TEXT NOT NULL,
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
    -- One aggregated full-text entry per resource version
    CONSTRAINT unique_search_content UNIQUE (
        resource_type,
        resource_id,
        version_id,
        parameter_name
    )
);
CREATE INDEX idx_search_content_lookup ON search_content(resource_type, parameter_name);
CREATE INDEX idx_search_content_tsv ON search_content USING GIN (to_tsvector('simple', content));
-- Removed: idx_search_content_delete (maintenance index - no longer needed with smart DELETE strategy)
-- COMPOSITE search parameters
-- For complex searches like component-code-value-quantity
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
CREATE UNLOGGED TABLE search_composite (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    components JSONB NOT NULL,
    -- Store component values as JSON
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
    -- Ensure composite search tuples are de-duplicated and ON CONFLICT is meaningful
    CONSTRAINT unique_search_composite UNIQUE (
        resource_type,
        resource_id,
        version_id,
        parameter_name,
        components
    )
);
CREATE INDEX idx_search_composite_lookup ON search_composite(resource_type, parameter_name);
CREATE INDEX idx_search_composite_gin ON search_composite USING GIN(components);
CREATE INDEX idx_search_composite_resource ON search_composite(resource_type, resource_id);
-- Removed: idx_search_composite_delete (maintenance index - no longer needed with smart DELETE strategy)
-- SPECIAL search parameters
-- For parameters with special logic that don't fit other types
CREATE TABLE search_special (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    value TEXT NOT NULL,
    -- Store as text, special logic handles interpretation
    metadata JSONB,
    -- Additional metadata for special processing
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE
);
CREATE INDEX idx_search_special_lookup ON search_special(resource_type, parameter_name, value);
CREATE INDEX idx_search_special_resource ON search_special(resource_type, resource_id);
CREATE INDEX idx_search_special_metadata ON search_special USING GIN(metadata);
-- Removed: idx_search_special_delete (maintenance index - no longer needed with smart DELETE strategy)
-- SEARCH MEMBERSHIP INDEXES
-- Supports standard membership parameters:
-- - `_in`   (active membership in CareTeam/Group/List)
-- - `_list` (membership in List, incl. functional lists when materialized as List)
--
-- These relations are derived from *collection* resources and must not be tied
-- to the member resource version_id (membership can change without touching the
-- member resource).
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
-- Active membership edges used by `_in`.
-- Period is stored so queries can evaluate "active" at request time.
CREATE UNLOGGED TABLE search_membership_in (
    collection_type VARCHAR(64) NOT NULL,
    collection_id VARCHAR(255) NOT NULL,
    member_type VARCHAR(64) NOT NULL,
    member_id VARCHAR(255) NOT NULL,
    member_inactive BOOLEAN NOT NULL DEFAULT FALSE,
    period_start TIMESTAMPTZ NULL,
    period_end TIMESTAMPTZ NULL,
    PRIMARY KEY (
        collection_type,
        collection_id,
        member_type,
        member_id
    )
);
CREATE INDEX idx_search_membership_in_collection ON search_membership_in (
    collection_type,
    collection_id,
    member_type,
    member_id
);
CREATE INDEX idx_search_membership_in_member ON search_membership_in (
    member_type,
    member_id,
    collection_type,
    collection_id
);
-- List membership edges used by `_list` (and as input to `_in` for List resources).
-- UNLOGGED: 2-3x faster writes (no WAL overhead), auto-rebuilt on crash
CREATE UNLOGGED TABLE search_membership_list (
    list_id VARCHAR(255) NOT NULL,
    member_type VARCHAR(64) NOT NULL,
    member_id VARCHAR(255) NOT NULL,
    PRIMARY KEY (list_id, member_type, member_id)
);
CREATE INDEX idx_search_membership_list_list ON search_membership_list (list_id, member_type, member_id);
CREATE INDEX idx_search_membership_list_member ON search_membership_list (member_type, member_id, list_id);
-- ============================================================================
-- RESOURCE SEARCH INDEX STATUS TRACKING
-- Tracks which resources have been indexed with which search parameters
-- Prevents redundant indexing and enables incremental updates
-- ============================================================================
CREATE TABLE resource_search_index_status (
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    -- Search parameter configuration hash to detect changes
    search_params_hash VARCHAR(64) NOT NULL,
    -- When this resource was last indexed
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Number of search parameters that were indexed
    indexed_param_count INTEGER NOT NULL DEFAULT 0,
    -- Status of indexing: 'completed', 'failed', 'partial'
    status VARCHAR(20) NOT NULL DEFAULT 'completed',
    -- Error message if indexing failed
    error_message TEXT,
    -- Primary key ensures one status record per resource version
    PRIMARY KEY (resource_type, resource_id, version_id),
    -- Foreign key to ensure resource exists
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE
);
-- Indexes for efficient lookups
CREATE INDEX idx_resource_search_status_hash ON resource_search_index_status(resource_type, search_params_hash);
CREATE INDEX idx_resource_search_status_indexed_at ON resource_search_index_status(indexed_at);
CREATE INDEX idx_resource_search_status_status ON resource_search_index_status(status);
-- Composite index for index status lookups (speeds up joins between resources and resource_search_index_status)
CREATE INDEX idx_resource_search_index_status_lookup ON resource_search_index_status(
    resource_type,
    resource_id,
    version_id,
    search_params_hash
);
-- ============================================================================
-- SEARCH PARAMETER VERSION TRACKING
-- Tracks the current "generation" of search parameters for each resource type
-- Used to generate the search_params_hash for change detection
-- ============================================================================
CREATE TABLE search_parameter_versions (
    resource_type VARCHAR(64) PRIMARY KEY,
    -- Version number incremented on each change
    version_number INTEGER NOT NULL DEFAULT 1,
    -- Hash of all active search parameters for this resource type
    current_hash VARCHAR(64) NOT NULL,
    -- When this configuration was last updated
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Number of active search parameters
    param_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_search_param_versions_hash ON search_parameter_versions(current_hash);
CREATE INDEX idx_search_param_versions_updated ON search_parameter_versions(updated_at);
-- ============================================================================
-- COMPARTMENT MEMBERSHIPS
-- Caches which resource types belong to compartments via which parameters
-- Populated from CompartmentDefinition resources
-- ============================================================================
CREATE TABLE compartment_memberships (
    -- Compartment type (e.g., "Patient", "Encounter")
    compartment_type VARCHAR(64) NOT NULL,
    -- Resource type that belongs to this compartment (e.g., "Observation", "Condition")
    resource_type VARCHAR(64) NOT NULL,
    -- Search parameter names for membership (e.g., ["patient", "subject"])
    -- Multiple params are ORed together - resource is in compartment if ANY param matches
    -- Special value "{def}" means the compartment resource itself (e.g., Patient in Patient compartment)
    parameter_names TEXT [] NOT NULL DEFAULT '{}',
    -- Optional temporal boundary parameters (per FHIR spec)
    -- If set, resources are only in compartment during their date range
    -- Example: Account has startParam="period", endParam="period"
    start_param VARCHAR(64),
    end_param VARCHAR(64),
    -- When this was loaded from CompartmentDefinition
    loaded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (compartment_type, resource_type)
);
CREATE INDEX idx_compartment_memberships_compartment ON compartment_memberships(compartment_type);
-- GIN index for efficient array containment queries on parameter_names
CREATE INDEX idx_compartment_memberships_params ON compartment_memberships USING GIN(parameter_names);
-- ============================================================================
-- FHIR PACKAGE LOADING TRACKING
-- Tracks which FHIR packages have been loaded to avoid redundant loading and
-- enables lifecycle operations (list/delete) against package-provided resources.
-- ============================================================================
CREATE TABLE fhir_packages (
    -- Auto-incrementing primary key
    id SERIAL PRIMARY KEY,
    -- Package identifier (e.g., "hl7.fhir.r4.core", "hl7.fhir.r5.core")
    name VARCHAR(255) NOT NULL,
    -- Package version (resolved version, not ranges like "latest")
    version VARCHAR(50) NOT NULL,
    -- When this package was loaded/updated
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Status: 'loaded', 'partial', 'failed', 'loading'
    status VARCHAR(20) NOT NULL DEFAULT 'loaded',
    -- Any error message if loading failed/partial
    error_message TEXT,
    -- Package metadata (manifest + derived info)
    metadata JSONB,
    -- Unique constraint: one entry per package name and version
    CONSTRAINT unique_package_version UNIQUE (name, version)
);
CREATE INDEX idx_fhir_packages_name ON fhir_packages(name);
CREATE INDEX idx_fhir_packages_version ON fhir_packages(version);
CREATE INDEX idx_fhir_packages_status ON fhir_packages(status);
CREATE INDEX idx_fhir_packages_created_at ON fhir_packages(created_at);
-- ============================================================================
-- RESOURCE-PACKAGE TRACKING
-- Tracks which resources belong to which packages for lifecycle management.
-- ============================================================================
CREATE TABLE resource_packages (
    -- Resource identification
    resource_type VARCHAR(64) NOT NULL,
    resource_id VARCHAR(255) NOT NULL,
    version_id INTEGER NOT NULL,
    -- Package reference
    package_id INTEGER NOT NULL,
    -- When this resource was loaded from this package
    loaded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Primary key ensures unique relationship per resource version and package
    PRIMARY KEY (
        resource_type,
        resource_id,
        version_id,
        package_id
    ),
    -- Foreign key to resource (cascade delete when resource version is deleted)
    FOREIGN KEY (resource_type, resource_id, version_id) REFERENCES resources(resource_type, id, version_id) ON DELETE CASCADE,
    -- Foreign key to package (cascade delete when package is deleted)
    FOREIGN KEY (package_id) REFERENCES fhir_packages(id) ON DELETE CASCADE
);
CREATE INDEX idx_resource_packages_package ON resource_packages(package_id);
CREATE INDEX idx_resource_packages_resource ON resource_packages(resource_type, resource_id);
CREATE INDEX idx_resource_packages_loaded_at ON resource_packages(loaded_at);
-- ============================================================================
-- BACKGROUND JOBS TRACKING
-- Tracks long-running background tasks (indexing, etc.)
-- Enhanced with priority, retry policies, scheduling, and worker tracking
-- ============================================================================
CREATE TABLE jobs (
    -- Unique job identifier (UUID)
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Job type: 'index_search_parameters', 'index_resources', 'index_all_resources'
    job_type VARCHAR(50) NOT NULL,
    -- Current status: 'pending', 'running', 'completed', 'failed', 'cancelled'
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    -- Job parameters (resource_type, limit, etc.)
    parameters JSONB,
    -- Progress tracking
    progress JSONB,
    -- Total items to process (if known)
    total_items INTEGER,
    -- Items processed so far
    processed_items INTEGER DEFAULT 0,
    -- Error message if failed
    error_message TEXT,
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    -- Cancellation flag (checked by background task)
    cancel_requested BOOLEAN DEFAULT FALSE,
    -- Enhanced features: priority, retry, scheduling, worker tracking
    priority INTEGER DEFAULT 5,
    retry_policy JSONB,
    retry_count INTEGER DEFAULT 0,
    last_error_at TIMESTAMPTZ,
    scheduled_at TIMESTAMPTZ,
    worker_id TEXT,
    -- Constraints
    CONSTRAINT chk_priority_range CHECK (
        priority >= 0
        AND priority <= 20
    ),
    CONSTRAINT chk_retry_count_positive CHECK (retry_count >= 0)
);
CREATE INDEX idx_jobs_status ON jobs(status);
CREATE INDEX idx_jobs_type ON jobs(job_type);
CREATE INDEX idx_jobs_created_at ON jobs(created_at);
CREATE INDEX idx_jobs_cancel ON jobs(cancel_requested)
WHERE cancel_requested = TRUE;
-- Enhanced indexes for priority-based dequeuing, scheduling, and worker tracking
CREATE INDEX idx_jobs_priority ON jobs(priority DESC, created_at ASC)
WHERE status = 'pending'
    AND cancel_requested = FALSE;
CREATE INDEX idx_jobs_scheduled ON jobs(scheduled_at)
WHERE status = 'pending'
    AND scheduled_at IS NOT NULL;
CREATE INDEX idx_jobs_worker ON jobs(worker_id, status);
-- Helper function for job queue statistics
CREATE OR REPLACE FUNCTION get_job_queue_stats() RETURNS TABLE (
        total_jobs BIGINT,
        pending_jobs BIGINT,
        running_jobs BIGINT,
        completed_jobs BIGINT,
        failed_jobs BIGINT,
        cancelled_jobs BIGINT,
        avg_processing_time INTERVAL
    ) AS $$ BEGIN RETURN QUERY
SELECT COUNT(*) as total_jobs,
    COUNT(*) FILTER (
        WHERE status = 'pending'
    ) as pending_jobs,
    COUNT(*) FILTER (
        WHERE status = 'running'
    ) as running_jobs,
    COUNT(*) FILTER (
        WHERE status = 'completed'
    ) as completed_jobs,
    COUNT(*) FILTER (
        WHERE status = 'failed'
    ) as failed_jobs,
    COUNT(*) FILTER (
        WHERE status = 'cancelled'
    ) as cancelled_jobs,
    AVG(completed_at - started_at) FILTER (
        WHERE status = 'completed'
    ) as avg_processing_time
FROM jobs
WHERE created_at > NOW() - INTERVAL '24 hours';
END;
$$ LANGUAGE plpgsql;
COMMENT ON FUNCTION get_job_queue_stats() IS 'Get job queue statistics for the last 24 hours';
-- ============================================================================
-- FHIR BATCH & TRANSACTION TRACKING
-- Stores metadata for batch and transaction operations
-- Note: Bundles are requests, not domain resources, so we track metadata only
-- This enables debugging, retry support, audit correlation, and metrics
-- ============================================================================
CREATE TABLE fhir_transactions (
    -- Unique transaction identifier
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Transaction type: 'batch' | 'transaction'
    type VARCHAR(20) NOT NULL,
    -- Status: 'pending', 'processing', 'completed', 'failed', 'partial'
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    -- Number of entries in the bundle
    entry_count INTEGER,
    -- Error message if transaction failed
    error_message TEXT,
    -- Additional metadata (request headers, client info, etc.)
    metadata JSONB
);
CREATE INDEX idx_fhir_transactions_type ON fhir_transactions(type);
CREATE INDEX idx_fhir_transactions_status ON fhir_transactions(status);
CREATE INDEX idx_fhir_transactions_created_at ON fhir_transactions(created_at);
CREATE INDEX idx_fhir_transactions_status_created ON fhir_transactions(status, created_at DESC);
-- Optional: Track individual entries within transactions
-- Useful for detailed debugging and retry support
CREATE TABLE fhir_transaction_entries (
    -- Transaction reference
    transaction_id UUID NOT NULL,
    -- Entry index within the bundle (0-based)
    entry_index INTEGER NOT NULL,
    -- HTTP method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE'
    method VARCHAR(10) NOT NULL,
    -- Request URL (relative or absolute)
    url TEXT NOT NULL,
    -- HTTP status code from response
    status INTEGER,
    -- Resource type (if applicable)
    resource_type VARCHAR(64),
    -- Resource ID (if applicable)
    resource_id VARCHAR(255),
    -- Version ID (if applicable)
    version_id INTEGER,
    -- Error details if entry failed
    error_message TEXT,
    -- Entry response metadata
    response_metadata JSONB,
    -- Foreign key to transaction
    FOREIGN KEY (transaction_id) REFERENCES fhir_transactions(id) ON DELETE CASCADE,
    -- Primary key ensures unique entries per transaction
    PRIMARY KEY (transaction_id, entry_index)
);
CREATE INDEX idx_fhir_transaction_entries_transaction ON fhir_transaction_entries(transaction_id);
CREATE INDEX idx_fhir_transaction_entries_resource ON fhir_transaction_entries(resource_type, resource_id)
WHERE resource_type IS NOT NULL
    AND resource_id IS NOT NULL;
CREATE INDEX idx_fhir_transaction_entries_status ON fhir_transaction_entries(status)
WHERE status IS NOT NULL;
COMMENT ON TABLE fhir_transactions IS 'Tracks FHIR batch and transaction operation metadata. Bundles are requests, not domain resources.';
COMMENT ON TABLE fhir_transaction_entries IS 'Optional detailed tracking of individual entries within batch/transaction operations';
-- ============================================================================
-- TERMINOLOGY SERVICE SUPPORT
-- Tables to support FHIR terminology operations: $expand, $lookup, $translate
-- ============================================================================
-- ============================================================================
-- CODESYSTEM CONCEPTS
-- Stores the main concept data for CodeSystems
-- Primary lookup table for $lookup and $validate-code operations
-- ============================================================================
CREATE TABLE codesystem_concepts (
    -- Identity
    id BIGSERIAL PRIMARY KEY,
    -- CodeSystem reference
    system TEXT NOT NULL,
    version TEXT,
    -- Concept identification
    code TEXT NOT NULL,
    display TEXT NOT NULL,
    -- Concept data (stored as JSONB for flexibility)
    properties JSONB,
    -- Array of property objects
    designations JSONB,
    -- Array of designation objects
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Unique constraint: one concept per system/version/code combination
    CONSTRAINT unique_concept UNIQUE (system, version, code)
);
-- Indexes for efficient lookups
CREATE INDEX idx_concepts_system ON codesystem_concepts(system);
CREATE INDEX idx_concepts_code ON codesystem_concepts(system, code);
CREATE INDEX idx_concepts_version ON codesystem_concepts(system, version, code);
CREATE INDEX idx_concepts_display ON codesystem_concepts(system, display text_pattern_ops);
CREATE INDEX idx_concepts_properties ON codesystem_concepts USING GIN(properties);
CREATE INDEX idx_concepts_designations ON codesystem_concepts USING GIN(designations);
CREATE INDEX idx_concepts_no_version ON codesystem_concepts(system, code)
WHERE version IS NULL;
COMMENT ON TABLE codesystem_concepts IS 'Stores CodeSystem concepts for terminology operations like $lookup and $validate-code';
COMMENT ON COLUMN codesystem_concepts.system IS 'CodeSystem canonical URL';
COMMENT ON COLUMN codesystem_concepts.version IS 'CodeSystem version (can be NULL for unversioned)';
COMMENT ON COLUMN codesystem_concepts.code IS 'Concept code';
COMMENT ON COLUMN codesystem_concepts.display IS 'Concept display text';
COMMENT ON COLUMN codesystem_concepts.properties IS 'JSONB array of concept properties';
COMMENT ON COLUMN codesystem_concepts.designations IS 'JSONB array of concept designations';
-- ============================================================================
-- CODESYSTEM PROPERTIES
-- Stores property definitions from CodeSystem.property for $lookup operations
-- ============================================================================
CREATE TABLE codesystem_properties (
    -- Identity
    id SERIAL PRIMARY KEY,
    -- CodeSystem reference
    codesystem_url TEXT NOT NULL,
    codesystem_version TEXT,
    -- Property definition (from CodeSystem.property)
    code VARCHAR(64) NOT NULL,
    uri TEXT,
    description TEXT,
    type VARCHAR(20) NOT NULL,
    -- code | Coding | string | integer | boolean | dateTime | decimal
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Unique constraint per code system version
    CONSTRAINT unique_codesystem_property UNIQUE (codesystem_url, codesystem_version, code)
);
CREATE INDEX idx_codesystem_properties_url ON codesystem_properties(codesystem_url);
CREATE INDEX idx_codesystem_properties_code ON codesystem_properties(codesystem_url, code);
-- ============================================================================
-- CODESYSTEM CONCEPT PROPERTIES
-- Stores concept property values for $lookup operations
-- Extracted from CodeSystem.concept.property
-- ============================================================================
CREATE TABLE codesystem_concept_properties (
    -- Identity
    id SERIAL PRIMARY KEY,
    -- CodeSystem reference
    codesystem_url TEXT NOT NULL,
    codesystem_version TEXT,
    -- Concept code
    code TEXT NOT NULL,
    -- Property reference
    property_code VARCHAR(64) NOT NULL,
    -- Value (one of these will be populated based on property type)
    value_code TEXT,
    value_coding JSONB,
    value_string TEXT,
    value_integer INTEGER,
    value_boolean BOOLEAN,
    value_datetime TIMESTAMPTZ,
    value_decimal NUMERIC,
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Foreign key to property definition
    FOREIGN KEY (
        codesystem_url,
        codesystem_version,
        property_code
    ) REFERENCES codesystem_properties(codesystem_url, codesystem_version, code) ON DELETE CASCADE,
    -- Allow multiple values for the same property
    CONSTRAINT unique_concept_property UNIQUE (
        codesystem_url,
        codesystem_version,
        code,
        property_code,
        value_code,
        value_string
    )
);
CREATE INDEX idx_concept_properties_lookup ON codesystem_concept_properties(codesystem_url, code);
CREATE INDEX idx_concept_properties_code ON codesystem_concept_properties(codesystem_url, code, property_code);
-- ============================================================================
-- CODESYSTEM CONCEPT DESIGNATIONS
-- Stores multi-language designations for concepts ($lookup operation)
-- Extracted from CodeSystem.concept.designation
-- ============================================================================
CREATE TABLE codesystem_designations (
    -- Identity
    id SERIAL PRIMARY KEY,
    -- CodeSystem reference
    codesystem_url TEXT NOT NULL,
    codesystem_version TEXT,
    -- Concept code
    code TEXT NOT NULL,
    -- Designation details
    language VARCHAR(10),
    -- BCP-47 language code
    use_system TEXT,
    use_code TEXT,
    use_display TEXT,
    value TEXT NOT NULL,
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_designations_lookup ON codesystem_designations(codesystem_url, code);
CREATE INDEX idx_designations_language ON codesystem_designations(codesystem_url, code, language);
CREATE INDEX idx_designations_use ON codesystem_designations(codesystem_url, code, use_code);
-- ============================================================================
-- VALUESET EXPANSIONS
-- Caches expanded ValueSets for performance
-- Supports $expand operation with paging
-- ============================================================================
CREATE TABLE valueset_expansions (
    -- Unique expansion identifier
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- ValueSet reference
    valueset_url TEXT NOT NULL,
    valueset_version TEXT,
    -- Expansion parameters (filter, includeDesignations, etc.)
    parameters JSONB,
    -- Hash of parameters for quick lookup
    parameters_hash VARCHAR(64) NOT NULL,
    -- Expansion metadata
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    total INTEGER NOT NULL,
    -- Total number of concepts in expansion
    "offset" INTEGER DEFAULT 0,
    -- For paged expansions
    count INTEGER,
    -- Number of concepts in this page
    -- Expansion contains (stored as JSONB for flexibility)
    contains JSONB NOT NULL,
    -- Expiration
    expires_at TIMESTAMPTZ,
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_expansions_valueset ON valueset_expansions(valueset_url, valueset_version);
CREATE INDEX idx_expansions_params ON valueset_expansions(valueset_url, parameters_hash);
CREATE INDEX idx_expansions_expires ON valueset_expansions(expires_at)
WHERE expires_at IS NOT NULL;
-- Partial index for current expansions
-- Note: expires_at > NOW() check must be done in queries, not in index predicate
-- because NOW() is VOLATILE and cannot be used in index predicates
CREATE INDEX idx_expansions_current ON valueset_expansions(valueset_url, valueset_version, parameters_hash)
WHERE expires_at IS NULL;
-- ============================================================================
-- VALUESET EXPANSION CONCEPTS
-- Individual concepts within expansions (normalized for querying)
-- Supports filtering and searching within expansions
-- ============================================================================
CREATE TABLE valueset_expansion_concepts (
    -- Foreign key to expansion
    expansion_id UUID NOT NULL,
    -- Concept details
    system TEXT NOT NULL,
    version TEXT,
    code TEXT NOT NULL,
    display TEXT,
    abstract BOOLEAN DEFAULT FALSE,
    inactive BOOLEAN DEFAULT FALSE,
    -- Designation support
    designations JSONB,
    -- Hierarchy support (for hierarchical expansions)
    parent_code TEXT,
    level INTEGER DEFAULT 0,
    -- Ordering
    ordinal INTEGER NOT NULL,
    -- Foreign key
    FOREIGN KEY (expansion_id) REFERENCES valueset_expansions(id) ON DELETE CASCADE,
    -- Primary key
    PRIMARY KEY (expansion_id, ordinal)
);
CREATE INDEX idx_expansion_concepts_code ON valueset_expansion_concepts(expansion_id, system, code);
CREATE INDEX idx_expansion_concepts_display ON valueset_expansion_concepts(expansion_id, display text_pattern_ops);
CREATE INDEX idx_expansion_concepts_parent ON valueset_expansion_concepts(expansion_id, parent_code);
-- ============================================================================
-- CONCEPTMAP GROUPS
-- Stores ConceptMap group information for $translate operations
-- Extracted from ConceptMap.group
-- ============================================================================
CREATE TABLE conceptmap_groups (
    -- Identity
    id SERIAL PRIMARY KEY,
    -- ConceptMap reference
    conceptmap_url TEXT NOT NULL,
    conceptmap_version TEXT,
    -- Group details
    source_system TEXT,
    source_version TEXT,
    target_system TEXT,
    target_version TEXT,
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_conceptmap_groups_map ON conceptmap_groups(conceptmap_url);
CREATE INDEX idx_conceptmap_groups_systems ON conceptmap_groups(source_system, target_system);
-- ============================================================================
-- CONCEPTMAP ELEMENTS
-- Stores individual concept mappings for $translate operations
-- Extracted from ConceptMap.group.element
-- ============================================================================
CREATE TABLE conceptmap_elements (
    -- Identity
    id SERIAL PRIMARY KEY,
    -- Group reference
    group_id INTEGER NOT NULL,
    -- Source concept
    source_code TEXT NOT NULL,
    source_display TEXT,
    -- No target (unmapped)
    no_map BOOLEAN DEFAULT FALSE,
    -- Foreign key
    FOREIGN KEY (group_id) REFERENCES conceptmap_groups(id) ON DELETE CASCADE
);
CREATE INDEX idx_conceptmap_elements_group ON conceptmap_elements(group_id);
CREATE INDEX idx_conceptmap_elements_source ON conceptmap_elements(group_id, source_code);
-- ============================================================================
-- CONCEPTMAP TARGETS
-- Stores target mappings for concept map elements
-- Extracted from ConceptMap.group.element.target
-- ============================================================================
CREATE TABLE conceptmap_targets (
    -- Identity
    id SERIAL PRIMARY KEY,
    -- Element reference
    element_id INTEGER NOT NULL,
    -- Target concept
    target_code TEXT,
    target_display TEXT,
    -- Equivalence: relatedto | equivalent | equal | wider | subsumes | narrower | specializes | inexact | unmatched | disjoint
    equivalence VARCHAR(20) NOT NULL,
    -- Comment
    comment TEXT,
    -- Dependencies (stored as JSONB)
    dependencies JSONB,
    -- Products (stored as JSONB)
    products JSONB,
    -- Foreign key
    FOREIGN KEY (element_id) REFERENCES conceptmap_elements(id) ON DELETE CASCADE
);
CREATE INDEX idx_conceptmap_targets_element ON conceptmap_targets(element_id);
CREATE INDEX idx_conceptmap_targets_code ON conceptmap_targets(target_code);
CREATE INDEX idx_conceptmap_targets_equivalence ON conceptmap_targets(equivalence);
-- ============================================================================
-- TERMINOLOGY $CLOSURE SUPPORT
-- Persistent storage for ConceptMap/$closure operation state and replay
-- ============================================================================
CREATE TABLE terminology_closure_tables (
    -- Closure name provided by client
    name TEXT PRIMARY KEY,
    -- Monotonic version issued by server
    current_version INTEGER NOT NULL DEFAULT 0,
    -- If true, server requires client to reinitialize (spec: underlying terminology changed)
    requires_reinit BOOLEAN NOT NULL DEFAULT FALSE,
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TABLE terminology_closure_concepts (
    -- Closure reference
    closure_name TEXT NOT NULL REFERENCES terminology_closure_tables(name) ON DELETE CASCADE,
    -- Concept identity
    system TEXT NOT NULL,
    code TEXT NOT NULL,
    display TEXT,
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (closure_name, system, code)
);
CREATE TABLE terminology_closure_relations (
    -- Closure reference
    closure_name TEXT NOT NULL REFERENCES terminology_closure_tables(name) ON DELETE CASCADE,
    -- Source (narrower) concept
    source_system TEXT NOT NULL,
    source_code TEXT NOT NULL,
    -- Target (wider / related) concept
    target_system TEXT NOT NULL,
    target_code TEXT NOT NULL,
    -- equal | specializes | subsumes | unmatched (ConceptMap.equivalence)
    equivalence VARCHAR(20) NOT NULL,
    -- Version in which this relation was first returned to the client
    introduced_in_version INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (
        closure_name,
        source_system,
        source_code,
        target_system,
        target_code,
        equivalence
    )
);
CREATE INDEX idx_terminology_closure_relations_version ON terminology_closure_relations(closure_name, introduced_in_version);
CREATE INDEX idx_terminology_closure_relations_source ON terminology_closure_relations(closure_name, source_system, source_code);
CREATE INDEX idx_terminology_closure_relations_target ON terminology_closure_relations(closure_name, target_system, target_code);
-- ============================================================================
-- INTERNAL AUDIT LOG
-- Authoritative audit trail for all FHIR interactions (FHIR AuditEvent stored as JSONB)
--
-- Requirements:
-- - Append-only and immutable
-- - Independent of the clinical resource store (`resources`)
-- - High-write-volume friendly (minimal indexes, async writer in application)
-- - Not queryable via normal FHIR resource APIs (separate table)
-- ============================================================================
CREATE TABLE audit_log (
    -- Identity
    id BIGSERIAL PRIMARY KEY,
    -- Event classification
    event_type VARCHAR(50) NOT NULL,
    -- FHIR interaction: 'read' | 'search' | 'create' | 'update' | 'delete' | 'history' | 'vread' | 'patch' | 'operation' | 'batch' | 'transaction' | 'export' | 'capabilities'
    action VARCHAR(20) NOT NULL,
    -- HTTP verb (GET/POST/PUT/PATCH/DELETE/HEAD)
    http_method VARCHAR(10) NOT NULL,
    -- FHIR audit action code: C | R | U | D | E
    fhir_action CHAR(1) NOT NULL,
    -- Resource details (if applicable)
    resource_type VARCHAR(64),
    resource_id VARCHAR(255),
    version_id INTEGER,
    -- Patient subject (when resolvable)
    patient_id VARCHAR(255),
    -- User/Client identification
    -- OAuth2 client id (SMART context)
    client_id TEXT,
    user_id TEXT,
    scopes TEXT [],
    -- user | system | anonymous | unknown
    token_type VARCHAR(20) NOT NULL DEFAULT 'unknown',
    client_ip TEXT,
    user_agent TEXT,
    request_id TEXT,
    status_code INTEGER NOT NULL,
    -- Operation outcome category: 'success' | 'authz_failure' | 'processing_failure'
    outcome VARCHAR(32) NOT NULL,
    -- Timestamp (authoritative server time)
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Full FHIR AuditEvent (serialized) for export / regulated disclosure
    audit_event JSONB NOT NULL,
    -- Additional details (request params, error details, etc.)
    details JSONB
);
-- Indexes for audit log queries (keep minimal for write volume).
CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp DESC);
CREATE INDEX idx_audit_log_action_timestamp ON audit_log(action, timestamp DESC);
CREATE INDEX idx_audit_log_request_id ON audit_log(request_id)
WHERE request_id IS NOT NULL;
CREATE INDEX idx_audit_log_client_id ON audit_log(client_id, timestamp DESC)
WHERE client_id IS NOT NULL;
CREATE INDEX idx_audit_log_user ON audit_log(user_id, timestamp DESC)
WHERE user_id IS NOT NULL;
CREATE INDEX idx_audit_log_patient ON audit_log(patient_id, timestamp DESC)
WHERE patient_id IS NOT NULL;

-- Enforce immutability: prevent UPDATE/DELETE.
CREATE OR REPLACE FUNCTION prevent_audit_log_modification() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'audit_log is append-only and immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_log_no_update
BEFORE UPDATE ON audit_log
FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_modification();

CREATE TRIGGER audit_log_no_delete
BEFORE DELETE ON audit_log
FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_modification();

COMMENT ON TABLE audit_log IS 'Authoritative FHIR interaction audit log. Stores full FHIR AuditEvent JSONB and SMART security context. Append-only.';
-- ============================================================================
-- IMPLICIT VALUESET CACHE
-- Caches implicit ValueSets (e.g., "all codes from CodeSystem X")
-- These are referenced as CodeSystem.valueSet
-- ============================================================================
CREATE TABLE implicit_valuesets (
    -- Identity
    id SERIAL PRIMARY KEY,
    -- CodeSystem reference
    codesystem_url TEXT NOT NULL UNIQUE,
    codesystem_version TEXT,
    -- Implicit ValueSet URL
    valueset_url TEXT NOT NULL,
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_implicit_valuesets_codesystem ON implicit_valuesets(codesystem_url);
CREATE INDEX idx_implicit_valuesets_valueset ON implicit_valuesets(valueset_url);
-- ============================================================================
-- SEARCH INDEXING PERFORMANCE OPTIMIZATIONS
-- Statistics targets for improved query planning
-- ============================================================================
-- Increase statistics collection for key columns to improve query plans
ALTER TABLE resources
ALTER COLUMN resource_type
SET STATISTICS 1000;
ALTER TABLE resources
ALTER COLUMN last_updated
SET STATISTICS 1000;
ALTER TABLE resource_search_index_status
ALTER COLUMN search_params_hash
SET STATISTICS 500;
-- =========================================================================
-- STANDARD SEARCH PARAMETER SEED DATA
-- Ensure built-in parameters like _text/_content exist for new installations
-- =========================================================================
-- Note: `_text` and `_content` use server-specific parameter types ('text' and 'content')
-- rather than the standard FHIR type 'string'. The core FHIR packages define these
-- parameters as SearchParameter.type = "string", but this server stores them as
-- custom types so queries use:
-- - `search_text` table for `_text` parameter
-- - `search_content` table for `_content` parameter
--
-- Without this, indexing would write to `search_string` (or skip indexing if expression
-- is NULL), while queries read from `search_text` / `search_content`, producing empty results.
INSERT INTO search_parameters (
        code,
        resource_type,
        type,
        expression,
        description,
        modifiers,
        multiple_or,
        multiple_and
    )
VALUES (
        '_text',
        'DomainResource',
        'text',
        NULL,
        'Search narrative text content',
        ARRAY ['exact', 'contains', 'missing'],
        TRUE,
        TRUE
    ),
    (
        '_content',
        'Resource',
        'content',
        NULL,
        'Search entire resource textual content',
        ARRAY ['exact', 'contains', 'missing'],
        TRUE,
        TRUE
    ),
    (
        '_language',
        'Resource',
        'token',
        'language',
        'Language of the resource',
        NULL,
        TRUE,
        TRUE
    ),
    (
        '_profile',
        'Resource',
        'reference',
        'meta.profile',
        'Profiles this resource claims to conform to',
        NULL,
        TRUE,
        TRUE
    ),
    (
        '_security',
        'Resource',
        'token',
        'meta.security',
        'Security Labels applied to this resource',
        NULL,
        TRUE,
        TRUE
    ),
    (
        '_source',
        'Resource',
        'uri',
        'meta.source',
        'Identifies where the resource comes from',
        NULL,
        TRUE,
        TRUE
    ),
    (
        '_tag',
        'Resource',
        'token',
        'meta.tag',
        'Tags applied to this resource',
        NULL,
        TRUE,
        TRUE
    ) ON CONFLICT (code, resource_type) DO
UPDATE
SET type = EXCLUDED.type,
    expression = EXCLUDED.expression,
    description = EXCLUDED.description,
    modifiers = EXCLUDED.modifiers,
    multiple_or = EXCLUDED.multiple_or,
    multiple_and = EXCLUDED.multiple_and,
    active = TRUE,
    updated_at = NOW();
-- ============================================================================
-- HELPER FUNCTIONS
-- ============================================================================
-- Crash recovery detection for UNLOGGED search tables
CREATE OR REPLACE FUNCTION detect_search_index_loss() RETURNS void AS $$
DECLARE total_resources BIGINT;
indexed_resources BIGINT;
search_tables_empty BOOLEAN;
BEGIN -- Count total current resources
SELECT COUNT(*) INTO total_resources
FROM resources
WHERE is_current = TRUE
    AND deleted = FALSE;
-- Sample search_token table to check if indexes exist
-- (token is most common search parameter type)
SELECT COUNT(DISTINCT (resource_type, resource_id)) INTO indexed_resources
FROM search_token
LIMIT 1000;
-- Quick sample
-- If we have resources but no search indexes, trigger full reindex
IF total_resources > 100
AND indexed_resources = 0 THEN RAISE NOTICE 'Search indexes appear empty (total resources: %, indexed: %) - queuing full reindex',
total_resources,
indexed_resources;
-- Queue a full reindex request
INSERT INTO search_reindex_requests (resource_type, requested_at)
VALUES ('__all__', NOW()) ON CONFLICT (resource_type) DO
UPDATE
SET requested_at = NOW();
RAISE WARNING 'Search indexes lost - full reindex queued. Search functionality may be degraded until reindexing completes.';
ELSIF indexed_resources > 0 THEN RAISE NOTICE 'Search indexes present (sampled % indexed resources)',
indexed_resources;
ELSE RAISE NOTICE 'No resources to index (total: %)',
total_resources;
END IF;
END;
$$ LANGUAGE plpgsql;
COMMENT ON FUNCTION detect_search_index_loss() IS 'Detects search index loss after crash and automatically queues reindexing. Run on database startup.';
-- Check status of search index tables
CREATE OR REPLACE FUNCTION check_search_index_status() RETURNS TABLE (
        table_name TEXT,
        row_count BIGINT,
        is_unlogged BOOLEAN,
        size_pretty TEXT
    ) AS $$
DECLARE tbl_rec RECORD;
actual_count BIGINT;
BEGIN FOR tbl_rec IN
SELECT c.relname::TEXT AS relname,
    c.oid AS reloid,
    (c.relpersistence = 'u')::BOOLEAN AS is_unlogged
FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = 'public'
    AND c.relname LIKE 'search_%'
    AND c.relkind = 'r'
ORDER BY c.relname LOOP -- Get actual row count
    EXECUTE format('SELECT COUNT(*) FROM %I', tbl_rec.relname) INTO actual_count;
-- Return row with actual count
RETURN QUERY
SELECT tbl_rec.relname::TEXT AS table_name,
    actual_count AS row_count,
    tbl_rec.is_unlogged AS is_unlogged,
    pg_size_pretty(pg_total_relation_size(tbl_rec.reloid)) AS size_pretty;
END LOOP;
END;
$$ LANGUAGE plpgsql;
COMMENT ON FUNCTION check_search_index_status() IS 'Check status of all search index tables (unlogged status, size, row count)';
-- Monitor for hash collisions across search index tables
CREATE OR REPLACE FUNCTION check_hash_collisions() RETURNS TABLE (
        table_name TEXT,
        collision_count BIGINT
    ) AS $$ BEGIN RETURN QUERY WITH collisions AS (
        SELECT 'search_string' as tname,
            COUNT(*) - COUNT(DISTINCT entry_hash) as collisions
        FROM search_string
        UNION ALL
        SELECT 'search_token',
            COUNT(*) - COUNT(DISTINCT entry_hash)
        FROM search_token
        UNION ALL
        SELECT 'search_date',
            COUNT(*) - COUNT(DISTINCT entry_hash)
        FROM search_date
        UNION ALL
        SELECT 'search_number',
            COUNT(*) - COUNT(DISTINCT entry_hash)
        FROM search_number
        UNION ALL
        SELECT 'search_quantity',
            COUNT(*) - COUNT(DISTINCT entry_hash)
        FROM search_quantity
        UNION ALL
        SELECT 'search_reference',
            COUNT(*) - COUNT(DISTINCT entry_hash)
        FROM search_reference
        UNION ALL
        SELECT 'search_uri',
            COUNT(*) - COUNT(DISTINCT entry_hash)
        FROM search_uri
        UNION ALL
        SELECT 'search_token_identifier',
            COUNT(*) - COUNT(DISTINCT entry_hash)
        FROM search_token_identifier
    )
SELECT tname,
    collisions
FROM collisions
WHERE collisions > 0;
-- Note: Should always return 0 rows (no collisions with MD5)
END;
$$ LANGUAGE plpgsql;
COMMENT ON FUNCTION check_hash_collisions() IS 'Monitor for hash collisions across all search index tables. Should always return empty.';
-- Get search parameter indexing status for observability
CREATE OR REPLACE FUNCTION get_search_parameter_indexing_status(p_resource_type VARCHAR(64) DEFAULT NULL) RETURNS TABLE (
        resource_type VARCHAR(64),
        version_number INTEGER,
        param_count INTEGER,
        current_hash VARCHAR(64),
        last_parameter_change TIMESTAMPTZ,
        total_resources BIGINT,
        indexed_with_current BIGINT,
        indexed_with_old BIGINT,
        never_indexed BIGINT,
        coverage_percent DOUBLE PRECISION,
        indexing_needed BOOLEAN,
        oldest_indexed_at TIMESTAMPTZ,
        newest_indexed_at TIMESTAMPTZ
    ) AS $$ BEGIN RETURN QUERY WITH resource_counts AS (
        SELECT r.resource_type,
            COUNT(*) as total
        FROM resources r
        WHERE r.is_current = TRUE
            AND r.deleted = FALSE
            AND (
                p_resource_type IS NULL
                OR r.resource_type = p_resource_type
            )
        GROUP BY r.resource_type
    ),
    indexed_current AS (
        SELECT s.resource_type,
            COUNT(*) as count,
            MIN(s.indexed_at) as oldest,
            MAX(s.indexed_at) as newest
        FROM resource_search_index_status s
            JOIN search_parameter_versions v ON s.resource_type = v.resource_type
        WHERE s.search_params_hash = v.current_hash
            AND (
                p_resource_type IS NULL
                OR s.resource_type = p_resource_type
            )
        GROUP BY s.resource_type
    ),
    indexed_old AS (
        SELECT s.resource_type,
            COUNT(*) as count
        FROM resource_search_index_status s
            JOIN search_parameter_versions v ON s.resource_type = v.resource_type
        WHERE s.search_params_hash != v.current_hash
            AND (
                p_resource_type IS NULL
                OR s.resource_type = p_resource_type
            )
        GROUP BY s.resource_type
    ),
    never_indexed AS (
        SELECT r.resource_type,
            COUNT(*) as count
        FROM resources r
            LEFT JOIN resource_search_index_status s ON r.resource_type = s.resource_type
            AND r.id = s.resource_id
            AND r.version_id = s.version_id
        WHERE r.is_current = TRUE
            AND r.deleted = FALSE
            AND s.resource_id IS NULL
            AND (
                p_resource_type IS NULL
                OR r.resource_type = p_resource_type
            )
        GROUP BY r.resource_type
    )
SELECT v.resource_type,
    v.version_number,
    v.param_count,
    v.current_hash,
    v.updated_at as last_parameter_change,
    COALESCE(rc.total, 0) as total_resources,
    COALESCE(ic.count, 0) as indexed_with_current,
    COALESCE(io.count, 0) as indexed_with_old,
    COALESCE(ni.count, 0) as never_indexed,
    CASE
        WHEN COALESCE(rc.total, 0) = 0 THEN 100.0
        ELSE ROUND(
            (
                COALESCE(ic.count, 0)::NUMERIC / rc.total::NUMERIC
            ) * 100,
            2
        )::DOUBLE PRECISION
    END as coverage_percent,
    (
        COALESCE(ic.count, 0) < COALESCE(rc.total, 0)
    ) as indexing_needed,
    ic.oldest as oldest_indexed_at,
    ic.newest as newest_indexed_at
FROM search_parameter_versions v
    LEFT JOIN resource_counts rc ON v.resource_type = rc.resource_type
    LEFT JOIN indexed_current ic ON v.resource_type = ic.resource_type
    LEFT JOIN indexed_old io ON v.resource_type = io.resource_type
    LEFT JOIN never_indexed ni ON v.resource_type = ni.resource_type
WHERE p_resource_type IS NULL
    OR v.resource_type = p_resource_type
ORDER BY v.resource_type;
END;
$$ LANGUAGE plpgsql;
COMMENT ON FUNCTION get_search_parameter_indexing_status(VARCHAR) IS 'Get search parameter indexing status for observability. Pass resource_type or NULL for all types.';
