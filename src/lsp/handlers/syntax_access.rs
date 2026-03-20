use std::sync::Arc;

use tower_lsp::lsp_types::Url;

use crate::language::{Language, LanguageRegistry};
use crate::syntax::java::{AstNode, JavaFile};
use crate::workspace::{SourceFile, Workspace};

pub fn ensure_parsed_source(
    workspace: &Workspace,
    uri: &Url,
    lang: &dyn Language,
) -> Option<Arc<SourceFile>> {
    let has_syntax = workspace
        .documents
        .with_doc(uri, |doc| doc.source().has_unified_syntax())
        .unwrap_or(false);

    if !has_syntax {
        workspace.documents.with_doc_mut(uri, |doc| {
            if doc.source().has_unified_syntax() {
                return;
            }

            let tree = lang.parse_tree(doc.source().text(), None);
            doc.set_tree(tree);
        })?;
    }

    workspace
        .documents
        .with_doc(uri, |doc| Arc::clone(doc.source()))
}

pub fn java_file_for_uri(
    workspace: &Workspace,
    registry: &LanguageRegistry,
    uri: &Url,
) -> Option<(Arc<SourceFile>, JavaFile)> {
    let lang_id = workspace
        .documents
        .with_doc(uri, |doc| doc.language_id().to_owned())?;
    let lang = registry.find(&lang_id)?;
    if lang.id() != "java" {
        return None;
    }

    let source = ensure_parsed_source(workspace, uri, lang)?;
    let syntax = source.syntax()?.root();
    let file = JavaFile::cast(syntax)?;
    Some((source, file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::LanguageRegistry;
    use crate::workspace::document::Document;
    use tower_lsp::lsp_types::Url;

    #[test]
    fn ensure_parsed_source_builds_unified_syntax_for_unparsed_doc() {
        let workspace = Workspace::new();
        let registry = LanguageRegistry::new();
        let uri = Url::parse("file:///test/Test.java").unwrap();

        workspace.documents.open(Document::new(SourceFile::new(
            uri.clone(),
            "java",
            1,
            "package demo; class Test {}",
            None,
        )));

        let lang = registry.find("java").unwrap();
        let source = ensure_parsed_source(&workspace, &uri, lang).unwrap();

        assert!(source.tree.is_some());
        assert!(source.has_unified_syntax());
    }

    #[test]
    fn java_file_for_uri_handles_incomplete_package_header() {
        let workspace = Workspace::new();
        let registry = LanguageRegistry::new();
        let uri = Url::parse("file:///test/Test.java").unwrap();

        workspace.documents.open(Document::new(SourceFile::new(
            uri.clone(),
            "java",
            1,
            "package demo\nimport java.util.List\nclass Test {}",
            None,
        )));

        let (_source, file) = java_file_for_uri(&workspace, &registry, &uri).unwrap();

        assert_eq!(
            file.package().and_then(|pkg| pkg.path_text()).as_deref(),
            Some("demo")
        );
        assert_eq!(file.imports().count(), 1);
    }
}
