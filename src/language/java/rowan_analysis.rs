use rust_asm::constants::{ACC_PUBLIC, ACC_VARARGS};
use std::sync::Arc;

use crate::index::{FieldSummary, MethodParams, MethodSummary};
use crate::language::java::members::is_java_keyword;
use crate::language::java::type_ctx::{SourceTypeCtx, build_java_descriptor};
use crate::semantic::context::CurrentClassMember;
use crate::syntax::{SyntaxElement, SyntaxNode, SyntaxSnapshot, TextSize, kind_name};

pub fn extract_current_class_members(
    syntax: &SyntaxSnapshot,
    offset: usize,
    type_ctx: &SourceTypeCtx,
) -> Vec<CurrentClassMember> {
    let Some(decl) = enclosing_type_decl(syntax, offset) else {
        return Vec::new();
    };
    extract_members_from_type_decl(&decl, type_ctx)
}

pub fn extract_enclosing_member(
    syntax: &SyntaxSnapshot,
    offset: usize,
    type_ctx: &SourceTypeCtx,
) -> Option<CurrentClassMember> {
    let token = syntax
        .root()
        .token_at_offset(TextSize::from(offset as u32))
        .right_biased()?;

    token
        .parent()
        .into_iter()
        .flat_map(|parent| parent.ancestors())
        .find_map(|node| match kind_name(node.kind()) {
            Some("method_declaration") => parse_method_node(&node, type_ctx),
            Some("constructor_declaration") | Some("compact_constructor_declaration") => {
                parse_constructor_node(&node, type_ctx)
            }
            _ => None,
        })
}

fn enclosing_type_decl(syntax: &SyntaxSnapshot, offset: usize) -> Option<SyntaxNode> {
    let token = syntax
        .root()
        .token_at_offset(TextSize::from(offset as u32))
        .right_biased()?;

    token
        .parent()
        .into_iter()
        .flat_map(|parent| parent.ancestors())
        .find(|node| {
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
        })
}

fn extract_members_from_type_decl(
    decl: &SyntaxNode,
    type_ctx: &SourceTypeCtx,
) -> Vec<CurrentClassMember> {
    let Some(body) = find_child_node(
        decl,
        &[
            "class_body",
            "interface_body",
            "enum_body",
            "annotation_type_body",
        ],
    ) else {
        return Vec::new();
    };

    let mut members = Vec::new();
    collect_members_from_body(&body, type_ctx, &mut members);
    members
}

fn collect_members_from_body(
    node: &SyntaxNode,
    type_ctx: &SourceTypeCtx,
    out: &mut Vec<CurrentClassMember>,
) {
    for child in node.children() {
        match kind_name(child.kind()) {
            Some("method_declaration") => {
                if let Some(member) = parse_method_node(&child, type_ctx) {
                    out.push(member);
                }
            }
            Some("constructor_declaration") | Some("compact_constructor_declaration") => {
                if let Some(member) = parse_constructor_node(&child, type_ctx) {
                    out.push(member);
                }
            }
            Some("field_declaration") => out.extend(parse_field_node(&child, type_ctx)),
            Some("ERROR")
            | Some("enum_body_declarations")
            | Some("class_body")
            | Some("interface_body")
            | Some("enum_body")
            | Some("annotation_type_body")
            | Some("program") => collect_members_from_body(&child, type_ctx, out),
            _ => {}
        }
    }
}

fn parse_method_node(node: &SyntaxNode, type_ctx: &SourceTypeCtx) -> Option<CurrentClassMember> {
    let mut flags = modifier_flags(node).unwrap_or(ACC_PUBLIC);
    let name = first_token_text(node, &["identifier"])?;
    if matches!(name.as_str(), "<init>" | "<clinit>") || is_java_keyword(&name) {
        return None;
    }

    let ret_type = first_node_text(
        node,
        &[
            "void_type",
            "integral_type",
            "floating_point_type",
            "boolean_type",
            "type_identifier",
            "scoped_type_identifier",
            "array_type",
            "generic_type",
        ],
    )
    .unwrap_or_else(|| "void".to_string());

    let params_node = find_child_node(node, &["formal_parameters"]);
    let params_text = params_node
        .as_ref()
        .map(|node| node.text().to_string())
        .unwrap_or_else(|| "()".to_string());
    if params_node.as_ref().is_some_and(has_spread_parameter) {
        flags |= ACC_VARARGS;
    }

    let descriptor = build_java_descriptor(&params_text, &ret_type, type_ctx);
    let param_names = params_node
        .as_ref()
        .map(extract_param_names)
        .unwrap_or_default();

    Some(CurrentClassMember::Method(Arc::new(MethodSummary {
        name: Arc::from(name),
        params: MethodParams::from_descriptor_and_names(&descriptor, &param_names),
        annotations: Vec::new(),
        access_flags: flags,
        is_synthetic: false,
        generic_signature: None,
        return_type: crate::semantic::types::parse_return_type_from_descriptor(&descriptor),
    })))
}

