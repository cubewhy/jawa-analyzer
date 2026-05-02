use base_db::SourceDatabase;
use tower_lsp::{
    Client,
    lsp_types::{Diagnostic, DiagnosticSeverity, Range},
};
use triomphe::Arc;

use crate::{
    GlobalState,
    lsp::convert::{file_id_to_url, offset_to_position},
};

pub async fn publish_diagnostics(client: &Client, state: Arc<GlobalState>, file_id: vfs::FileId) {
    let db = state.db_snapshot().await;
    let vfs = state.get_vfs().await;

    let Some(uri) = file_id_to_url(&vfs, file_id) else {
        return;
    };

    drop(vfs);

    let Some(parse_result) = db.parse_node(file_id) else {
        return;
    };

    let text = db.file_text(file_id).text(&db);

    let diagnostics = parse_result
        .errors(&db)
        .into_iter()
        .map(|err| {
            let lsp_range = Range {
                start: offset_to_position(text, err.range.start()),
                end: offset_to_position(text, err.range.end()),
            };

            Diagnostic {
                range: lsp_range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("parser".to_string()),
                message: err.message.clone(),
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();

    // TODO: collect diagnostics from validator

    client.publish_diagnostics(uri, diagnostics, None).await;
}
