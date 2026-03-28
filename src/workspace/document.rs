use std::num::NonZeroUsize;
use std::sync::Arc;

use dashmap::DashMap;
use lru::LruCache;
use tower_lsp::lsp_types::{SemanticToken, Url};
use tree_sitter::Tree;

use crate::build_integration::SourceRootId;
use crate::index::{ClasspathId, ModuleId};
use crate::semantic::SemanticContext;

use super::source_file::SourceFile;

const SEMANTIC_CONTEXT_CACHE_LIMIT: usize = 256;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DocumentStoreStats {
    pub open_document_count: usize,
    pub text_bytes: usize,
    pub rope_bytes: usize,
    pub tree_count: usize,
    pub semantic_token_entries: usize,
    pub semantic_token_bytes: usize,
    pub semantic_context_entries: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct DocumentCacheStats {
    semantic_token_entries: usize,
    semantic_token_bytes: usize,
    semantic_context_entries: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemanticContextCacheKey {
    pub document_version: i32,
    pub workspace_version: u64,
    pub module: ModuleId,
    pub classpath: ClasspathId,
    pub source_root: Option<SourceRootId>,
    pub overlay_class_count: usize,
    pub offset: usize,
    pub trigger: Option<char>,
}

/// Per-document mutable LSP state.
///
/// `Document` owns the current [`SourceFile`] snapshot plus any LSP-level
/// caches that survive across re-parses (e.g. the semantic-token result
/// cache used for incremental delta responses).
///
/// Immutable analysis (completion, inlay hints, semantic tokens, …) should
/// read through `doc.source()` and never access the fields of `Document`
/// directly except to update LSP caches.
#[derive(Debug)]
pub struct Document {
    /// The current parsed snapshot.  Replaced atomically on every
    /// `didChange` / `didOpen` / `didSave`.
    source: Arc<SourceFile>,

    /// Cached semantic token result: `(result_id, flat token data)`.
    /// Keyed on `result_id` (= document version as string) and invalidated
    /// whenever `source` is replaced with a new version.
    pub semantic_token_cache: Option<(String, Vec<SemanticToken>)>,

    /// Cached semantic contexts for the current document version.
    semantic_context_cache: LruCache<SemanticContextCacheKey, Arc<SemanticContext>>,
}

impl Document {
    /// Create a new `Document` from an initial [`SourceFile`].
    pub fn new(source: SourceFile) -> Self {
        Self {
            source: Arc::new(source),
            semantic_token_cache: None,
            semantic_context_cache: LruCache::new(
                NonZeroUsize::new(SEMANTIC_CONTEXT_CACHE_LIMIT)
                    .expect("semantic context cache limit must be non-zero"),
            ),
        }
    }

    /// Read access to the current source snapshot.
    #[inline]
    pub fn source(&self) -> &Arc<SourceFile> {
        &self.source
    }

    #[inline]
    pub fn snapshot(&self) -> Arc<SourceFile> {
        Arc::clone(&self.source)
    }

    /// Replace the current source snapshot with a new one.
    /// Invalidates all version-sensitive caches.
    pub fn update_source(&mut self, source: SourceFile) {
        self.source = Arc::new(source);
        self.clear_caches();
    }

    /// Attach an already-incremented tree to the current source, producing a
    /// new `SourceFile` with the updated tree.  Used by the `did_change`
    /// handler after `tree.edit` + `parser.parse(…, Some(&old))`.
    pub fn set_tree(&mut self, tree: Option<Tree>) {
        // Avoid a full clone of the Arc<SourceFile> by replacing source
        // with a new SourceFile that shares everything except the tree.
        let prev = Arc::unwrap_or_clone(Arc::clone(&self.source));
        self.source = Arc::new(prev.with_tree(tree));
        self.semantic_context_cache.clear();
        // Tree change does not invalidate semantic-token cache by itself;
        // text already changed before the tree was updated.
    }

    pub fn cached_semantic_context(
        &self,
        key: &SemanticContextCacheKey,
    ) -> Option<Arc<SemanticContext>> {
        self.semantic_context_cache.peek(key).cloned()
    }

    pub fn cache_semantic_context(
        &mut self,
        key: SemanticContextCacheKey,
        context: Arc<SemanticContext>,
    ) {
        self.semantic_context_cache.put(key, context);
    }

    pub fn clear_caches(&mut self) {
        self.semantic_token_cache = None;
        self.semantic_context_cache.clear();
    }

    fn cache_stats(&self) -> DocumentCacheStats {
        let (semantic_token_entries, semantic_token_bytes) = self
            .semantic_token_cache
            .as_ref()
            .map(|(result_id, tokens)| {
                (
                    1,
                    result_id.len() + tokens.len() * std::mem::size_of::<SemanticToken>(),
                )
            })
            .unwrap_or((0, 0));

        DocumentCacheStats {
            semantic_token_entries,
            semantic_token_bytes,
            semantic_context_entries: self.semantic_context_cache.len(),
        }
    }

    // ── Convenience pass-throughs ────────────────────────────────────────

    pub fn uri(&self) -> &Url {
        &self.source.uri
    }

    pub fn language_id(&self) -> &str {
        &self.source.language_id
    }

    pub fn version(&self) -> i32 {
        self.source.version
    }

    pub fn text(&self) -> &str {
        self.source.text()
    }
}

/// Thread-safe store of open LSP documents.
pub struct DocumentStore {
    docs: DashMap<Url, Document>,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self {
            docs: DashMap::new(),
        }
    }

    pub fn open(&self, doc: Document) {
        self.docs.insert(doc.uri().clone(), doc);
    }

    pub fn close(&self, uri: &Url) {
        self.docs.remove(uri);
    }

    /// Read-only access — do NOT `.await` inside `f`.
    pub fn with_doc<R>(&self, uri: &Url, f: impl FnOnce(&Document) -> R) -> Option<R> {
        self.docs.get(uri).map(|d| f(&d))
    }

    /// Mutable access — do NOT `.await` inside `f`.
    pub fn with_doc_mut<R>(&self, uri: &Url, f: impl FnOnce(&mut Document) -> R) -> Option<R> {
        self.docs.get_mut(uri).map(|mut d| f(&mut d))
    }

    pub fn snapshot_documents(&self) -> Vec<(Url, String, String)> {
        self.docs
            .iter()
            .map(|entry| {
                let doc = entry.value();
                (
                    doc.uri().clone(),
                    doc.language_id().to_owned(),
                    doc.text().to_owned(),
                )
            })
            .collect()
    }

    pub(crate) fn stats(&self) -> DocumentStoreStats {
        let mut stats = DocumentStoreStats::default();
        for entry in &self.docs {
            let doc = entry.value();
            let cache_stats = doc.cache_stats();
            stats.open_document_count += 1;
            stats.text_bytes += doc.source.text.len();
            stats.rope_bytes += doc.source.rope.len_bytes();
            stats.tree_count += usize::from(doc.source.tree.is_some());
            stats.semantic_token_entries += cache_stats.semantic_token_entries;
            stats.semantic_token_bytes += cache_stats.semantic_token_bytes;
            stats.semantic_context_entries += cache_stats.semantic_context_entries;
        }
        stats
    }

    pub(crate) fn clear_lsp_caches(&self) {
        for mut entry in self.docs.iter_mut() {
            entry.value_mut().clear_caches();
        }
    }
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self::new()
    }
}
