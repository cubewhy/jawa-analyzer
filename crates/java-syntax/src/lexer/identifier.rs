use unicode_general_category::{GeneralCategory, get_general_category};

pub fn is_java_identifier_start(c: char) -> bool {
    use GeneralCategory::*;
    matches!(
        get_general_category(c),
        UppercaseLetter      | // Lu
        LowercaseLetter      | // Ll
        TitlecaseLetter      | // Lt
        ModifierLetter       | // Lm
        OtherLetter          | // Lo
        LetterNumber         | // Nl
        CurrencySymbol       | // Sc
        ConnectorPunctuation // Pc
    )
}

fn is_java_identifier_ignorable(c: char) -> bool {
    matches!(
        c,
        '\u{0000}'..='\u{0008}' |
        '\u{000E}'..='\u{001B}' |
        '\u{007F}'..='\u{009F}'
    ) || get_general_category(c) == GeneralCategory::Format // Cf
}

pub fn is_java_identifier_part(c: char) -> bool {
    use GeneralCategory::*;
    if is_java_identifier_start(c) {
        return true;
    }

    matches!(
        get_general_category(c),
        DecimalNumber  | // Nd
        SpacingMark    | // Mc
        NonspacingMark | // Mn
        Format // Cf (covered by ignorable)
    ) || is_java_identifier_ignorable(c)
}
