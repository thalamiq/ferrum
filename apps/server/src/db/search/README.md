# Search Architecture (Rust)

This directory implements FHIR search (R4-style) on top of the `resources` table plus `search_*` index tables.

## Request Semantics

- Parameter occurrences are preserved in-order (no lossy `HashMap` parsing).
- FHIR AND/OR semantics are modeled explicitly:
  - Repeating the same parameter is **AND**: `name=John&name=Smith`
  - Comma-separated values within one occurrence are **OR**: `name=John,Smith`

## Modules

- `params.rs`
  - Parses request items into `SearchParameters`.
  - Treats control/result parameters (`_count`, `_sort`, `_include`, `_format`, …) separately from filter parameters.
- `parameter_lookup.rs`
  - Loads `search_parameters` metadata (type, comparators, modifiers, multiple_or/and, chains).
  - Used to resolve filter parameters and track unknown/unsupported ones.
- `query_builder.rs`
  - Converts resolved parameters into SQL using `EXISTS` subqueries (avoids duplicate result rows from joins).
  - Implements core parameter types: `string`, `token`, `date`, `number`, `quantity`, `reference`, `uri`, `text`, `content`, plus specials `_id`, `_lastUpdated`, `_in`, and `_list`.
- `mod.rs`
  - Orchestrates resolution, execution, totals, compartments, and `_include/_revinclude` fetching.

## Implemented Features

- Robust parsing and URL encoding/decoding for GET/POST search (form-url-encoded).
- `_summary=count` short-circuits fetching resources and returns only the count bundle.
- Totals (`_total=accurate|estimate`) run a matching COUNT query (not a filter-less count).
- Compartment search joins via `compartment_memberships` + `search_reference`.
- Membership search via `_in` and `_list` (including `reference._in` chaining).
- `_include` / `_revinclude`
  - Supports wildcards (`*`, `Resource:*`) and `:iterate` (depth-limited).
  - Deduplicates included resources.
- Bundle filtering:
  - `_summary` and `_elements` apply to `Bundle.entry[].resource`, not the Bundle itself.

## Chaining Support

**NEW: Full FHIR-conformant chaining is now supported!**

Chaining allows following reference parameters to filter on the referenced resource's properties.

**Supported syntax:**
- `[param].[chain_param]=[value]` - Basic chaining (e.g., `subject.name=peter`)
- `[param]:[type].[chain_param]=[value]` - Type-restricted chaining (e.g., `subject:Patient.name=peter`)
- All parameter types supported on the chained resource (string, token, date, etc.)
- OR values: `subject.name=Smith,Jones`
- AND semantics: Multiple chains evaluated independently (per FHIR spec note)

**Examples:**
```
# Find DiagnosticReports for patients named Peter
GET /DiagnosticReport?subject.name=peter

# Explicitly restrict to Patient references
GET /DiagnosticReport?subject:Patient.name=peter

# Chain to date parameter with prefix
GET /DiagnosticReport?subject.birthdate=ge1990

# Chain to token parameter
GET /Observation?subject.identifier=123456

# Multiple chains (AND semantics)
GET /DiagnosticReport?subject.name=Smith&subject.birthdate=1990
```

**Implementation details:**
- Resolution: `resolve.rs` parses chains, determines target types, resolves chained parameter
- Query building: `query_builder/claueses/special.rs` builds EXISTS clause with JOIN through search_reference
- Independent evaluation: Each chain creates a separate EXISTS clause (per FHIR spec)

## Known Gaps / Future Work

- Recursive chaining (e.g., `subject.organization.name`) - currently only single-level chains supported
- Composite parameters.
- Sorting by indexed search parameters (currently only `_id` and `_lastUpdated` are supported).
- Full modifier/comparator validation against `search_parameters.comparators/modifiers/chains` (the wiring is there; extend as needed).
