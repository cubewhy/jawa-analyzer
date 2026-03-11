use std::ops::Range;
use std::sync::Arc;

use ropey::Rope;
use tree_sitter::Node;

use crate::index::{IndexView, NameTable};
use crate::language::java::editor_semantics::{
    JavaInvocationSite, intersects_range, render_type_for_ui, resolve_invocation,
    semantic_context_at_offset,
};
use crate::language::java::type_ctx::SourceTypeCtx;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaInlayHintKind {
    Type,
    Parameter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInlayHint {
    pub offset: usize,
    pub label: String,
    pub kind: JavaInlayHintKind,
}

pub fn collect_java_inlay_hints(
    source: &str,
    rope: &Rope,
    root: Node,
    name_table: Option<Arc<NameTable>>,
    view: &IndexView,
    byte_range: Range<usize>,
) -> Vec<JavaInlayHint> {
    let mut hints = Vec::new();
    collect_var_hints(
        source,
        rope,
        root,
        root,
        name_table.clone(),
        view,
        &byte_range,
        &mut hints,
    );
    collect_parameter_hints(
        source,
        rope,
        root,
        root,
        name_table,
        view,
        &byte_range,
        &mut hints,
    );
    hints.sort_by_key(|hint| hint.offset);
    hints
}

fn collect_var_hints(
    source: &str,
    rope: &Rope,
    root: Node,
    node: Node,
    name_table: Option<Arc<NameTable>>,
    view: &IndexView,
    byte_range: &Range<usize>,
    out: &mut Vec<JavaInlayHint>,
) {
    if !intersects_range(node, byte_range) {
        return;
    }
    if node.kind() == "local_variable_declaration"
        && let Some(type_node) = node.child_by_field_name("type")
        && type_node.utf8_text(source.as_bytes()).ok() == Some("var")
    {
        let mut walker = node.walk();
        for declarator in node.named_children(&mut walker) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            let Some(name_node) = declarator.child_by_field_name("name") else {
                continue;
            };
            if !intersects_range(name_node, byte_range) {
                continue;
            }
            let Some(ctx) = semantic_context_at_offset(
                source,
                rope,
                root,
                name_node.end_byte(),
                name_table.clone(),
                view,
            ) else {
                continue;
            };
            let Some(local) = name_node
                .utf8_text(source.as_bytes())
                .ok()
                .and_then(|name| {
                    ctx.local_variables
                        .iter()
                        .rev()
                        .find(|lv| lv.name.as_ref() == name)
                })
            else {
                continue;
            };
            if local.type_internal.erased_internal() == "var"
                || local.type_internal.erased_internal() == "unknown"
            {
                continue;
            }
            out.push(JavaInlayHint {
                offset: name_node.end_byte(),
                label: format!(": {}", render_type_for_ui(&local.type_internal, view, &ctx)),
                kind: JavaInlayHintKind::Type,
            });
        }
    }

    let mut walker = node.walk();
    for child in node.children(&mut walker) {
        collect_var_hints(
            source,
            rope,
            root,
            child,
            name_table.clone(),
            view,
            byte_range,
            out,
        );
    }
}

fn collect_parameter_hints(
    source: &str,
    rope: &Rope,
    root: Node,
    node: Node,
    name_table: Option<Arc<NameTable>>,
    view: &IndexView,
    byte_range: &Range<usize>,
    out: &mut Vec<JavaInlayHint>,
) {
    if !intersects_range(node, byte_range) {
        return;
    }

    if let Some(site) = invocation_site_for_node(source, node)
        && let Some(arguments) = invocation_arguments(node)
    {
        let arg_nodes = named_argument_nodes(arguments);
        if !arg_nodes.is_empty() {
            let ctx_offset = arguments.start_byte().saturating_add(1);
            if let Some(ctx) =
                semantic_context_at_offset(source, rope, root, ctx_offset, name_table.clone(), view)
                && let Some(type_ctx) = ctx.extension::<SourceTypeCtx>()
                && let Some(call) = resolve_invocation(&ctx, view, type_ctx, &site, None)
            {
                for (arg_index, arg_node) in arg_nodes.into_iter().enumerate() {
                    if !intersects_range(arg_node, byte_range) {
                        continue;
                    }
                    let Some(param_name) = call.parameter_name_for_argument(arg_index) else {
                        continue;
                    };
                    if should_skip_parameter_hint(param_name.as_ref(), arg_node, source) {
                        continue;
                    }
                    out.push(JavaInlayHint {
                        offset: arg_node.start_byte(),
                        label: format!("{param_name}:"),
                        kind: JavaInlayHintKind::Parameter,
                    });
                }
            }
        }
    }

    let mut walker = node.walk();
    for child in node.children(&mut walker) {
        collect_parameter_hints(
            source,
            rope,
            root,
            child,
            name_table.clone(),
            view,
            byte_range,
            out,
        );
    }
}

fn invocation_site_for_node(source: &str, node: Node) -> Option<JavaInvocationSite> {
    match node.kind() {
        "method_invocation" => {
            let method_name = node
                .child_by_field_name("name")?
                .utf8_text(source.as_bytes())
                .ok()?;
            let receiver_expr = node
                .child_by_field_name("object")
                .and_then(|receiver| receiver.utf8_text(source.as_bytes()).ok())
                .unwrap_or("this");
            let arg_texts = invocation_arguments(node)
                .map(named_argument_nodes)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|arg| arg.utf8_text(source.as_bytes()).ok().map(ToOwned::to_owned))
                .collect();
            Some(JavaInvocationSite::Method {
                receiver_expr: receiver_expr.to_string(),
                method_name: method_name.to_string(),
                arg_texts,
            })
        }
        "object_creation_expression" => {
            let call_text = node.utf8_text(source.as_bytes()).ok()?.to_string();
            let arg_texts = invocation_arguments(node)
                .map(named_argument_nodes)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|arg| arg.utf8_text(source.as_bytes()).ok().map(ToOwned::to_owned))
                .collect();
            Some(JavaInvocationSite::Constructor {
                call_text,
                arg_texts,
            })
        }
        _ => None,
    }
}

