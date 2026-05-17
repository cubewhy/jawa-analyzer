use index::symbol::GlobalSymbolIndex;
use project_model::{DependencyKind, DependencyScope, ProjectData, WorkspaceGraph};
use syntax::ClassStub;
use triomphe::Arc;

pub struct WorkspaceResolver<'a> {
    graph: &'a WorkspaceGraph,
    index: &'a GlobalSymbolIndex,
    project: &'a ProjectData,
}

impl<'a> WorkspaceResolver<'a> {
    pub fn new(
        graph: &'a WorkspaceGraph,
        index: &'a GlobalSymbolIndex,
        project: &'a ProjectData,
    ) -> Self {
        Self {
            graph,
            index,
            project,
        }
    }

    /// Resolves a Fully Qualified Name (FQN) exactly as the JVM/Compiler would,
    /// respecting visibility scopes (Compile vs Test).
    pub fn resolve_fqn(&self, fqn: &str, file_scope: DependencyScope) -> Option<Arc<ClassStub>> {
        // 1. Always look inside this project's own source code first
        if let Some(stub) = self.index.resolve_class(self.project.library_id, fqn) {
            return Some(stub);
        }

        // 2. Iterate through the project's explicit dependencies
        for dep in &self.project.dependencies {
            // Visibility Check:
            // - If we are resolving inside production code (Compile), skip Test-only dependencies.
            // - If we are resolving inside Test code, we can see both Compile and Test dependencies.
            if file_scope == DependencyScope::Compile && dep.scope == DependencyScope::Test {
                continue;
            }

            // Map the dependency kind to a concrete LibraryId
            let target_lib_id = match dep.kind {
                DependencyKind::External(lib_id) => lib_id,
                DependencyKind::Internal(proj_id) => {
                    // Resolve the internal ProjectId to its ProjectData to grab its LibraryId
                    if let Some(target_proj) = self.graph.projects.get(&proj_id) {
                        target_proj.library_id
                    } else {
                        continue;
                    }
                }
            };

            // Query the global symbol index for this specific library container
            if let Some(stub) = self.index.resolve_class(target_lib_id, fqn) {
                return Some(stub);
            }
        }

        None
    }
}
