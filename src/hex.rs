pub fn format_hex_lines(bytes: &[u8], start_offset: usize, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();

    let mut offset = start_offset;
    for _ in 0..max_lines {
        if offset >= bytes.len() {
            break;
        }

        let chunk = &bytes[offset..bytes.len().min(offset + 16)];
        let mut hex_part = String::new();
        for (i, b) in chunk.iter().enumerate() {
            if i > 0 {
                hex_part.push(' ');
            }
            hex_part.push_str(&format!("{:02X}", b));
        }

        // pad hex section to fixed width (16 bytes => 47 chars incl spaces)
        while hex_part.len() < 16 * 3 - 1 {
            hex_part.push(' ');
        }

        let ascii_part: String = chunk
            .iter()
            .map(|b| {
                let c = *b as char;
                if c.is_ascii_graphic() || c == ' ' {
                    c
                } else {
                    '.'
                }
            })
            .collect();

        lines.push(format!("{:08X}  {}  |{}|", offset, hex_part, ascii_part));
        offset += 16;
    }

    lines
}
