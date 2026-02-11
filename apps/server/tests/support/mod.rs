pub mod assertions;
pub mod builders;
pub mod fixtures;
pub mod search_helpers;
pub mod shared;

use anyhow::Context as _;
use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode},
    Router,
};
use futures::FutureExt as _;
use sqlx::Connection as _;
use zunder::{
    api::create_router,
    state::{AppStateOptions, JobQueueKind},
    AppState, Config,
};
use tower::ServiceExt as _;
use url::Url;
use uuid::Uuid;

// Re-export commonly used items
pub use assertions::*;
pub use builders::*;
pub use fixtures::*;
pub use search_helpers::*;

pub struct TestApp {
    pub router: Router,
    pub state: AppState,
    schema: String,
    admin_database_url: String,
}

impl TestApp {
    pub async fn new() -> anyhow::Result<Self> {
        Self::new_with_config(|_| {}).await
    }

    pub async fn new_with_config(configure: impl FnOnce(&mut Config)) -> anyhow::Result<Self> {
        let shared = shared::shared().await?;
        let mut config = shared.base_config.clone();
        configure(&mut config);

        // Per-test schema and DB pool.
        let admin_database_url = config.database.url.clone();

        let schema = format!("test_{}", Uuid::new_v4().simple());
        let mut admin_conn = sqlx::PgConnection::connect(&admin_database_url)
            .await
            .context("connect admin db for schema create")?;
        sqlx::query(&format!(r#"CREATE SCHEMA "{}""#, schema))
            .execute(&mut admin_conn)
            .await
            .context("create test schema")?;

        config.database.url = with_search_path(&admin_database_url, &schema)?;
        config.database.pool_min_size = 0;
        // Keep per-test DB pools small to avoid exhausting Postgres connections when tests run
        // in parallel (each test creates its own pool + schema).
        config.database.pool_max_size = 2;
        config.database.pool_timeout_seconds = 30;
        config.database.statement_timeout_seconds = 30;
        config.database.lock_timeout_seconds = 5;

        let state = AppState::new_with_options(
            config,
            AppStateOptions {
                run_migrations: true,
                install_packages: false,
                load_operation_definitions: false,
                job_queue: JobQueueKind::Inline,
            },
        )
        .await
        .context("initialize AppState")?;

        let router = create_router(state.clone());

        Ok(Self {
            router,
            state,
            schema,
            admin_database_url,
        })
    }

    pub async fn cleanup(self) -> anyhow::Result<()> {
        self.state.db_pool.close().await;

        let mut admin_conn = sqlx::PgConnection::connect(&self.admin_database_url)
            .await
            .context("connect admin db for schema drop")?;
        sqlx::query(&format!(r#"DROP SCHEMA "{}" CASCADE"#, self.schema))
            .execute(&mut admin_conn)
            .await
            .context("drop test schema")?;

        Ok(())
    }

    pub async fn request(
        &self,
        method: Method,
        path_and_query: &str,
        body: Option<Bytes>,
    ) -> anyhow::Result<(StatusCode, HeaderMap, Bytes)> {
        self.request_with_extra_headers(method, path_and_query, body, &[])
            .await
    }

    pub async fn request_with_extra_headers(
        &self,
        method: Method,
        path_and_query: &str,
        body: Option<Bytes>,
        extra_headers: &[(&str, &str)],
    ) -> anyhow::Result<(StatusCode, HeaderMap, Bytes)> {
        let request = Request::builder()
            .method(method)
            .uri(path_and_query)
            .header("host", "example.org")
            .header("accept", "application/fhir+json")
            .header("content-type", "application/fhir+json")
            .body(match body {
                Some(bytes) => Body::from(bytes),
                None => Body::empty(),
            })
            .context("build request")?;

        let mut request = request;
        for (name, value) in extra_headers {
            request.headers_mut().insert(
                name.parse::<HeaderName>().context("parse header name")?,
                value.parse::<HeaderValue>().context("parse header value")?,
            );
        }

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .context("dispatch request")?;

        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .context("read response body")?;

        Ok((status, headers, body))
    }
}

pub async fn with_test_app<F>(f: F) -> anyhow::Result<()>
where
    F: for<'a> FnOnce(
        &'a TestApp,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<()>> + 'a>,
    >,
{
    with_test_app_with_config(|_| {}, f).await
}

pub async fn with_test_app_with_config<C, F>(configure: C, f: F) -> anyhow::Result<()>
where
    C: FnOnce(&mut Config),
    F: for<'a> FnOnce(
        &'a TestApp,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<()>> + 'a>,
    >,
{
    let app = TestApp::new_with_config(configure).await?;

    let result = std::panic::AssertUnwindSafe(f(&app)).catch_unwind().await;
    let cleanup_result = app.cleanup().await;

    if let Err(e) = cleanup_result {
        eprintln!("test schema cleanup failed: {e:?}");
    }

    match result {
        Ok(r) => r,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

fn with_search_path(database_url: &str, schema: &str) -> anyhow::Result<String> {
    let mut url = Url::parse(database_url).context("parse database URL")?;
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={}", schema));
    Ok(url.to_string())
}
