use base_db::SourceDatabase;
use tower_lsp::lsp_types::TextEdit;
use triomphe::Arc;

use crate::GlobalState;

pub async fn incremental_parse(state: Arc<GlobalState>, file_id: vfs::FileId, edit: TextEdit) {
    let db = state.db_snapshot().await;

    let content = db.file_text(file_id).text(&db);

    // TODO: incremental parse
}

pub async fn full_parse(state: Arc<GlobalState>, file_id: vfs::FileId) {
    let db = state.db_snapshot().await;

    let content = db.file_text(file_id).text(&db);

    // TODO: parse
}
