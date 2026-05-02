use std::cell::RefCell;

use rowan::{GreenNode, NodeCache, TextRange};

use crate::{FileText, LanguageId, SourceDatabase};

thread_local! {
    static SYNTAX_CACHE: RefCell<NodeCache> = RefCell::new(NodeCache::default());
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SyntaxError {
    pub message: String,
    pub range: TextRange,
}

// TODO: friendly error message
impl SyntaxError {
    fn from_java_lexer(lex_err: &java_syntax::LexicalError) -> Self {
        let message = match &lex_err.kind {
            java_syntax::LexicalErrorKind::UnexpectedChar(c) => format!("Unexpected {c}"),
            java_syntax::LexicalErrorKind::UnterminatedString => "Unterminated string".to_string(),
            java_syntax::LexicalErrorKind::UnterminatedComment => {
                "Unterminated comment".to_string()
            }
            java_syntax::LexicalErrorKind::InvalidChar => "Invalid char".to_string(),
            java_syntax::LexicalErrorKind::IllegalTextBlockOpen => {
                "Expected newline after \"\"\"".to_string()
            }
            java_syntax::LexicalErrorKind::UnterminatedTextBlock => {
                "Unterminated text block".to_string()
            }
            java_syntax::LexicalErrorKind::InvalidNumber => "Invalid number".to_string(),
            java_syntax::LexicalErrorKind::InvalidUnicodeEscape => {
                "Invalid unicode escape".to_string()
            }
            java_syntax::LexicalErrorKind::UnterminatedChar => {
                "Unterminated char literal".to_string()
            }
            java_syntax::LexicalErrorKind::InvalidEscapeSequence => "Invalid escape".to_string(),
            java_syntax::LexicalErrorKind::UnterminatedTemplate => {
                "Unterminated string template".to_string()
            }
        };

        Self {
            message,
            range: lex_err.range,
        }
    }

    fn from_java_parser(parse_err: &java_syntax::ParseError) -> Self {
        let message = match &parse_err.kind {
            java_syntax::ParseErrorKind::ExpectedToken {
                expected,
                found: __found,
            } => {
                if expected.len() == 1 {
                    let expected_token = expected.first().unwrap();
                    format!("Expect {:?}", expected_token)
                } else {
                    format!(
                        "Expect [{}]",
                        expected
                            .iter()
                            .map(|kind| format!("{kind:?}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            java_syntax::ParseErrorKind::ExpectedContextualKeyword { keyword, found } => {
                format!(
                    "Expect keyword {}, found {}",
                    keyword.as_str(),
                    found
                        .map(|kind| format!("{kind:?}"))
                        .unwrap_or_else(|| "EOF".to_string())
                )
            }
            java_syntax::ParseErrorKind::ExpectedConstruct(expected_construct) => {
                format!("Expect construct: {expected_construct:?}")
            }
            java_syntax::ParseErrorKind::Message(msg) => msg.to_string(),
        };

        Self {
            message,
            range: parse_err.range,
        }
    }
}

#[salsa::tracked]
pub struct ParseResult<'db> {
    pub green_node: GreenNode,
    pub errors: Vec<SyntaxError>,
}

#[salsa::tracked]
pub fn parse_node(db: &dyn SourceDatabase, file_text: FileText) -> Option<ParseResult<'_>> {
    let language_id = file_text.language(db);
    match language_id {
        LanguageId::Java => Some(parse_java_node(db, file_text)),
        LanguageId::Kotlin => None,
        LanguageId::Unknown => None,
    }
}

#[salsa::tracked]
pub fn parse_java_node(db: &dyn SourceDatabase, file_text: FileText) -> ParseResult<'_> {
    let content = file_text.text(db);

    let mut errors = Vec::new();
    let (tokens, lex_errors) = java_syntax::lex(content);

    let output = SYNTAX_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let parser = java_syntax::Parser::new(tokens);

        parser.parse_with_cache(Some(&mut cache))
    });

    for lex_err in lex_errors {
        errors.push(SyntaxError::from_java_lexer(&lex_err));
    }

    let parse_errors = output.errors();
    for parse_err in parse_errors {
        errors.push(SyntaxError::from_java_parser(parse_err));
    }

    ParseResult::new(db, output.into_green_node(), errors)
}
