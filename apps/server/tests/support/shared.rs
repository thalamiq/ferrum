use anyhow::Context as _;
use std::sync::Arc;
use zunder::Config;
use tokio::sync::OnceCell;

static SHARED: OnceCell<Arc<SharedTestResources>> = OnceCell::const_new();

pub struct SharedTestResources {
    pub base_config: Config,
}

pub async fn shared() -> anyhow::Result<Arc<SharedTestResources>> {
    SHARED
        .get_or_try_init(|| async {
            init_tracing();

            let mut config = Config::load().context("load Config for tests")?;
            if let Some(url) = &config.database.test_database_url {
                config.database.url = url.clone();
            }

            // Keep tests deterministic and fast:
            // - No background workers
            // - No registry package installation
            // - No AuditEvent writes (avoid background DB writes during request tests)
            config.workers.enabled = false;
            config.logging.audit.enabled = false;
            config.fhir.default_packages.core.install = false;
            config.fhir.default_packages.core.install_examples = false;
            config.fhir.default_packages.extensions.install = false;
            config.fhir.default_packages.extensions.install_examples = false;
            config.fhir.default_packages.terminology.install = false;
            config.fhir.default_packages.terminology.install_examples = false;
            config.fhir.install_internal_packages = false;
            config.fhir.packages.clear();

            // DB pool sizing for per-test pools (TestApp overrides per schema).
            config.database.pool_min_size = 0;
            config.database.pool_max_size = 5;
            config.database.pool_timeout_seconds = 30;

            Ok(Arc::new(SharedTestResources {
                base_config: config,
            }))
        })
        .await
        .cloned()
}

fn init_tracing() {
    use std::sync::OnceLock;
    use tracing_subscriber::prelude::*;
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        let _ = tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "fhir_server=info,sqlx=warn".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .try_init();
    });
}
