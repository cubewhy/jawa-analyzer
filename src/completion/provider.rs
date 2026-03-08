use super::candidate::CompletionCandidate;
use crate::index::{IndexScope, IndexView};
use crate::semantic::SemanticContext;

pub trait CompletionProvider: Send + Sync {
    fn name(&self) -> &'static str;

    fn is_applicable(&self, _ctx: &SemanticContext) -> bool {
        true
    }

    fn provide(
        &self,
        scope: IndexScope,
        ctx: &SemanticContext,
        index: &IndexView,
    ) -> Vec<CompletionCandidate>;
}
