use rowan::{TextRange, TextSize};

use crate::{
    SyntaxKind::{self, *},
    lexer::{
        identifier::{is_kotlin_identifier_part, is_kotlin_identifier_start, is_kotlin_newline},
        reader::SourceReader,
        token::Token,
    },
};

mod identifier;
mod reader;
pub mod token;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LexerMode {
    Default,
    String { is_raw: bool },
}

pub struct Lexer<'a> {
    reader: SourceReader<'a>,
    tokens: Vec<Token<'a>>,
    errors: Vec<LexicalError>,
    mode_stack: Vec<LexerMode>,
    template_brace_depths: Vec<usize>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            reader: SourceReader::new(source),
            tokens: Vec::new(),
            errors: Vec::new(),

            mode_stack: vec![LexerMode::Default],
            template_brace_depths: Vec::new(),
        }
    }

    fn complete_token(&mut self, kind: SyntaxKind) {
        self.tokens.push(Token::new(
            kind,
            self.reader.current_lexeme(),
            self.reader.start(),
        ));
    }

    fn report_error(&mut self, kind: LexicalErrorKind) {
        let start = TextSize::from(self.reader.start() as u32);
        let end = TextSize::from(self.reader.current() as u32);
        let error = LexicalError::new(kind, TextRange::new(start, end));
        self.errors.push(error);
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
        self.reader.new_token();

        let current_mode = *self.mode_stack.last().unwrap_or(&LexerMode::Default);

        match current_mode {
            LexerMode::Default => self.scan_default_mode(),
            LexerMode::String { is_raw } => self.scan_string_mode(is_raw),
        }
    }

    fn scan_default_mode(&mut self) {
        match self.reader.peek() {
            '{' => {
                self.reader.advance(); // {
                // If we are tracking template depths, increment the top one
                if let Some(depth) = self.template_brace_depths.last_mut() {
                    *depth += 1;
                }
                self.complete_token(L_BRACE);
            }
            '}' => {
                self.reader.advance(); // }
                self.complete_token(R_BRACE);

                // Check if this '}' closes a string template
                if let Some(depth) = self.template_brace_depths.last_mut() {
                    *depth -= 1;
                    if *depth == 0 {
                        // Template is done! Pop the depth and return to String mode
                        self.template_brace_depths.pop();
                        self.mode_stack.pop();
                    }
                }
            }
            '"' => {
                // Check for Raw String (""")
                if self.reader.peek_next() == '"' && self.reader.peek_n(2) == '"' {
                    self.reader.advance();
                    self.reader.advance();
                    self.reader.advance();
                    self.mode_stack.push(LexerMode::String { is_raw: true });
                    self.complete_token(OPEN_RAW_QUOTE); // """
                } else {
                    // Standard String (")
                    self.reader.advance();
                    self.mode_stack.push(LexerMode::String { is_raw: false });
                    self.complete_token(OPEN_QUOTE); // "
                }
            }
            '\'' => self.handle_char_literal(),
            '+' => self.handle_plus(),
            '-' => self.handle_minus(),
            '*' => self.handle_star(),
            '/' => self.handle_slash(),
            '%' => self.handle_modulo(),
            '`' => self.handle_backtick_identifier(),
            c if is_kotlin_newline(c) => self.handle_newline(),
            ' ' | '\t' => self.handle_horizontal_whitespace(),
            c if is_kotlin_identifier_start(c) => self.handle_identifier(),
            _ => match self.reader.advance() {
                '(' => self.complete_token(L_PAREN),
                ')' => self.complete_token(R_PAREN),
                '[' => self.complete_token(L_BRACKET),
                ']' => self.complete_token(R_BRACKET),
                '.' => self.complete_token(DOT),
                ',' => self.complete_token(COMMA),
                ';' => self.complete_token(SEMICOLON),
                '$' => self.complete_token(DOLLAR),
                '@' => self.complete_token(AT),

                c => {
                    self.report_error(LexicalErrorKind::UnexpectedChar(c));
                    self.reader.advance();
                }
            },
        }
    }

    fn scan_string_mode(&mut self, is_raw: bool) {
        if self.reader.is_at_end() {
            self.report_error(LexicalErrorKind::UnterminatedString);
            self.mode_stack.pop();
            return;
        }

        let c = self.reader.peek();

        // Handle Closing Quotes
        if is_raw {
            if c == '"' && self.reader.peek_next() == '"' && self.reader.peek_n(2) == '"' {
                self.reader.advance();
                self.reader.advance();
                self.reader.advance();
                self.mode_stack.pop();
                self.complete_token(CLOSE_RAW_QUOTE);
                return;
            }
        } else {
            if c == '"' {
                self.reader.advance();
                self.mode_stack.pop();
                self.complete_token(CLOSE_QUOTE);
                return;
            }
            if is_kotlin_newline(c) {
                self.report_error(LexicalErrorKind::UnterminatedString);
                self.mode_stack.pop(); // End string mode to recover
                return;
            }
        }

        // Handle Escape Sequences (Only in non-raw strings)
        if !is_raw && c == '\\' {
            self.reader.advance(); // \
            match self.reader.peek() {
                't' | 'b' | 'n' | 'r' | '\'' | '"' | '\\' | '$' => {
                    self.reader.advance(); // Consume valid escape
                    self.complete_token(ESCAPE_SEQUENCE);
                }
                _ => {
                    // Unsupported escape sequence error!
                    self.report_error(LexicalErrorKind::UnsupportedEscapeSequence);
                    if !self.reader.is_at_end() {
                        self.reader.advance(); // Consume invalid char to recover
                    }
                    // Still complete it as an escape sequence to avoid AST gaps
                    self.complete_token(ESCAPE_SEQUENCE);
                }
            }
            return;
        }

        // Handle String Templates
        if c == '$' {
            self.reader.advance();

            if self.reader.peek() == '{' {
                // Long Template: ${...}
                self.reader.advance(); // {
                self.complete_token(TEMPLATE_EXPR_START); // Emits `${`

                // Push default mode so the lexer processes normal code next
                self.mode_stack.push(LexerMode::Default);
                // Start tracking braces: we start at depth 1 because we just consumed '{'
                self.template_brace_depths.push(1);
                return;
            } else if is_kotlin_identifier_start(self.reader.peek()) {
                // Short Template: $identifier
                self.complete_token(TEMPLATE_SHORT_START); // Emits `$`

                // identifier after '$'
                self.reader.new_token();
                while !self.reader.is_at_end() && is_kotlin_identifier_part(self.reader.peek()) {
                    self.reader.advance();
                }
                self.complete_token(IDENTIFIER);
                return;
            }
            // If it's a lone '$' (like "$ "), it falls through to become normal text
        }

        // Handle Standard String Text
        // Consume characters until we hit a delimiter (Quote, Escape, or Template)
        while !self.reader.is_at_end() {
            let next = self.reader.peek();

            let hit_raw_end = is_raw
                && next == '"'
                && self.reader.peek_next() == '"'
                && self.reader.peek_n(2) == '"';
            let hit_std_end = !is_raw && (next == '"' || is_kotlin_newline(next) || next == '\\');

            if hit_raw_end || hit_std_end || next == '$' {
                break;
            }
            self.reader.advance();
        }

        self.complete_token(STRING_CONTENT);
    }

    fn handle_horizontal_whitespace(&mut self) {
        while !self.reader.is_at_end() {
            let c = self.reader.peek();
            if c == ' ' || c == '\t' {
                self.reader.advance();
            } else {
                break;
            }
        }
        self.complete_token(WHITESPACE);
    }

    fn handle_newline(&mut self) {
        while !self.reader.is_at_end() {
            let c = self.reader.peek();
            if c == '\n' {
                self.reader.advance();
            } else if c == '\r' {
                self.reader.advance();
                // Handle CRLF: peek for \n after \r
                if self.reader.peek() == '\n' {
                    self.reader.advance();
                }
            } else {
                break;
            }
        }
        self.complete_token(NEWLINE);
    }

    fn handle_identifier(&mut self) {
        while !self.reader.is_at_end() && is_kotlin_identifier_part(self.reader.peek()) {
            self.reader.advance(); // consume next char
        }

        let text = self.reader.current_lexeme();
        let token_type = match text {
            "as" => AS_KW,
            "break" => BREAK_KW,
            "class" => CONTINUE_KW,
            "do" => DO_KW,
            "if" => IF_KW,
            "else" => ELSE_KW,
            "false" => FALSE_KW,
            "fun" => FUN_KW,
            "in" => IN_KW,
            "interface" => INTERFACE_KW,
            "null" => NULL_KW,
            "object" => OBJECT_KW,
            "package" => PACKAGE_KW,
            "return" => RETURN_KW,
            "super" => SUPER_KW,
            "this" => THIS_KW,
            "throw" => THROW_KW,
            "true" => TRUE_KW,
            "try" => TRY_KW,
            "typealias" => TYPEALIAS_KW,
            "typeof" => TYPEOF_KW,
            "val" => VAL_KW,
            "var" => VAR_KW,
            "when" => WHEN_KW,
            "while" => WHILE_KW,
            "_" => UNDERSCORE,

            _ => IDENTIFIER,
        };

        self.complete_token(token_type);
    }

    fn handle_backtick_identifier(&mut self) {
        self.reader.advance(); // `

        // Check for empty backticks (``) which are invalid in Kotlin
        if self.reader.peek() == '`' {
            self.report_error(LexicalErrorKind::EmptyIdentifier);
            self.reader.advance();
            self.complete_token(IDENTIFIER);
            return;
        }

        while !self.reader.is_at_end() && self.reader.peek() != '`' {
            if is_kotlin_newline(self.reader.peek()) {
                // Backtick identifiers cannot span multiple lines
                self.report_error(LexicalErrorKind::UnterminatedIdentifier);
                break;
            }
            self.reader.advance();
        }

        if self.reader.is_at_end() {
            self.report_error(LexicalErrorKind::UnterminatedIdentifier);
        } else if self.reader.peek() == '`' {
            self.reader.advance(); // `
        }

        self.complete_token(IDENTIFIER);
    }

    fn handle_char_literal(&mut self) {
        self.reader.advance(); // Consume the opening '\''

        // Handle empty char literal error: ''
        if self.reader.peek() == '\'' {
            self.report_error(LexicalErrorKind::EmptyCharLiteral);
            self.reader.advance(); // Consume the closing '\''
            self.complete_token(CHAR_LITERAL);
            return;
        }

        // Handle the contents of the char literal
        if !self.reader.is_at_end() {
            if self.reader.peek() == '\\' {
                self.reader.advance(); // Consume '\'
                if !self.reader.is_at_end() {
                    self.reader.advance(); // Consume the escaped char (e.g., 'n', 't', '\'')
                }
            } else {
                self.reader.advance(); // Consume the standard char
            }
        }

        // Expect a closing single quote
        if self.reader.peek() == '\'' {
            self.reader.advance(); // Consume the closing '\''
        } else {
            self.report_error(LexicalErrorKind::UnterminatedCharLiteral);

            // Optional: Keep advancing until we find a closing quote or whitespace
            // to recover gracefully and prevent cascade errors.
            while !self.reader.is_at_end()
                && self.reader.peek() != '\''
                && self.reader.peek() != ' '
            {
                self.reader.advance();
            }
            if self.reader.peek() == '\'' {
                self.reader.advance();
            }
        }

        self.complete_token(CHAR_LITERAL);
    }

    fn handle_plus(&mut self) {
        self.reader.advance(); // +

        let c = self.reader.peek();

        let token_kind = match c {
            '+' => {
                // ++
                self.reader.advance();
                PLUS_PLUS
            }
            '=' => {
                // +=
                self.reader.advance();
                PLUS_EQUAL
            }
            _ => PLUS,
        };

        self.complete_token(token_kind);
    }

    fn handle_minus(&mut self) {
        self.reader.advance(); // -

        let c = self.reader.peek();

        let token_kind = match c {
            '-' => {
                // --
                self.reader.advance();
                MINUS_MINUS
            }
            '=' => {
                // -=
                self.reader.advance();
                MINUS_EQUAL
            }
            _ => MINUS,
        };

        self.complete_token(token_kind);
    }

    fn handle_star(&mut self) {
        self.reader.advance(); // *

        let c = self.reader.peek();

        let token_kind = match c {
            '=' => {
                // *=
                self.reader.advance();
                MUL_EQUAL
            }
            _ => STAR,
        };

        self.complete_token(token_kind);
    }

    fn handle_modulo(&mut self) {
        self.reader.advance(); // %

        let c = self.reader.peek();

        let token_kind = match c {
            '=' => {
                // %=
                self.reader.advance();
                MODULO_EQUAL
            }
            _ => MODULO,
        };

        self.complete_token(token_kind);
    }

    fn handle_slash(&mut self) {
        self.reader.advance(); // consume the first '/'

        match self.reader.peek() {
            '/' => {
                self.reader.advance(); // consume the second '/'
                self.process_line_comment();
            }
            '*' => {
                self.reader.advance(); // consume the '*'
                self.process_block_comment();
            }
            '=' => {
                self.reader.advance(); // consume the '='
                self.complete_token(DIV_EQUAL);
            }
            _ => {
                self.complete_token(SLASH);
            }
        }
    }

    fn process_line_comment(&mut self) {
        while !self.reader.is_at_end() && self.reader.peek() != '\n' {
            self.reader.advance();
        }

        self.complete_token(LINE_COMMENT);
    }

    fn process_block_comment(&mut self) {
        let mut depth = 1;
        let mut is_kdoc = false;

        // Check if this is a KDoc comment (starts with /**)
        if self.reader.peek() == '*' {
            self.reader.advance(); // Consume the second '*'

            // Handle edge case where the comment is exactly `/**/`
            if self.reader.peek() == '/' {
                self.reader.advance();
                depth -= 1; // It closes immediately
            } else {
                is_kdoc = true;
            }
        }

        // Process the rest of the comment, accounting for nesting
        while depth > 0 && !self.reader.is_at_end() {
            match self.reader.peek() {
                '/' => {
                    self.reader.advance();
                    if self.reader.peek() == '*' {
                        self.reader.advance();
                        depth += 1; // Entering a nested block comment
                    }
                }
                '*' => {
                    self.reader.advance();
                    if self.reader.peek() == '/' {
                        self.reader.advance();
                        depth -= 1; // Exiting a block comment
                    }
                }
                _ => {
                    // Standard character inside a comment, just advance
                    self.reader.advance();
                }
            }
        }

        // If we hit EOF before depth reaches 0, the comment wasn't closed
        if depth > 0 {
            // You'll need to add UnterminatedBlockComment to your LexicalErrorKind enum
            self.report_error(LexicalErrorKind::UnterminatedBlockComment);
        }

        if is_kdoc {
            self.complete_token(KDOC);
        } else {
            self.complete_token(BLOCK_COMMENT);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LexicalError {
    pub kind: LexicalErrorKind,
    pub range: TextRange,
}

impl LexicalError {
    pub fn new(kind: LexicalErrorKind, range: TextRange) -> Self {
        Self { kind, range }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LexicalErrorKind {
    UnterminatedBlockComment,
    UnterminatedString,
    EmptyCharLiteral,
    UnterminatedCharLiteral,
    UnsupportedEscapeSequence,
    EmptyIdentifier,
    UnterminatedIdentifier,
    UnexpectedChar(char),
}

pub fn lex(src: &str) -> (Vec<Token<'_>>, Vec<LexicalError>) {
    Lexer::new(src).scan_tokens()
}
