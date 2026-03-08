use crate::{
    completion::fuzzy,
    index::{ClassMetadata, IndexView},
    semantic::{context::SemanticContext, types::symbol_resolver::SymbolResolver},
};
use std::sync::Arc;

pub(crate) fn qualified_nested_type_matches(
    prefix: &str,
    ctx: &SemanticContext,
    index: &IndexView,
) -> Vec<Arc<ClassMetadata>> {
    let Some(dot) = prefix.rfind('.') else {
        return vec![];
    };
    let qualifier = prefix[..dot].trim();
    let member_prefix = prefix[dot + 1..].trim();
    if qualifier.is_empty() {
        return vec![];
    }

    let resolver = SymbolResolver::new(index);
    let Some(owner_internal) = resolver.resolve_type_name(ctx, qualifier) else {
        return vec![];
    };

    index
        .direct_inner_classes_of(&owner_internal)
        .into_iter()
        .filter(|inner| {
            member_prefix.is_empty()
                || fuzzy::fuzzy_match(&member_prefix.to_lowercase(), &inner.name.to_lowercase())
                    .is_some()
        })
        .collect()
}
