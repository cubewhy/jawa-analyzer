use super::candidate::CompletionCandidate;
use crate::index::{IndexScope, IndexView};
use crate::lsp::{request_cancellation::RequestResult, request_context::RequestContext};
use crate::semantic::SemanticContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSearchSpace {
    Narrow,
    Broad,
}

#[derive(Debug, Default)]
pub struct ProviderCompletionResult {
    pub candidates: Vec<CompletionCandidate>,
    pub is_incomplete: bool,
}

impl From<Vec<CompletionCandidate>> for ProviderCompletionResult {
    fn from(value: Vec<CompletionCandidate>) -> Self {
        Self {
            candidates: value,
            is_incomplete: false,
        }
    }
}

impl ProviderCompletionResult {
    pub fn incomplete(self) -> Self {
        Self {
            candidates: self.candidates,
            is_incomplete: true,
        }
    }
}

pub trait CompletionProvider: Send + Sync {
    fn name(&self) -> &'static str;

    fn is_applicable(&self, _ctx: &SemanticContext) -> bool {
        true
    }

    fn search_space(&self, _ctx: &SemanticContext) -> ProviderSearchSpace {
        ProviderSearchSpace::Narrow
    }

    fn provide(
        &self,
        scope: IndexScope,
        ctx: &SemanticContext,
        index: &IndexView,
        request: Option<&RequestContext>,
        _limit: Option<usize>,
    ) -> RequestResult<ProviderCompletionResult>;

    #[cfg(test)]
    fn provide_test(
        &self,
        scope: IndexScope,
        ctx: &SemanticContext,
        index: &IndexView,
        limit: Option<usize>,
    ) -> ProviderCompletionResult {
        self.provide(scope, ctx, index, None, limit)
            .expect("provider result")
    }
}
