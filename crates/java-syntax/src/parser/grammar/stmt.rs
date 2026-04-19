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

// TODO: parse block
pub fn block(p: &mut Parser) {
    let m = p.start();
    p.expect(L_BRACE);

    while !p.is_at_end() && !p.at(R_BRACE) {
        block_statement_stub(p);
    }

    p.expect(R_BRACE);
    m.complete(p, BLOCK);
}

fn block_statement_stub(p: &mut Parser) {
    if p.at(L_BRACE) {
        block(p);
        return;
    }

    while !p.is_at_end() && !p.at(SEMICOLON) && !p.at(R_BRACE) {
        p.bump();
    }

    p.eat(SEMICOLON);
}
