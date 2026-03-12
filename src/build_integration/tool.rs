use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tower_lsp::Client;

use super::detection::{BuildWatchInterest, DetectedBuildTool};
use super::model::WorkspaceModelSnapshot;

#[derive(Debug, Clone, Copy)]
pub struct BuildToolLabels {
    pub importing_workspace: &'static str,
}

#[derive(Clone)]
pub struct BuildToolImportRequest {
    pub root: PathBuf,
    pub generation: u64,
    pub java_home: Option<PathBuf>,
    pub client: Client,
}

#[async_trait]
pub trait BuildToolIntegration: Send + Sync {
    fn detect(&self, root: &Path) -> Option<DetectedBuildTool>;
    fn watch_interest(&self) -> BuildWatchInterest;
    fn labels(&self) -> BuildToolLabels;
    async fn import_workspace(
        &self,
        request: BuildToolImportRequest,
    ) -> Result<WorkspaceModelSnapshot>;
}

#[derive(Clone)]
pub struct ResolvedBuildTool {
    pub detected: DetectedBuildTool,
    pub integration: Arc<dyn BuildToolIntegration>,
}

#[derive(Clone)]
pub struct BuildToolRegistry {
    integrations: Arc<[Arc<dyn BuildToolIntegration>]>,
    fallback_watch_interest: BuildWatchInterest,
}

impl BuildToolRegistry {
    pub fn new(integrations: Vec<Arc<dyn BuildToolIntegration>>) -> Self {
        let mut file_names = Vec::new();
        for integration in &integrations {
            for file_name in integration.watch_interest().file_names {
                if !file_names.contains(&file_name) {
                    file_names.push(file_name);
                }
            }
        }

        Self {
            integrations: integrations.into(),
            fallback_watch_interest: BuildWatchInterest { file_names },
        }
    }

    pub fn detect(&self, root: &Path) -> Option<ResolvedBuildTool> {
        self.integrations.iter().find_map(|integration| {
            integration.detect(root).map(|detected| ResolvedBuildTool {
                detected,
                integration: Arc::clone(integration),
            })
        })
    }

    pub fn fallback_watch_interest(&self) -> &BuildWatchInterest {
        &self.fallback_watch_interest
    }
}
