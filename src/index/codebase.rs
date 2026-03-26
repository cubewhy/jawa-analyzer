use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

use super::incremental::{SourceParseSession, SourceTextInput};
use super::{ClassMetadata, ClassOrigin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SourceScanMode {
    Default,
    IncludeGenerated,
}

/// Scan result
pub struct CodebaseIndex {
    pub classes: Vec<ClassMetadata>,
    /// Number of files actually scanned
    pub file_count: usize,
}

/// Stateful codebase indexer that preserves Java Salsa parse snapshots across rescans.
#[derive(Default)]
pub struct CodebaseIndexSession {
    java_session: SourceParseSession,
}

impl CodebaseIndexSession {
    pub fn index_codebase<P: AsRef<Path>>(
        &mut self,
        root: P,
        name_table: Option<Arc<crate::index::NameTable>>,
    ) -> CodebaseIndex {
        let root = root.as_ref();
        tracing::info!(root = %root.display(), "codebase indexing started");
        let source_files = collect_source_files([root.to_path_buf()]);
        self.index_source_files(source_files, name_table)
    }

    pub fn index_codebase_paths<I>(
        &mut self,
        roots: I,
        name_table: Option<Arc<crate::index::NameTable>>,
    ) -> CodebaseIndex
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let source_files = collect_source_files(roots);
        self.index_source_files(source_files, name_table)
    }

    pub fn index_source_text(
        &mut self,
        uri: &str,
        content: &str,
        lang: &str,
        name_table: Option<Arc<crate::index::NameTable>>,
    ) -> Vec<ClassMetadata> {
        let origin = ClassOrigin::SourceFile(Arc::from(uri));
        match lang {
            "java" => {
                let input = SourceTextInput::new(
                    Arc::from(uri),
                    Arc::from("java"),
                    content.to_owned(),
                    origin,
                );
                let Some(prepared) = self.java_session.prepare_source(input) else {
                    return vec![];
                };
                prepared.extract_classes(name_table)
            }
            // TODO: use incremental parsing for kotlin
            _ => super::source::parse_source_str(content, lang, origin, name_table),
        }
    }

    pub fn java_parse_origin(&self, uri: &str) -> Option<crate::salsa_db::ParseTreeOrigin> {
        self.java_session.parse_tree_origin_for_uri(uri)
    }

    fn index_source_files(
        &mut self,
        source_files: Vec<PathBuf>,
        name_table: Option<Arc<crate::index::NameTable>>,
    ) -> CodebaseIndex {
        let file_count = source_files.len();
        let source_inputs = load_source_inputs(source_files);
        let classes = self.index_source_inputs(source_inputs, name_table);

        tracing::info!(
            classes = classes.len(),
            files = file_count,
            "Codebase indexing complete"
        );

        CodebaseIndex {
            classes,
            file_count,
        }
    }

    fn index_source_inputs(
        &mut self,
        source_inputs: Vec<SourceTextInput>,
        name_table: Option<Arc<crate::index::NameTable>>,
    ) -> Vec<ClassMetadata> {
        let current_java_uris = source_inputs
            .iter()
            .filter(|source| source.language_id.as_ref() == "java")
            .map(|source| Arc::clone(&source.uri))
            .collect::<HashSet<_>>();
        self.java_session.prune_sources(&current_java_uris);

        tracing::debug!("discovering stubs...");
        let prepared_sources = self.java_session.prepare_sources(source_inputs);

        let discovered_names: Vec<Arc<str>> = prepared_sources
            .par_iter()
            .flat_map(|source| source.discover_internal_names())
            .collect();

        let discovered_names_len = discovered_names.len();
        let enriched_name_table = match name_table {
            Some(existing) => existing.extend_with(discovered_names),
            None => crate::index::NameTable::from_names(discovered_names),
        };

        tracing::debug!(
            discovered_names_len,
            enriched_name_table_len = enriched_name_table.len(),
            "full structural analysis",
        );
        prepared_sources
            .into_par_iter()
            .flat_map(|source| source.extract_classes(Some(enriched_name_table.clone())))
            .collect()
    }
}

/// Index the entire codebase directory
///
/// - Recursively scan all `.java` / `.kt` files under `root`
/// - Parallel parsing
/// - Skip directories such as `target/`, `build/`, `.git/`, etc.
pub fn index_codebase<P: AsRef<Path>>(
    root: P,
    name_table: Option<Arc<crate::index::NameTable>>,
) -> CodebaseIndex {
    CodebaseIndexSession::default().index_codebase(root, name_table)
}

