use crate::global_state::GlobalStateSnapshot;

use lsp_types::*;
use vfs::VfsPath;

use crate::lsp::diagnostics;

pub fn on_diagnostic(
    state: GlobalStateSnapshot,
    params: DocumentDiagnosticParams,
) -> anyhow::Result<DocumentDiagnosticReportResult> {
    tracing::info!(uri = ?params.text_document.uri, "request diagnostics");

    let vfs_path = VfsPath::from(&params.text_document.uri);

    let (file_id, text) = {
        let vfs = state.vfs.read();
        let Some(id) = vfs.file_id(&vfs_path) else {
            anyhow::bail!("failed to get file id from vfs path: {vfs_path:?}");
        };
        let Ok(content) = vfs.fetch_content(id) else {
            anyhow::bail!("failed to get file content");
        };
        let text = String::from_utf8_lossy(&content).to_string();
        (id, text)
    };

    let diagnostics = diagnostics::collect_diagnostics(state.analysis, file_id, text)?;

    Ok(DocumentDiagnosticReportResult::Report(
        DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
            related_documents: None,
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: None,
                items: diagnostics,
            },
        }),
    ))
}
