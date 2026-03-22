use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use java_analyzer::index::{IndexScope, ModuleId, WorkspaceIndex};
use java_analyzer::salsa_db::{Database, FileId, SourceFile};
use java_analyzer::salsa_queries::java::extract_java_semantic_context_at_offset;
use tower_lsp::lsp_types::Url;

#[test]
fn semantic_context_build_does_not_block_on_workspace_index_write_lock() {
    let workspace_index = Arc::new(parking_lot::RwLock::new(WorkspaceIndex::new()));
    let source = r#"
class Test {
    void demo() {
        int value = 1;
        val
    }
}
"#
    .to_string();
    let offset = source.find("val").expect("completion marker") + "val".len();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn({
        let workspace_index = Arc::clone(&workspace_index);
        let source = source.clone();
        move || {
            let db = Database::with_workspace_index(Arc::clone(&workspace_index));
            let file = SourceFile::new(
                &db,
                FileId::new(Url::parse("file:///test/Test.java").expect("uri")),
                source,
                Arc::from("java"),
            );
            let view = workspace_index.read().view(IndexScope {
                module: ModuleId::ROOT,
            });

            let _write_guard = workspace_index.write();
            let result = extract_java_semantic_context_at_offset(&db, file, offset, view, None);
            let _ = tx.send(result.is_some());
        }
    });

    let completed = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("semantic context extraction timed out behind a workspace-index write lock");
    assert!(
        completed,
        "semantic context extraction should still produce a context"
    );
}
