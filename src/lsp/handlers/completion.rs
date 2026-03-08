use std::sync::Arc;
use tower_lsp::lsp_types::*;
use tracing::debug;

use super::super::converters::candidate_to_lsp;
use crate::completion::CandidateKind;
use crate::completion::candidate::CompletionCandidate;
use crate::completion::engine::CompletionEngine;
use crate::language::{LanguageRegistry, ParseEnv};
use crate::semantic::CursorLocation;
use crate::workspace::Workspace;

pub async fn handle_completion(
    workspace: Arc<Workspace>,
    engine: Arc<CompletionEngine>,
    registry: Arc<LanguageRegistry>,
    params: CompletionParams,
) -> Option<CompletionResponse> {
    let uri = &params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let trigger = params
        .context
        .as_ref()
        .and_then(|ctx| ctx.trigger_character.as_deref())
        .and_then(|s| s.chars().next());

    let lang_id = workspace
        .documents
        .with_doc(uri, |doc| doc.language_id.clone())?;

    let lang = registry.find(&lang_id)?;

    let uri_str = uri.as_str();

    tracing::debug!(
        uri = %uri,
        lang = lang.id(),
        line = position.line,
        character = position.character,
        trigger = ?trigger,
        "completion request"
    );

    workspace.documents.with_doc_mut(uri, |doc| {
        if doc.tree.is_none() {
            let mut parser = lang.make_parser();
            doc.tree = parser.parse(&doc.text, None);
        }
    })?;

    let scope = workspace.scope_for_uri(uri);
    let index = workspace.index.read().await;
    let view = index.view(scope);

    let env = ParseEnv {
        name_table: Some(view.build_name_table()),
    };

    let (ctx, source_for_edits) = workspace.documents.with_doc(uri, |doc| {
        let tree = doc.tree.as_ref()?;
        let ctx = lang
            .parse_completion_context_with_tree(
                &doc.text,
                &doc.rope,
                tree.root_node(),
                position.line,
                position.character,
                trigger,
                &env,
            )?
            .with_file_uri(Arc::from(uri_str))
            .with_language_id(crate::language::LanguageId::new(lang_id.clone()));

        Some((ctx, doc.text.clone()))
    })??;

    tracing::debug!(location = ?ctx.location, query = %ctx.query, "parsed context");

    let candidates = engine.complete(scope, ctx.clone(), lang, &view);

    if candidates.is_empty() {
        debug!("no candidates");
        return None;
    }

    let candidates = lang.post_process_candidates(candidates, &ctx);

    const MAX_ITEMS: usize = 100;
    let items: Vec<CompletionItem> = candidates
        .iter()
        .take(MAX_ITEMS)
        .map(|c| map_candidate_item(c, &ctx.location, &source_for_edits, position))
        .collect();

    let is_incomplete = candidates.len() > MAX_ITEMS;

    debug!(
        count = items.len(),
        incomplete = is_incomplete,
        "returning completions"
    );

    Some(CompletionResponse::List(CompletionList {
        is_incomplete,
        items,
    }))
}

fn map_candidate_item(
    c: &CompletionCandidate,
    location: &CursorLocation,
    source_for_edits: &str,
    position: Position,
) -> CompletionItem {
    let mut item = candidate_to_lsp(c, source_for_edits);

    if matches!(location, CursorLocation::Import { .. }) {
        if let Some(edit) = crate::completion::import_completion::make_import_text_edit(
            &c.insert_text,
            source_for_edits,
            position,
        ) {
            item.text_edit = Some(edit);
            item.insert_text = None;
            item.insert_text_format = None;
        }
        item.filter_text = Some(c.insert_text.clone());
    } else if matches!(
        location,
        CursorLocation::MemberAccess { .. } | CursorLocation::StaticAccess { .. }
    ) && matches!(c.kind, CandidateKind::ClassName)
    {
        // In member/static access contexts, only replace the current member segment.
        // For `ChainCheck.;`, this becomes a zero-width insert right after the dot.
        if let Some(edit) = make_member_access_text_edit(&c.insert_text, source_for_edits, position)
        {
            item.text_edit = Some(edit);
            item.insert_text = None;
            item.insert_text_format = None;
        }
        item.filter_text = Some(c.label.to_string());
    } else if matches!(c.kind, CandidateKind::Package | CandidateKind::ClassName)
        && matches!(
            location,
            CursorLocation::Expression { .. } | CursorLocation::TypeAnnotation { .. }
        )
    {
        if let Some(edit) = make_package_text_edit(&c.insert_text, source_for_edits, position) {
            item.text_edit = Some(edit);
            item.insert_text = None;
            item.insert_text_format = None;
        }
        item.filter_text = Some(c.label.to_string());
    } else if c.source == "override" {
        if let Some(edit) = make_override_text_edit(&c.insert_text, source_for_edits, position) {
            item.text_edit = Some(edit);
            item.insert_text = None;
            item.insert_text_format = None;
        }
        item.filter_text = Some(c.label.to_string());
    }

    tracing::debug!(
        label = %item.label,
        filter_text = ?item.filter_text,
        insert_text = ?item.insert_text,
        text_edit = ?item.text_edit,
        sort_text = ?item.sort_text,
        kind = ?item.kind,
        additional_text_edits = ?item.additional_text_edits,
        source = c.source,
        "completion item emitted"
    );

    item
}

