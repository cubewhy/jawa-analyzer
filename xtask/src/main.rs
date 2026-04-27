use std::process;

use clap::Parser;

use crate::args::{Cli, ParseLanguage};

mod args;
mod prepare;
mod tree;

fn main() {
    let args = Cli::parse();

    match args {
        Cli::Prepare { target } => {
            prepare::prepare(target);
        }
        Cli::Parse { lang, file } => {
            let Some(lang) = lang.or_else(|| {
                probe_lang_by_extension(
                    file.extension()
                        .map(|os_str| os_str.to_string_lossy())
                        .as_deref(),
                )
            }) else {
                tracing::error!("Unknown file type");
                process::exit(1);
            };
            tree::render_tree(lang, file);
        }
    }
}

fn probe_lang_by_extension(extension: Option<&str>) -> Option<ParseLanguage> {
    match extension {
        Some("java") => Some(ParseLanguage::Java),
        _ => None,
    }
}
