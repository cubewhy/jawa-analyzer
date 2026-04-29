use crate::{
    EntryPoint, Lang,
    SyntaxKind::{self, *},
    parse,
    parser::parse_partial,
};
use rowan::{GreenNode, SyntaxNode, TextRange, TextSize};

pub struct TextEdit<'a> {
    pub text: &'a str,
    pub start: usize,
    pub end: usize,
}

impl<'a> TextEdit<'a> {
    pub fn get_text_range(&self) -> TextRange {
        let start = TextSize::new(self.start as u32);
        let end = TextSize::new(self.end as u32);
        TextRange::new(start, end)
    }
}

fn get_supported_parent(mut node: SyntaxNode<Lang>) -> SyntaxNode<Lang> {
    loop {
        match node.kind() {
            ROOT | BLOCK | CLASS_BODY | INTERFACE_BODY | SWITCH_BLOCK | ANNOTATION_TYPE_BODY
            | ENUM_BODY | RECORD_BODY | MODULE_BODY | ARRAY_INITIALIZER => return node,
            _ => {}
        }
        if let Some(parent) = node.parent() {
            node = parent;
        } else {
            break;
        }
    }
    node
}

pub fn find_changed_node(edit: &TextEdit, tree: &SyntaxNode<Lang>) -> SyntaxNode<Lang> {
    let Some(child) = tree.child_or_token_at_range(edit.get_text_range()) else {
        return tree.clone();
    };

    let node = match child {
        rowan::NodeOrToken::Node(n) => n,
        rowan::NodeOrToken::Token(t) => t.parent().unwrap_or_else(|| tree.clone()),
    };

    get_supported_parent(node)
}

fn is_boundary_safe_braces(new_text: &str) -> bool {
    let mut depth = 0;
    for c in new_text.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

pub fn apply_edit_to_node(edit: &TextEdit, target_node: &SyntaxNode<Lang>) -> String {
    let new_input = edit.text;
    let node_range = target_node.text_range();
    let node_start = u32::from(node_range.start());

    let edit_start_u32 = edit.start as u32;
    let edit_end_u32 = edit.end as u32;

    let relative_start = TextSize::new(edit_start_u32.saturating_sub(node_start));
    let relative_end = TextSize::new(edit_end_u32.saturating_sub(node_start));

    let syntax_text = target_node.text();

    let old_len = usize::from(syntax_text.len());
    let replaced_len = usize::from(relative_end) - usize::from(relative_start);
    let new_len = old_len - replaced_len + new_input.len();

    let mut new_string = String::with_capacity(new_len);

    syntax_text
        .slice(TextRange::new(TextSize::new(0), relative_start))
        .for_each_chunk(|chunk| new_string.push_str(chunk));

    new_string.push_str(new_input);

    syntax_text
        .slice(TextRange::new(relative_end, syntax_text.len()))
        .for_each_chunk(|chunk| new_string.push_str(chunk));

    new_string
}

pub fn incremental_reparse(edit: &TextEdit, tree: SyntaxNode<Lang>) -> SyntaxNode<Lang> {
    let mut target_node = find_changed_node(edit, &tree);

    loop {
        if target_node.kind() == ROOT {
            let full_new_text = apply_edit_to_node(edit, &target_node);
            return do_full_parse(&full_new_text);
        }

        let node_range = target_node.text_range();
        let node_range_start = u32::from(node_range.start());
        let edit_start = edit.start as u32;
        let edit_end = edit.end as u32;

        let relative_start = TextSize::new(edit_start.saturating_sub(node_range_start));
        let relative_end = TextSize::new(edit_end.saturating_sub(node_range_start));
        let replaced_text = target_node
            .text()
            .slice(TextRange::new(relative_start, relative_end))
            .to_string();

        let safe_braces =
            is_boundary_safe_braces(&replaced_text) && is_boundary_safe_braces(edit.text);

        let touches_boundaries = edit_start <= node_range_start || edit_end >= node_range_start;

        if safe_braces && !touches_boundaries {
            let new_source = apply_edit_to_node(edit, &target_node);

            if let Ok(new_green_node) = try_parse_partial(target_node.kind(), &new_source) {
                return replace_green_node_in_tree(target_node, new_green_node);
            }
        }

        if let Some(parent) = target_node.parent() {
            target_node = get_supported_parent(parent);
        } else {
            break;
        }
    }

    let full_new_text = apply_edit_to_node(edit, &tree);
    do_full_parse(&full_new_text)
}

fn do_full_parse(source: &str) -> SyntaxNode<Lang> {
    parse(source).into_syntax_node()
}

fn try_parse_partial(kind: SyntaxKind, source: &str) -> Result<GreenNode, ()> {
    let entry = EntryPoint::try_from(kind)?;
    Ok(parse_partial(source, entry).into_green_node())
}

pub fn replace_green_node_in_tree(
    old_node: SyntaxNode<Lang>,
    new_green: GreenNode,
) -> SyntaxNode<Lang> {
    let new_root_green = old_node.replace_with(new_green);

    SyntaxNode::new_root(new_root_green)
}
