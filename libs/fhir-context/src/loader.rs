use crate::error::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use ferrum_package::FhirPackage;

#[async_trait]
pub trait PackageLoader: Send + Sync {
    async fn load_package_with_dependencies(
        &self,
        package_name: &str,
        version: Option<&str>,
    ) -> Result<Vec<FhirPackage>>;

    async fn load_or_download_package(
        &self,
        package_name: &str,
        version: &str,
    ) -> Result<FhirPackage>;
}

#[cfg(feature = "registry-loader")]
#[async_trait]
impl<C> PackageLoader for ferrum_registry_client::RegistryClient<C>
where
    C: ferrum_registry_client::PackageCache + Send + Sync + 'static,
{
    async fn load_package_with_dependencies(
        &self,
        package_name: &str,
        version: Option<&str>,
    ) -> Result<Vec<FhirPackage>> {
        ferrum_registry_client::RegistryClient::load_package_with_dependencies(
            self,
            package_name,
            version,
        )
        .await
        .map_err(|e| Error::PackageLoader(e.to_string()))
    }

    async fn load_or_download_package(
        &self,
        package_name: &str,
        version: &str,
    ) -> Result<FhirPackage> {
        ferrum_registry_client::RegistryClient::load_or_download_package(
            self,
            package_name,
            version,
        )
        .await
        .map_err(|e| Error::PackageLoader(e.to_string()))
    }
}

pub fn default_package_loader() -> Result<Arc<dyn PackageLoader>> {
    #[cfg(feature = "registry-loader")]
    {
        Ok(Arc::new(ferrum_registry_client::RegistryClient::new(
            None,
        )))
    }

    #[cfg(not(feature = "registry-loader"))]
    {
        Err(Error::PackageLoaderUnavailable)
    }
}
