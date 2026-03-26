use std::sync::Arc;
use tree_sitter::Node;
use tree_sitter_utils::traversal::first_child_of_kind;

use crate::{
    index::{
        AnnotationSummary, ClassMetadata, FieldSummary, MethodParam, MethodParams, MethodSummary,
    },
    language::java::{
        JavaContextExtractor,
        lombok::{
            types::{AccessLevel, LombokBuilderMethod, annotations},
            utils::{find_lombok_annotation, get_bool_param, get_string_param, parse_access_level},
        },
        members::parse_annotations_in_node,
        synthetic::{
            SyntheticDefinition, SyntheticDefinitionKind, SyntheticInput, SyntheticMemberRule,
            SyntheticMemberSet, SyntheticOrigin,
        },
        type_ctx::SourceTypeCtx,
    },
};

pub struct BuilderRule;

impl SyntheticMemberRule for BuilderRule {
    fn synthesize(
        &self,
        input: &SyntheticInput<'_>,
        out: &mut SyntheticMemberSet,
        explicit_methods: &[MethodSummary],
        explicit_fields: &[FieldSummary],
    ) {
        // Only process class declarations
        if input.decl.kind() != "class_declaration" {
            return;
        }

        // Check for class-level @Builder annotation
        let class_annotations = extract_class_annotations(input.ctx, input.decl, input.type_ctx);
        let builder_annotation = find_lombok_annotation(&class_annotations, annotations::BUILDER);

        if let Some(builder_anno) = builder_annotation {
            generate_builder_for_class(input, builder_anno, explicit_fields, explicit_methods, out);
        }
    }
}

/// Extract class-level annotations
fn extract_class_annotations(
    ctx: &JavaContextExtractor,
    decl: Node,
    type_ctx: &SourceTypeCtx,
) -> Vec<AnnotationSummary> {
    first_child_of_kind(decl, "modifiers")
        .map(|modifiers| parse_annotations_in_node(ctx, modifiers, type_ctx))
        .unwrap_or_default()
}

/// Generate builder pattern for a class with @Builder annotation
fn generate_builder_for_class(
    input: &SyntheticInput<'_>,
    builder_anno: &AnnotationSummary,
    explicit_fields: &[FieldSummary],
    explicit_methods: &[MethodSummary],
    out: &mut SyntheticMemberSet,
) {
    use rust_asm::constants::ACC_STATIC;

    // Get the class name
    let class_name = get_class_name(input.ctx, input.decl);
    if class_name.is_empty() {
        return;
    }

    // Get builder configuration
    let builder_method_name =
        get_string_param(builder_anno, "builderMethodName").unwrap_or("builder");
    let build_method_name = get_string_param(builder_anno, "buildMethodName").unwrap_or("build");
    let builder_class_name_str = get_string_param(builder_anno, "builderClassName")
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}Builder", class_name));
    let builder_class_name = builder_class_name_str.as_str();

    let access_level = parse_access_level(builder_anno);
    let to_builder = get_bool_param(builder_anno, "toBuilder", false);

    // Filter fields that should be included in the builder
    let builder_fields: Vec<&FieldSummary> = explicit_fields
        .iter()
        .filter(|f| {
            // Skip static fields
            (f.access_flags & ACC_STATIC) == 0
        })
        .collect();

    // Generate the static builder() method on the original class
    if !method_exists(explicit_methods, builder_method_name, "()") {
        generate_builder_method(builder_method_name, builder_class_name, access_level, out);
    }

    // Generate toBuilder() instance method if requested
    if to_builder && !method_exists(explicit_methods, "toBuilder", "()") {
        generate_to_builder_method(builder_class_name, out);
    }

    // Generate the Builder nested class
    generate_builder_class(
        input,
        builder_class_name,
        &class_name,
        build_method_name,
        &builder_fields,
        access_level,
        out,
    );
}

/// Generate the static builder() method
fn generate_builder_method(
    method_name: &str,
    builder_class_name: &str,
    access_level: AccessLevel,
    out: &mut SyntheticMemberSet,
) {
    use rust_asm::constants::ACC_STATIC;

    let access_flags = access_level.to_access_flags() | ACC_STATIC;
    let descriptor = format!("()L{};", builder_class_name);

    out.methods.push(MethodSummary {
        name: Arc::from(method_name),
        params: MethodParams::empty(),
        annotations: vec![],
        access_flags,
        is_synthetic: false,
        generic_signature: None,
        return_type: Some(Arc::from(format!("L{};", builder_class_name))),
    });

    out.definitions.push(SyntheticDefinition {
        kind: SyntheticDefinitionKind::Method,
        name: Arc::from(method_name),
        descriptor: Some(Arc::from(descriptor)),
        origin: SyntheticOrigin::LombokBuilder {
            builder_method: LombokBuilderMethod::Builder,
        },
    });
}

