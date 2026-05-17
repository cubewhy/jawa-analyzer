use std::{path::PathBuf, sync::Arc};

use dashmap::DashMap;
use index::symbol::{GlobalSymbolIndex, LibraryId};
use parking_lot::RwLock;
use project_model::WorkspaceGraph;
use syntax::{ClassStub, SyntaxError};
use vfs::FileId;

pub struct ParsedFile {
    pub green_node: rowan::GreenNode,
    pub syntax_errors: Vec<SyntaxError>,
}

impl ParsedFile {
    pub fn new(green_node: rowan::GreenNode, syntax_errors: Vec<SyntaxError>) -> Self {
        Self {
            green_node,
            syntax_errors,
        }
    }
}

#[derive(Default)]
pub struct ParseCache {
    trees: DashMap<FileId, Arc<ParsedFile>>,
    file_revisions: DashMap<FileId, u64>,
}

impl ParseCache {
    pub fn get_tree(&self, file_id: FileId) -> Option<Arc<ParsedFile>> {
        self.trees
            .get(&file_id)
            .map(|parsed_file| parsed_file.clone())
    }

    /// Bumps the revision for a file and returns the new revision number.
    pub fn bump_revision(&self, file_id: FileId) -> u64 {
        let mut rev = self.file_revisions.entry(file_id).or_insert(0);
        *rev += 1;
        *rev
    }

    /// Checks if a given task revision is still the latest.
    pub fn is_cancelled(&self, file_id: FileId, task_revision: u64) -> bool {
        if let Some(current_rev) = self.file_revisions.get(&file_id) {
            *current_rev != task_revision
        } else {
            // File was removed
            true
        }
    }

    pub fn update(&self, file_id: FileId, parsed: ParsedFile) {
        self.trees.insert(file_id, Arc::new(parsed));
    }

    pub fn remove(&self, file_id: FileId) {
        self.trees.remove(&file_id);
        self.file_revisions.remove(&file_id);
    }
}

/// Snapshot of [AnalysisHost]
pub struct Analysis {
    pub symbol_index: Arc<GlobalSymbolIndex>,
    pub workspace_graph: Arc<WorkspaceGraph>,
    pub parse_cache: Arc<ParseCache>,
}

impl Analysis {}

impl std::panic::UnwindSafe for Analysis {}

pub struct AnalysisHost {
    pub(crate) symbol_index: Arc<GlobalSymbolIndex>,
    pub(crate) workspace_graph: RwLock<Arc<WorkspaceGraph>>,
    pub parse_cache: Arc<ParseCache>,
}

impl AnalysisHost {
    pub fn new(cache_dir: &PathBuf) -> Self {
        Self {
            symbol_index: Arc::new(GlobalSymbolIndex::new(cache_dir, 2048)),
            workspace_graph: RwLock::new(Arc::new(WorkspaceGraph::default())),
            parse_cache: Arc::new(ParseCache::default()),
        }
    }

    pub fn snapshot(&self) -> Analysis {
        Analysis {
            symbol_index: self.symbol_index.clone(),
            workspace_graph: Arc::clone(&self.workspace_graph.read()),
            parse_cache: Arc::clone(&self.parse_cache),
        }
    }

    pub fn update_file(&self, workspace_id: LibraryId, file_id: FileId, stubs: Vec<ClassStub>) {
        self.symbol_index
            .update_workspace_file(workspace_id, file_id, stubs);
    }

    pub fn remove_file(&self, file_id: FileId) {
        self.parse_cache.remove(file_id);
        self.symbol_index.remove_file(file_id);
    }

    pub fn set_workspace_graph(&self, graph: WorkspaceGraph) {
        let mut write_guard = self.workspace_graph.write();
        *write_guard = Arc::new(graph);
    }
}
