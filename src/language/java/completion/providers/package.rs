use crate::{
    completion::{
        candidate::ReplacementMode,
        provider::{CompletionProvider, ProviderCompletionResult},
    },
    index::{IndexScope, IndexView},
    semantic::context::{CursorLocation, SemanticContext},
};

pub struct PackageProvider;

impl CompletionProvider for PackageProvider {
    fn name(&self) -> &'static str {
        "package"
    }

    fn is_applicable(&self, ctx: &SemanticContext) -> bool {
        match &ctx.location {
            CursorLocation::Import { .. } => true,
            CursorLocation::Expression { prefix } | CursorLocation::TypeAnnotation { prefix } => {
                prefix.contains('.')
            }
            CursorLocation::MemberAccess {
                receiver_semantic_type,
                receiver_type,
                receiver_expr,
                arguments,
                ..
            } => {
                if receiver_semantic_type.is_some()
                    || receiver_type.is_some()
                    || ctx.typed_chain_receiver.is_some()
                {
                    return false;
                }
                if receiver_expr.trim().is_empty() || arguments.is_some() {
                    return false;
                }
                true
            }
            _ => false,
        }
    }

    fn provide(
        &self,
        scope: IndexScope,
        ctx: &SemanticContext,
        index: &IndexView,
        request: Option<&crate::lsp::request_context::RequestContext>,
        _limit: Option<usize>,
    ) -> crate::lsp::request_cancellation::RequestResult<ProviderCompletionResult> {
        match &ctx.location {
            CursorLocation::Import { prefix } => {
                Ok(crate::completion::import_completion::candidates_for_import(
                    prefix, scope, index, request,
                )?
                .into_iter()
                .map(|c| {
                    let filter_text = c.insert_text.clone();
                    c.with_replacement_mode(ReplacementMode::ImportPath)
                        .with_filter_text(filter_text)
                })
                .collect::<Vec<_>>()
                .into())
            }
            CursorLocation::Expression { prefix } | CursorLocation::TypeAnnotation { prefix } => {
                if !prefix.contains('.') {
                    return Ok(ProviderCompletionResult::default());
                }
                Ok(crate::completion::import_completion::candidates_for_import(
                    prefix, scope, index, request,
                )?
                .into_iter()
                .map(|c| {
                    let filter_text = c.label.to_string();
                    c.with_replacement_mode(ReplacementMode::PackagePath)
                        .with_filter_text(filter_text)
                })
                .collect::<Vec<_>>()
                .into())
            }
            CursorLocation::MemberAccess {
                receiver_expr,
                member_prefix,
                ..
            } => {
                if receiver_expr.trim().is_empty() {
                    return Ok(ProviderCompletionResult::default());
                }
                if !receiver_expr.contains('.')
                    && receiver_expr
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_uppercase())
                {
                    return Ok(ProviderCompletionResult::default());
                }
                let full_prefix = if member_prefix.is_empty() {
                    format!("{}.", receiver_expr)
                } else {
                    format!("{}.{}", receiver_expr, member_prefix)
                };
                Ok(crate::completion::import_completion::candidates_for_import(
                    &full_prefix,
                    scope,
                    index,
                    request,
                )?
                .into_iter()
                .map(|c| {
                    let filter_text = c.label.to_string();
                    c.with_replacement_mode(ReplacementMode::MemberSegment)
                        .with_filter_text(filter_text)
                })
                .collect::<Vec<_>>()
                .into())
            }
            _ => Ok(ProviderCompletionResult::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completion::CandidateKind;
    use crate::index::WorkspaceIndex;
    use crate::index::{ClassMetadata, ClassOrigin, IndexScope, ModuleId};
    use crate::semantic::context::{CursorLocation, SemanticContext};
    use rust_asm::constants::ACC_PUBLIC;
    use std::sync::Arc;

    fn root_scope() -> IndexScope {
        IndexScope {
            module: ModuleId::ROOT,
        }
    }

    fn make_index() -> WorkspaceIndex {
        let idx = WorkspaceIndex::new();
        idx.add_jar_classes(
            IndexScope {
                module: ModuleId::ROOT,
            },
            vec![
                make_cls("org/cubewhy", "Main"),
                make_cls("org/cubewhy", "Main2"),
                make_cls("org/cubewhy/utils", "StringUtil"),
                make_cls("org/cubewhy/utils", "FileUtil"),
                make_cls("com/other", "Other"),
            ],
        );
        idx
    }

    fn make_cls(pkg: &str, name: &str) -> ClassMetadata {
        ClassMetadata {
            package: Some(Arc::from(pkg)),
            name: Arc::from(name),
            internal_name: Arc::from(format!("{}/{}", pkg, name).as_str()),
            super_name: None,
            interfaces: vec![],
            annotations: vec![],
            methods: vec![],
            fields: vec![],
            access_flags: ACC_PUBLIC,
            generic_signature: None,
            inner_class_of: None,
            origin: ClassOrigin::Unknown,
        }
    }

    fn import_ctx(prefix: &str) -> SemanticContext {
        SemanticContext::new(
            CursorLocation::Import {
                prefix: prefix.to_string(),
            },
            prefix,
            vec![],
            None,
            None,
            None,
            vec![],
        )
    }

    fn expr_ctx(prefix: &str) -> SemanticContext {
        SemanticContext::new(
            CursorLocation::Expression {
                prefix: prefix.to_string(),
            },
            prefix,
            vec![],
            None,
            None,
            None,
            vec![],
        )
    }

    #[test]
    fn test_top_level_package_no_dot() {
        let idx = make_index();
        let scope = IndexScope {
            module: ModuleId::ROOT,
        };
        let results = PackageProvider
            .provide_test(scope, &import_ctx("org"), &idx.view(root_scope()), None)
            .candidates;
        let org = results.iter().find(|c| c.label.as_ref() == "org.");
        assert!(
            org.is_some(),
            "should suggest 'org.': {:?}",
            results.iter().map(|c| c.label.as_ref()).collect::<Vec<_>>()
        );
        assert_eq!(org.unwrap().insert_text, "org.");
    }

    #[test]
    fn test_expression_no_dot_no_package_completion() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(
                root_scope(),
                &expr_ctx("Main"),
                &idx.view(root_scope()),
                None,
            )
            .candidates;
        assert!(results.is_empty(), "no dot = no package completion");
    }

    #[test]
    fn test_empty_prefix_no_crash() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(root_scope(), &import_ctx(""), &idx.view(root_scope()), None)
            .candidates;
        assert!(results.is_empty());
    }

    #[test]
    fn test_member_access_uppercase_receiver_not_package() {
        // String.| → 不是包路径
        let idx = make_index();
        let ctx = SemanticContext::new(
            CursorLocation::MemberAccess {
                receiver_semantic_type: None,
                receiver_type: None,
                member_prefix: "".to_string(),
                receiver_expr: "String".to_string(),
                arguments: None,
            },
            "",
            vec![],
            None,
            None,
            None,
            vec![],
        );
        let results = PackageProvider
            .provide_test(root_scope(), &ctx, &idx.view(root_scope()), None)
            .candidates;
        assert!(
            results.is_empty(),
            "String.| should not trigger package completion"
        );
    }

    #[test]
    fn test_member_access_with_semantic_receiver_not_applicable() {
        let ctx = SemanticContext::new(
            CursorLocation::MemberAccess {
                receiver_semantic_type: Some(crate::semantic::types::type_name::TypeName::new(
                    "org/cubewhy/ChainCheck",
                )),
                receiver_type: None,
                member_prefix: "".to_string(),
                receiver_expr: "ChainCheck".to_string(),
                arguments: None,
            },
            "",
            vec![],
            None,
            None,
            None,
            vec![],
        );
        assert!(!PackageProvider.is_applicable(&ctx));
    }

    #[test]
    fn test_member_access_empty_receiver_call_not_applicable() {
        let ctx = SemanticContext::new(
            CursorLocation::MemberAccess {
                receiver_semantic_type: None,
                receiver_type: None,
                member_prefix: "join".to_string(),
                receiver_expr: "".to_string(),
                arguments: Some("(\"-\", \"java\")".to_string()),
            },
            "join",
            vec![],
            Some(Arc::from("VarargsExample")),
            Some(Arc::from("VarargsExample")),
            None,
            vec![],
        );
        assert!(
            !PackageProvider.is_applicable(&ctx),
            "empty-receiver callsite should not dispatch package provider"
        );
    }

    #[test]
    fn test_top_level_package_label_has_dot() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(
                root_scope(),
                &import_ctx("org"),
                &idx.view(root_scope()),
                None,
            )
            .candidates;
        let org = results.iter().find(|c| c.label.as_ref() == "org.").unwrap();
        assert_eq!(org.insert_text, "org.");
    }

    #[test]
    fn test_import_pkg_dot_lists_classes() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(
                root_scope(),
                &import_ctx("org.cubewhy."),
                &idx.view(root_scope()),
                None,
            )
            .candidates;
        let labels: Vec<&str> = results.iter().map(|c| c.label.as_ref()).collect();
        assert!(labels.contains(&"org.cubewhy.Main"), "{:?}", labels);
        assert!(labels.contains(&"org.cubewhy.Main2"), "{:?}", labels);
    }

    #[test]
    fn test_import_pkg_dot_lists_sub_packages() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(
                root_scope(),
                &import_ctx("org.cubewhy."),
                &idx.view(root_scope()),
                None,
            )
            .candidates;
        let labels: Vec<&str> = results.iter().map(|c| c.label.as_ref()).collect();
        assert!(labels.contains(&"org.cubewhy.utils."), "{:?}", labels);
    }

    #[test]
    fn test_import_pkg_with_name_prefix() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(
                root_scope(),
                &import_ctx("org.cubewhy.Ma"),
                &idx.view(root_scope()),
                None,
            )
            .candidates;
        let labels: Vec<&str> = results.iter().map(|c| c.label.as_ref()).collect();
        assert!(labels.contains(&"org.cubewhy.Main"), "{:?}", labels);
        assert!(labels.contains(&"org.cubewhy.Main2"), "{:?}", labels);
        assert!(labels.iter().all(|l| !l.contains("Other")), "{:?}", labels);
    }

    #[test]
    fn test_import_insert_text_is_fqn() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(
                root_scope(),
                &import_ctx("org.cubewhy.Ma"),
                &idx.view(root_scope()),
                None,
            )
            .candidates;
        let main = results
            .iter()
            .find(|c| c.label.as_ref() == "org.cubewhy.Main")
            .unwrap();
        assert_eq!(main.insert_text, "org.cubewhy.Main");
    }

    #[test]
    fn test_sub_package_insert_text_ends_with_dot() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(
                root_scope(),
                &import_ctx("org.cubewhy."),
                &idx.view(root_scope()),
                None,
            )
            .candidates;
        let utils = results
            .iter()
            .find(|c| c.label.as_ref() == "org.cubewhy.utils.")
            .unwrap();
        assert!(utils.insert_text.ends_with('.'), "{:?}", utils.insert_text);
    }

    #[test]
    fn test_sub_package_kind_is_package() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(
                root_scope(),
                &import_ctx("org.cubewhy."),
                &idx.view(root_scope()),
                None,
            )
            .candidates;
        let utils = results
            .iter()
            .find(|c| c.label.as_ref() == "org.cubewhy.utils.")
            .unwrap();
        assert_eq!(utils.kind, CandidateKind::Package);
    }

    #[test]
    fn test_import_no_dot_returns_top_level_packages() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(
                root_scope(),
                &import_ctx("Main"),
                &idx.view(root_scope()),
                None,
            )
            .candidates;
        // 大写开头不匹配包名，但现在 candidates_for_import 会匹配类名
        // PackageProvider 在 Import 场景下直接转发给 candidates_for_import
        // "Main" 大写开头 → 返回类，不返回包
        assert!(
            results.iter().all(|c| c.kind != CandidateKind::Package),
            "uppercase prefix should not match packages: {:?}",
            results.iter().map(|c| c.label.as_ref()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_sub_package_label_is_full_path() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(
                root_scope(),
                &import_ctx("org.cubewhy."),
                &idx.view(root_scope()),
                None,
            )
            .candidates;
        let utils = results
            .iter()
            .find(|c| c.label.as_ref() == "org.cubewhy.utils.")
            .unwrap();
        assert_eq!(utils.kind, CandidateKind::Package);
    }

    #[test]
    fn test_expression_with_dot_triggers() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(
                root_scope(),
                &expr_ctx("org.cubewhy."),
                &idx.view(root_scope()),
                None,
            )
            .candidates;
        assert!(!results.is_empty(), "prefix with dot should trigger");
    }

    #[test]
    fn test_expression_no_dot_no_completion() {
        let idx = make_index();
        let results = PackageProvider
            .provide_test(
                root_scope(),
                &expr_ctx("Main"),
                &idx.view(root_scope()),
                None,
            )
            .candidates;
        assert!(results.is_empty(), "no dot = no package completion");
    }

    #[test]
    fn test_member_access_single_uppercase_no_package() {
        let idx = make_index();
        let ctx = SemanticContext::new(
            CursorLocation::MemberAccess {
                receiver_semantic_type: None,
                receiver_type: None,
                member_prefix: "".to_string(),
                receiver_expr: "String".to_string(),
                arguments: None,
            },
            "",
            vec![],
            None,
            None,
            None,
            vec![],
        );
        let results = PackageProvider
            .provide_test(root_scope(), &ctx, &idx.view(root_scope()), None)
            .candidates;
        assert!(
            results.is_empty(),
            "single uppercase segment should not trigger package completion"
        );
    }

    #[test]
    fn test_member_access_insert_text_trimmed() {
        // org.cubewhy.| → insert_text 应该是 "Main" 而不是 "org.cubewhy.Main"
        let idx = make_index();
        let ctx = SemanticContext::new(
            CursorLocation::MemberAccess {
                receiver_semantic_type: None,
                receiver_type: None,
                member_prefix: "".to_string(),
                receiver_expr: "org.cubewhy".to_string(),
                arguments: None,
            },
            "",
            vec![],
            None,
            None,
            None,
            vec![],
        );
        let results = PackageProvider
            .provide_test(root_scope(), &ctx, &idx.view(root_scope()), None)
            .candidates;
        let main = results
            .iter()
            .find(|c| c.label.as_ref() == "org.cubewhy.Main")
            .unwrap();
        assert_eq!(
            main.insert_text, "org.cubewhy.Main",
            "insert_text should be trimmed to just 'Main'"
        );
    }

    #[test]
    fn test_member_access_package_like_triggers() {
        let idx = make_index();
        let ctx = SemanticContext::new(
            CursorLocation::MemberAccess {
                receiver_semantic_type: None,
                receiver_type: None,
                member_prefix: "".to_string(),
                receiver_expr: "org.cubewhy".to_string(),
                arguments: None,
            },
            "",
            vec![],
            None,
            None,
            None,
            vec![],
        );
        let results = PackageProvider
            .provide_test(root_scope(), &ctx, &idx.view(root_scope()), None)
            .candidates;
        assert!(
            !results.is_empty(),
            "org.cubewhy.| should trigger package completion: {:?}",
            results.iter().map(|c| c.label.as_ref()).collect::<Vec<_>>()
        );
        // label 是 FQN，insert_text 是截断后的短名
        assert!(
            results
                .iter()
                .any(|c| c.label.as_ref() == "org.cubewhy.Main"),
            "{:?}",
            results.iter().map(|c| c.label.as_ref()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_member_access_package_with_name_prefix() {
        let idx = make_index();
        let ctx = SemanticContext::new(
            CursorLocation::MemberAccess {
                receiver_semantic_type: None,
                receiver_type: None,
                member_prefix: "Ma".to_string(),
                receiver_expr: "org.cubewhy".to_string(),
                arguments: None,
            },
            "Ma",
            vec![],
            None,
            None,
            None,
            vec![],
        );
        let results = PackageProvider
            .provide_test(root_scope(), &ctx, &idx.view(root_scope()), None)
            .candidates;
        assert!(
            results
                .iter()
                .any(|c| c.label.as_ref() == "org.cubewhy.Main"),
            "{:?}",
            results.iter().map(|c| c.label.as_ref()).collect::<Vec<_>>()
        );
        assert!(
            results
                .iter()
                .any(|c| c.label.as_ref() == "org.cubewhy.Main2"),
            "{:?}",
            results.iter().map(|c| c.label.as_ref()).collect::<Vec<_>>()
        );
        // insert_text 应该是截断后的短名
        let main = results
            .iter()
            .find(|c| c.label.as_ref() == "org.cubewhy.Main")
            .unwrap();
        assert_eq!(main.insert_text, "org.cubewhy.Main");
    }
}
