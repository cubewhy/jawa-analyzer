use std::sync::Arc;

use crate::completion::parser::parse_chain_from_expr;
use crate::index::IndexView;
use crate::language::java::type_ctx::SourceTypeCtx;
use crate::semantic::LocalVar;
use crate::semantic::types::type_name::TypeName;
use crate::semantic::types::{
    ChainSegment, TypeResolver, parse_single_type_to_internal, singleton_descriptor_to_type,
};
use tree_sitter::Node;

pub(crate) fn resolve_expression_type(
    expr: &str,
    locals: &[LocalVar],
    enclosing_internal: Option<&Arc<str>>,
    resolver: &TypeResolver,
    type_ctx: &SourceTypeCtx,
    view: &IndexView,
) -> Option<TypeName> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }

    if let Some(ty) =
        resolve_expression_type_ast(expr, locals, enclosing_internal, resolver, type_ctx, view)
    {
        return Some(ty);
    }

    if looks_like_array_access(expr) {
        return resolve_array_access_type(
            expr,
            locals,
            enclosing_internal,
            resolver,
            type_ctx,
            view,
        );
    }

    let chain = parse_chain_from_expr(expr);
    if chain.is_empty() {
        return resolver.resolve(expr, locals, enclosing_internal);
    }
    evaluate_chain(&chain, locals, enclosing_internal, resolver, type_ctx, view)
}

fn resolve_expression_type_ast(
    expr: &str,
    locals: &[LocalVar],
    enclosing_internal: Option<&Arc<str>>,
    resolver: &TypeResolver,
    type_ctx: &SourceTypeCtx,
    view: &IndexView,
) -> Option<TypeName> {
    let wrapped = format!("class __ExprTyping {{ Object __e() {{ return {expr}; }} }}");
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(&wrapped, None)?;
    let root = tree.root_node();
    let return_stmt = find_first_node_of_kind(root, "return_statement")?;
    let expr_node = return_stmt
        .child_by_field_name("value")
        .or_else(|| first_named_child(return_stmt))?;

    resolve_ast_node_type(
        expr_node,
        wrapped.as_bytes(),
        locals,
        enclosing_internal,
        resolver,
        type_ctx,
        view,
    )
}

fn first_named_child<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).next()
}

fn find_first_node_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_first_node_of_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn resolve_ast_node_type(
    node: Node,
    bytes: &[u8],
    locals: &[LocalVar],
    enclosing_internal: Option<&Arc<str>>,
    resolver: &TypeResolver,
    type_ctx: &SourceTypeCtx,
    view: &IndexView,
) -> Option<TypeName> {
    match node.kind() {
        "parenthesized_expression" => {
            let inner = node
                .child_by_field_name("expression")
                .or_else(|| first_named_child(node))?;
            resolve_ast_node_type(
                inner,
                bytes,
                locals,
                enclosing_internal,
                resolver,
                type_ctx,
                view,
            )
        }
        "identifier" => {
            let text = node.utf8_text(bytes).ok()?;
            resolver.resolve(text.trim(), locals, enclosing_internal)
        }
        "decimal_integer_literal"
        | "hex_integer_literal"
        | "octal_integer_literal"
        | "binary_integer_literal" => {
            let text = node.utf8_text(bytes).ok()?;
            resolve_integer_literal_type(text)
        }
        "decimal_floating_point_literal" | "hex_floating_point_literal" => {
            let text = node.utf8_text(bytes).ok()?;
            resolve_floating_point_literal_type(text)
        }
        "string_literal" | "text_block" => Some(TypeName::new("java/lang/String")),
        "binary_expression" => resolve_binary_expression_type(
            node,
            bytes,
            locals,
            enclosing_internal,
            resolver,
            type_ctx,
            view,
        ),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_binary_expression_type(
    node: Node,
    bytes: &[u8],
    locals: &[LocalVar],
    enclosing_internal: Option<&Arc<str>>,
    resolver: &TypeResolver,
    type_ctx: &SourceTypeCtx,
    view: &IndexView,
) -> Option<TypeName> {
    let op = binary_operator(node, bytes)?;
    let left = node.child_by_field_name("left")?;
    let right = node.child_by_field_name("right")?;
    let left_type = resolve_ast_node_type(
        left,
        bytes,
        locals,
        enclosing_internal,
        resolver,
        type_ctx,
        view,
    )?;
    let right_type = resolve_ast_node_type(
        right,
        bytes,
        locals,
        enclosing_internal,
        resolver,
        type_ctx,
        view,
    )?;

    match op.as_str() {
        "+" => {
            if is_string_type(&left_type) || is_string_type(&right_type) {
                Some(TypeName::new("java/lang/String"))
            } else {
                numeric_binary_result_type(&left_type, &right_type)
            }
        }
        "-" | "*" | "/" | "%" => numeric_binary_result_type(&left_type, &right_type),
        _ => None,
    }
}

fn binary_operator<'a>(node: Node<'a>, bytes: &[u8]) -> Option<String> {
    if let Some(op) = node.child_by_field_name("operator") {
        return op.utf8_text(bytes).ok().map(|s| s.trim().to_string());
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "+" | "-" | "*" | "/" | "%" | "<<" | ">>" | ">>>"
        ) {
            return Some(child.kind().to_string());
        }
    }
    None
}

