use index::symbol::LibraryId;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use triomphe::Arc;
use vfs::{AbsPathBuf, FileId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectId(pub u32);

/// Represents the type of dependency edge between modules/libraries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Dependency {
    /// A dependency on another source module within the same workspace.
    /// E.g., `implementation project(':core')`
    Internal(ProjectId),

    /// A dependency on an external compiled artifact (JAR).
    /// E.g., `implementation 'com.google.guava:guava:31.0.1-jre'`
    External(LibraryId),
}

/// Represents a specific Maven/Gradle module in the workspace.
#[derive(Debug, Clone)]
pub struct ProjectData {
    pub id: ProjectId,
    pub name: SmolStr,

    pub root_path: AbsPathBuf,

    /// The LibraryId representing the compiled output/symbols of THIS project itself.
    pub library_id: LibraryId,

    /// The list of modules and JARs this project depends on.
    pub dependencies: Vec<Dependency>,
}

/// The state of the workspace build graph.
#[derive(Default, Debug, Clone)]
pub struct WorkspaceGraph {
    pub projects: FxHashMap<ProjectId, Arc<ProjectData>>,

    /// Maps a physical file to the module it belongs to.
    pub file_to_project: FxHashMap<FileId, ProjectId>,
}

impl WorkspaceGraph {
    pub fn resolve_project(&self, file_id: FileId) -> Option<Arc<ProjectData>> {
        self.file_to_project
            .get(&file_id)
            .and_then(|project_id| self.projects.get(project_id))
            .cloned()
    }
}
