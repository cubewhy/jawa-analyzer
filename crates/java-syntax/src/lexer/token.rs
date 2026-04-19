use crate::kinds::SyntaxKind;

#[derive(Debug, Clone, Copy)]
pub struct Token<'source> {
    pub kind: SyntaxKind,
    pub lexeme: &'source str,
    pub offset: usize, // the start position of the token
}

impl<'s> Token<'s> {
    pub fn new(kind: SyntaxKind, lexeme: &'s str, offset: usize) -> Self {
        Self {
            kind,
            lexeme,
            offset,
        }
    }
}
