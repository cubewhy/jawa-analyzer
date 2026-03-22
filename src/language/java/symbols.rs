use tower_lsp::lsp_types::{DocumentSymbol, SymbolKind};
use tree_sitter::Node;

use crate::lsp::converters::ts_node_to_range;

pub fn collect_java_symbols<'a>(
    root: Node<'a>,
    bytes: &'a [u8],
    request: Option<&crate::lsp::request_context::RequestContext>,
) -> crate::lsp::request_cancellation::RequestResult<Vec<DocumentSymbol>> {
    let mut out = Vec::new();
    collect_type_declarations(root, bytes, request, &mut out)?;
    Ok(out)
}

fn collect_type_declarations<'a>(
    node: Node<'a>,
    bytes: &'a [u8],
    request: Option<&crate::lsp::request_context::RequestContext>,
    out: &mut Vec<DocumentSymbol>,
) -> crate::lsp::request_cancellation::RequestResult<()> {
    let mut cursor = node.walk();
    for (index, child) in node.children(&mut cursor).enumerate() {
        if index % 32 == 0
            && let Some(request) = request
        {
            request.check_cancelled("document_symbol.type_declarations")?;
        }
        if is_type_declaration(child.kind()) {
            if let Some(symbol) = build_type_symbol(child, bytes, request)? {
                out.push(symbol);
            }
            continue;
        }
        if matches!(child.kind(), "program" | "ERROR") {
            collect_type_declarations(child, bytes, request, out)?;
        }
    }
    Ok(())
}

fn is_type_declaration(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
    )
}

fn build_type_symbol<'a>(
    node: Node<'a>,
    bytes: &'a [u8],
    request: Option<&crate::lsp::request_context::RequestContext>,
) -> crate::lsp::request_cancellation::RequestResult<Option<DocumentSymbol>> {
    let Some((mut sym, body)) = start_type_symbol(node, bytes) else {
        return Ok(None);
    };
    let children = if let Some(body_node) = body {
        collect_type_members(body_node, bytes, request)?
    } else {
        Vec::new()
    };
    sym.children = Some(children);
    Ok(Some(sym))
}

fn collect_type_members<'a>(
    body: Node<'a>,
    bytes: &'a [u8],
    request: Option<&crate::lsp::request_context::RequestContext>,
) -> crate::lsp::request_cancellation::RequestResult<Vec<DocumentSymbol>> {
    let mut out = Vec::new();
    let mut cursor = body.walk();
    for (index, child) in body.children(&mut cursor).enumerate() {
        if index % 32 == 0
            && let Some(request) = request
        {
            request.check_cancelled("document_symbol.type_members")?;
        }
        if is_type_declaration(child.kind()) {
            if let Some(symbol) = build_type_symbol(child, bytes, request)? {
                out.push(symbol);
            }
            continue;
        }

        match child.kind() {
            "method_declaration"
            | "constructor_declaration"
            | "compact_constructor_declaration" => {
                if let Some(symbol) = parse_method_symbol(child, bytes) {
                    out.push(symbol);
                }
            }
            "enum_constant" | "enum_constant_declaration" => {
                if let Some(symbol) = parse_enum_constant_symbol(child, bytes) {
                    out.push(symbol);
                }
            }
            "enum_body_declarations" | "ERROR" => {
                out.extend(collect_type_members(child, bytes, request)?);
            }
            "field_declaration" => out.extend(parse_field_symbols(child, bytes)),
            _ => {}
        }
    }
    Ok(out)
}

/// Generate a "type symbol (children empty for now) + body node (for continued traversal)"
fn start_type_symbol<'a>(
    node: Node<'a>,
    bytes: &'a [u8],
) -> Option<(DocumentSymbol, Option<Node<'a>>)> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(bytes).ok()?.to_string();

    let kind = match node.kind() {
        "interface_declaration" | "annotation_type_declaration" => SymbolKind::INTERFACE,
        "enum_declaration" => SymbolKind::ENUM,
        _ => SymbolKind::CLASS, // Classes and records are both CLASS
    };

    let range = ts_node_to_range(&node);
    let selection_range = ts_node_to_range(&name_node);
    let body = node.child_by_field_name("body");

    #[allow(deprecated)]
    let sym = DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: None,
    };

    Some((sym, body))
}