pub fn index_codebase_paths<I>(
    roots: I,
    name_table: Option<Arc<crate::index::NameTable>>,
) -> CodebaseIndex
where
    I: IntoIterator<Item = PathBuf>,
{
    CodebaseIndexSession::default().index_codebase_paths(roots, name_table)
}

/// Parse source text from memory (for LSP textDocument/didChange)
pub fn index_source_text(
    uri: &str,
    content: &str,
    lang: &str,
    name_table: Option<Arc<crate::index::NameTable>>,
) -> Vec<ClassMetadata> {
    CodebaseIndexSession::default().index_source_text(uri, content, lang, name_table)
}

fn is_excluded_with_mode(path: &Path, mode: SourceScanMode) -> bool {
    path.components().any(|c| {
        let component = c.as_os_str().to_str().unwrap_or("");
        matches!(
            component,
            ".git"
                | ".gradle"
                | "node_modules"
                | ".idea"
                | "out"
                | "dist"
                | ".kotlin"
                | "__pycache__"
        ) || (component == "generated" && mode != SourceScanMode::IncludeGenerated)
            || (matches!(component, "build" | "target") && mode != SourceScanMode::IncludeGenerated)
    })
}

pub(crate) fn collect_source_files<I>(roots: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    roots
        .into_iter()
        .flat_map(|root| collect_source_files_for_root(root, SourceScanMode::Default))
        .collect()
}

pub(crate) fn collect_source_files_for_root(root: PathBuf, mode: SourceScanMode) -> Vec<PathBuf> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_excluded_with_mode(e.path(), mode))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| should_index_source_path(e.path(), mode))
        .map(|e| e.into_path())
        .collect()
}

pub(crate) fn should_index_source_path(path: &Path, mode: SourceScanMode) -> bool {
    if is_excluded_with_mode(path, mode) {
        return false;
    }

    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("java") | Some("kt")
    )
}

pub(crate) fn load_source_inputs(source_files: Vec<PathBuf>) -> Vec<SourceTextInput> {
    source_files
        .into_par_iter()
        .filter_map(load_source_input)
        .collect()
}

pub(crate) fn load_source_input(path: PathBuf) -> Option<SourceTextInput> {
    let content = std::fs::read_to_string(&path).ok()?;
    let uri = path_to_uri_str(&path);
    let origin = ClassOrigin::SourceFile(Arc::from(uri.as_str()));
    let language_id: Arc<str> = if path.extension().and_then(|s| s.to_str()) == Some("kt") {
        Arc::from("kotlin")
    } else {
        Arc::from("java")
    };
    Some(SourceTextInput::new(
        Arc::from(uri.as_str()),
        language_id,
        content,
        origin,
    ))
}

