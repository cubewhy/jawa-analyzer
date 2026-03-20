use std::sync::Arc;
use tower_lsp::lsp_types::*;

use crate::language::LanguageRegistry;
use crate::workspace::Workspace;

use super::syntax_access::ensure_parsed_source;

pub async fn handle_document_symbol(
    registry: Arc<LanguageRegistry>,
    workspace: Arc<Workspace>,
    params: DocumentSymbolParams,
) -> Option<DocumentSymbolResponse> {
    let uri = params.text_document.uri;

    let lang_id = workspace
        .documents
        .with_doc(&uri, |doc| doc.language_id().to_owned())?;

    let lang = registry.find(&lang_id)?;

    let source = ensure_parsed_source(&workspace, &uri, lang)?;

    if lang.id() == "java"
        && let Some(symbols) = crate::language::java::rowan_symbols::collect_java_symbols(&source)
    {
        return Some(DocumentSymbolResponse::Nested(symbols));
    }

    let symbols = workspace.documents.with_doc(&uri, |doc| {
        let file = doc.source();
        let root = file.root_node()?;
        lang.collect_symbols(root, file)
    })??;

    Some(DocumentSymbolResponse::Nested(symbols))
}
