use lsp_types::{Position, Url};
use rowan::TextSize;
use vfs::{FileId, Vfs};

pub fn file_id_to_url(vfs: &Vfs, file_id: FileId) -> Option<Url> {
    let path = vfs.file_path(file_id)?;

    Some(path.to_url())
}

pub fn offset_to_position(text: &str, offset: TextSize) -> Position {
    let offset = u32::from(offset) as usize;
    let safe_offset = offset.min(text.len());
    let head = &text[..safe_offset];

    let line = head.chars().filter(|&c| c == '\n').count() as u32;

    let last_line_start = head.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let last_line_text = &head[last_line_start..];

    let character = last_line_text.encode_utf16().count() as u32;

    Position { line, character }
}

pub fn position_to_offset(text: &str, position: Position) -> Option<TextSize> {
    let mut lines = text.split('\n');
    let mut bytes_offset = 0;

    for _ in 0..position.line {
        let line_text = lines.next()?;
        bytes_offset += line_text.len() + 1; // \n
    }

    let target_line = lines.next()?;
    let mut current_utf16_idx = 0;
    let mut utf8_bytes_inside_line = 0;

    for c in target_line.chars() {
        if current_utf16_idx >= position.character {
            break;
        }
        current_utf16_idx += c.len_utf16() as u32;
        utf8_bytes_inside_line += c.len_utf8();
    }

    Some(TextSize::from(
        (bytes_offset + utf8_bytes_inside_line) as u32,
    ))
}