fn resolve_integer_literal_type(text: &str) -> Option<TypeName> {
    let sanitized: String = text.trim().chars().filter(|c| *c != '_').collect();
    if sanitized.is_empty() {
        return None;
    }
    if let Some(without_suffix) = sanitized
        .strip_suffix('l')
        .or_else(|| sanitized.strip_suffix('L'))
        && !without_suffix.is_empty()
    {
        return Some(TypeName::new("long"));
    }
    Some(TypeName::new("int"))
}

fn resolve_floating_point_literal_type(text: &str) -> Option<TypeName> {
    let sanitized: String = text.trim().chars().filter(|c| *c != '_').collect();
    if sanitized.is_empty() {
        return None;
    }
    if sanitized.ends_with('f') || sanitized.ends_with('F') {
        return Some(TypeName::new("float"));
    }
    Some(TypeName::new("double"))
}

fn numeric_binary_result_type(left: &TypeName, right: &TypeName) -> Option<TypeName> {
    if !is_numeric_primitive(left) || !is_numeric_primitive(right) {
        return None;
    }

    if is_type(left, "double") || is_type(right, "double") {
        return Some(TypeName::new("double"));
    }
    if is_type(left, "float") || is_type(right, "float") {
        return Some(TypeName::new("float"));
    }
    if is_type(left, "long") || is_type(right, "long") {
        return Some(TypeName::new("long"));
    }
    Some(TypeName::new("int"))
}

fn is_string_type(ty: &TypeName) -> bool {
    matches!(ty.erased_internal(), "java/lang/String")
}

fn is_type(ty: &TypeName, name: &str) -> bool {
    ty.erased_internal() == name
}

fn is_numeric_primitive(ty: &TypeName) -> bool {
    matches!(
        ty.erased_internal(),
        "byte" | "short" | "char" | "int" | "long" | "float" | "double"
    )
}

pub(crate) fn resolve_var_init_expr(
    expr: &str,
    locals: &[LocalVar],
    enclosing_internal: Option<&Arc<str>>,
    resolver: &TypeResolver,
    type_ctx: &SourceTypeCtx,
    view: &IndexView,
) -> Option<TypeName> {
    let expr = expr.trim();
    if let Some(rest) = expr.strip_prefix("new ") {
        let mut boundary_idx = rest.find(['(', '[', '{']).unwrap_or(rest.len());
        if let Some(gen_start) = rest.find('<')
            && gen_start < boundary_idx
        {
            if let Some(gen_end) = find_matching_angle(rest, gen_start) {
                boundary_idx = gen_end + 1;
            } else {
                return None;
            }
        }
        let type_name = rest[..boundary_idx].trim();
        let resolved_base: TypeName = match type_name {
            "byte" | "short" | "int" | "long" | "float" | "double" | "boolean" | "char" => {
                TypeName::new(type_name)
            }
            _ => resolve_constructor_type_name(type_name, enclosing_internal, type_ctx, view)?,
        };
        let after_type = rest[boundary_idx..].trim_start();
        if after_type.starts_with('[') || after_type.starts_with('{') {
            let brace_idx = after_type.find('{').unwrap_or(after_type.len());
            let dimensions = after_type[..brace_idx].matches('[').count();
            let mut array_ty = resolved_base;
            for _ in 0..dimensions {
                array_ty = array_ty.wrap_array();
            }
            return Some(array_ty);
        }
        return Some(resolved_base);
    }

    resolve_expression_type(expr, locals, enclosing_internal, resolver, type_ctx, view)
}

fn resolve_constructor_type_name(
    type_name: &str,
    enclosing_internal: Option<&Arc<str>>,
    type_ctx: &SourceTypeCtx,
    view: &IndexView,
) -> Option<TypeName> {
    if let Some(strict) = type_ctx.resolve_type_name_strict(type_name) {
        return Some(strict);
    }
    let resolve_head = |head: &str| {
        if let Some(strict) = type_ctx.resolve_type_name_strict(head) {
            return Some(Arc::from(strict.erased_internal()));
        }
        if let Some(enclosing_internal) = enclosing_internal {
            return view
                .resolve_scoped_inner_class(enclosing_internal, head)
                .map(|c| c.internal_name.clone());
        }
        None
    };
    view.resolve_qualified_type_path(type_name, &resolve_head)
        .map(|c| TypeName::new(c.internal_name.as_ref()))
}

pub(crate) fn looks_like_array_access(expr: &str) -> bool {
    expr.contains('[') && expr.trim_end().ends_with(']')
}

