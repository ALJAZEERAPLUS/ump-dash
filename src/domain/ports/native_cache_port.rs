use crate::domain::native_cache::{
    CachedIosLaunchRequest, IosSimulatorCacheHit, IosSimulatorCacheLookup,
    IosSimulatorCacheStoreRequest,
};
use std::path::PathBuf;

#[async_trait::async_trait]
pub trait NativeCachePort: Send + Sync {
    async fn lookup_ios_simulator(
        &self,
        worktree_path: PathBuf,
    ) -> anyhow::Result<IosSimulatorCacheLookup>;

    async fn store_ios_simulator(
        &self,
        request: IosSimulatorCacheStoreRequest,
    ) -> anyhow::Result<IosSimulatorCacheHit>;

    async fn install_and_launch_ios_simulator(
        &self,
        request: CachedIosLaunchRequest,
    ) -> anyhow::Result<Vec<String>>;
}