/// Override candidate textEdit: Replace the entire access-modifier prefix before the cursor with the full method stub
fn make_override_text_edit(
    insert_text: &str,
    source: &str,
    position: Position,
) -> Option<CompletionTextEdit> {
    let line = source.lines().nth(position.line as usize)?;
    let before_cursor = &line[..position.character as usize];

    let start_char = before_cursor
        .rfind(|c: char| !c.is_alphabetic())
        .map(|p| p + 1)
        .unwrap_or(0) as u32;

    Some(CompletionTextEdit::Edit(TextEdit {
        range: Range {
            start: Position {
                line: position.line,
                character: start_char,
            },
            end: Position {
                line: position.line,
                character: position.character,
            },
        },
        new_text: insert_text.to_string(),
    }))
}

/// Expression/MemberAccess 场景下 Package 候选的 textEdit：
/// 替换光标所在的整个"包路径词"（从行首非空白到光标）
fn make_package_text_edit(
    insert_text: &str,
    source: &str,
    position: Position,
) -> Option<CompletionTextEdit> {
    let line = source.lines().nth(position.line as usize)?;
    let before_cursor = &line[..position.character as usize];
    let start_char = before_cursor
        .rfind(|c: char| !c.is_alphanumeric() && c != '.' && c != '_')
        .map(|p| p + 1)
        .unwrap_or(0) as u32;

    Some(CompletionTextEdit::Edit(TextEdit {
        range: Range {
            start: Position {
                line: position.line,
                character: start_char,
            },
            end: Position {
                line: position.line,
                character: position.character,
            },
        },
        new_text: insert_text.to_string(),
    }))
}

fn make_member_access_text_edit(
    insert_text: &str,
    source: &str,
    position: Position,
) -> Option<CompletionTextEdit> {
    let line = source.lines().nth(position.line as usize)?;
    let before_cursor = &line[..position.character as usize];
    let start_char = before_cursor
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|p| p + 1)
        .unwrap_or(0) as u32;

    Some(CompletionTextEdit::Edit(TextEdit {
        range: Range {
            start: Position {
                line: position.line,
                character: start_char,
            },
            end: Position {
                line: position.line,
                character: position.character,
            },
        },
        new_text: insert_text.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn edit_range(edit: &CompletionTextEdit) -> Range {
        match edit {
            CompletionTextEdit::Edit(te) => te.range,
            CompletionTextEdit::InsertAndReplace(te) => te.insert,
        }
    }

    fn edit_text(edit: &CompletionTextEdit) -> &str {
        match edit {
            CompletionTextEdit::Edit(te) => te.new_text.as_str(),
            CompletionTextEdit::InsertAndReplace(te) => te.new_text.as_str(),
        }
    }

    #[test]
    fn test_member_access_text_edit_empty_prefix_is_zero_width() {
        let src = "ChainCheck.;";
        let pos = Position {
            line: 0,
            character: "ChainCheck.".len() as u32,
        };
        let edit = make_member_access_text_edit("Box", src, pos).expect("text edit");
        let range = edit_range(&edit);
        assert_eq!(range.start.character, pos.character);
        assert_eq!(range.end.character, pos.character);
        assert_eq!(edit_text(&edit), "Box");
    }

    #[test]
    fn test_member_access_text_edit_replaces_only_member_segment() {
        let src = "ChainCheck.Bo;";
        let pos = Position {
            line: 0,
            character: "ChainCheck.Bo".len() as u32,
        };
        let edit = make_member_access_text_edit("Box", src, pos).expect("text edit");
        let range = edit_range(&edit);
        assert_eq!(range.start.character, "ChainCheck.".len() as u32);
        assert_eq!(range.end.character, pos.character);
        assert_eq!(edit_text(&edit), "Box");
    }

    #[test]
    fn test_map_candidate_item_static_access_class_uses_member_edit() {
        let c = CompletionCandidate::new(
            Arc::from("Box"),
            "Box",
            CandidateKind::ClassName,
            "expression",
        );
        let loc = CursorLocation::StaticAccess {
            class_internal_name: Arc::from("org/cubewhy/ChainCheck"),
            member_prefix: String::new(),
        };
        let src = "ChainCheck.;";
        let pos = Position {
            line: 0,
            character: "ChainCheck.".len() as u32,
        };
        let item = map_candidate_item(&c, &loc, src, pos);
        let edit = item.text_edit.expect("text_edit expected");
        let range = edit_range(&edit);
        assert_eq!(item.label, "Box");
        assert_eq!(item.filter_text.as_deref(), Some("Box"));
        assert_eq!(range.start.character, pos.character);
        assert_eq!(range.end.character, pos.character);
        assert_eq!(edit_text(&edit), "Box");
    }
}
