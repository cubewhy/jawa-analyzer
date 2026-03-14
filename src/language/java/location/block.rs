use crate::language::java::JavaContextExtractor;
use crate::language::java::location::utils;
use crate::language::java::utils::strip_sentinel;
use crate::semantic::CursorLocation;
use tree_sitter::Node;

use super::text::detect_new_keyword_before_cursor;
use super::utils::{cursor_truncated_text, detect_variable_name_position};

/// cursor_node is the block itself (cases 4, 9, 10, 11, 13, 14)
pub(super) fn handle_block_as_cursor(
    ctx: &JavaContextExtractor,
    block: Node,
) -> (CursorLocation, String) {
    // Find last named child ending at or before cursor
    let last_child = {
        let mut wc = block.walk();
        let mut last: Option<Node> = None;
        for child in block.named_children(&mut wc) {
            if child.end_byte() <= ctx.offset {
                last = Some(child);
            }
        }
        last
    };

    if let Some(child) = last_child
        && child.kind() == "ERROR"
    {
        return handle_error_as_last_block_child(ctx, child);
    }

    // No ERROR: check for variable name position (e.g. `List<String> |`)
    if let Some(var_loc) = detect_variable_name_position(ctx, block) {
        return var_loc;
    }

    // Empty block or cursor after complete statement → Expression
    (
        CursorLocation::Expression {
            prefix: String::new(),
        },
        String::new(),
    )
}

/// ERROR is the last child of block before cursor (cases 9, 11, 12, 13)
fn handle_error_as_last_block_child(
    ctx: &JavaContextExtractor,
    error_node: Node,
) -> (CursorLocation, String) {
    // Case 9: `a.put` → ERROR contains scoped_type_identifier
    {
        let mut wc = error_node.walk();
        for child in error_node.named_children(&mut wc) {
            if child.kind() == "scoped_type_identifier"
                && let Some(r) = scoped_type_to_member_access(ctx, child)
            {
                return r;
            }
        }
    }

    // Type awaiting variable name: `String |`, `int |`, `String[] |`, `List<String> |`
    if let Some(var_loc) = detect_type_awaiting_name_in_error(ctx, error_node) {
        return var_loc;
    }

    let before = &ctx.source[..ctx.offset.min(ctx.source.len())];
    // Case 12/cases with `new`: detect new keyword
    if let Some((class_prefix, expected_type)) = detect_new_keyword_before_cursor(before) {
        return (
            CursorLocation::ConstructorCall {
                class_prefix: class_prefix.clone(),
                expected_type,
            },
            class_prefix,
        );
    }
    // Case 11/13: incomplete assignment `int x =`) → Expression
    (
        CursorLocation::Expression {
            prefix: String::new(),
        },
        String::new(),
    )
}

/// ERROR contains a single type node and cursor is after it → VariableName position.
fn detect_type_awaiting_name_in_error(
    ctx: &JavaContextExtractor,
    error_node: Node,
) -> Option<(CursorLocation, String)> {
    if ctx.offset < error_node.end_byte() {
        return None;
    }
    let mut wc = error_node.walk();
    let named_children: Vec<Node> = error_node.named_children(&mut wc).collect();
    if named_children.len() != 1 {
        return None;
    }
    let inner = named_children[0];
    if !utils::is_type_like_node_kind(inner.kind()) {
        return None;
    }
    if inner.kind() == "identifier" {
        let text = ctx.node_text(inner).trim();
        if crate::language::java::members::is_java_keyword(text) {
            return None;
        }
    }
    let mut wc2 = error_node.walk();
    let has_assignment_or_semi = error_node
        .children(&mut wc2)
        .any(|c| matches!(c.kind(), "=" | ";"));
    if has_assignment_or_semi {
        return None;
    }
    let type_name = ctx.node_text(inner).trim().to_string();
    if type_name.is_empty() {
        return None;
    }
    Some((CursorLocation::VariableName { type_name }, String::new()))
}

/// `a.put` parsed as scoped_type_identifier → MemberAccess{receiver="a", prefix="put"}
pub(super) fn scoped_type_to_member_access(
    ctx: &JavaContextExtractor,
    scoped: Node,
) -> Option<(CursorLocation, String)> {
    let mut wc = scoped.walk();
    let parts: Vec<Node> = scoped.named_children(&mut wc).collect();
    if parts.len() < 2 {
        return None;
    }
    let member_node = *parts.last()?;
    // dot is one byte before member_node
    let receiver_end = member_node.start_byte().saturating_sub(1);
    let receiver_start = scoped.start_byte();
    if receiver_end <= receiver_start {
        return None;
    }
    let receiver_expr = ctx.source[receiver_start..receiver_end].trim().to_string();
    if receiver_expr.is_empty() || receiver_expr.contains(' ') || receiver_expr.contains('\t') {
        return None;
    }

    let member_prefix = strip_sentinel(&cursor_truncated_text(ctx, member_node));

    if receiver_expr.is_empty() {
        return None;
    }

    Some((
        CursorLocation::MemberAccess {
            receiver_semantic_type: None,
            receiver_type: None,
            member_prefix: member_prefix.clone(),
            receiver_expr,
            arguments: None,
        },
        member_prefix,
    ))
}
