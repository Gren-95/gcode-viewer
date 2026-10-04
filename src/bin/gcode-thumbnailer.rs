use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process::ExitCode;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use image::{ImageFormat, RgbaImage};

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const THUMBNAIL_BEGIN: &str = "; thumbnail begin";
const THUMBNAIL_END: &str = "; thumbnail end";

const LABEL: &str = "GCODE";
const LABEL_FONT: &[u8] = include_bytes!("../../assets/LiberationSans-Regular.ttf");
const LABEL_COLOR: [u8; 3] = [70, 70, 70];
const LABEL_HEIGHT_RATIO: f32 = 0.12;
const LABEL_PADDING_RATIO: f32 = 0.04;
const BACKGROUND: [u8; 3] = [255, 255, 255];

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

/// Composites the image onto a white background so the label stays readable
/// on dark file manager themes.
fn flatten_on_white(image: &RgbaImage) -> RgbaImage {
    let mut flattened = image.clone();
    for pixel in flattened.pixels_mut() {
        let alpha = f32::from(pixel[3]) / 255.0;
        for channel in 0..3 {
            let blended = f32::from(pixel[channel]) * alpha + f32::from(BACKGROUND[channel]) * (1.0 - alpha);
            pixel[channel] = blended.round() as u8;
        }
        pixel[3] = 255;
    }
    flattened
}

/// Draws the file format label in the top right corner, sized relative to the image.
fn draw_label(image: &mut RgbaImage, text: &str) -> Option<()> {
    let font = FontRef::try_from_slice(LABEL_FONT).ok()?;
    let height = image.height() as f32;
    // PxScale spans ascent to descent, so convert from the em size the ratio describes.
    let em_pixels = (height * LABEL_HEIGHT_RATIO).max(8.0);
    let scale = PxScale::from(em_pixels * font.height_unscaled() / font.units_per_em()?);
    let scaled = font.as_scaled(scale);
    let padding = height * LABEL_PADDING_RATIO;

    let text_width: f32 = text.chars().map(|c| scaled.h_advance(scaled.glyph_id(c))).sum();
    let mut cursor = image.width() as f32 - padding - text_width;
    let baseline = padding + scaled.ascent();

    for character in text.chars() {
        let glyph_id = scaled.glyph_id(character);
        let glyph = glyph_id.with_scale_and_position(scaled.scale(), point(cursor, baseline));
        cursor += scaled.h_advance(glyph_id);
        let Some(outlined) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|x, y, coverage| {
            let (px, py) = (bounds.min.x as i64 + i64::from(x), bounds.min.y as i64 + i64::from(y));
            if px < 0 || py < 0 || px >= i64::from(image.width()) || py >= i64::from(image.height()) {
                return;
            }
            let pixel = image.get_pixel_mut(px as u32, py as u32);
            for channel in 0..3 {
                let blended = f32::from(pixel[channel]) * (1.0 - coverage) + f32::from(LABEL_COLOR[channel]) * coverage;
                pixel[channel] = blended.round() as u8;
            }
        });
    }
    Some(())
}

fn render_labelled_thumbnail(png: &[u8]) -> Option<RgbaImage> {
    let decoded = image::load_from_memory_with_format(png, ImageFormat::Png).ok()?;
    let mut flattened = flatten_on_white(&decoded.to_rgba8());
    draw_label(&mut flattened, LABEL)?;
    Some(flattened)
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
    let Some(labelled) = render_labelled_thumbnail(&thumbnail.png) else {
        eprintln!("could not decode the embedded thumbnail in {input}");
        return ExitCode::FAILURE;
    };
    match labelled.save_with_format(&output, ImageFormat::Png) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => {
            eprintln!("could not write {output}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use image::Rgba;

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

    #[test]
    fn should_flatten_transparent_pixels_to_white() {
        let image = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 0]));
        assert_eq!(flatten_on_white(&image).get_pixel(0, 0), &Rgba([255, 255, 255, 255]));
    }

    #[test]
    fn should_draw_the_label_in_the_top_right_corner() {
        let mut image = RgbaImage::from_pixel(200, 200, Rgba([255, 255, 255, 255]));
        draw_label(&mut image, LABEL).unwrap();
        let is_dark = |x: u32, y: u32| image.get_pixel(x, y)[0] < 200;
        let dark_in = |x_range: std::ops::Range<u32>, y_range: std::ops::Range<u32>| {
            x_range.flat_map(|x| y_range.clone().map(move |y| (x, y))).any(|(x, y)| is_dark(x, y))
        };
        assert!(dark_in(100..200, 0..40));
        assert!(!dark_in(0..90, 0..200));
        assert!(!dark_in(0..200, 60..200));
    }
}
