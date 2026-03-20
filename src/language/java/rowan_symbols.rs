use tower_lsp::lsp_types::{DocumentSymbol, SymbolKind};

use crate::lsp::converters::text_range_to_lsp_range;
use crate::syntax::{SyntaxElement, SyntaxNode, TextRange, kind_name};
use crate::workspace::SourceFile;

pub fn collect_java_symbols(file: &SourceFile) -> Option<Vec<DocumentSymbol>> {
    let root = file.syntax()?.root();
    let mut out = Vec::new();
    collect_type_declarations(&root, file, &mut out);
    Some(out)
}

fn collect_type_declarations(node: &SyntaxNode, file: &SourceFile, out: &mut Vec<DocumentSymbol>) {
    for child in node.children() {
        if is_type_declaration(&child) {
            if let Some(symbol) = build_type_symbol(&child, file) {
                out.push(symbol);
            }
            continue;
        }

        if is_container_node(&child) {
            collect_type_declarations(&child, file, out);
        }
    }
}

fn collect_type_members(body: &SyntaxNode, file: &SourceFile) -> Vec<DocumentSymbol> {
    let mut out = Vec::new();

    for child in body.children() {
        if is_type_declaration(&child) {
            if let Some(symbol) = build_type_symbol(&child, file) {
                out.push(symbol);
            }
            continue;
        }

        match kind_name(child.kind()) {
            Some("method_declaration")
            | Some("constructor_declaration")
            | Some("compact_constructor_declaration") => {
                if let Some(sym) = parse_method_symbol(&child, file) {
                    out.push(sym);
                }
            }
            Some("field_declaration") => out.extend(parse_field_symbols(&child, file)),
            Some("enum_constant") | Some("enum_constant_declaration") => {
                if let Some(sym) = parse_enum_constant_symbol(&child, file) {
                    out.push(sym);
                }
            }
            Some("enum_body_declarations") | Some("ERROR") => {
                out.extend(collect_type_members(&child, file));
            }
            _ => {}
        }
    }

    out
}

fn build_type_symbol(node: &SyntaxNode, file: &SourceFile) -> Option<DocumentSymbol> {
    let name = find_named_text(node, &["identifier", "type_identifier"])?;
    let name_range = find_named_range(node, &["identifier", "type_identifier"])?;
    let body = find_named_child(
        node,
        &[
            "class_body",
            "interface_body",
            "enum_body",
            "annotation_type_body",
        ],
    );
    let kind = match kind_name(node.kind()) {
        Some("interface_declaration") | Some("annotation_type_declaration") => {
            SymbolKind::INTERFACE
        }
        Some("enum_declaration") => SymbolKind::ENUM,
        _ => SymbolKind::CLASS,
    };

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range: text_range_to_lsp_range(file, node.text_range()),
        selection_range: text_range_to_lsp_range(file, name_range),
        children: Some(
            body.map(|body| collect_type_members(&body, file))
                .unwrap_or_default(),
        ),
    })
}

fn parse_method_symbol(node: &SyntaxNode, file: &SourceFile) -> Option<DocumentSymbol> {
    let name = find_named_text(node, &["identifier"])?;
    let name_range = find_named_range(node, &["identifier"])?;
    let kind = match kind_name(node.kind()) {
        Some("constructor_declaration") | Some("compact_constructor_declaration") => {
            SymbolKind::CONSTRUCTOR
        }
        _ => SymbolKind::METHOD,
    };

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range: text_range_to_lsp_range(file, node.text_range()),
        selection_range: text_range_to_lsp_range(file, name_range),
        children: None,
    })
}

fn parse_field_symbols(node: &SyntaxNode, file: &SourceFile) -> Vec<DocumentSymbol> {
    let type_text = node
        .children()
        .find(|child| {
            kind_name(child.kind())
                .is_some_and(|kind| kind.ends_with("_type") || kind == "type_identifier")
        })
        .map(|child| child.text().to_string());

    node.children()
        .filter(|child| kind_name(child.kind()) == Some("variable_declarator"))
        .filter_map(|decl| {
            let name = find_named_text(&decl, &["identifier"])?;
            let name_range = find_named_range(&decl, &["identifier"])?;
            #[allow(deprecated)]
            Some(DocumentSymbol {
                name,
                detail: type_text.clone(),
                kind: SymbolKind::FIELD,
                tags: None,
                deprecated: None,
                range: text_range_to_lsp_range(file, node.text_range()),
                selection_range: text_range_to_lsp_range(file, name_range),
                children: None,
            })
        })
        .collect()
}

fn parse_enum_constant_symbol(node: &SyntaxNode, file: &SourceFile) -> Option<DocumentSymbol> {
    let name = find_named_text(node, &["identifier"])?;
    let name_range = find_named_range(node, &["identifier"])?;

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: None,
        kind: SymbolKind::ENUM_MEMBER,
        tags: None,
        deprecated: None,
        range: text_range_to_lsp_range(file, node.text_range()),
        selection_range: text_range_to_lsp_range(file, name_range),
        children: None,
    })
}

fn find_named_text(node: &SyntaxNode, kinds: &[&str]) -> Option<String> {
    find_named_element(node, kinds).map(|el| match el {
        SyntaxElement::Node(node) => node.text().to_string(),
        SyntaxElement::Token(token) => token.text().to_string(),
    })
}

fn find_named_range(node: &SyntaxNode, kinds: &[&str]) -> Option<TextRange> {
    find_named_element(node, kinds).map(|el| match el {
        SyntaxElement::Node(node) => node.text_range(),
        SyntaxElement::Token(token) => token.text_range(),
    })
}

fn find_named_element(node: &SyntaxNode, kinds: &[&str]) -> Option<SyntaxElement> {
    node.children_with_tokens().find(|child| match child {
        SyntaxElement::Node(node) => {
            kind_name(node.kind()).is_some_and(|kind| kinds.contains(&kind))
        }
        SyntaxElement::Token(token) => {
            kind_name(token.kind()).is_some_and(|kind| kinds.contains(&kind))
        }
    })
}

fn find_named_child(node: &SyntaxNode, kinds: &[&str]) -> Option<SyntaxNode> {
    node.children()
        .find(|child| kind_name(child.kind()).is_some_and(|kind| kinds.contains(&kind)))
}

fn is_type_declaration(node: &SyntaxNode) -> bool {
    matches!(
        kind_name(node.kind()),
        Some(
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        )
    )
}

fn is_container_node(node: &SyntaxNode) -> bool {
    matches!(kind_name(node.kind()), Some("program") | Some("ERROR"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::java::make_java_parser;
    use crate::workspace::SourceFile;
    use insta::assert_ron_snapshot;
    use tower_lsp::lsp_types::Url;

    fn collect(src: &str) -> Vec<DocumentSymbol> {
        let mut parser = make_java_parser();
        let tree = parser.parse(src, None).expect("parse java");
        let file = SourceFile::new(
            Url::parse("file:///test/Test.java").unwrap(),
            "java",
            1,
            src,
            Some(tree),
        );
        collect_java_symbols(&file).unwrap()
    }

    #[test]
    fn nested_class_symbols_preserve_ownership_boundaries_rowan() {
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
    fn malformed_header_still_yields_symbols_rowan() {
        let src = "package demo\nclass Test { int field; void run() {} }";
        let syms = collect(src);

        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Test");
        assert_eq!(syms[0].children.as_ref().map(|c| c.len()), Some(2));
    }
}
