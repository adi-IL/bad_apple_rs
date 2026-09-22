pub const WIDTH: u32 = 80;
pub const HEIGHT: u32 = 60;
pub const ASCII_CHARS: &[u8] = b" .:-=+*#%@";

pub fn pixel_to_ascii(pixel: u8) -> u8 {
    let idx = (pixel as usize * (ASCII_CHARS.len() - 1)) / 255;
    ASCII_CHARS[idx]
}

#[allow(dead_code)]
pub fn compute_padding(
    term_width: u16,
    term_height: u16,
    frame_w: u32,
    frame_h: u32,
) -> (u16, u16) {
    let vp = compute_viewport(term_width, term_height, frame_w, frame_h);
    (vp.pad_x, vp.pad_y)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportConfig {
    pub pad_x: u16,
    pub pad_y: u16,
    pub crop_x: usize,
    pub crop_y: usize,
    pub visible_w: usize,
    pub visible_h: usize,
}

pub fn compute_viewport(
    term_width: u16,
    term_height: u16,
    frame_w: u32,
    frame_h: u32,
) -> ViewportConfig {
    if term_width == 0 || term_height == 0 || frame_w == 0 || frame_h == 0 {
        return ViewportConfig {
            pad_x: 0,
            pad_y: 0,
            crop_x: 0,
            crop_y: 0,
            visible_w: 0,
            visible_h: 0,
        };
    }

    let (pad_x, crop_x, visible_w) = if (term_width as u32) >= frame_w {
        (((term_width as u32 - frame_w) / 2) as u16, 0, frame_w as usize)
    } else {
        (
            0,
            ((frame_w - term_width as u32) / 2) as usize,
            term_width as usize,
        )
    };

    let (pad_y, crop_y, visible_h) = if (term_height as u32) >= frame_h {
        (((term_height as u32 - frame_h) / 2) as u16, 0, frame_h as usize)
    } else {
        (
            0,
            ((frame_h - term_height as u32) / 2) as usize,
            term_height as usize,
        )
    };

    ViewportConfig {
        pad_x,
        pad_y,
        crop_x,
        crop_y,
        visible_w,
        visible_h,
    }
}

#[derive(Debug)]
pub struct FrameRenderer {
    buffer: String,
}

impl Default for FrameRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameRenderer {
    pub fn new() -> Self {
        Self {
            buffer: String::with_capacity((WIDTH * HEIGHT) as usize + (80 * 25)),
        }
    }

    pub fn render(&mut self, buffer: &[u8], term_width: u16, term_height: u16) -> &str {
        self.buffer.clear();
        let vp = compute_viewport(term_width, term_height, WIDTH, HEIGHT);
        if vp.visible_w == 0 || vp.visible_h == 0 {
            return &self.buffer;
        }

        for _ in 0..vp.pad_y {
            self.buffer.push_str("\r\n");
        }

        for i in 0..vp.visible_h {
            let y = vp.crop_y + i;
            for _ in 0..vp.pad_x {
                self.buffer.push(' ');
            }

            let row_start = (y * WIDTH as usize) + vp.crop_x;
            let row_end = row_start + vp.visible_w;
            if let Some(slice) = buffer.get(row_start..row_end) {
                // Frames are ASCII-by-construction; non-graphic bytes always
                // indicate corruption (including valid-UTF-8 control chars).
                for &b in slice {
                    let ch = if b.is_ascii_graphic() || b == b' ' {
                        b as char
                    } else {
                        ' '
                    };
                    self.buffer.push(ch);
                }
            }
            if i + 1 < vp.visible_h || vp.pad_y > 0 {
                self.buffer.push_str("\r\n");
            }
        }

        &self.buffer
    }

    #[cfg(test)]
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }
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
        let mut renderer = FrameRenderer::new();
        let dummy = vec![b' '; (WIDTH * HEIGHT) as usize];
        let frame = renderer.render(&dummy, 80, 60);
        assert!(
            frame.contains("\r\n"),
            "Rendered frames in raw mode must contain \\r\\n"
        );
    }

    #[test]
    fn test_frame_renderer_reuses_capacity() {
        let mut renderer = FrameRenderer::new();
        let dummy = vec![b' '; (WIDTH * HEIGHT) as usize];

        renderer.render(&dummy, 80, 60);
        let initial_capacity = renderer.capacity();
        assert!(initial_capacity >= (WIDTH * HEIGHT) as usize);

        for _ in 0..10 {
            renderer.render(&dummy, 80, 60);
            assert_eq!(renderer.capacity(), initial_capacity);
        }
    }
    #[test]
    fn test_render_frame_no_trailing_newline_on_exact_dimensions() {
        let mut renderer = FrameRenderer::default();
        let dummy = vec![b' '; (WIDTH * HEIGHT) as usize];
        let frame = renderer.render(&dummy, 80, 60);
        assert!(
            !frame.ends_with("\r\n"),
            "Exact 80x60 rendering must not end with \\r\\n to avoid scroll"
        );
    }

    #[test]
    fn test_compute_viewport_edge_cases() {
        let vp_zero = compute_viewport(0, 0, 80, 60);
        assert_eq!(vp_zero.visible_w, 0);
        assert_eq!(vp_zero.visible_h, 0);

        let vp_zero_frame = compute_viewport(80, 60, 0, 0);
        assert_eq!(vp_zero_frame.visible_w, 0);
        assert_eq!(vp_zero_frame.visible_h, 0);

        let vp_small = compute_viewport(40, 20, 80, 60);
        assert_eq!(vp_small.visible_w, 40);
        assert_eq!(vp_small.crop_x, 20);
        assert_eq!(vp_small.visible_h, 20);
        assert_eq!(vp_small.crop_y, 20);
        assert_eq!(vp_small.pad_x, 0);
        assert_eq!(vp_small.pad_y, 0);

        let vp_large = compute_viewport(120, 80, 80, 60);
        assert_eq!(vp_large.visible_w, 80);
        assert_eq!(vp_large.crop_x, 0);
        assert_eq!(vp_large.visible_h, 60);
        assert_eq!(vp_large.crop_y, 0);
        assert_eq!(vp_large.pad_x, 20);
        assert_eq!(vp_large.pad_y, 10);

        // frame_w >= 65536 must not truncate via `as u16`
        let vp_huge = compute_viewport(80, 60, 65536, 70000);
        assert_eq!(vp_huge.visible_w, 80);
        assert_eq!(vp_huge.visible_h, 60);
        assert_eq!(vp_huge.pad_x, 0);
        assert_eq!(vp_huge.pad_y, 0);
        assert_eq!(vp_huge.crop_x, (65536 - 80) / 2);
        assert_eq!(vp_huge.crop_y, (70000 - 60) / 2);
    }

    #[test]
    fn test_render_undersized_viewport_no_overflow() {
        let mut renderer = FrameRenderer::default();
        let dummy = vec![b'#'; (WIDTH * HEIGHT) as usize];
        let frame = renderer.render(&dummy, 40, 20);
        let lines: Vec<&str> = frame.split("\r\n").collect();
        assert_eq!(lines.len(), 20, "Must contain exactly 20 lines for 20 row terminal");
        for line in lines {
            assert_eq!(line.len(), 40, "Line width must be clamped to 40 characters");
        }
    }

    #[test]
    fn test_render_corrupt_non_utf8_fallback() {
        let mut renderer = FrameRenderer::default();
        let mut dummy = vec![b'#'; (WIDTH * HEIGHT) as usize];
        dummy[0] = 0xFF;
        dummy[1] = 0xFE;
        let frame = renderer.render(&dummy, 80, 60);
        let first_line = frame.lines().next().unwrap();
        assert_eq!(first_line.len(), 80);
        assert!(first_line.starts_with("  ##"));
    }

    #[test]
    fn test_render_sanitizes_ascii_control_bytes() {
        let mut renderer = FrameRenderer::default();
        let mut dummy = vec![b'#'; (WIDTH * HEIGHT) as usize];
        dummy[0] = b'\n';
        dummy[1] = b'\r';
        dummy[2] = 0x1B; // ESC
        dummy[3] = 0x07; // BEL
        dummy[4] = b'\t';
        let frame = renderer.render(&dummy, 80, 60);
        let first_line = frame.split("\r\n").next().unwrap();
        assert_eq!(first_line.len(), 80);
        assert!(
            first_line.starts_with("     #"),
            "control bytes must become spaces: {first_line:?}"
        );
        assert!(!first_line.chars().any(|c| c.is_control()));
    }
}
