use std::{fs::read_to_string, path::PathBuf};

use java_syntax::{Parser, lex};

use crate::args::ParseLanguage;

pub fn render_tree(lang: ParseLanguage, file_path: PathBuf) {
    let content = match read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            tracing::error!("Failed to read file: {e:#}");
            return;
        }
    };
    match lang {
        ParseLanguage::Java => {
            render_java_tree(content);
        }
    }
}

pub fn render_java_tree(content: String) {
    let tokens = match lex(&content) {
        Ok(tokens) => tokens,
        Err((tokens, errors)) => {
            for err in errors {
                println!("Lexical error: {err:?}");
            }
            tokens
        }
    };

    let parse = Parser::new(tokens).parse();
    let res = parse.debug_dump();
    println!("{res}");
}