fn find_matching_angle(s: &str, start: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in s.char_indices().skip(start) {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn resolve_array_access_type(
    expr: &str,
    locals: &[LocalVar],
    enclosing_internal: Option<&Arc<str>>,
    resolver: &TypeResolver,
    type_ctx: &SourceTypeCtx,
    view: &IndexView,
) -> Option<TypeName> {
    let bracket = expr.rfind('[')?;
    if !expr.trim_end().ends_with(']') {
        return None;
    }
    let array_expr = expr[..bracket].trim();
    if array_expr.is_empty() {
        return None;
    }
    let array_type = resolve_expression_type(
        array_expr,
        locals,
        enclosing_internal,
        resolver,
        type_ctx,
        view,
    )?;
    array_type.element_type()
}

pub(crate) fn evaluate_chain(
    chain: &[ChainSegment],
    locals: &[LocalVar],
    enclosing_internal: Option<&Arc<str>>,
    resolver: &TypeResolver,
    type_ctx: &SourceTypeCtx,
    view: &IndexView,
) -> Option<TypeName> {
    let mut current: Option<TypeName> = None;
    let resolve_qualifier = |q: &str| type_ctx.resolve_type_name_strict(q);
    for (i, seg) in chain.iter().enumerate() {
        let bracket_idx = seg.name.find('[');
        let base_name = if let Some(idx) = bracket_idx {
            &seg.name[..idx]
        } else {
            &seg.name
        };
        let dimensions = seg.name.matches('[').count();

        if i == 0 {
            if seg.arg_count.is_some() {
                let recv_internal = enclosing_internal?;
                let arg_types: Vec<TypeName> = seg
                    .arg_texts
                    .iter()
                    .filter_map(|t| resolver.resolve(t.trim(), locals, enclosing_internal))
                    .collect();
                let arg_types_ref: &[TypeName] = if arg_types.len() == seg.arg_texts.len() {
                    &arg_types
                } else {
                    &[]
                };
                current = resolver.resolve_method_return_with_callsite_and_qualifier_resolver(
                    recv_internal.as_ref(),
                    base_name,
                    seg.arg_count.unwrap_or(-1),
                    arg_types_ref,
                    &seg.arg_texts,
                    locals,
                    enclosing_internal,
                    Some(&resolve_qualifier),
                );
            } else {
                current = resolver.resolve(base_name, locals, enclosing_internal);
                if current.is_none() {
                    if let Some(enclosing) = enclosing_internal {
                        let enclosing_simple = enclosing
                            .rsplit('/')
                            .next()
                            .unwrap_or(enclosing)
                            .rsplit('$')
                            .next()
                            .unwrap_or(enclosing);

                        if base_name == enclosing_simple {
                            current = Some(TypeName::new(enclosing.as_ref()));
                        }
                    }

                    if current.is_none() {
                        current = type_ctx.resolve_type_name_strict(base_name);
                    }
                }
            }
        } else {
            let recv = current.as_ref()?;
            if base_name.is_empty() {
                current = Some(recv.clone());
            } else {
                let recv_full: TypeName = if recv.contains_slash() {
                    recv.clone()
                } else {
                    let mut canonical =
                        type_ctx.resolve_type_name_strict(recv.erased_internal())?;
                    if !recv.args.is_empty() {
                        canonical.args = recv.args.clone();
                    }
                    canonical.array_dims = recv.array_dims;
                    canonical
                };

                if seg.arg_count.is_some() {
                    let arg_types: Vec<TypeName> = seg
                        .arg_texts
                        .iter()
                        .filter_map(|t| resolver.resolve(t.trim(), locals, enclosing_internal))
                        .collect();
                    let arg_types_ref: &[TypeName] = if arg_types.len() == seg.arg_texts.len() {
                        &arg_types
                    } else {
                        &[]
                    };
                    let receiver_internal = recv_full.to_internal_with_generics();
                    current = resolver.resolve_method_return_with_callsite_and_qualifier_resolver(
                        &receiver_internal,
                        base_name,
                        seg.arg_count.unwrap_or(-1),
                        arg_types_ref,
                        &seg.arg_texts,
                        locals,
                        enclosing_internal,
                        Some(&resolve_qualifier),
                    );
                } else {
                    let (methods, fields) =
                        view.collect_inherited_members(recv_full.erased_internal());

                    if let Some(f) = fields.iter().find(|f| f.name.as_ref() == base_name) {
                        if let Some(ty) = singleton_descriptor_to_type(&f.descriptor) {
                            current = Some(TypeName::new(ty));
                        } else {
                            current = parse_single_type_to_internal(&f.descriptor);
                        }
                    } else if methods.iter().any(|m| m.name.as_ref() == base_name) {
                        current = None;
                    } else {
                        current = None;
                    }
                }
            }
        }

        if dimensions > 0
            && let Some(mut ty) = current.take()
        {
            let mut success = true;
            for _ in 0..dimensions {
                if let Some(el) = ty.element_type() {
                    ty = el;
                } else {
                    success = false;
                    break;
                }
            }
            if success {
                current = Some(ty);
            }
        }
    }
    current
}