fn parse_constructor_node(
    node: &SyntaxNode,
    type_ctx: &SourceTypeCtx,
) -> Option<CurrentClassMember> {
    let mut flags = modifier_flags(node).unwrap_or(ACC_PUBLIC);
    let params_node = if kind_name(node.kind()) == Some("compact_constructor_declaration") {
        node.parent()
            .and_then(|parent| parent.parent())
            .and_then(|record| find_child_node(&record, &["formal_parameters"]))
    } else {
        find_child_node(node, &["formal_parameters"])
    };

    let params_text = params_node
        .as_ref()
        .map(|node| node.text().to_string())
        .unwrap_or_else(|| "()".to_string());
    if params_node.as_ref().is_some_and(has_spread_parameter) {
        flags |= ACC_VARARGS;
    }

    let descriptor = build_java_descriptor(&params_text, "void", type_ctx);
    let param_names = params_node
        .as_ref()
        .map(extract_param_names)
        .unwrap_or_default();

    Some(CurrentClassMember::Method(Arc::new(MethodSummary {
        name: Arc::from("<init>"),
        params: MethodParams::from_descriptor_and_names(&descriptor, &param_names),
        annotations: Vec::new(),
        access_flags: flags,
        is_synthetic: false,
        generic_signature: None,
        return_type: None,
    })))
}

fn parse_field_node(node: &SyntaxNode, type_ctx: &SourceTypeCtx) -> Vec<CurrentClassMember> {
    let flags = modifier_flags(node).unwrap_or(ACC_PUBLIC);
    let field_type = first_node_text(
        node,
        &[
            "integral_type",
            "floating_point_type",
            "boolean_type",
            "type_identifier",
            "scoped_type_identifier",
            "array_type",
            "generic_type",
        ],
    )
    .unwrap_or_else(|| "Object".to_string());
    let descriptor = Arc::<str>::from(type_ctx.to_descriptor(&field_type));

    node.children()
        .filter(|child| kind_name(child.kind()) == Some("variable_declarator"))
        .filter_map(|decl| first_token_text(&decl, &["identifier"]))
        .filter(|name| !is_java_keyword(name))
        .map(|name| {
            CurrentClassMember::Field(Arc::new(FieldSummary {
                name: Arc::from(name),
                descriptor: descriptor.clone(),
                access_flags: flags,
                annotations: Vec::new(),
                is_synthetic: false,
                generic_signature: None,
            }))
        })
        .collect()
}

fn modifier_flags(node: &SyntaxNode) -> Option<u16> {
    find_child_node(node, &["modifiers"]).map(|modifiers| {
        crate::language::java::utils::parse_java_modifiers(modifiers.text().to_string().as_str())
    })
}

fn first_node_text(node: &SyntaxNode, kinds: &[&str]) -> Option<String> {
    node.children()
        .find(|child| kind_name(child.kind()).is_some_and(|kind| kinds.contains(&kind)))
        .map(|child| child.text().to_string())
}

fn first_token_text(node: &SyntaxNode, kinds: &[&str]) -> Option<String> {
    node.children_with_tokens()
        .find_map(|element| match element {
            SyntaxElement::Token(token)
                if kind_name(token.kind()).is_some_and(|kind| kinds.contains(&kind)) =>
            {
                Some(token.text().to_string())
            }
            SyntaxElement::Node(child) => first_token_text(&child, kinds),
            _ => None,
        })
}

fn find_child_node(node: &SyntaxNode, kinds: &[&str]) -> Option<SyntaxNode> {
    node.children()
        .find(|child| kind_name(child.kind()).is_some_and(|kind| kinds.contains(&kind)))
}

fn has_spread_parameter(node: &SyntaxNode) -> bool {
    node.descendants()
        .any(|desc| kind_name(desc.kind()) == Some("spread_parameter"))
}

fn extract_param_names(params: &SyntaxNode) -> Vec<Arc<str>> {
    params
        .children()
        .filter(|child| {
            matches!(
                kind_name(child.kind()),
                Some("formal_parameter") | Some("spread_parameter")
            )
        })
        .filter_map(|param| first_token_text(&param, &["identifier"]))
        .map(Arc::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::java::make_java_parser;

    fn parse_snapshot(src: &str) -> SyntaxSnapshot {
        let mut parser = make_java_parser();
        let tree = parser.parse(src, None).unwrap();
        SyntaxSnapshot::from_tree("java", src, &tree)
    }

    fn type_ctx() -> SourceTypeCtx {
        SourceTypeCtx::new(None, Vec::new(), None)
    }

    #[test]
    fn extracts_members_from_enclosing_class_with_rowan() {
        let src = "class Test { int field; void run(int x) {} Test() {} }";
        let offset = src.find("run").unwrap();
        let members = extract_current_class_members(&parse_snapshot(src), offset, &type_ctx());

        assert_eq!(members.len(), 3);
        assert!(members.iter().any(|m| m.name().as_ref() == "field"));
        assert!(members.iter().any(|m| m.name().as_ref() == "run"));
        assert!(members.iter().any(|m| m.name().as_ref() == "<init>"));
    }

    #[test]
    fn extracts_enclosing_method_with_rowan() {
        let src = "class Test { void run() { run(); } }";
        let offset = src.rfind("run").unwrap();
        let member = extract_enclosing_member(&parse_snapshot(src), offset, &type_ctx()).unwrap();

        assert_eq!(member.name().as_ref(), "run");
    }
}
