use index::symbol::LibraryId;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use triomphe::Arc;
use vfs::{AbsPathBuf, FileId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectId(pub u32);

/// Represents the type of dependency edge between modules/libraries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DependencyKind {
    /// A dependency on another source module within the same workspace.
    /// E.g., `implementation project(':core')`
    Internal(ProjectId),

    /// A dependency on an external compiled artifact (JAR).
    /// E.g., `implementation 'com.google.guava:guava:31.0.1-jre'`
    External(LibraryId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyScope {
    /// Available everywhere (e.g., normal application code)
    Compile,
    /// Available only in test files (e.g., JUnit, Mockito)
    Test,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Dependency {
    pub kind: DependencyKind,
    pub scope: DependencyScope,
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

    /// Maps a directory root (e.g., /path/to/src/main/java) to its ProjectId.
    pub root_to_project: FxHashMap<AbsPathBuf, ProjectId>,
}

impl WorkspaceGraph {
    /// Resolves which project owns a file by walking up its path ancestors.
    pub fn resolve_project_for_path(&self, file_path: &AbsPathBuf) -> Option<Arc<ProjectData>> {
        // Walk up the directory tree: file -> parent -> grandparent -> etc.
        for ancestor in file_path.ancestors() {
            if let Ok(abs_ancestor) = AbsPathBuf::try_from(ancestor.to_path_buf()) {
                if let Some(&project_id) = self.root_to_project.get(&abs_ancestor) {
                    return self.projects.get(&project_id).cloned();
                }
            }
        }
        None
    }
}
