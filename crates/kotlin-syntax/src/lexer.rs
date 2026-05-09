use rowan::TextRange;

use crate::lexer::{reader::SourceReader, token::Token};

mod reader;
pub mod token;

pub struct Lexer<'a> {
    reader: SourceReader<'a>,
    tokens: Vec<Token<'a>>,
    errors: Vec<LexicalError>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            reader: SourceReader::new(source),
            tokens: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn scan_tokens(mut self) -> (Vec<Token<'a>>, Vec<LexicalError>) {
        if self.reader.peek() == '\u{FEFF}' {
            self.reader.advance();
        }

        while !self.reader.is_at_end() {
            self.scan_next_token();
        }

        (self.tokens, self.errors)
    }

    fn scan_next_token(&mut self) {
        self.reader.start();

        // TODO: kotlin lexer
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LexicalError {
    pub kind: LexicalErrorKind,
    pub range: TextRange,
}

impl LexicalError {
    pub fn new(error_type: LexicalErrorKind, range: TextRange) -> Self {
        Self {
            kind: error_type,
            range,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LexicalErrorKind {}
