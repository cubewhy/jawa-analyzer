use rowan::TextSize;

use crate::syntax_kind::SyntaxKind;

pub struct Token<'a> {
    pub kind: SyntaxKind,
    pub lexeme: &'a str,
    pub offset: TextSize,
}

impl<'s> Token<'s> {
    pub fn new(kind: SyntaxKind, lexeme: &'s str, offset: usize) -> Self {
        Self {
            kind,
            lexeme,
            offset: TextSize::new(offset as u32),
        }
    }
}
