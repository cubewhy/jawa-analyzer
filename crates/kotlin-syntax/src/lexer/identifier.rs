use unicode_general_category::{GeneralCategory, get_general_category};

fn is_unicode_letter(c: char) -> bool {
    use GeneralCategory::*;
    match get_general_category(c) {
        UppercaseLetter |   // Lu
        LowercaseLetter |   // Ll
        TitlecaseLetter |   // Lt
        ModifierLetter |    // Lm
        OtherLetter |       // Lo
        LetterNumber => true, // Nl
        _ => false,
    }
}

fn is_unicode_digit(c: char) -> bool {
    matches!(get_general_category(c), GeneralCategory::DecimalNumber) // Nd
}

pub fn is_kotlin_identifier_start(c: char) -> bool {
    c == '_' || is_unicode_letter(c)
}

pub fn is_kotlin_identifier_part(c: char) -> bool {
    is_kotlin_identifier_start(c) || is_unicode_digit(c)
}