fn parse_method_symbol<'a>(node: Node<'a>, bytes: &'a [u8]) -> Option<DocumentSymbol> {
    let name_node = node
        .child_by_field_name("name")
        .or_else(|| node.child_by_field_name("identifier"))?; // constructor 用 identifier
    let name = name_node.utf8_text(bytes).ok()?.to_string();

    let kind = if node.kind() == "constructor_declaration"
        || node.kind() == "compact_constructor_declaration"
    {
        SymbolKind::CONSTRUCTOR
    } else {
        SymbolKind::METHOD
    };

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range: ts_node_to_range(&node),
        selection_range: ts_node_to_range(&name_node),
        children: None,
    })
}

fn parse_field_symbols<'a>(node: Node<'a>, bytes: &'a [u8]) -> Vec<DocumentSymbol> {
    let mut results = Vec::new();

    // Find type: used for detail display
    let type_text = {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .find(|c| c.kind().ends_with("_type") || c.kind() == "type_identifier")
            .and_then(|c| c.utf8_text(bytes).ok())
            .map(|t| t.to_string())
    };

    // parse variable_declarator
    let mut cursor = node.walk();
    for declarator in node
        .children(&mut cursor)
        .filter(|c| c.kind() == "variable_declarator")
    {
        let Some(name_node) = declarator.child_by_field_name("name") else {
            continue;
        };
        let Ok(name) = name_node.utf8_text(bytes) else {
            continue;
        };

        #[allow(deprecated)]
        results.push(DocumentSymbol {
            name: name.to_string(),
            detail: type_text.clone(),
            kind: SymbolKind::FIELD,
            tags: None,
            deprecated: None,
            range: ts_node_to_range(&node),
            selection_range: ts_node_to_range(&name_node),
            children: None,
        });
    }

    results
}

fn parse_enum_constant_symbol<'a>(node: Node<'a>, bytes: &'a [u8]) -> Option<DocumentSymbol> {
    let name_node = node
        .child_by_field_name("name")
        .or_else(|| node.child_by_field_name("identifier"))
        .or_else(|| tree_sitter_utils::traversal::any_child_of_kind(node, "identifier"))?;

    let name = name_node.utf8_text(bytes).ok()?.to_string();

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: None,
        kind: SymbolKind::ENUM_MEMBER,
        tags: None,
        deprecated: None,
        range: ts_node_to_range(&node),
        selection_range: ts_node_to_range(&name_node),
        children: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::java::make_java_parser;
    use insta::assert_ron_snapshot;

    fn collect(src: &str) -> Vec<DocumentSymbol> {
        let mut parser = make_java_parser();
        let tree = parser.parse(src, None).expect("parse java");
        collect_java_symbols(tree.root_node(), src.as_bytes(), None).expect("symbol collection")
    }

    #[test]
    fn nested_class_symbols_preserve_ownership_boundaries() {
        let src = indoc::indoc! {r#"
            package org.cubewhy;

            class ChainCheck {
                int outerField;
                void outerMethod() {}

                static class Box<T> {
                    int innerField;
                    T get() { return null; }
                    static class BoxV<V> {
                        V getV() { return null; }
                    }
                }
            }
        "#};
        let syms = collect(src);
        assert_ron_snapshot!(syms);
    }

    #[test]
    fn nested_class_members_do_not_absorb_parent_members() {
        let src = indoc::indoc! {r#"
            class Outer {
                int outerField;
                void outerMethod() {}
                static class Inner {
                    int innerField;
                    void innerMethod() {}
                }
            }
        "#};
        let syms = collect(src);
        let outer = syms
            .iter()
            .find(|s| s.name == "Outer")
            .expect("outer symbol");
        let outer_children = outer.children.as_ref().expect("outer children");
        let inner = outer_children
            .iter()
            .find(|s| s.name == "Inner")
            .expect("inner symbol");
        let inner_children = inner.children.as_ref().expect("inner children");

        assert!(
            inner_children.iter().all(|s| s.name != "outerField"),
            "inner must not contain parent field"
        );
        assert!(
            inner_children.iter().all(|s| s.name != "outerMethod"),
            "inner must not contain parent method"
        );
        assert!(
            outer_children.iter().any(|s| s.name == "outerField")
                && outer_children.iter().any(|s| s.name == "outerMethod"),
            "outer must keep its own members"
        );
    }
}
