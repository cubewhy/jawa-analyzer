use std::sync::Arc;

use tower_lsp::lsp_types::{InlayHint, InlayHintParams};

use crate::language::LanguageRegistry;
use crate::lsp::request_context::PreparedRequest;
use crate::workspace::Workspace;

pub async fn handle_inlay_hints(
    workspace: Arc<Workspace>,
    registry: Arc<LanguageRegistry>,
    params: InlayHintParams,
) -> Option<Vec<InlayHint>> {
    let started = std::time::Instant::now();
    let uri = &params.text_document.uri;
    let request = PreparedRequest::prepare(
        Arc::clone(&workspace),
        registry.as_ref(),
        uri,
        "inlay_hints",
    )?;
    let lang = request.lang();
    if !lang.supports_inlay_hints() {
        return None;
    }

    let env = request.parse_env();
    let analysis = request.analysis();
    tracing::debug!(
        request_id = request.metrics().request_id(),
        uri = %request.metrics().uri(),
        module = analysis.module.0,
        classpath = ?analysis.classpath,
        source_root = ?analysis.source_root.map(|id| id.0),
        range_start = ?params.range.start,
        range_end = ?params.range.end,
        "inlay request start"
    );
    let hints =
        lang.collect_inlay_hints_with_tree(request.file(), params.range, &env, request.view());
    request.metrics().log_summary(
        analysis.module.0,
        analysis.classpath,
        analysis.source_root.map(|id| id.0),
        started.elapsed().as_secs_f64() * 1000.0,
    );
    hints
}
