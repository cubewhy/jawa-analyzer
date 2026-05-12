use crate::common::lexer_snapshot;
use indoc::indoc;

mod common;

lexer_snapshot!(
    lex_string,
    indoc! {r#"
    "a normal string"
"#}
);

lexer_snapshot!(
    lex_raw_string,
    indoc! {r#"
    """
    a
    raw
    string"""
"#}
);
