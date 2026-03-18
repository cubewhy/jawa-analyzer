use std::sync::Arc;
use tree_sitter::Node;
use tree_sitter_utils::traversal::first_child_of_kind;

use crate::{
    index::{AnnotationValue, FieldSummary, MethodSummary},
    language::java::{
        JavaContextExtractor,
        lombok::{
            types::annotations,
            utils::{find_lombok_annotation, get_annotation_value},
        },
        members::parse_annotations_in_node,
        synthetic::{
            SyntheticDefinition, SyntheticDefinitionKind, SyntheticInput, SyntheticMemberRule,
            SyntheticMemberSet, SyntheticOrigin,
        },
        type_ctx::SourceTypeCtx,
    },
};

pub struct DelegateRule;

impl SyntheticMemberRule for DelegateRule {
    fn synthesize(
        &self,
        input: &SyntheticInput<'_>,
        out: &mut SyntheticMemberSet,
        explicit_methods: &[MethodSummary],
        explicit_fields: &[FieldSummary],
    ) {
        // Only process class-like declarations
        if !matches!(
            input.decl.kind(),
            "class_declaration" | "interface_declaration"
        ) {
            return;
        }

        // Process each field with @Delegate annotation
        for field in explicit_fields {
            if let Some(delegate_anno) =
                find_lombok_annotation(&field.annotations, annotations::DELEGATE)
            {
                // Skip static fields
                if (field.access_flags & rust_asm::constants::ACC_STATIC) != 0 {
                    continue;
                }

                generate_delegate_methods(field, delegate_anno, explicit_methods, out);
            }
        }
    }
}

/// Generate delegate methods for a field
fn generate_delegate_methods(
    field: &FieldSummary,
    annotation: &crate::index::AnnotationSummary,
    explicit_methods: &[MethodSummary],
    out: &mut SyntheticMemberSet,
) {
    // Parse types parameter (which interfaces/classes to delegate)
    let types_to_delegate = get_types_parameter(annotation);

    // Parse excludes parameter (which types to exclude)
    let types_to_exclude = get_excludes_parameter(annotation);

    // For a static analysis tool without full classpath resolution,
    // we generate marker methods that indicate delegation is happening.
    // The actual method signatures would need full type resolution.

    // If types are explicitly specified, we know what to delegate
    if !types_to_delegate.is_empty() {
        // Generate placeholder methods for explicitly specified types
        // In a full implementation, we would resolve these types and generate
        // all their public methods
        generate_delegate_markers_for_types(
            field,
            &types_to_delegate,
            &types_to_exclude,
            explicit_methods,
            out,
        );
    } else {
        // Delegate all public methods of the field's type
        // Without full type resolution, we generate a marker
        generate_delegate_marker_for_field_type(field, &types_to_exclude, explicit_methods, out);
    }
}

/// Get the types parameter from @Delegate annotation
fn get_types_parameter(annotation: &crate::index::AnnotationSummary) -> Vec<Arc<str>> {
    if let Some(value) = get_annotation_value(annotation, "types") {
        match value {
            AnnotationValue::Array(items) => items
                .iter()
                .filter_map(|item| match item {
                    AnnotationValue::Class(class_name) => Some(Arc::clone(class_name)),
                    _ => None,
                })
                .collect(),
            AnnotationValue::Class(class_name) => vec![Arc::clone(class_name)],
            _ => vec![],
        }
    } else {
        vec![]
    }
}

/// Get the excludes parameter from @Delegate annotation
fn get_excludes_parameter(annotation: &crate::index::AnnotationSummary) -> Vec<Arc<str>> {
    if let Some(value) = get_annotation_value(annotation, "excludes") {
        match value {
            AnnotationValue::Array(items) => items
                .iter()
                .filter_map(|item| match item {
                    AnnotationValue::Class(class_name) => Some(Arc::clone(class_name)),
                    _ => None,
                })
                .collect(),
            AnnotationValue::Class(class_name) => vec![Arc::clone(class_name)],
            _ => vec![],
        }
    } else {
        vec![]
    }
}

