use super::candidate::CompletionCandidate;
use crate::index::GlobalIndex;
use crate::semantic::SemanticContext;

pub trait CompletionProvider: Send + Sync {
    fn name(&self) -> &'static str;

    fn provide(&self, ctx: &SemanticContext, index: &mut GlobalIndex) -> Vec<CompletionCandidate>;
}
