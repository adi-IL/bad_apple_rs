use crate::render::{HEIGHT, WIDTH, pixel_to_ascii};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

struct TempFileCleanup<'a> {
    path: &'a Path,
    armed: bool,
}

impl<'a> TempFileCleanup<'a> {
    fn new(path: &'a Path) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl<'a> Drop for TempFileCleanup<'a> {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(self.path);
        }
    }
}

pub fn build_frames(frames_dir: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    build_frames_with_cancel(frames_dir, output, &crate::INTERRUPTED)
}

pub fn build_frames_with_cancel(
    frames_dir: &str,
    output: &str,
    interrupted: &AtomicBool,
) -> Result<(), Box<dyn std::error::Error>> {
    let first_frame = format!("{frames_dir}/frame_0001.png");
    if !Path::new(&first_frame).exists() {
        return Err(format!("No frames found in directory: {frames_dir}").into());
    }

    let output_path = Path::new(output);
    if let Some(parent) = output_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }

    let file_name = output_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output.bin");

    let temp_output = match output_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => parent.join(format!(".tmp_{}_{}", std::process::id(), file_name)),
        None => PathBuf::from(format!(".tmp_{}_{}", std::process::id(), file_name)),
    };

    let mut cleanup = TempFileCleanup::new(&temp_output);
    let out_file = File::create(&temp_output)?;
    let mut out_file = BufWriter::new(out_file);
    let mut i = 1;
    let mut processed = 0;

    loop {
        if interrupted.load(Ordering::Relaxed) {
            return Err("Build aborted by user interrupt".into());
        }

        let frame_path = format!("{}/frame_{:04}.png", frames_dir, i);
        if !Path::new(&frame_path).exists() {
            break;
        }

        let img = image::open(&frame_path)?;
        let img = if img.width() != WIDTH || img.height() != HEIGHT {
            img.resize_exact(WIDTH, HEIGHT, image::imageops::FilterType::Nearest)
        } else {
            img
        };
        let gray = img.to_luma8();
        let mut frame_data = Vec::with_capacity((WIDTH * HEIGHT) as usize);

        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let pixel = gray.get_pixel(x, y)[0];
                frame_data.push(pixel_to_ascii(pixel));
            }
        }
        out_file.write_all(&frame_data)?;
        i += 1;
        processed += 1;
    }

    out_file.flush()?;
    out_file.get_ref().sync_all()?;
    drop(out_file);
    if interrupted.load(Ordering::Relaxed) {
        return Err("Build aborted by user interrupt".into());
    }
    std::fs::rename(&temp_output, output)?;
    if let Some(parent) = output_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let _ = File::open(parent).and_then(|dir| dir.sync_all());
    }
    cleanup.disarm();

    println!("Finished processing {processed} frames.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_missing_frames_returns_error() {
        let temp_dir =
            std::env::temp_dir().join(format!("bad_apple_missing_frames_{}", std::process::id()));
        let out_file = temp_dir.join("out.bin");
        let result = build_frames(
            "nonexistent_frames_directory_xyz",
            out_file.to_str().unwrap(),
        );
        assert!(
            result.is_err(),
            "Missing frames directory must return an Err"
        );
        assert!(
            !out_file.exists(),
            "Output file must not be created on error"
        );
    }

    #[test]
    fn test_build_frames_does_not_truncate_existing_file_on_error() {
        let temp_dir =
            std::env::temp_dir().join(format!("bad_apple_truncate_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let out_file = temp_dir.join("existing.bin");
        std::fs::write(&out_file, b"preserve me").unwrap();

        let result = build_frames("nonexistent_frames_dir_123", out_file.to_str().unwrap());
        assert!(result.is_err());
        assert_eq!(
            std::fs::read(&out_file).unwrap(),
            b"preserve me",
            "Existing output file must not be truncated when frames are missing"
        );

        let frames_dir = temp_dir.join("frames");
        std::fs::create_dir_all(&frames_dir).unwrap();

        let valid_img = image::GrayImage::from_pixel(80, 60, image::Luma([128]));
        valid_img.save(frames_dir.join("frame_0001.png")).unwrap();
        std::fs::write(frames_dir.join("frame_0002.png"), b"not a valid png file").unwrap();

        let result_mid = build_frames(frames_dir.to_str().unwrap(), out_file.to_str().unwrap());
        assert!(result_mid.is_err(), "Corrupt frame must return an Err");
        assert_eq!(
            std::fs::read(&out_file).unwrap(),
            b"preserve me",
            "Existing output file must not be truncated when mid-processing error occurs"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_build_frames_resizes_non_standard_image() {
        let temp_dir =
            std::env::temp_dir().join(format!("bad_apple_test_frames_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let img = image::GrayImage::from_pixel(160, 120, image::Luma([128]));
        img.save(temp_dir.join("frame_0001.png")).unwrap();

        let out_file = temp_dir.join("out.bin");
        let result = build_frames(temp_dir.to_str().unwrap(), out_file.to_str().unwrap());
        assert!(
            result.is_ok(),
            "build_frames should resize and convert non-80x60 images"
        );

        let data = std::fs::read(&out_file).unwrap();
        assert_eq!(data.len(), (WIDTH * HEIGHT) as usize);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_build_frames_cleanup_on_interrupt() {
        let temp_dir =
            std::env::temp_dir().join(format!("bad_apple_interrupt_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let frames_dir = temp_dir.join("frames");
        std::fs::create_dir_all(&frames_dir).unwrap();
        let valid_img = image::GrayImage::from_pixel(80, 60, image::Luma([128]));
        valid_img.save(frames_dir.join("frame_0001.png")).unwrap();

        let out_file = temp_dir.join("out.bin");

        let interrupted = AtomicBool::new(true);
        let result = build_frames_with_cancel(
            frames_dir.to_str().unwrap(),
            out_file.to_str().unwrap(),
            &interrupted,
        );

        assert!(result.is_err(), "Interrupt signal must abort build");
        assert!(
            !out_file.exists(),
            "Output file must not be created on interrupt"
        );

        for entry in std::fs::read_dir(&temp_dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(
                !name.starts_with(".tmp_"),
                "Temp file was not cleaned up: {name}"
            );
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
