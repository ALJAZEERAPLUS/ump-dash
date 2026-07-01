use crate::domain::native_cache::{
    AndroidCacheHit, AndroidCacheLookup, AndroidCacheStoreRequest, CachedAndroidLaunchRequest,
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

    /// Removes all cached native builds (iOS + Android) produced by the given
    /// worktree. Called when a worktree is deleted so its cached artifacts do
    /// not leak. Returns the cache entry directories that were removed.
    async fn prune_worktree(&self, worktree_path: PathBuf) -> anyhow::Result<Vec<PathBuf>>;

    async fn store_ios_simulator(
        &self,
        request: IosSimulatorCacheStoreRequest,
    ) -> anyhow::Result<IosSimulatorCacheHit>;

    async fn lookup_android(&self, worktree_path: PathBuf) -> anyhow::Result<AndroidCacheLookup>;

    async fn store_android(
        &self,
        request: AndroidCacheStoreRequest,
    ) -> anyhow::Result<AndroidCacheHit>;

    async fn install_and_launch_ios_simulator(
        &self,
        request: CachedIosLaunchRequest,
    ) -> anyhow::Result<Vec<String>>;

    async fn install_and_launch_android(
        &self,
        request: CachedAndroidLaunchRequest,
    ) -> anyhow::Result<Vec<String>>;
}