/// Generate the toBuilder() instance method
fn generate_to_builder_method(builder_class_name: &str, out: &mut SyntheticMemberSet) {
    use rust_asm::constants::ACC_PUBLIC;

    let descriptor = format!("()L{};", builder_class_name);

    out.methods.push(MethodSummary {
        name: Arc::from("toBuilder"),
        params: MethodParams::empty(),
        annotations: vec![],
        access_flags: ACC_PUBLIC,
        is_synthetic: false,
        generic_signature: None,
        return_type: Some(Arc::from(format!("L{};", builder_class_name))),
    });

    out.definitions.push(SyntheticDefinition {
        kind: SyntheticDefinitionKind::Method,
        name: Arc::from("toBuilder"),
        descriptor: Some(Arc::from(descriptor)),
        origin: SyntheticOrigin::LombokBuilder {
            builder_method: LombokBuilderMethod::Builder,
        },
    });
}

/// Generate the Builder nested class with all its methods
fn generate_builder_class(
    input: &SyntheticInput<'_>,
    builder_class_name: &str,
    owner_class_name: &str,
    build_method_name: &str,
    builder_fields: &[&FieldSummary],
    access_level: AccessLevel,
    out: &mut SyntheticMemberSet,
) {
    use rust_asm::constants::{ACC_PUBLIC, ACC_STATIC};

    // Get the owner's internal name
    let owner_internal = input.owner_internal.unwrap_or(owner_class_name);
    let builder_internal_name = format!("{}${}", owner_internal, builder_class_name);

    // Create the Builder class metadata
    let mut builder_class = ClassMetadata {
        name: Arc::from(builder_class_name),
        internal_name: Arc::from(builder_internal_name.as_str()),
        package: Some(Arc::from("")), // Will be inherited from owner
        access_flags: access_level.to_access_flags() | ACC_STATIC,
        super_name: Some(Arc::from("java/lang/Object")),
        interfaces: vec![],
        fields: vec![],
        methods: vec![],
        inner_class_of: Some(Arc::from(owner_internal)),
        generic_signature: None,
        annotations: vec![],
        origin: crate::index::ClassOrigin::Unknown,
    };

    // Add fields to the builder class (one for each buildable field)
    for field in builder_fields {
        builder_class.fields.push(FieldSummary {
            name: Arc::clone(&field.name),
            descriptor: Arc::clone(&field.descriptor),
            access_flags: 0, // package-private
            annotations: vec![],
            is_synthetic: false,
            generic_signature: None,
        });
    }

    // Add setter methods to the builder class
    for field in builder_fields {
        builder_class.methods.push(MethodSummary {
            name: Arc::clone(&field.name),
            params: MethodParams {
                items: vec![MethodParam {
                    descriptor: Arc::clone(&field.descriptor),
                    name: Arc::clone(&field.name),
                    annotations: vec![],
                }],
            },
            annotations: vec![],
            access_flags: ACC_PUBLIC,
            is_synthetic: false,
            generic_signature: None,
            return_type: Some(Arc::from(format!("L{};", builder_class_name))),
        });
    }

    // Add build() method
    builder_class.methods.push(MethodSummary {
        name: Arc::from(build_method_name),
        params: MethodParams::empty(),
        annotations: vec![],
        access_flags: ACC_PUBLIC,
        is_synthetic: false,
        generic_signature: None,
        return_type: Some(Arc::from(format!("L{};", owner_internal))),
    });

    // Add toString() method to builder
    builder_class.methods.push(MethodSummary {
        name: Arc::from("toString"),
        params: MethodParams::empty(),
        annotations: vec![],
        access_flags: ACC_PUBLIC,
        is_synthetic: false,
        generic_signature: None,
        return_type: Some(Arc::from("Ljava/lang/String;")),
    });

    // Add the builder class to the output
    out.nested_classes.push(builder_class);
}

/// Get the simple class name from a class declaration node
fn get_class_name(ctx: &JavaContextExtractor, decl: Node) -> String {
    decl.child_by_field_name("name")
        .map(|n| ctx.node_text(n).to_string())
        .unwrap_or_default()
}

/// Check if a method with the given name and descriptor prefix already exists
fn method_exists(methods: &[MethodSummary], name: &str, descriptor_prefix: &str) -> bool {
    methods
        .iter()
        .any(|m| m.name.as_ref() == name && m.desc().as_ref().starts_with(descriptor_prefix))
}
