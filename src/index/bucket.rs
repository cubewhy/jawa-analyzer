use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};

use lru::LruCache;
use nucleo_matcher::{
    Config as MatcherConfig, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use parking_lot::RwLock;
use rustc_hash::FxHashSet;

use crate::index::{
    ArchiveClassStub, ClassMetadata, ClassOrigin, FieldSummary, IndexedJavaModule, MethodSummary,
    NameTable, intern_str,
};

type ClassId = usize;
type OwnerKey = Arc<str>;

const MRO_CACHE_LIMIT: usize = 1024;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BucketStats {
    pub class_count: usize,
    pub module_count: usize,
    pub origin_count: usize,
    pub simple_name_entry_count: usize,
    pub package_entry_count: usize,
    pub owner_entry_count: usize,
    pub name_table_size: usize,
    pub mro_cache_entries: usize,
}

type MroCacheMap = LruCache<Arc<str>, (Vec<Arc<MethodSummary>>, Vec<Arc<FieldSummary>>)>;

#[derive(Debug)]
enum StoredClassBody {
    Rich(Arc<ClassMetadata>),
    ArchiveStub {
        stub: Arc<ArchiveClassStub>,
        materialized: OnceLock<Arc<ClassMetadata>>,
    },
}

#[derive(Debug)]
struct StoredClass {
    internal_name: Arc<str>,
    name: Arc<str>,
    package: Option<Arc<str>>,
    inner_class_of: Option<Arc<str>>,
    origin: ClassOrigin,
    body: StoredClassBody,
}

impl StoredClass {
    fn from_metadata(class: Arc<ClassMetadata>) -> Self {
        Self {
            internal_name: Arc::clone(&class.internal_name),
            name: Arc::clone(&class.name),
            package: class.package.clone(),
            inner_class_of: class.inner_class_of.clone(),
            origin: class.origin.clone(),
            body: StoredClassBody::Rich(class),
        }
    }

    fn from_archive_stub(stub: ArchiveClassStub) -> Self {
        Self {
            internal_name: Arc::clone(&stub.internal_name),
            name: Arc::clone(&stub.name),
            package: stub.package.clone(),
            inner_class_of: stub.inner_class_of.clone(),
            origin: stub.origin.clone(),
            body: StoredClassBody::ArchiveStub {
                stub: Arc::new(stub),
                materialized: OnceLock::new(),
            },
        }
    }

    fn materialize(&self) -> Arc<ClassMetadata> {
        match &self.body {
            StoredClassBody::Rich(class) => Arc::clone(class),
            StoredClassBody::ArchiveStub { stub, materialized } => {
                Arc::clone(materialized.get_or_init(|| Arc::new(stub.materialize())))
            }
        }
    }
}

#[derive(Debug)]
struct BucketState {
    classes: Vec<Option<StoredClass>>,
    free_class_ids: Vec<ClassId>,
    by_internal: HashMap<Arc<str>, ClassId>,
    modules: HashMap<Arc<str>, Arc<IndexedJavaModule>>,
    by_origin: HashMap<ClassOrigin, Vec<ClassId>>,
    simple_name_index: HashMap<Arc<str>, Vec<ClassId>>,
    package_index: HashMap<Arc<str>, Vec<ClassId>>,
    owner_index: HashMap<OwnerKey, Vec<ClassId>>,
    mro_cache: MroCacheMap,
}

#[derive(Debug)]
pub struct BucketIndex {
    inner: RwLock<BucketState>,
}

impl BucketIndex {
    pub fn new() -> Self {
        let state = BucketState {
            classes: Vec::with_capacity(100_000),
            free_class_ids: Vec::new(),
            by_internal: HashMap::with_capacity(100_000),
            modules: HashMap::new(),
            by_origin: HashMap::new(),
            simple_name_index: HashMap::with_capacity(100_000),
            package_index: HashMap::with_capacity(10_000),
            owner_index: HashMap::with_capacity(50_000),
            mro_cache: LruCache::new(
                NonZeroUsize::new(MRO_CACHE_LIMIT).expect("MRO cache limit must be non-zero"),
            ),
        };

        Self {
            inner: RwLock::new(state),
        }
    }

    pub fn add_classes(&self, classes: Vec<ClassMetadata>) {
        let mut inner = self.inner.write();

        for mut class in classes {
            Self::intern_class(&mut class);
            Self::insert_record_locked(&mut inner, StoredClass::from_metadata(Arc::new(class)));
        }

        inner.mro_cache.clear();
    }

    pub fn add_archive_classes(&self, classes: Vec<ClassMetadata>) {
        let mut inner = self.inner.write();

        for mut class in classes {
            Self::intern_class(&mut class);
            let stub = ArchiveClassStub::from_class_metadata(class);
            Self::insert_record_locked(&mut inner, StoredClass::from_archive_stub(stub));
        }

        inner.mro_cache.clear();
    }

    pub fn add_modules(&self, modules: Vec<IndexedJavaModule>) {
        let mut inner = self.inner.write();
        for module in modules {
            let name = Arc::from(module.name());
            inner.modules.insert(name, Arc::new(module));
        }
    }

    pub fn update_source(&self, origin: ClassOrigin, classes: Vec<ClassMetadata>) -> bool {
        let mut filtered = Vec::new();

        for c in classes {
            if let Some(existing) = self.get_class(&c.internal_name)
                && matches!(existing.origin, ClassOrigin::Jar(_))
            {
                // the LSP only trusts .class
                tracing::warn!(class = %c.internal_name, "BLOCKED: Prevented source AST from corrupting bytecode index!");
                continue;
            }

            filtered.push(c);
        }

        if filtered.is_empty() {
            return false;
        }

        if self.same_classes_for_origin(&origin, &filtered) {
            return false;
        }

        self.remove_by_origin(&origin);
        self.add_classes(filtered);
        true
    }

    pub fn get_class(&self, internal_name: &str) -> Option<Arc<ClassMetadata>> {
        let inner = self.inner.read();
        let id = *inner.by_internal.get(internal_name)?;
        inner
            .classes
            .get(id)
            .and_then(|class| class.as_ref())
            .map(StoredClass::materialize)
    }

    pub fn get_module(&self, module_name: &str) -> Option<Arc<IndexedJavaModule>> {
        let inner = self.inner.read();
        inner.modules.get(module_name).cloned()
    }

    pub fn get_source_type_name(&self, internal: &str) -> Option<String> {
        let class = self.get_class(internal)?;
        Some(class.qualified_source_name_with(|owner_internal| self.get_class(owner_internal)))
    }

    pub fn get_classes_by_simple_name(&self, simple_name: &str) -> Vec<Arc<ClassMetadata>> {
        let inner = self.inner.read();
        inner
            .simple_name_index
            .get(simple_name)
            .map(|ids| Self::resolve_class_ids_locked(&inner, ids))
            .unwrap_or_default()
    }

    pub fn direct_inner_classes_of(&self, owner_internal: &str) -> Vec<Arc<ClassMetadata>> {
        self.direct_inner_classes_by_owner(owner_internal)
    }

    pub fn direct_inner_classes_by_owner(&self, owner_internal: &str) -> Vec<Arc<ClassMetadata>> {
        let inner = self.inner.read();
        inner
            .owner_index
            .get(owner_internal)
            .map(|ids| Self::resolve_class_ids_locked(&inner, ids))
            .unwrap_or_default()
    }

    pub fn resolve_direct_inner_class(
        &self,
        owner_internal: &str,
        simple_name: &str,
    ) -> Option<Arc<ClassMetadata>> {
        self.direct_inner_classes_of(owner_internal)
            .into_iter()
            .find(|c| c.matches_simple_name(simple_name))
    }

    pub fn classes_in_package(&self, pkg: &str) -> Vec<Arc<ClassMetadata>> {
        let normalized = pkg.replace('.', "/");
        let inner = self.inner.read();
        inner
            .package_index
            .get(normalized.as_str())
            .map(|ids| Self::resolve_class_ids_locked(&inner, ids))
            .unwrap_or_default()
    }

    pub fn has_package(&self, pkg: &str) -> bool {
        let normalized = pkg.replace('.', "/");
        let inner = self.inner.read();
        inner.package_index.contains_key(normalized.as_str())
    }

    pub fn has_classes_in_package(&self, pkg: &str) -> bool {
        let normalized = pkg.replace('.', "/");
        let inner = self.inner.read();
        inner
            .package_index
            .get(normalized.as_str())
            .is_some_and(|ids| !ids.is_empty())
    }

    pub fn package_names(&self) -> Vec<Arc<str>> {
        let inner = self.inner.read();
        inner.package_index.keys().cloned().collect()
    }

    pub fn module_names(&self) -> Vec<Arc<str>> {
        let inner = self.inner.read();
        inner.modules.keys().cloned().collect()
    }

    pub fn resolve_imports(&self, imports: &[Arc<str>]) -> Vec<Arc<ClassMetadata>> {
        let mut result = Vec::new();
        for import in imports {
            if import.ends_with(".*") {
                let pkg = import.trim_end_matches(".*").replace('.', "/");
                let classes = self.classes_in_package(&pkg);
                tracing::debug!(
                    import = import.as_ref(),
                    pkg,
                    count = classes.len(),
                    "wildcard import expanded"
                );
                result.extend(classes.into_iter());
            } else {
                let internal = import.replace('.', "/");
                tracing::debug!(import = import.as_ref(), internal, "exact import lookup");
                if let Some(cls) = self.get_class(&internal) {
                    tracing::debug!(internal, "exact import found");
                    result.push(cls);
                } else {
                    tracing::debug!(internal, "exact import NOT FOUND");
                }
            }
        }
        result
    }

    pub fn collect_inherited_members(
        &self,
        class_internal: &str,
    ) -> (Vec<Arc<MethodSummary>>, Vec<Arc<FieldSummary>>) {
        if let Some(cached) = {
            let inner = self.inner.read();
            inner.mro_cache.peek(class_internal).cloned()
        } {
            return cached;
        }

        let mut methods: Vec<Arc<MethodSummary>> = Vec::new();
        let mut fields: Vec<Arc<FieldSummary>> = Vec::new();
        let mut seen_methods: FxHashSet<(Arc<str>, Arc<str>)> = Default::default();
        let mut seen_fields: FxHashSet<Arc<str>> = Default::default();
        let mut seen_classes: FxHashSet<Arc<str>> = Default::default();
        let mut queue: VecDeque<Arc<str>> = Default::default();

        queue.push_back(Arc::from(class_internal));

        while let Some(internal) = queue.pop_front() {
            if !seen_classes.insert(Arc::clone(&internal)) {
                continue;
            }

            let meta = match self.get_class(&internal) {
                Some(m) => m,
                None => continue,
            };

            for method in &meta.methods {
                let key = (Arc::clone(&method.name), Arc::clone(&method.desc()));
                if seen_methods.insert(key) {
                    methods.push(Arc::new(method.clone()));
                }
            }
            for field in &meta.fields {
                if seen_fields.insert(Arc::clone(&field.name)) {
                    fields.push(Arc::new(field.clone()));
                }
            }

            if let Some(ref super_name) = meta.super_name
                && !super_name.is_empty()
            {
                queue.push_back(super_name.clone());
            }
            for iface in &meta.interfaces {
                if !iface.is_empty() {
                    queue.push_back(Arc::clone(iface));
                }
            }
        }
        let result = (methods, fields);
        self.inner
            .write()
            .mro_cache
            .put(Arc::from(class_internal), result.clone());

        result
    }

    pub fn mro(&self, class_internal: &str) -> Vec<Arc<ClassMetadata>> {
        let mut result = Vec::new();
        let mut seen: std::collections::HashSet<Arc<str>> = std::collections::HashSet::new();
        let mut queue: VecDeque<Arc<str>> = VecDeque::new();

        queue.push_back(Arc::from(class_internal));
        while let Some(internal) = queue.pop_front() {
            if !seen.insert(internal.clone()) {
                continue;
            }
            let meta = match self.get_class(&internal) {
                Some(m) => m,
                None => continue,
            };
            if let Some(ref super_name) = meta.super_name
                && !super_name.is_empty()
            {
                queue.push_back(super_name.clone());
            }
            for iface in &meta.interfaces {
                if !iface.is_empty() {
                    queue.push_back(iface.clone());
                }
            }
            result.push(meta);
        }
        result
    }

    pub fn fuzzy_autocomplete(&self, query: &str, limit: usize) -> Vec<Arc<str>> {
        if limit == 0 {
            return vec![];
        }

        let inner = self.inner.read();
        if query.is_empty() {
            let mut names: Vec<_> = inner.simple_name_index.keys().cloned().collect();
            names.sort_unstable_by(|a, b| a.as_ref().cmp(b.as_ref()));
            names.truncate(limit);
            return names;
        }

        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut matcher = Matcher::new(MatcherConfig::DEFAULT);
        let mut utf32_buf = Vec::new();
        let mut scored = Vec::new();

        for name in inner.simple_name_index.keys() {
            utf32_buf.clear();
            let haystack = Utf32Str::new(name.as_ref(), &mut utf32_buf);
            if let Some(score) = pattern.score(haystack, &mut matcher) {
                scored.push((Arc::clone(name), score));
            }
        }

        scored.sort_unstable_by(|(left_name, left_score), (right_name, right_score)| {
            right_score
                .cmp(left_score)
                .then_with(|| left_name.as_ref().cmp(right_name.as_ref()))
        });
        scored.truncate(limit);
        scored.into_iter().map(|(name, _)| name).collect()
    }

    pub fn fuzzy_search_classes(&self, query: &str, limit: usize) -> Vec<Arc<ClassMetadata>> {
        if query.is_empty() {
            return self.iter_all_classes().into_iter().take(limit).collect();
        }
        let simple_names = self.fuzzy_autocomplete(query, limit);
        simple_names
            .into_iter()
            .flat_map(|name| self.get_classes_by_simple_name(&name).into_iter())
            .collect()
    }

    pub fn exact_match_keys(&self) -> Vec<Arc<str>> {
        let inner = self.inner.read();
        let keys: Vec<Arc<str>> = inner.by_internal.keys().cloned().collect();
        tracing::debug!(
            key_count = keys.len(),
            sample_keys = ?keys.iter().take(5).map(|k| k.as_ref()).collect::<Vec<_>>(),
            "BucketIndex::exact_match_keys"
        );
        keys
    }

    pub fn class_count(&self) -> usize {
        let inner = self.inner.read();
        inner.by_internal.len()
    }

    pub fn iter_all_classes(&self) -> Vec<Arc<ClassMetadata>> {
        let inner = self.inner.read();
        inner
            .classes
            .iter()
            .filter_map(|class| class.as_ref().map(StoredClass::materialize))
            .collect()
    }

    pub fn build_name_table(&self) -> Arc<NameTable> {
        NameTable::from_names(self.exact_match_keys())
    }

    pub(crate) fn stats(&self) -> BucketStats {
        let inner = self.inner.read();
        BucketStats {
            class_count: inner.by_internal.len(),
            module_count: inner.modules.len(),
            origin_count: inner.by_origin.len(),
            simple_name_entry_count: inner.simple_name_index.len(),
            package_entry_count: inner.package_index.len(),
            owner_entry_count: inner.owner_index.len(),
            name_table_size: 0,
            mro_cache_entries: inner.mro_cache.len(),
        }
    }

    pub(crate) fn clear_query_caches(&self) {
        self.inner.write().mro_cache.clear();
    }

    pub fn remove_by_origin(&self, origin: &ClassOrigin) -> bool {
        let mut inner = self.inner.write();
        let class_ids = match inner.by_origin.remove(origin) {
            Some(v) => v,
            None => return false,
        };

        for class_id in class_ids {
            Self::detach_class_locked(&mut inner, class_id);
        }

        inner.mro_cache.clear();
        true
    }

    fn same_classes_for_origin(&self, origin: &ClassOrigin, classes: &[ClassMetadata]) -> bool {
        let inner = self.inner.read();
        let Some(class_ids) = inner.by_origin.get(origin) else {
            return false;
        };
        if class_ids.len() != classes.len() {
            return false;
        }

        let mut existing = class_ids
            .iter()
            .filter_map(|id| {
                inner
                    .classes
                    .get(*id)
                    .and_then(|record| record.as_ref())
                    .map(StoredClass::materialize)
            })
            .map(|meta| (*meta).clone())
            .collect::<Vec<_>>();
        drop(inner);

        existing.sort_unstable_by(|left, right| left.internal_name.cmp(&right.internal_name));

        let mut incoming = classes.to_vec();
        incoming.sort_unstable_by(|left, right| left.internal_name.cmp(&right.internal_name));

        existing == incoming
    }

    fn resolve_class_ids_locked(inner: &BucketState, ids: &[ClassId]) -> Vec<Arc<ClassMetadata>> {
        ids.iter()
            .filter_map(|id| {
                inner
                    .classes
                    .get(*id)
                    .and_then(|class| class.as_ref())
                    .map(StoredClass::materialize)
            })
            .collect()
    }

    fn insert_record_locked(inner: &mut BucketState, class: StoredClass) {
        let internal = Arc::clone(&class.internal_name);
        let simple = Arc::clone(&class.name);
        let pkg = class.package.clone();
        let owner_internal = class.inner_class_of.clone();
        let origin = class.origin.clone();

        if let Some(existing_id) = inner.by_internal.get(internal.as_ref()).copied() {
            Self::remove_class_by_id_locked(inner, existing_id);
        }

        let class_id = if let Some(reused_id) = inner.free_class_ids.pop() {
            inner.classes[reused_id] = Some(class);
            reused_id
        } else {
            let next_id = inner.classes.len();
            inner.classes.push(Some(class));
            next_id
        };

        inner.by_internal.insert(internal, class_id);
        inner
            .simple_name_index
            .entry(simple)
            .or_default()
            .push(class_id);

        if let Some(pkg) = pkg {
            inner.package_index.entry(pkg).or_default().push(class_id);
        }

        if let Some(owner_name) = owner_internal {
            inner
                .owner_index
                .entry(owner_name)
                .or_default()
                .push(class_id);
        }

        inner.by_origin.entry(origin).or_default().push(class_id);
    }

    fn remove_class_by_id_locked(inner: &mut BucketState, class_id: ClassId) -> bool {
        let Some(meta) = Self::detach_class_locked(inner, class_id) else {
            return false;
        };

        let origin = meta.origin.clone();
        let remove_origin_entry = inner.by_origin.get_mut(&origin).is_some_and(|ids| {
            ids.retain(|candidate| *candidate != class_id);
            ids.is_empty()
        });
        if remove_origin_entry {
            inner.by_origin.remove(&origin);
        }
        true
    }

    fn detach_class_locked(inner: &mut BucketState, class_id: ClassId) -> Option<StoredClass> {
        let class = inner.classes.get_mut(class_id)?.take()?;
        if inner.by_internal.get(class.internal_name.as_ref()).copied() == Some(class_id) {
            inner.by_internal.remove(class.internal_name.as_ref());
        }

        Self::trim_id_index_locked(&mut inner.simple_name_index, class.name.as_ref(), class_id);

        if let Some(pkg) = class.package.as_deref() {
            Self::trim_id_index_locked(&mut inner.package_index, pkg, class_id);
        }
        if let Some(owner_name) = class.inner_class_of.as_deref() {
            Self::trim_id_index_locked(&mut inner.owner_index, owner_name, class_id);
        }

        inner.free_class_ids.push(class_id);
        Some(class)
    }

    fn trim_id_index_locked(
        index: &mut HashMap<Arc<str>, Vec<ClassId>>,
        key: &str,
        class_id: ClassId,
    ) {
        let remove_entry = index.get_mut(key).is_some_and(|ids| {
            ids.retain(|candidate| *candidate != class_id);
            ids.is_empty()
        });
        if remove_entry {
            index.remove(key);
        }
    }

    fn intern_class(class: &mut ClassMetadata) {
        class.name = intern_str(&class.name);
        class.internal_name = intern_str(&class.internal_name);
        if let Some(pkg) = &class.package {
            class.package = Some(intern_str(pkg));
        }
        for m in &mut class.methods {
            m.name = intern_str(&m.name);
            if let Some(rt) = &m.return_type {
                m.return_type = Some(intern_str(rt));
            }
            if let Some(gs) = &m.generic_signature {
                m.generic_signature = Some(intern_str(gs));
            }
        }
        for f in &mut class.fields {
            f.name = intern_str(&f.name);
            f.descriptor = intern_str(&f.descriptor);
        }

        match &class.origin {
            ClassOrigin::Jar(j) => {
                class.origin = ClassOrigin::Jar(intern_str(j));
            }
            ClassOrigin::SourceFile(s) => {
                class.origin = ClassOrigin::SourceFile(intern_str(s));
            }
            ClassOrigin::ZipSource {
                zip_path,
                entry_name,
            } => {
                class.origin = ClassOrigin::ZipSource {
                    zip_path: intern_str(zip_path),
                    entry_name: intern_str(entry_name),
                };
            }
            _ => {
                // tracing::error!("Unknown class source found for class {}", class.name);
            }
        }
    }
}

impl Default for BucketIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{AnnotationSummary, AnnotationValue, MethodParam, MethodParams};
    use rust_asm::constants::{ACC_ANNOTATION, ACC_PUBLIC};

    fn target_annotation(targets: &[&str]) -> AnnotationSummary {
        AnnotationSummary {
            internal_name: Arc::from("java/lang/annotation/Target"),
            runtime_visible: true,
            elements: rustc_hash::FxHashMap::from_iter([(
                Arc::from("value"),
                AnnotationValue::Array(
                    targets
                        .iter()
                        .map(|target| AnnotationValue::Enum {
                            type_name: Arc::from("java/lang/annotation/ElementType"),
                            const_name: Arc::from(*target),
                        })
                        .collect(),
                ),
            )]),
        }
    }

    fn retention_annotation(policy: &str) -> AnnotationSummary {
        AnnotationSummary {
            internal_name: Arc::from("java/lang/annotation/Retention"),
            runtime_visible: true,
            elements: rustc_hash::FxHashMap::from_iter([(
                Arc::from("value"),
                AnnotationValue::Enum {
                    type_name: Arc::from("java/lang/annotation/RetentionPolicy"),
                    const_name: Arc::from(policy),
                },
            )]),
        }
    }

    #[test]
    fn archive_classes_materialize_annotation_semantics_and_param_names() {
        let bucket = BucketIndex::new();
        let annotation = ClassMetadata {
            package: Some(Arc::from("com/example")),
            name: Arc::from("MyAnnotation"),
            internal_name: Arc::from("com/example/MyAnnotation"),
            super_name: Some(Arc::from("java/lang/Object")),
            interfaces: vec![Arc::from("java/lang/annotation/Annotation")],
            annotations: vec![
                target_annotation(&["TYPE", "METHOD"]),
                retention_annotation("RUNTIME"),
            ],
            methods: vec![MethodSummary {
                name: Arc::from("value"),
                params: MethodParams {
                    items: vec![MethodParam {
                        descriptor: Arc::from("Ljava/lang/String;"),
                        name: Arc::from("name"),
                        annotations: Vec::new(),
                    }],
                },
                annotations: Vec::new(),
                access_flags: ACC_PUBLIC,
                is_synthetic: false,
                generic_signature: None,
                return_type: Some(Arc::from("Ljava/lang/String;")),
            }],
            fields: Vec::new(),
            access_flags: ACC_PUBLIC | ACC_ANNOTATION,
            generic_signature: None,
            inner_class_of: None,
            origin: ClassOrigin::Jar(Arc::from("jar:///annotations.jar")),
        };

        bucket.add_archive_classes(vec![annotation]);

        let materialized = bucket
            .get_class("com/example/MyAnnotation")
            .expect("archive class should materialize");

        assert_eq!(
            materialized.annotation_targets(),
            Some(vec![Arc::from("TYPE"), Arc::from("METHOD")])
        );
        assert_eq!(materialized.annotation_retention(), Some("RUNTIME"));
        assert_eq!(
            materialized.methods[0].params.param_names(),
            vec![Arc::from("name")]
        );
    }
}
