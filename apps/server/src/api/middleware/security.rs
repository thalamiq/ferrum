//! Security headers middleware

use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};

/// Security headers middleware.
///
/// This is not a replacement for proper authentication/authorization, but it avoids
/// common unsafe defaults and improves baseline production posture.
pub async fn security_headers_middleware(req: Request, next: Next) -> Response {
    let is_https = req
        .headers()
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("https"))
        .unwrap_or(false)
        || req
            .uri()
            .scheme_str()
            .map(|s| s.eq_ignore_ascii_case("https"))
            .unwrap_or(false);

    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    // Avoid MIME sniffing.
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    // Prevent clickjacking.
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    // Avoid leaking referrers.
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    // Tight default CSP for an API surface.
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static("default-src 'none'"),
    );

    // Cross-origin isolation defaults (API-safe).
    headers.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-site"),
    );

    // HSTS only when HTTPS is used (or terminated upstream).
    if is_https {
        headers.insert(
            "strict-transport-security",
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }

    response
}
