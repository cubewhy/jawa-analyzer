use std::sync::Arc;

use crate::language::java::type_ctx::SourceTypeCtx;
use crate::language::java::utils::java_type_to_internal;
use crate::semantic::{LocalVar, types::type_name::TypeName};
use crate::syntax::{SyntaxNode, SyntaxSnapshot, TextSize, kind_name};

pub fn extract_visible_locals(
    syntax: &SyntaxSnapshot,
    offset: usize,
    type_ctx: Option<&SourceTypeCtx>,
) -> Vec<LocalVar> {
    let Some(scope) = enclosing_method_like(syntax, offset) else {
        return Vec::new();
    };

    let mut locals = extract_params(&scope, type_ctx);
    locals.extend(extract_local_declarations(&scope, offset, type_ctx));
    locals
}

fn enclosing_method_like(syntax: &SyntaxSnapshot, offset: usize) -> Option<SyntaxNode> {
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
                Some("method_declaration")
                    | Some("constructor_declaration")
                    | Some("compact_constructor_declaration")
            )
        })
}

fn extract_params(scope: &SyntaxNode, type_ctx: Option<&SourceTypeCtx>) -> Vec<LocalVar> {
    let params_node = if kind_name(scope.kind()) == Some("compact_constructor_declaration") {
        scope
            .parent()
            .and_then(|body| body.parent())
            .and_then(|record| find_child_node(&record, &["formal_parameters"]))
    } else {
        find_child_node(scope, &["formal_parameters"])
    };

    params_node
        .into_iter()
        .flat_map(|params| params.children())
        .filter(|child| {
            matches!(
                kind_name(child.kind()),
                Some("formal_parameter") | Some("spread_parameter")
            )
        })
        .filter_map(|param| {
            let name = first_token_text(&param, &["identifier"])?;
            let raw_type = param_type_text(&param)?;
            Some(LocalVar {
                name: Arc::from(name),
                type_internal: resolve_declared_source_type(&raw_type, type_ctx),
                init_expr: None,
            })
        })
        .collect()
}

fn extract_local_declarations(
    scope: &SyntaxNode,
    offset: usize,
    type_ctx: Option<&SourceTypeCtx>,
) -> Vec<LocalVar> {
    scope
        .descendants()
        .filter(|node| kind_name(node.kind()) == Some("local_variable_declaration"))
        .filter(|node| usize::from(node.text_range().start()) < offset)
        .filter(|node| declaration_visible_at_offset(node, offset))
        .flat_map(|decl| locals_from_declaration(&decl, type_ctx))
        .collect()
}

fn declaration_visible_at_offset(decl: &SyntaxNode, offset: usize) -> bool {
    decl.ancestors()
        .find(|ancestor| {
            matches!(
                kind_name(ancestor.kind()),
                Some(
                    "block"
                        | "switch_block_statement_group"
                        | "method_declaration"
                        | "constructor_declaration"
                        | "compact_constructor_declaration"
                )
            )
        })
        .is_none_or(|scope| {
            let range = scope.text_range();
            let start = usize::from(range.start());
            let end = usize::from(range.end());
            start <= offset && offset <= end
        })
}

fn locals_from_declaration(decl: &SyntaxNode, type_ctx: Option<&SourceTypeCtx>) -> Vec<LocalVar> {
    let raw_type = declaration_type_text(decl).unwrap_or_else(|| "Object".to_string());

    decl.children()
        .filter(|child| kind_name(child.kind()) == Some("variable_declarator"))
        .filter_map(|declarator| {
            let name = first_token_text(&declarator, &["identifier"])?;
            let init_expr = if raw_type == "var" {
                initializer_text(&declarator)
            } else {
                None
            };

            Some(LocalVar {
                name: Arc::from(name),
                type_internal: if raw_type == "var" {
                    TypeName::new("var")
                } else {
                    resolve_declared_source_type(&raw_type, type_ctx)
                },
                init_expr,
            })
        })
        .collect()
}