fn path_to_uri_str(path: &Path) -> String {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", abs.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::salsa_db::ParseTreeOrigin;
    use std::fs;
    use tempfile::TempDir;

    fn make_test_dir() -> TempDir {
        let dir = tempfile::tempdir().unwrap();

        fs::write(
            dir.path().join("Foo.java"),
            r#"
package com.example;
public class Foo {
    private String name;
    public String getName() { return name; }
    public void setName(String name) { this.name = name; }
    public static class Inner {}
}
"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("Bar.kt"),
            r#"
package com.example
class Bar(val value: String) {
    fun process(input: Int): String = ""
    companion object {
        fun create(): Bar = Bar("")
    }
}
"#,
        )
        .unwrap();

        // target/ dir: should be skipped
        fs::create_dir_all(dir.path().join("target")).unwrap();
        fs::write(
            dir.path().join("target/Ignored.java"),
            "package x; class Ignored {}",
        )
        .unwrap();

        dir
    }

    #[test]
    fn test_index_codebase_finds_files() {
        let dir = make_test_dir();
        let result = index_codebase(dir.path(), None);

        assert_eq!(
            result.file_count, 2,
            "should find 2 source files (not target/)"
        );
        assert!(
            result.classes.iter().any(|c| c.name.as_ref() == "Foo"),
            "classes: {:?}",
            result
                .classes
                .iter()
                .map(|c| c.name.as_ref())
                .collect::<Vec<_>>()
        );
        assert!(result.classes.iter().any(|c| c.name.as_ref() == "Bar"));
    }

    #[test]
    fn test_index_codebase_skips_excluded() {
        let dir = make_test_dir();
        let result = index_codebase(dir.path(), None);
        assert!(result.classes.iter().all(|c| c.name.as_ref() != "Ignored"));
    }

    #[test]
    fn test_index_codebase_package() {
        let dir = make_test_dir();
        let result = index_codebase(dir.path(), None);
        let foo = result
            .classes
            .iter()
            .find(|c| c.name.as_ref() == "Foo")
            .unwrap();
        assert_eq!(foo.package.as_deref(), Some("com/example"));
        assert_eq!(foo.internal_name.as_ref(), "com/example/Foo");
    }

    #[test]
    fn test_index_codebase_inner_class() {
        let dir = make_test_dir();
        let result = index_codebase(dir.path(), None);
        let inner = result.classes.iter().find(|c| c.name.as_ref() == "Inner");
        assert!(inner.is_some(), "Inner class should be indexed");
        assert_eq!(
            inner.unwrap().inner_class_of.as_deref(),
            Some("com/example/Foo")
        );
    }

    #[test]
    fn test_index_source_text_java() {
        let src = r#"
package org.test;
public class MyService {
    private int count;
    public void run() {}
    public int getCount() { return count; }
}
"#;
        let classes = index_source_text("file:///MyService.java", src, "java", None);
        assert_eq!(classes.len(), 1);
        let cls = &classes[0];
        assert_eq!(cls.name.as_ref(), "MyService");
        assert_eq!(cls.package.as_deref(), Some("org/test"));
        assert!(cls.methods.iter().any(|m| m.name.as_ref() == "run"));
        assert!(cls.methods.iter().any(|m| m.name.as_ref() == "getCount"));
        assert!(cls.fields.iter().any(|f| f.name.as_ref() == "count"));
    }

    #[test]
    fn test_index_source_text_kotlin() {
        let src = r#"
package org.test
class UserRepo(val db: String) {
    fun findById(id: Int): String = ""
    fun save(entity: String) {}
}
"#;
        let classes = index_source_text("file:///UserRepo.kt", src, "kotlin", None);
        assert!(
            classes.iter().any(|c| c.name.as_ref() == "UserRepo"),
            "classes: {:?}",
            classes.iter().map(|c| c.name.as_ref()).collect::<Vec<_>>()
        );
        let cls = classes
            .iter()
            .find(|c| c.name.as_ref() == "UserRepo")
            .unwrap();
        assert!(cls.methods.iter().any(|m| m.name.as_ref() == "findById"));
        assert!(cls.methods.iter().any(|m| m.name.as_ref() == "save"));
    }

    #[test]
    fn test_index_source_text_origin() {
        let src = "package x;\npublic class A {}";
        let uri = "file:///workspace/A.java";
        let classes = index_source_text(uri, src, "java", None);
        assert!(
            classes
                .iter()
                .all(|c| { matches!(&c.origin, ClassOrigin::SourceFile(u) if u.as_ref() == uri) })
        );
    }

    #[test]
    fn test_codebase_index_session_reuses_incremental_java_parse_tree() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Demo.java");
        fs::write(&path, "package org.test;\nclass Demo { int value; }\n").unwrap();

        let uri = path_to_uri_str(&path);
        let mut session = CodebaseIndexSession::default();

        let first = session.index_codebase(dir.path(), None);
        assert!(
            first
                .classes
                .iter()
                .any(|class| class.name.as_ref() == "Demo"),
            "classes: {:?}",
            first
                .classes
                .iter()
                .map(|class| class.name.as_ref())
                .collect::<Vec<_>>()
        );
        assert_eq!(session.java_parse_origin(&uri), Some(ParseTreeOrigin::Full));

        fs::write(
            &path,
            "package org.test;\nclass Demo { String value; int count; }\n",
        )
        .unwrap();

        let second = session.index_codebase(dir.path(), None);
        assert!(second.classes.iter().any(|class| {
            class
                .fields
                .iter()
                .any(|field| field.name.as_ref() == "count")
        }));
        assert_eq!(
            session.java_parse_origin(&uri),
            Some(ParseTreeOrigin::Incremental)
        );
    }

    #[test]
    fn test_codebase_index_session_reuses_java_source_text_by_uri() {
        let uri = "file:///workspace/Demo.java";
        let mut session = CodebaseIndexSession::default();

        let first = session.index_source_text(uri, "class Demo { int value; }", "java", None);
        assert_eq!(first.len(), 1);
        assert_eq!(session.java_parse_origin(uri), Some(ParseTreeOrigin::Full));

        let second =
            session.index_source_text(uri, "class Demo { int value; int count; }", "java", None);
        assert_eq!(second.len(), 1);
        assert_eq!(
            session.java_parse_origin(uri),
            Some(ParseTreeOrigin::Incremental)
        );
    }
}
