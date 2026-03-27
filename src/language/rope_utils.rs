use ropey::Rope;
use tower_lsp::lsp_types::{Position, Range};

pub fn rope_line_col_to_offset(rope: &Rope, line: u32, character: u32) -> Option<usize> {
    let line_idx = line as usize;
    if line_idx >= rope.len_lines() {
        return None;
    }

    let line_byte_start = rope.line_to_byte(line_idx);
    let line_slice = rope.line(line_idx);

    let mut utf16_units = 0usize;
    let mut byte_offset = 0usize;

    for ch in line_slice.chars() {
        if utf16_units >= character as usize {
            break;
        }
        utf16_units += ch.len_utf16();
        byte_offset += ch.len_utf8();
    }

    Some(line_byte_start + byte_offset)
}

pub fn line_col_to_offset(source: &str, line: u32, character: u32) -> Option<usize> {
    let rope = Rope::from_str(source);
    rope_line_col_to_offset(&rope, line, character)
}

pub fn rope_byte_offset_to_line_col(rope: &Rope, offset: usize) -> (u32, u32) {
    let char_idx = rope.byte_to_char(offset.min(rope.len_bytes()));
    let line_idx = rope.char_to_line(char_idx);
    let line_char_start = rope.line_to_char(line_idx);
    let character = rope
        .slice(line_char_start..char_idx)
        .chars()
        .map(char::len_utf16)
        .sum::<usize>() as u32;
    (line_idx as u32, character)
}

pub fn byte_offset_to_line_col(source: &str, offset: usize) -> (u32, u32) {
    let rope = Rope::from_str(source);
    rope_byte_offset_to_line_col(&rope, offset)
}

pub fn rope_byte_offset_to_position(rope: &Rope, offset: usize) -> Position {
    let (line, character) = rope_byte_offset_to_line_col(rope, offset);
    Position { line, character }
}

pub fn rope_byte_range_to_range(rope: &Rope, start: usize, end: usize) -> Range {
    Range {
        start: rope_byte_offset_to_position(rope, start),
        end: rope_byte_offset_to_position(rope, end),
    }
}

#[cfg(test)]
pub fn byte_range_to_range(source: &str, start: usize, end: usize) -> Range {
    let rope = Rope::from_str(source);
    rope_byte_range_to_range(&rope, start, end)
}

pub fn rope_identifier_end_position(rope: &Rope, line: u32, character: u32) -> Option<Position> {
    let start_offset = rope_line_col_to_offset(rope, line, character)?;
    let start_char = rope.byte_to_char(start_offset);
    let mut end_offset = start_offset;
    let mut found_identifier = false;

    for ch in rope.slice(start_char..).chars() {
        if !(ch.is_alphanumeric() || ch == '_') {
            break;
        }
        found_identifier = true;
        end_offset += ch.len_utf8();
    }

    if !found_identifier {
        return Some(Position { line, character });
    }

    Some(rope_byte_offset_to_position(rope, end_offset))
}

#[cfg(test)]
mod tests {
    use crate::language::rope_utils::{
        byte_offset_to_line_col, byte_range_to_range, line_col_to_offset,
        rope_identifier_end_position,
    };
    use ropey::Rope;
    use tower_lsp::lsp_types::{Position, Range};

    #[test]
    fn test_line_col_to_offset() {
        let src = "hello\nworld";
        assert_eq!(line_col_to_offset(src, 0, 5), Some(5));
        assert_eq!(line_col_to_offset(src, 1, 3), Some(9));
        assert_eq!(line_col_to_offset(src, 5, 0), None);
    }

    #[test]
    fn test_byte_offset_to_line_col() {
        let src = "a😀\n中b";
        assert_eq!(byte_offset_to_line_col(src, 0), (0, 0));
        assert_eq!(byte_offset_to_line_col(src, 1), (0, 1));
        assert_eq!(byte_offset_to_line_col(src, 5), (0, 3));
        assert_eq!(byte_offset_to_line_col(src, src.len()), (1, 2));
    }

    #[test]
    fn test_byte_range_to_range() {
        let src = "/*😀*/foo";
        let start = src.find("foo").expect("foo");
        let end = start + "foo".len();
        assert_eq!(
            byte_range_to_range(src, start, end),
            Range {
                start: Position::new(0, 6),
                end: Position::new(0, 9),
            }
        );
    }

    #[test]
    fn test_rope_identifier_end_position() {
        let src = "/*😀*/token suffix";
        let rope = Rope::from_str(src);
        let pos = rope_identifier_end_position(&rope, 0, 8).expect("token end");
        assert_eq!(pos, Position::new(0, 11));
    }
}
