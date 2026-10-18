//! Preview rendering: a 480x480 bounding-box WebP for every supported type.

use crate::error::{invalid, FileStoreError, Result};
use image::{ImageDecoder, ImageFormat, ImageReader};
use pdfium_render::prelude::{PdfRenderConfig, Pdfium};
use std::io::Cursor;
use std::sync::LazyLock;

static PDFIUM: LazyLock<std::result::Result<Pdfium, String>> = LazyLock::new(bind_pdfium);

fn bind_pdfium() -> std::result::Result<Pdfium, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let directory = executable
        .parent()
        .ok_or_else(|| "Cannot locate the server directory".to_string())?;
    let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(directory))
        .or_else(|_| Pdfium::bind_to_system_library())
        .map_err(|e| e.to_string())?;
    Ok(Pdfium::new(bindings))
}

pub(crate) fn thumbnail(bytes: &[u8], content_type: &str) -> Result<Vec<u8>> {
    let image = if content_type == "application/pdf" {
        if !bytes.starts_with(b"%PDF-") {
            return Err(invalid("File content is not a PDF"));
        }
        let pdfium = PDFIUM.as_ref().map_err(|reason| {
            FileStoreError::Backend(anyhow::anyhow!(
                "PDF thumbnails require the server PDFium library: {reason}"
            ))
        })?;
        let document = pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|_| invalid("Invalid or password-protected PDF"))?;
        let page = document
            .pages()
            .get(0)
            .map_err(|_| invalid("PDF has no readable pages"))?;
        let bitmap = page
            .render_with_config(
                &PdfRenderConfig::new()
                    .set_target_width(480)
                    .set_maximum_height(480),
            )
            .map_err(|_| invalid("Could not render PDF thumbnail"))?;
        bitmap
            .as_image()
            .map_err(|_| invalid("Could not encode PDF thumbnail"))?
    } else {
        let expected = match content_type {
            "image/jpeg" => ImageFormat::Jpeg,
            "image/png" => ImageFormat::Png,
            "image/webp" => ImageFormat::WebP,
            _ => {
                return Err(invalid(
                    "Only JPEG, PNG, WebP and PDF attachments are supported",
                ))
            }
        };
        if image::guess_format(bytes).ok() != Some(expected) {
            return Err(invalid("File content does not match its type"));
        }
        decode_image(bytes, expected)?
    };
    let mut output = Cursor::new(Vec::new());
    image
        .thumbnail(480, 480)
        .write_to(&mut output, ImageFormat::WebP)
        .map_err(|e| FileStoreError::Backend(e.into()))?;
    Ok(output.into_inner())
}

fn decode_image(bytes: &[u8], expected: ImageFormat) -> Result<image::DynamicImage> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), expected);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(12000);
    limits.max_image_height = Some(12000);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|_| invalid("Invalid image or image exceeds decoding limits"))
}

pub(crate) fn optimize_image(bytes: &[u8]) -> Result<Vec<u8>> {
    let format = image::guess_format(bytes).map_err(|_| invalid("Invalid image"))?;
    let mut image = decode_image(bytes, format)?;
    let orientation = ImageReader::with_format(Cursor::new(bytes), format)
        .into_decoder()
        .and_then(|mut decoder| decoder.orientation())
        .map_err(|_| invalid("Invalid image orientation"))?;
    image.apply_orientation(orientation);
    let (mut width, mut height) = (image.width(), image.height());
    while width.max(height) > 1000 {
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    let image = if (width, height) == (image.width(), image.height()) {
        image
    } else {
        image.resize_exact(width, height, image::imageops::FilterType::Lanczos3)
    };
    let rgba = image.to_rgba8();
    webp::Encoder::from_rgba(rgba.as_raw(), width, height)
        .encode_simple(false, 50.0)
        .map(|encoded| encoded.to_vec())
        .map_err(|_| FileStoreError::Backend(anyhow::anyhow!("Could not encode WebP image")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_images_use_lossy_webp_and_power_of_two_resizing() {
        for (width, height, expected) in [
            (40, 20, (40, 20)),
            (1000, 600, (1000, 600)),
            (2000, 1200, (1000, 600)),
            (1200, 4000, (300, 1000)),
        ] {
            let mut input = Cursor::new(Vec::new());
            image::DynamicImage::new_rgba8(width, height)
                .write_to(&mut input, ImageFormat::Png)
                .unwrap();
            let output = optimize_image(input.get_ref()).unwrap();
            assert_eq!(image::guess_format(&output).unwrap(), ImageFormat::WebP);
            assert!(output.windows(4).any(|chunk| chunk == b"VP8 "));
            let decoded = image::load_from_memory(&output).unwrap();
            assert_eq!((decoded.width(), decoded.height()), expected);
            assert_eq!(decoded.to_rgba8().get_pixel(0, 0).0[3], 0);
        }
    }

    #[test]
    fn saved_image_preserves_exif_orientation_when_metadata_is_stripped() {
        let mut jpeg = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(40, 20)
            .write_to(&mut jpeg, ImageFormat::Jpeg)
            .unwrap();
        // Little-endian EXIF with orientation 6 (rotate 90 degrees clockwise).
        let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut input = vec![0xff, 0xd8, 0xff, 0xe1];
        input.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        input.extend_from_slice(exif);
        input.extend_from_slice(&jpeg.get_ref()[2..]);
        let output = optimize_image(&input).unwrap();
        let decoded = image::load_from_memory(&output).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (20, 40));
    }

    #[test]
    fn file_types_are_checked() {
        assert!(thumbnail(b"<svg/>", "image/png").is_err());
        assert!(thumbnail(b"not a PDF", "application/pdf").is_err());
        assert!(thumbnail(b"GIF89a", "image/gif").is_err());
    }

    #[cfg(feature = "thumbnails")]
    #[test]
    fn pdf_previews_render_more_than_once_per_process() {
        let pdf = include_bytes!("../../../e2e/fixtures/transaction-attachments/receipt.pdf");
        if std::env::var_os("WF_TEST_PDFIUM_REQUIRED").is_none()
            && thumbnail(pdf, "application/pdf").is_err()
        {
            return;
        }
        thumbnail(pdf, "application/pdf").unwrap();
        thumbnail(pdf, "application/pdf").unwrap();
    }

    #[test]
    fn image_preview_is_an_aspect_preserving_webp() {
        let image = image::DynamicImage::new_rgb8(1500, 900);
        let mut input = Cursor::new(Vec::new());
        image.write_to(&mut input, ImageFormat::Png).unwrap();
        let output = thumbnail(input.get_ref(), "image/png").unwrap();
        assert_eq!(image::guess_format(&output).unwrap(), ImageFormat::WebP);
        let decoded = image::load_from_memory(&output).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (480, 288));
        assert_ne!(input.get_ref(), &output);

        let portrait = image::DynamicImage::new_rgb8(900, 1500);
        let mut input = Cursor::new(Vec::new());
        portrait.write_to(&mut input, ImageFormat::Png).unwrap();
        let decoded =
            image::load_from_memory(&thumbnail(input.get_ref(), "image/png").unwrap()).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (288, 480));
    }
}