fn declaration_type_text(decl: &SyntaxNode) -> Option<String> {
    decl.children()
        .find(|child| {
            kind_name(child.kind()).is_some_and(|kind| {
                matches!(
                    kind,
                    "integral_type"
                        | "floating_point_type"
                        | "boolean_type"
                        | "void_type"
                        | "identifier"
                        | "type_identifier"
                        | "scoped_type_identifier"
                        | "array_type"
                        | "generic_type"
                )
            })
        })
        .map(|node| node.text().to_string())
        .or_else(|| {
            let first_decl = decl
                .children()
                .find(|child| kind_name(child.kind()) == Some("variable_declarator"))?;
            let prefix = decl.text().to_string();
            let name_start = usize::from(first_decl.text_range().start())
                .saturating_sub(usize::from(decl.text_range().start()));
            let raw = prefix[..name_start].trim();
            (!raw.is_empty()).then_some(raw.to_string())
        })
}

fn param_type_text(param: &SyntaxNode) -> Option<String> {
    param
        .children()
        .find(|child| {
            kind_name(child.kind()).is_some_and(|kind| {
                matches!(
                    kind,
                    "integral_type"
                        | "floating_point_type"
                        | "boolean_type"
                        | "void_type"
                        | "identifier"
                        | "type_identifier"
                        | "scoped_type_identifier"
                        | "array_type"
                        | "generic_type"
                )
            })
        })
        .map(|node| node.text().to_string())
        .or_else(|| Some("unknown".to_string()))
}

fn initializer_text(declarator: &SyntaxNode) -> Option<String> {
    let eq_pos = declarator
        .children_with_tokens()
        .position(|child| child.into_token().is_some_and(|token| token.text() == "="))?;
    let text = declarator
        .children_with_tokens()
        .skip(eq_pos + 1)
        .map(|el| el.to_string())
        .collect::<String>()
        .trim()
        .to_string();
    (!text.is_empty()).then_some(text)
}

fn resolve_declared_source_type(raw_ty: &str, type_ctx: Option<&SourceTypeCtx>) -> TypeName {
    if let Some(type_ctx) = type_ctx
        && let Some(resolved) = type_ctx.resolve_type_name_relaxed(raw_ty.trim())
    {
        if raw_ty.contains('<') && resolved.ty.args.is_empty() {
            return TypeName::new(java_type_to_internal(raw_ty).as_str());
        }
        return resolved.ty;
    }
    TypeName::new(java_type_to_internal(raw_ty).as_str())
}

fn find_child_node(node: &SyntaxNode, kinds: &[&str]) -> Option<SyntaxNode> {
    node.children()
        .find(|child| kind_name(child.kind()).is_some_and(|kind| kinds.contains(&kind)))
}

fn first_token_text(node: &SyntaxNode, kinds: &[&str]) -> Option<String> {
    node.children_with_tokens()
        .find_map(|element| match element {
            crate::syntax::SyntaxElement::Token(token)
                if kind_name(token.kind()).is_some_and(|kind| kinds.contains(&kind)) =>
            {
                Some(token.text().to_string())
            }
            crate::syntax::SyntaxElement::Node(child) => first_token_text(&child, kinds),
            _ => None,
        })
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

    #[test]
    fn extracts_params_and_prior_locals() {
        let src = "class T { void run(String name) { int count = 1; cou } }";
        let offset = src.find("cou").unwrap() + 3;
        let locals = extract_visible_locals(&parse_snapshot(src), offset, None);

        assert!(locals.iter().any(|local| local.name.as_ref() == "name"));
        assert!(locals.iter().any(|local| local.name.as_ref() == "count"));
    }

    #[test]
    fn keeps_var_initializer_text() {
        let src = "class T { void run() { var name = compute(); nam } }";
        let offset = src.find("nam }").unwrap() + 3;
        let locals = extract_visible_locals(&parse_snapshot(src), offset, None);
        let name = locals
            .iter()
            .find(|local| local.name.as_ref() == "name")
            .unwrap();

        assert_eq!(name.type_internal.erased_internal(), "var");
        assert_eq!(name.init_expr.as_deref(), Some("compute()"));
    }
}
