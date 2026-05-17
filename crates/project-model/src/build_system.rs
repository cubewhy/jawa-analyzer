use crate::workspace::WorkspaceGraph;
use std::path::Path;

/// Represents a tool that can resolve the workspace structure.
pub trait BuildSystem: Send + Sync {
    /// The name of the build system (e.g., "Gradle", "Maven")
    fn name(&self) -> &'static str;

    /// Checks if this build system manages the given directory
    /// (e.g., by looking for build.gradle or pom.xml)
    fn is_applicable(&self, workspace_root: &Path) -> bool;

    /// Executes the tool to build and return the workspace graph.
    fn sync(&self, workspace_root: &Path, java_home: &Path) -> anyhow::Result<WorkspaceGraph>;
}
