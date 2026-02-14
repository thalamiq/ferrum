#![allow(dead_code)]

use std::sync::{Arc, OnceLock};
use ferrum_context::DefaultFhirContext;
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static BLOCK_ON_GUARD: OnceLock<std::sync::Mutex<()>> = OnceLock::new();

fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime::new().expect("failed to create Tokio runtime for tests"))
}

pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let guard = BLOCK_ON_GUARD.get_or_init(|| std::sync::Mutex::new(()));
    let _lock = guard.lock().expect("failed to lock block_on guard");
    runtime().block_on(future)
}

static CONTEXT_R4: OnceLock<Arc<DefaultFhirContext>> = OnceLock::new();

pub fn context_r4() -> &'static DefaultFhirContext {
    CONTEXT_R4
        .get_or_init(|| {
            Arc::new(
                block_on(DefaultFhirContext::from_fhir_version_async(None, "R4"))
                    .expect("Failed to create R4 context"),
            )
        })
        .as_ref()
}
