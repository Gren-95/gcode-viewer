use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process::ExitCode;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const THUMBNAIL_BEGIN: &str = "; thumbnail begin";
const THUMBNAIL_END: &str = "; thumbnail end";

struct Thumbnail {
    pixel_count: u32,
    png: Vec<u8>,
}

fn decode_base64(encoded: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(encoded.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in encoded.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(output)
}

fn parse_dimensions(begin_line: &str) -> Option<u32> {
    let dimensions = begin_line.strip_prefix(THUMBNAIL_BEGIN)?.split_whitespace().next()?;
    let (width, height) = dimensions.split_once('x')?;
    Some(width.parse::<u32>().ok()? * height.parse::<u32>().ok()?)
}

fn read_largest_thumbnail(file: File) -> Option<Thumbnail> {
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let mut best: Option<Thumbnail> = None;
    let mut current: Option<(u32, String)> = None;
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line).ok()? == 0 {
            break;
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim();
        if text.starts_with(THUMBNAIL_BEGIN) {
            current = parse_dimensions(text).map(|pixels| (pixels, String::new()));
        } else if text.starts_with(THUMBNAIL_END) {
            let Some((pixel_count, encoded)) = current.take() else {
                continue;
            };
            let png = decode_base64(&encoded)?;
            if png.starts_with(&PNG_SIGNATURE)
                && best.as_ref().is_none_or(|b| pixel_count > b.pixel_count)
            {
                best = Some(Thumbnail { pixel_count, png });
            }
        } else if let Some((_, encoded)) = current.as_mut() {
            encoded.push_str(text.trim_start_matches(';').trim());
        } else if best.is_some() && !text.is_empty() && !text.starts_with(';') {
            break;
        }
    }
    best
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: gcode-thumbnailer <input.gcode> <output.png>");
        return ExitCode::from(2);
    };
    let Ok(file) = File::open(&input) else {
        eprintln!("could not open {input}");
        return ExitCode::FAILURE;
    };
    let Some(thumbnail) = read_largest_thumbnail(file) else {
        eprintln!("no embedded thumbnail in {input}");
        return ExitCode::FAILURE;
    };
    match std::fs::write(&output, thumbnail.png) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => {
            eprintln!("could not write {output}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_decode_base64_with_padding() {
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode_base64("aGVs\nbG8=").unwrap(), b"hello");
    }

    #[test]
    fn should_reject_invalid_base64() {
        assert!(decode_base64("a$b").is_none());
    }

    #[test]
    fn should_parse_thumbnail_dimensions() {
        assert_eq!(parse_dimensions("; thumbnail begin 144x144 4304"), Some(20736));
        assert_eq!(parse_dimensions("; thumbnail begin nonsense"), None);
    }
}
