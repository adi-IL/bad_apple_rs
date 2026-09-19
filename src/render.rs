pub const WIDTH: u32 = 80;
pub const HEIGHT: u32 = 60;
pub const ASCII_CHARS: &[u8] = b" .:-=+*#%@";

pub fn pixel_to_ascii(pixel: u8) -> u8 {
    let idx = (pixel as usize * (ASCII_CHARS.len() - 1)) / 255;
    ASCII_CHARS[idx]
}

pub fn compute_padding(
    term_width: u16,
    term_height: u16,
    frame_w: u32,
    frame_h: u32,
) -> (u16, u16) {
    let pad_x = if term_width > frame_w as u16 {
        (term_width - frame_w as u16) / 2
    } else {
        0
    };
    let pad_y = if term_height > frame_h as u16 {
        (term_height - frame_h as u16) / 2
    } else {
        0
    };
    (pad_x, pad_y)
}

pub fn render_frame(buffer: &[u8], term_width: u16, term_height: u16) -> String {
    let frame_size = (WIDTH * HEIGHT) as usize;
    let (pad_x, pad_y) = compute_padding(term_width, term_height, WIDTH, HEIGHT);

    let mut output =
        String::with_capacity(frame_size + (term_height as usize * term_width as usize));

    for _ in 0..pad_y {
        output.push_str("\r\n");
    }

    for y in 0..HEIGHT {
        for _ in 0..pad_x {
            output.push(' ');
        }

        let start = (y * WIDTH) as usize;
        let end = start + WIDTH as usize;
        let line = std::str::from_utf8(&buffer[start..end]).unwrap_or("");
        output.push_str(line);
        output.push_str("\r\n");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_to_ascii_extremes() {
        assert_eq!(pixel_to_ascii(0), b' ');
        assert_eq!(pixel_to_ascii(255), b'@');
    }

    #[test]
    fn test_compute_padding_centered() {
        let (pad_x, pad_y) = compute_padding(100, 80, 80, 60);
        assert_eq!(pad_x, 10);
        assert_eq!(pad_y, 10);
    }

    #[test]
    fn test_compute_padding_clamped_at_zero() {
        let (pad_x, pad_y) = compute_padding(60, 40, 80, 60);
        assert_eq!(pad_x, 0);
        assert_eq!(pad_y, 0);
    }

    #[test]
    fn test_render_frame_contains_carriage_returns() {
        let dummy = vec![b' '; (WIDTH * HEIGHT) as usize];
        let frame = render_frame(&dummy, 80, 60);
        assert!(
            frame.contains("\r\n"),
            "Rendered frames in raw mode must contain \\r\\n"
        );
    }
}
