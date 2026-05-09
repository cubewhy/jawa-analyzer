pub(crate) mod lexer;
pub(crate) mod syntax_kind;

pub use lexer::{Lexer, LexicalError, token::Token};
pub use syntax_kind::SyntaxKind;
