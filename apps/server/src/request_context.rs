//! Per-request context injected by middleware.

#[derive(Debug, Clone)]
pub struct RequestContext {
    pub request_id: String,
}
