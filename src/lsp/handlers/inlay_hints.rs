use std::sync::Arc;

use tower_lsp::jsonrpc::{self, Result as LspResult};
use tower_lsp::lsp_types::{InlayHint, InlayHintParams};

use crate::language::LanguageRegistry;
use crate::lsp::request_context::{PreparedRequest, RequestContext};
use crate::workspace::Workspace;

pub async fn handle_inlay_hints(
    workspace: Arc<Workspace>,
    registry: Arc<LanguageRegistry>,
    params: InlayHintParams,
    request: Arc<RequestContext>,
) -> LspResult<Option<Vec<InlayHint>>> {
    let task = tokio::task::spawn_blocking(move || {
        handle_inlay_hints_blocking(workspace, registry, params, request)
    });

    match task.await {
        Ok(result) => result.map_err(|cancelled| cancelled.into_lsp_error()),
        Err(error) => {
            tracing::error!(%error, "inlay hints worker panicked");
            Err(jsonrpc::Error::internal_error())
        }
    }
}

fn handle_inlay_hints_blocking(
    workspace: Arc<Workspace>,
    registry: Arc<LanguageRegistry>,
    params: InlayHintParams,
    request: Arc<RequestContext>,
) -> crate::lsp::request_cancellation::RequestResult<Option<Vec<InlayHint>>> {
    let started = std::time::Instant::now();
    let uri = &params.text_document.uri;
    let Some(prepared) = PreparedRequest::prepare(
        Arc::clone(&workspace),
        registry.as_ref(),
        uri,
        Arc::clone(&request),
    )?
    else {
        return Ok(None);
    };
    let lang = prepared.lang();
    if !lang.supports_inlay_hints() {
        return Ok(None);
    }

    let env = prepared.parse_env();
    let analysis = prepared.analysis();
    tracing::debug!(
        request_id = prepared.metrics().request_id(),
        uri = %prepared.metrics().uri(),
        module = analysis.module.0,
        classpath = ?analysis.classpath,
        source_root = ?analysis.source_root.map(|id| id.0),
        range_start = ?params.range.start,
        range_end = ?params.range.end,
        "inlay request start"
    );
    let hints =
        lang.collect_inlay_hints_with_tree(prepared.file(), params.range, &env, prepared.view())?;
    prepared.metrics().log_summary(
        analysis.module.0,
        analysis.classpath,
        analysis.source_root.map(|id| id.0),
        started.elapsed().as_secs_f64() * 1000.0,
    );
    Ok(hints)
}
