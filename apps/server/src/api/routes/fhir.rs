//! FHIR API Routes
//!
//! This module defines all FHIR REST API routes according to the FHIR specification.
//!
//! # URL Handling Compliance
//!
//! Per FHIR spec (http://hl7.org/fhir/http.html#url):
//! - **Case Sensitivity**: All URLs and IDs in URLs are case-sensitive.
//!   Axum's routing is case-sensitive by default, preserving case in path parameters.
//! - **UTF-8 Encoding**: Clients SHOULD encode URLs using UTF-8, and servers SHOULD decode
//!   them assuming UTF-8. Axum's `Path` extractor automatically decodes percent-encoded
//!   URLs using UTF-8, ensuring proper handling of Unicode characters in resource types
//!   and IDs.
//! - **Trailing Slashes**: Servers SHALL support both forms (with and without trailing slash).
//!   Both `/Patient` and `/Patient/` are supported. No redirects are used - both forms
//!   are handled directly by registering duplicate routes.
//!
//! Examples:
//! - `/Patient/123` ≠ `/patient/123` (case-sensitive)
//! - `/Patient` and `/Patient/` both work (trailing slash optional)
//! - `/Patient/abc%20def` → `/Patient/abc def` (UTF-8 decoded)
//! - `/Patient/%E4%B8%AD` → `/Patient/中` (UTF-8 decoded)

use crate::api::handlers::{batch, crud, metadata, operations, search, smart};
use crate::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

pub fn fhir_routes() -> Router<AppState> {
    Router::new()
        // Exact routes first (more specific)
        // SMART on FHIR well-known configuration (with and without trailing slash)
        .route(
            "/.well-known/smart-configuration",
            get(smart::smart_configuration),
        )
        .route(
            "/.well-known/smart-configuration/",
            get(smart::smart_configuration),
        )
        // Metadata (with and without trailing slash)
        .route("/metadata", get(metadata::capability_statement))
        .route("/metadata/", get(metadata::capability_statement))
        // System-level search (must come before /_history to match exactly)
        .route("/_search", post(search::search_system))
        .route("/_search/", post(search::search_system))
        // System-level operations (before /_history)
        .route(
            "/$:operation",
            get(operations::operation_system).post(operations::operation_system),
        )
        .route(
            "/$:operation/",
            get(operations::operation_system).post(operations::operation_system),
        )
        // System-level history
        .route("/_history", get(crud::system_history))
        .route("/_history/", get(crud::system_history))
        .route(
            "/",
            get(search::search_system)
                .post(batch::batch_transaction)
                .delete(crud::system_delete),
        )
        // CRUD operations (parameterized routes come after exact routes)
        // Type-level operations (with and without trailing slash)
        .route("/:resource_type/_history", get(crud::type_history))
        .route("/:resource_type/_history/", get(crud::type_history))
        // Type-level search (must come before /:resource_type to match _search exactly)
        .route("/:resource_type/_search", post(search::search_type))
        .route("/:resource_type/_search/", post(search::search_type))
        // Type-level FHIR operations
        .route(
            "/:resource_type/$:operation",
            get(operations::operation_type).post(operations::operation_type),
        )
        .route(
            "/:resource_type/$:operation/",
            get(operations::operation_type).post(operations::operation_type),
        )
        .route(
            "/:resource_type",
            post(crud::create_resource)
                .get(search::search_type)
                .put(crud::conditional_update_resource)
                .patch(crud::conditional_patch_resource)
                .delete(crud::conditional_delete_resource),
        )
        .route(
            "/:resource_type/",
            post(crud::create_resource)
                .get(search::search_type)
                .put(crud::conditional_update_resource)
                .patch(crud::conditional_patch_resource)
                .delete(crud::conditional_delete_resource),
        )
        // Instance-level operations (with and without trailing slash)
        .route(
            "/:resource_type/:id",
            get(crud::read_resource)
                .head(crud::head_resource)
                .put(crud::update_resource)
                .patch(crud::patch_resource)
                .delete(crud::delete_resource),
        )
        .route(
            "/:resource_type/:id/",
            get(crud::read_resource)
                .head(crud::head_resource)
                .put(crud::update_resource)
                .patch(crud::patch_resource)
                .delete(crud::delete_resource),
        )
        .route(
            "/:resource_type/:id/_history/:vid",
            get(crud::vread_resource)
                .head(crud::head_vread_resource)
                .delete(crud::delete_resource_history_version),
        )
        .route(
            "/:resource_type/:id/_history/:vid/",
            get(crud::vread_resource)
                .head(crud::head_vread_resource)
                .delete(crud::delete_resource_history_version),
        )
        .route(
            "/:resource_type/:id/_history",
            get(crud::resource_history).delete(crud::delete_resource_history),
        )
        .route(
            "/:resource_type/:id/_history/",
            get(crud::resource_history).delete(crud::delete_resource_history),
        )
        // Compartment search (must come before instance-level operations to avoid ambiguity)
        //
        // Spec: 3.2.0.11.4 Variant Searches
        // - GET  /{Compartment}/{id}/*{?params}
        // - POST /{Compartment}/{id}/_search{?params}
        // - GET  /{Compartment}/{id}/{type}{?params}
        // - POST /{Compartment}/{id}/{type}/_search{?params}
        .route(
            "/:compartment_type/:compartment_id/_search",
            post(search::search_compartment),
        )
        .route(
            "/:compartment_type/:compartment_id/_search/",
            post(search::search_compartment),
        )
        .route(
            "/:compartment_type/:compartment_id/:resource_type/_search",
            post(search::search_compartment),
        )
        .route(
            "/:compartment_type/:compartment_id/:resource_type/_search/",
            post(search::search_compartment),
        )
        .route(
            "/:compartment_type/:compartment_id/:resource_type",
            get(search::search_compartment),
        )
        .route(
            "/:compartment_type/:compartment_id/:resource_type/",
            get(search::search_compartment),
        )
        // Instance-level FHIR operations
        .route(
            "/:resource_type/:id/$:operation",
            get(operations::operation_instance).post(operations::operation_instance),
        )
        .route(
            "/:resource_type/:id/$:operation/",
            get(operations::operation_instance).post(operations::operation_instance),
        )
}
