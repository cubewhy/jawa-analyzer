use crate::{
    kinds::SyntaxKind::*,
    parser::{Parser, grammar::modifiers::expression},
};

pub fn argument_list(p: &mut Parser) {
    let m = p.start();
    p.expect(L_PAREN);

    if !p.at(R_PAREN) {
        expression(p);
        while p.eat(COMMA) {
            expression(p);
        }
    }

    p.expect(R_PAREN);
    m.complete(p, ARGUMENT_LIST);
}
