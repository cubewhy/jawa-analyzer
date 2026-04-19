use crate::kinds::SyntaxKind::*;
use crate::parser::Parser;

pub fn method_body_or_semicolon(p: &mut Parser) {
    if p.at(L_BRACE) {
        // {
        block(p);
    } else {
        // ;
        p.expect(SEMICOLON);
    }
}

pub fn block(p: &mut Parser) {
    let m = p.start();

    p.expect(L_BRACE);

    // TODO: parse block

    p.expect(R_BRACE);

    m.complete(p, BLOCK);
}
