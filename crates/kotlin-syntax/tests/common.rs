#![allow(unused)]
use kotlin_syntax::{LexicalError, Token, lex};

use rowan::SyntaxNode;

pub fn dump_tokens(tokens: &[Token<'_>]) -> String {
    let mut out = String::new();
    for tok in tokens {
        out.push_str(&format!("{:?} {:?}\n", tok.kind, tok.lexeme));
    }
    out
}

pub fn dump_lex_errors(errors: &[LexicalError]) -> String {
    if errors.is_empty() {
        return "<none>\n".to_string();
    }

    let mut out = String::new();
    for err in errors {
        out.push_str(&format!("{err:?}\n"));
    }
    out
}

pub fn check_lexer(src: &str) -> String {
    let (tokens, lex_errors) = lex(src);
    format!(
        "\
SOURCE:
{src}
TOKENS:
{}
LEX_ERRORS:
{}",
        dump_tokens(&tokens),
        dump_lex_errors(&lex_errors),
    )
}

pub fn check_lexer_ok(src: &str) -> String {
    let (tokens, lex_errors) = lex(src);
    assert!(
        lex_errors.is_empty(),
        "lexing failed unexpectedly for input:\n{src}\n\nTOKENS:\n{}LEX_ERRORS:\n{}",
        dump_tokens(&tokens),
        dump_lex_errors(&lex_errors),
    );

    format!(
        "\
SOURCE:
{src}
TOKENS:
{}
LEX_ERRORS:
{}",
        dump_tokens(&tokens),
        dump_lex_errors(&lex_errors),
    )
}

pub fn check_lexer_error(src: &str) -> String {
    let (tokens, lex_errors) = lex(src);
    assert!(
        !lex_errors.is_empty(),
        "expected lexing to fail, but it succeeded for input:\n{src}\n\nTOKENS:\n{}",
        dump_tokens(&tokens),
    );

    format!(
        "\
SOURCE:
{src}
TOKENS:
{}
LEX_ERRORS:
{}",
        dump_tokens(&tokens),
        dump_lex_errors(&lex_errors),
    )
}

pub fn parse_edit_markers(src: &str) -> (String, usize, usize) {
    let mut clean_src = String::with_capacity(src.len());
    let mut start = 0;
    let mut end = 0;
    let mut markers_found = 0;

    for c in src.chars() {
        if c == '§' {
            if markers_found == 0 {
                start = clean_src.len();
                end = start;
            } else if markers_found == 1 {
                end = clean_src.len();
            }
            markers_found += 1;
        } else {
            clean_src.push(c);
        }
    }

    (clean_src, start, end)
}

macro_rules! lexer_error_snapshot {
    ($name:ident, $src:expr $(,)?) => {
        #[test]
        fn $name() {
            let out = crate::common::check_lexer_error($src);
            insta::assert_snapshot!(stringify!($name), out);
        }
    };
}

macro_rules! lexer_snapshot {
    ($name:ident, $src:expr $(,)?) => {
        #[test]
        fn $name() {
            let out = crate::common::check_lexer($src);
            insta::assert_snapshot!(stringify!($name), out);
        }
    };
}

pub(crate) use lexer_error_snapshot;
pub(crate) use lexer_snapshot;
