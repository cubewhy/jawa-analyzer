use std::sync::Arc;
use tower_lsp::jsonrpc::{self, Result as LspResult};
use tower_lsp::lsp_types::*;

use crate::language::LanguageRegistry;
use crate::lsp::request_context::RequestContext;
use crate::workspace::Workspace;

pub async fn handle_document_symbol(
    registry: Arc<LanguageRegistry>,
    workspace: Arc<Workspace>,
    params: DocumentSymbolParams,
    request: Arc<RequestContext>,
) -> LspResult<Option<DocumentSymbolResponse>> {
    let task = tokio::task::spawn_blocking(move || {
        handle_document_symbol_blocking(registry, workspace, params, request)
    });
    match task.await {
        Ok(result) => result.map_err(|cancelled| cancelled.into_lsp_error()),
        Err(error) => {
            tracing::error!(%error, "document symbol worker panicked");
            Err(jsonrpc::Error::internal_error())
        }
    }
}

fn handle_document_symbol_blocking(
    registry: Arc<LanguageRegistry>,
    workspace: Arc<Workspace>,
    params: DocumentSymbolParams,
    request: Arc<RequestContext>,
) -> crate::lsp::request_cancellation::RequestResult<Option<DocumentSymbolResponse>> {
    let uri = params.text_document.uri;

    let lang_id = workspace
        .documents
        .with_doc(&uri, |doc| doc.language_id().to_owned());
    let Some(lang_id) = lang_id else {
        return Ok(None);
    };

    let Some(lang) = registry.find(&lang_id) else {
        return Ok(None);
    };

    // Ensure tree is parsed.
    request.check_cancelled("document_symbol.before_ensure_tree")?;
    let has_tree = workspace
        .documents
        .with_doc(&uri, |doc| doc.source().tree.is_some())
        .unwrap_or(false);
    if !has_tree {
        workspace.documents.with_doc_mut(&uri, |doc| {
            if doc.source().tree.is_some() {
                return;
            }
            let tree = lang.parse_tree(doc.source().text(), None);
            doc.set_tree(tree);
        });
    }

    let symbols = workspace.documents.with_doc(&uri, |doc| {
        let file = doc.source();
        let Some(root) = file.root_node() else {
            return Ok(None);
        };
        lang.collect_symbols(root, file, Some(&request))
    });
    let Some(symbols) = symbols else {
        return Ok(None);
    };
    let Some(symbols) = symbols? else {
        return Ok(None);
    };

    Ok(Some(DocumentSymbolResponse::Nested(symbols)))
}
