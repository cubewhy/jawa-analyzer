pub(crate) mod kinds;
pub(crate) mod lexer;
pub(crate) mod parser;
pub(crate) mod reader;

pub use kinds::SyntaxKind as JavaSyntaxKind;
pub use lexer::{
    Lexer as JavaLexer, LexicalError as JavaLexicalError, LexicalErrorKind as JavaLexicalErrorType,
};
pub use parser::Lang as JavaLanguage;
