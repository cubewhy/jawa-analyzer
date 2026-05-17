use index::symbol::LibraryId;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use triomphe::Arc;
use vfs::{AbsPathBuf, FileId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectId(pub u32);

/// Represents a specific Maven/Gradle module in the workspace.
#[derive(Debug, Clone)]
pub struct ProjectData {
    pub id: ProjectId,
    pub name: SmolStr,

    pub root_path: AbsPathBuf,
    pub library_id: LibraryId,

    pub classpath: Vec<LibraryId>,
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