/// Generate delegate markers for explicitly specified types
fn generate_delegate_markers_for_types(
    field: &FieldSummary,
    types: &[Arc<str>],
    _excludes: &[Arc<str>],
    _explicit_methods: &[MethodSummary],
    out: &mut SyntheticMemberSet,
) {
    // For each type specified in the types parameter, we would need to:
    // 1. Resolve the type to find all its public methods
    // 2. Generate delegate methods for each
    //
    // Since this is a static analysis tool without full classpath,
    // we generate a marker definition that IDEs can use to trigger
    // more sophisticated type resolution.

    for _type_name in types {
        // Generate a marker that indicates delegation is configured
        // Real implementation would resolve the type and generate actual methods
        out.definitions.push(SyntheticDefinition {
            kind: SyntheticDefinitionKind::Method,
            name: Arc::from(format!("$delegate${}", field.name)),
            descriptor: None,
            origin: SyntheticOrigin::LombokDelegate {
                field_name: Arc::clone(&field.name),
            },
        });
    }
}

/// Generate delegate marker for field's type
fn generate_delegate_marker_for_field_type(
    field: &FieldSummary,
    _excludes: &[Arc<str>],
    _explicit_methods: &[MethodSummary],
    out: &mut SyntheticMemberSet,
) {
    // Without full type resolution, we generate a marker
    // that indicates this field has delegation configured
    out.definitions.push(SyntheticDefinition {
        kind: SyntheticDefinitionKind::Method,
        name: Arc::from(format!("$delegate${}", field.name)),
        descriptor: None,
        origin: SyntheticOrigin::LombokDelegate {
            field_name: Arc::clone(&field.name),
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::AnnotationSummary;
    use crate::language::java::{make_java_parser, scope::extract_imports, scope::extract_package};
    use rust_asm::constants::{ACC_PRIVATE, ACC_STATIC};
    use rustc_hash::FxHashMap;

    fn parse_env(src: &str) -> (JavaContextExtractor, tree_sitter::Tree, SourceTypeCtx) {
        let ctx = JavaContextExtractor::for_indexing(src, None);
        let mut parser = make_java_parser();
        let tree = parser.parse(src, None).expect("parse");
        let root = tree.root_node();
        let type_ctx = SourceTypeCtx::new(
            extract_package(&ctx, root),
            extract_imports(&ctx, root),
            None,
        );
        (ctx, tree, type_ctx)
    }

    fn first_decl(root: Node) -> Node {
        root.named_children(&mut root.walk())
            .find(|node| matches!(node.kind(), "class_declaration" | "interface_declaration"))
            .expect("type declaration")
    }

    #[test]
    fn test_delegate_generates_marker() {
        let src = r#"
            import lombok.experimental.Delegate;
            import java.util.List;
            
            class MyList {
                @Delegate
                private List<String> items;
            }
        "#;
        let (ctx, tree, type_ctx) = parse_env(src);
        let decl = first_decl(tree.root_node());

        let synthetic = crate::language::java::synthetic::synthesize_for_type(
            &ctx,
            decl,
            Some("MyList"),
            &type_ctx,
            &[],
            &[FieldSummary {
                name: Arc::from("items"),
                descriptor: Arc::from("Ljava/util/List;"),
                access_flags: ACC_PRIVATE,
                annotations: vec![AnnotationSummary {
                    internal_name: Arc::from("lombok/experimental/Delegate"),
                    runtime_visible: true,
                    elements: FxHashMap::default(),
                }],
                is_synthetic: false,
                generic_signature: None,
            }],
        );

        // Should generate a delegate marker
        assert!(
            !synthetic.definitions.is_empty(),
            "Should generate delegate definitions"
        );

        let has_delegate_marker = synthetic
            .definitions
            .iter()
            .any(|d| matches!(d.origin, SyntheticOrigin::LombokDelegate { .. }));
        assert!(has_delegate_marker, "Should have delegate marker");
    }

    #[test]
    fn test_delegate_not_generated_for_static_field() {
        let src = r#"
            import lombok.experimental.Delegate;
            import java.util.List;
            
            class MyList {
                @Delegate
                private static List<String> SHARED_LIST;
            }
        "#;
        let (ctx, tree, type_ctx) = parse_env(src);
        let decl = first_decl(tree.root_node());

        let synthetic = crate::language::java::synthetic::synthesize_for_type(
            &ctx,
            decl,
            Some("MyList"),
            &type_ctx,
            &[],
            &[FieldSummary {
                name: Arc::from("SHARED_LIST"),
                descriptor: Arc::from("Ljava/util/List;"),
                access_flags: ACC_PRIVATE | ACC_STATIC,
                annotations: vec![AnnotationSummary {
                    internal_name: Arc::from("lombok/experimental/Delegate"),
                    runtime_visible: true,
                    elements: FxHashMap::default(),
                }],
                is_synthetic: false,
                generic_signature: None,
            }],
        );

        // Should not generate delegate for static field
        let has_delegate_marker = synthetic
            .definitions
            .iter()
            .any(|d| matches!(d.origin, SyntheticOrigin::LombokDelegate { .. }));
        assert!(
            !has_delegate_marker,
            "Should not generate delegate for static field"
        );
    }

    #[test]
    fn test_delegate_with_types_parameter() {
        let src = r#"
            import lombok.experimental.Delegate;
            import java.util.Collection;
            
            class MyCollection {
                @Delegate(types = Collection.class)
                private java.util.ArrayList<String> items;
            }
        "#;
        let (ctx, tree, type_ctx) = parse_env(src);
        let decl = first_decl(tree.root_node());

        let mut elements = FxHashMap::default();
        elements.insert(
            Arc::from("types"),
            AnnotationValue::Class(Arc::from("Ljava/util/Collection;")),
        );

        let synthetic = crate::language::java::synthetic::synthesize_for_type(
            &ctx,
            decl,
            Some("MyCollection"),
            &type_ctx,
            &[],
            &[FieldSummary {
                name: Arc::from("items"),
                descriptor: Arc::from("Ljava/util/ArrayList;"),
                access_flags: ACC_PRIVATE,
                annotations: vec![AnnotationSummary {
                    internal_name: Arc::from("lombok/experimental/Delegate"),
                    runtime_visible: true,
                    elements,
                }],
                is_synthetic: false,
                generic_signature: None,
            }],
        );

        // Should generate delegate marker
        let has_delegate_marker = synthetic
            .definitions
            .iter()
            .any(|d| matches!(d.origin, SyntheticOrigin::LombokDelegate { .. }));
        assert!(
            has_delegate_marker,
            "Should generate delegate marker with types parameter"
        );
    }

    #[test]
    fn test_delegate_with_excludes_parameter() {
        let src = r#"
            import lombok.experimental.Delegate;
            import java.util.List;
            
            class MyList {
                @Delegate(excludes = java.util.Collection.class)
                private List<String> items;
            }
        "#;
        let (ctx, tree, type_ctx) = parse_env(src);
        let decl = first_decl(tree.root_node());

        let mut elements = FxHashMap::default();
        elements.insert(
            Arc::from("excludes"),
            AnnotationValue::Class(Arc::from("Ljava/util/Collection;")),
        );

        let synthetic = crate::language::java::synthetic::synthesize_for_type(
            &ctx,
            decl,
            Some("MyList"),
            &type_ctx,
            &[],
            &[FieldSummary {
                name: Arc::from("items"),
                descriptor: Arc::from("Ljava/util/List;"),
                access_flags: ACC_PRIVATE,
                annotations: vec![AnnotationSummary {
                    internal_name: Arc::from("lombok/experimental/Delegate"),
                    runtime_visible: true,
                    elements,
                }],
                is_synthetic: false,
                generic_signature: None,
            }],
        );

        // Should generate delegate marker
        let has_delegate_marker = synthetic
            .definitions
            .iter()
            .any(|d| matches!(d.origin, SyntheticOrigin::LombokDelegate { .. }));
        assert!(
            has_delegate_marker,
            "Should generate delegate marker with excludes parameter"
        );
    }
}
