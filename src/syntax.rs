use std::sync::Arc;

use rowan::{GreenNode, GreenNodeBuilder};
use tree_sitter::Tree;

const TRIVIA_KIND_RAW: u16 = u16::MAX - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RawSyntaxKind(pub u16);

impl RawSyntaxKind {
    pub const TRIVIA: Self = Self(TRIVIA_KIND_RAW);

    pub fn is_trivia(self) -> bool {
        self == Self::TRIVIA
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnifiedLanguage {
    Java,
    Kotlin,
    Unknown,
}

impl UnifiedLanguage {
    pub fn from_language_id(language_id: &str) -> Self {
        match language_id {
            "java" => Self::Java,
            "kotlin" => Self::Kotlin,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Java => "java",
            Self::Kotlin => "kotlin",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnifiedLanguageTag;

impl rowan::Language for UnifiedLanguageTag {
    type Kind = RawSyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        RawSyntaxKind(raw.0)
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind.0)
    }
}

pub type SyntaxNode = rowan::SyntaxNode<UnifiedLanguageTag>;
pub type SyntaxToken = rowan::SyntaxToken<UnifiedLanguageTag>;
pub type SyntaxElement = rowan::SyntaxElement<UnifiedLanguageTag>;
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<UnifiedLanguageTag>;
pub type SyntaxElementChildren = rowan::SyntaxElementChildren<UnifiedLanguageTag>;
pub type TextRange = rowan::TextRange;
pub type TextSize = rowan::TextSize;

pub fn kind_name(raw: RawSyntaxKind) -> Option<&'static str> {
    if raw.is_trivia() {
        return Some("__trivia");
    }

    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    lang.node_kind_for_id(raw.0)
}

#[derive(Debug, Clone)]
pub struct SyntaxError {
    pub kind: Arc<str>,
    pub range: TextRange,
}

#[derive(Debug, Clone)]
pub struct SyntaxSnapshot {
    language: UnifiedLanguage,
    green: GreenNode,
    errors: Arc<[SyntaxError]>,
}

impl SyntaxSnapshot {
    pub fn from_tree(language_id: &str, text: &str, tree: &Tree) -> Self {
        let language = UnifiedLanguage::from_language_id(language_id);
        let mut builder = GreenNodeBuilder::new();
        let mut errors = Vec::new();
        build_green_root_from_ts_node(tree.root_node(), text, &mut builder, &mut errors);
        let green = builder.finish();

        Self {
            language,
            green,
            errors: errors.into(),
        }
    }

    pub fn language(&self) -> UnifiedLanguage {
        self.language
    }

    pub fn root(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    pub fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

fn build_green_root_from_ts_node(
    node: tree_sitter::Node,
    text: &str,
    builder: &mut GreenNodeBuilder<'_>,
    errors: &mut Vec<SyntaxError>,
) {
    builder.start_node(rowan::SyntaxKind(node.kind_id()));

    if node.is_error() || node.is_missing() {
        errors.push(SyntaxError {
            kind: Arc::from(node.kind()),
            range: TextRange::new(
                TextSize::from(node.start_byte() as u32),
                TextSize::from(node.end_byte() as u32),
            ),
        });
    }

    if node.start_byte() > 0 {
        builder.token(
            rowan::SyntaxKind(TRIVIA_KIND_RAW),
            &text[..node.start_byte()],
        );
    }

    let mut cursor = node.walk();
    let mut next_start = node.start_byte();
    for child in node.children(&mut cursor) {
        if next_start < child.start_byte() {
            builder.token(
                rowan::SyntaxKind(TRIVIA_KIND_RAW),
                &text[next_start..child.start_byte()],
            );
        }
        build_green_from_ts_node(child, text, builder, errors);
        next_start = child.end_byte();
    }

    if next_start < node.end_byte() {
        builder.token(
            rowan::SyntaxKind(TRIVIA_KIND_RAW),
            &text[next_start..node.end_byte()],
        );
    }

    if node.end_byte() < text.len() {
        builder.token(rowan::SyntaxKind(TRIVIA_KIND_RAW), &text[node.end_byte()..]);
    }

    builder.finish_node();
}

fn build_green_from_ts_node(
    node: tree_sitter::Node,
    text: &str,
    builder: &mut GreenNodeBuilder<'_>,
    errors: &mut Vec<SyntaxError>,
) {
    if node.child_count() == 0 {
        let token_text = &text[node.start_byte()..node.end_byte()];
        builder.token(rowan::SyntaxKind(node.kind_id()), token_text);

        if node.is_error() || node.is_missing() {
            errors.push(SyntaxError {
                kind: Arc::from(node.kind()),
                range: TextRange::new(
                    TextSize::from(node.start_byte() as u32),
                    TextSize::from(node.end_byte() as u32),
                ),
            });
        }
        return;
    }

    builder.start_node(rowan::SyntaxKind(node.kind_id()));

    if node.is_error() || node.is_missing() {
        errors.push(SyntaxError {
            kind: Arc::from(node.kind()),
            range: TextRange::new(
                TextSize::from(node.start_byte() as u32),
                TextSize::from(node.end_byte() as u32),
            ),
        });
    }

    let mut cursor = node.walk();
    let mut next_start = node.start_byte();
    for child in node.children(&mut cursor) {
        if next_start < child.start_byte() {
            builder.token(
                rowan::SyntaxKind(TRIVIA_KIND_RAW),
                &text[next_start..child.start_byte()],
            );
        }
        build_green_from_ts_node(child, text, builder, errors);
        next_start = child.end_byte();
    }

    if next_start < node.end_byte() {
        builder.token(
            rowan::SyntaxKind(TRIVIA_KIND_RAW),
            &text[next_start..node.end_byte()],
        );
    }

    builder.finish_node();
}

pub mod java {
    use super::{RawSyntaxKind, SyntaxElement, SyntaxNode, SyntaxToken};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum JavaSyntaxKind {
        Program,
        PackageDeclaration,
        ImportDeclaration,
        ScopedIdentifier,
        Identifier,
        Asterisk,
        Dot,
        Semicolon,
        Package,
        Import,
        Static,
        Comment,
        LineComment,
        BlockComment,
        Trivia,
        Error,
        Unknown(RawSyntaxKind),
    }

    impl JavaSyntaxKind {
        pub fn from_raw(raw: RawSyntaxKind) -> Self {
            match super::kind_name(raw) {
                Some("program") => Self::Program,
                Some("package_declaration") => Self::PackageDeclaration,
                Some("import_declaration") => Self::ImportDeclaration,
                Some("scoped_identifier") => Self::ScopedIdentifier,
                Some("identifier") => Self::Identifier,
                Some("*") => Self::Asterisk,
                Some(".") => Self::Dot,
                Some(";") => Self::Semicolon,
                Some("package") => Self::Package,
                Some("import") => Self::Import,
                Some("static") => Self::Static,
                Some("comment") => Self::Comment,
                Some("line_comment") => Self::LineComment,
                Some("block_comment") => Self::BlockComment,
                Some("ERROR") => Self::Error,
                Some("__trivia") => Self::Trivia,
                _ => Self::Unknown(raw),
            }
        }
    }

    pub trait AstNode: Sized {
        fn can_cast(kind: JavaSyntaxKind) -> bool;
        fn cast(node: SyntaxNode) -> Option<Self>;
        fn syntax(&self) -> &SyntaxNode;
    }

    pub trait AstToken: Sized {
        fn can_cast(kind: JavaSyntaxKind) -> bool;
        fn cast(token: SyntaxToken) -> Option<Self>;
        fn syntax(&self) -> &SyntaxToken;
    }

    #[derive(Debug, Clone)]
    pub struct JavaFile {
        syntax: SyntaxNode,
    }

    impl AstNode for JavaFile {
        fn can_cast(kind: JavaSyntaxKind) -> bool {
            kind == JavaSyntaxKind::Program
        }

        fn cast(node: SyntaxNode) -> Option<Self> {
            Self::can_cast(JavaSyntaxKind::from_raw(node.kind())).then_some(Self { syntax: node })
        }

        fn syntax(&self) -> &SyntaxNode {
            &self.syntax
        }
    }

    impl JavaFile {
        pub fn package(&self) -> Option<PackageDecl> {
            self.syntax.children().find_map(PackageDecl::cast)
        }

        pub fn imports(&self) -> impl Iterator<Item = ImportDecl> + '_ {
            self.syntax.children().filter_map(ImportDecl::cast)
        }
    }

    #[derive(Debug, Clone)]
    pub struct PackageDecl {
        syntax: SyntaxNode,
    }

    impl AstNode for PackageDecl {
        fn can_cast(kind: JavaSyntaxKind) -> bool {
            kind == JavaSyntaxKind::PackageDeclaration
        }

        fn cast(node: SyntaxNode) -> Option<Self> {
            Self::can_cast(JavaSyntaxKind::from_raw(node.kind())).then_some(Self { syntax: node })
        }

        fn syntax(&self) -> &SyntaxNode {
            &self.syntax
        }
    }

    impl PackageDecl {
        pub fn path_text(&self) -> Option<String> {
            path_text_from_children(&self.syntax, false).map(|s| s.replace('.', "/"))
        }
    }

    #[derive(Debug, Clone)]
    pub struct ImportDecl {
        syntax: SyntaxNode,
    }

    impl AstNode for ImportDecl {
        fn can_cast(kind: JavaSyntaxKind) -> bool {
            kind == JavaSyntaxKind::ImportDeclaration
        }

        fn cast(node: SyntaxNode) -> Option<Self> {
            Self::can_cast(JavaSyntaxKind::from_raw(node.kind())).then_some(Self { syntax: node })
        }

        fn syntax(&self) -> &SyntaxNode {
            &self.syntax
        }
    }

    impl ImportDecl {
        pub fn is_static(&self) -> bool {
            self.syntax
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .any(|tok| JavaSyntaxKind::from_raw(tok.kind()) == JavaSyntaxKind::Static)
        }

        pub fn path_text(&self) -> Option<String> {
            path_text_from_children(&self.syntax, true)
        }
    }

    #[derive(Debug, Clone)]
    pub struct IdentifierToken {
        syntax: SyntaxToken,
    }

    impl AstToken for IdentifierToken {
        fn can_cast(kind: JavaSyntaxKind) -> bool {
            matches!(
                kind,
                JavaSyntaxKind::Identifier | JavaSyntaxKind::ScopedIdentifier
            )
        }

        fn cast(token: SyntaxToken) -> Option<Self> {
            Self::can_cast(JavaSyntaxKind::from_raw(token.kind())).then_some(Self { syntax: token })
        }

        fn syntax(&self) -> &SyntaxToken {
            &self.syntax
        }
    }

    fn path_text_from_children(node: &SyntaxNode, include_asterisk: bool) -> Option<String> {
        let mut out = String::new();

        for element in node.children_with_tokens() {
            match element {
                SyntaxElement::Node(child) => match JavaSyntaxKind::from_raw(child.kind()) {
                    JavaSyntaxKind::ScopedIdentifier => out.push_str(&child.text().to_string()),
                    JavaSyntaxKind::Identifier => out.push_str(&child.text().to_string()),
                    JavaSyntaxKind::Unknown(_) if include_asterisk && child.text() == "*" => {
                        out.push('*')
                    }
                    _ => {}
                },
                SyntaxElement::Token(token) => match JavaSyntaxKind::from_raw(token.kind()) {
                    JavaSyntaxKind::Identifier => out.push_str(token.text()),
                    JavaSyntaxKind::Dot => out.push('.'),
                    JavaSyntaxKind::Asterisk if include_asterisk => out.push('*'),
                    JavaSyntaxKind::Unknown(_) if include_asterisk && token.text() == "*" => {
                        out.push('*')
                    }
                    _ => {}
                },
            }
        }

        if out.is_empty() { None } else { Some(out) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::java::{AstNode, ImportDecl, JavaFile};

    fn parse_java_snapshot(text: &str) -> SyntaxSnapshot {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(text, None).unwrap();
        SyntaxSnapshot::from_tree("java", text, &tree)
    }

    #[test]
    fn builds_rowan_snapshot_from_java_tree() {
        let text = "class Test { int x; }";
        let snapshot = parse_java_snapshot(text);

        assert_eq!(snapshot.language().as_str(), "java");
        assert_eq!(snapshot.root().text().to_string(), text);
    }

    #[test]
    fn java_file_extracts_package_and_imports() {
        let text = "package com.example.test;\nimport java.util.List;\nimport static java.util.Collections.*;\nclass Test {}";
        let snapshot = parse_java_snapshot(text);
        let file = JavaFile::cast(snapshot.root()).unwrap();

        assert_eq!(
            file.package().and_then(|pkg| pkg.path_text()).as_deref(),
            Some("com/example/test")
        );

        let imports: Vec<ImportDecl> = file.imports().collect();
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].path_text().as_deref(), Some("java.util.List"));
        assert!(!imports[0].is_static());
        assert_eq!(
            imports[1].path_text().as_deref(),
            Some("java.util.Collections.*")
        );
        assert!(imports[1].is_static());
    }

    #[test]
    fn java_file_handles_missing_semicolon_import_during_edit() {
        let text = "package demo\nimport java.util.List\nclass Test {}";
        let snapshot = parse_java_snapshot(text);
        let file = JavaFile::cast(snapshot.root()).unwrap();

        assert_eq!(
            file.package().and_then(|pkg| pkg.path_text()).as_deref(),
            Some("demo")
        );

        let imports: Vec<_> = file.imports().collect();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].path_text().as_deref(), Some("java.util.List"));
    }

    #[test]
    fn syntax_snapshot_preserves_leading_trivia() {
        let text = "  \npackage demo;\nclass Test {}";
        let snapshot = parse_java_snapshot(text);

        assert_eq!(snapshot.root().text().to_string(), text);
        assert!(snapshot.root().children_with_tokens().next().is_some());
    }

    #[test]
    fn java_file_ignores_static_imports_in_legacy_extraction_path() {
        let imports = crate::language::java::class_parser::extract_imports_from_source(
            "import java.util.List;\nimport static java.util.Collections.emptyList;\nclass Test {}",
        );

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].as_ref(), "java.util.List");
    }

    #[test]
    fn java_package_extraction_recovers_without_semicolon() {
        let package = crate::language::java::class_parser::extract_package_from_source(
            "package com.example\nclass Test {}",
        );

        assert_eq!(package.as_deref(), Some("com/example"));
    }
}
