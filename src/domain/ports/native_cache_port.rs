use crate::domain::native_cache::{CachedIosLaunchRequest, IosSimulatorCacheHit};
use std::path::PathBuf;

#[async_trait::async_trait]
pub trait NativeCachePort: Send + Sync {
    async fn lookup_ios_simulator(
        &self,
        worktree_path: PathBuf,
    ) -> anyhow::Result<Option<IosSimulatorCacheHit>>;

    async fn install_and_launch_ios_simulator(
        &self,
        request: CachedIosLaunchRequest,
    ) -> anyhow::Result<Vec<String>>;
}
