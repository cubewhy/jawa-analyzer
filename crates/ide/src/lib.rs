use hir::model::{Library, ProjectWorkspace};
use salsa::{Database, Durability};

/// Snapshot of [AnalysisHost]
pub struct Analysis {}

impl Analysis {}

impl std::panic::UnwindSafe for Analysis {}

pub struct AnalysisHost {}

impl AnalysisHost {}