fn invocation_arguments(node: Node) -> Option<Node> {
    node.child_by_field_name("arguments")
}

fn named_argument_nodes(arguments: Node) -> Vec<Node> {
    let mut walker = arguments.walk();
    arguments.named_children(&mut walker).collect()
}

fn should_skip_parameter_hint(param_name: &str, arg_node: Node, source: &str) -> bool {
    match arg_node.kind() {
        "identifier" => arg_node
            .utf8_text(source.as_bytes())
            .ok()
            .is_some_and(|text| text == param_name),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{
        ClassMetadata, ClassOrigin, IndexScope, MethodParams, MethodSummary, ModuleId,
        WorkspaceIndex,
    };
    use crate::language::java::make_java_parser;
    use rust_asm::constants::ACC_PUBLIC;

    fn make_view() -> crate::index::IndexView {
        let idx = Box::leak(Box::new(WorkspaceIndex::new()));
        idx.add_jar_classes(
            IndexScope {
                module: ModuleId::ROOT,
            },
            vec![
                ClassMetadata {
                    package: Some(Arc::from("java/util")),
                    name: Arc::from("ArrayList"),
                    internal_name: Arc::from("java/util/ArrayList"),
                    super_name: None,
                    interfaces: vec![],
                    annotations: vec![],
                    methods: vec![
                        MethodSummary {
                            name: Arc::from("<init>"),
                            params: MethodParams::empty(),
                            annotations: vec![],
                            access_flags: ACC_PUBLIC,
                            is_synthetic: false,
                            generic_signature: None,
                            return_type: None,
                        },
                        MethodSummary {
                            name: Arc::from("<init>"),
                            params: MethodParams::from([("I", "initialCapacity")]),
                            annotations: vec![],
                            access_flags: ACC_PUBLIC,
                            is_synthetic: false,
                            generic_signature: None,
                            return_type: None,
                        },
                    ],
                    fields: vec![],
                    access_flags: ACC_PUBLIC,
                    inner_class_of: None,
                    generic_signature: None,
                    origin: ClassOrigin::Jar(Arc::from("rt.jar")),
                },
                ClassMetadata {
                    package: Some(Arc::from("java/lang")),
                    name: Arc::from("String"),
                    internal_name: Arc::from("java/lang/String"),
                    super_name: Some(Arc::from("java/lang/Object")),
                    interfaces: vec![],
                    annotations: vec![],
                    methods: vec![],
                    fields: vec![],
                    access_flags: ACC_PUBLIC,
                    inner_class_of: None,
                    generic_signature: None,
                    origin: ClassOrigin::Jar(Arc::from("rt.jar")),
                },
                ClassMetadata {
                    package: Some(Arc::from("test")),
                    name: Arc::from("T"),
                    internal_name: Arc::from("test/T"),
                    super_name: None,
                    interfaces: vec![],
                    annotations: vec![],
                    methods: vec![MethodSummary {
                        name: Arc::from("foo"),
                        params: MethodParams::from([("I", "count"), ("Z", "enabled")]),
                        annotations: vec![],
                        access_flags: ACC_PUBLIC,
                        is_synthetic: false,
                        generic_signature: None,
                        return_type: Some(Arc::from("V")),
                    }],
                    fields: vec![],
                    access_flags: ACC_PUBLIC,
                    inner_class_of: None,
                    generic_signature: None,
                    origin: ClassOrigin::SourceFile("T.java".into()),
                },
            ],
        );
        idx.view(IndexScope {
            module: ModuleId::ROOT,
        })
    }

    #[test]
    fn emits_var_and_parameter_hints_from_shared_semantics() {
        let view = make_view();
        let src = r#"
            package test;
            import java.util.ArrayList;
            class T {
                void m() {
                    var a = 1;
                    new ArrayList<>(1);
                    foo(1, true);
                }
            }
        "#;
        let rope = Rope::from_str(src);
        let mut parser = make_java_parser();
        let tree = parser.parse(src, None).expect("tree");
        let hints = collect_java_inlay_hints(
            src,
            &rope,
            tree.root_node(),
            Some(view.build_name_table()),
            &view,
            0..src.len(),
        );

        assert!(hints.iter().any(|hint| hint.label == ": int"));
        assert!(hints.iter().any(|hint| hint.label == "initialCapacity:"));
        assert!(hints.iter().any(|hint| hint.label == "count:"));
        assert!(hints.iter().any(|hint| hint.label == "enabled:"));
    }

    #[test]
    fn var_hint_uses_rendered_semantic_type() {
        let view = make_view();
        let src = r#"
            package test;
            class T {
                void m() {
                    var xs = new String[0];
                }
            }
        "#;
        let rope = Rope::from_str(src);
        let mut parser = make_java_parser();
        let tree = parser.parse(src, None).expect("tree");
        let hints = collect_java_inlay_hints(
            src,
            &rope,
            tree.root_node(),
            Some(view.build_name_table()),
            &view,
            0..src.len(),
        );

        assert!(
            hints.iter().any(|hint| {
                hint.kind == JavaInlayHintKind::Type && hint.label.contains("String")
            }),
            "expected a rendered type hint, got {:?}",
            hints
        );
    }
}
