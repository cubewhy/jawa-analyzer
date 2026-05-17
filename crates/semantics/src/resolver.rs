use index::symbol::GlobalSymbolIndex;
use project_model::ProjectData;
use syntax::ClassStub;
use triomphe::Arc;

pub struct WorkspaceResolver<'a> {
    index: &'a GlobalSymbolIndex,
    project: &'a ProjectData,
}

impl<'a> WorkspaceResolver<'a> {
    pub fn new(index: &'a GlobalSymbolIndex, project: &'a ProjectData) -> Self {
        Self { index, project }
    }

    /// Resolves a Fully Qualified Name (FQN) exactly as the JVM would.
    pub fn resolve_fqn(&self, fqn: &str) -> Option<Arc<ClassStub>> {
        for &lib_id in &self.project.classpath {
            if let Some(stub) = self.index.resolve_class(lib_id, fqn) {
                return Some(stub);
            }
        }

        None
    }
}
