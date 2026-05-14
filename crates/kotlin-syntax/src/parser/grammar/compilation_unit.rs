use crate::{Parser, SyntaxKind::*};

/// https://kotlinlang.org/spec/syntax-and-grammar.html#grammar-rule-kotlinFile
pub fn root(p: &mut Parser) {
    let m = p.start();

    // Shebang line
    p.eat(SHEBANG_LINE);

    // TODO:
    // optional file annotation
    // package header
    // import list
    // {topLevelObject}

    m.complete(p, ROOT);
}
