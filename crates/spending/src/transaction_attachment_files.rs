//! Private, bounded server files. Originals never have a public/static URL.
use crate::{
    transaction_attachments::{
        TransactionAttachment, TransactionAttachmentsService, MAX_ATTACHMENT_BYTES,
    },
    SpendingError,
};
use anyhow::{Context, Result};
use image::{ImageFormat, ImageReader};
use pdfium_render::prelude::{PdfRenderConfig, Pdfium};
use std::{
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

pub struct TransactionAttachmentFiles {
    root: PathBuf,
    attachments: Arc<TransactionAttachmentsService>,
    render_slots: Arc<Semaphore>,
}

fn invalid(message: &str) -> anyhow::Error {
    SpendingError::InvalidInput {
        message: message.into(),
    }
    .into()
}

fn validate_filename(filename: &str) -> Result<()> {
    if filename.is_empty()
        || filename.len() > 180
        || filename
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
        || filename == "."
        || filename == ".."
    {
        return Err(invalid("Invalid attachment filename (maximum 180 bytes)"));
    }
    Ok(())
}

fn thumbnail(bytes: &[u8], content_type: &str) -> Result<Vec<u8>> {
    let image = if content_type == "application/pdf" {
        if !bytes.starts_with(b"%PDF-") {
            return Err(invalid("File content is not a PDF"));
        }
        let executable = std::env::current_exe().context("Cannot locate the server executable")?;
        let directory = executable
            .parent()
            .context("Cannot locate the server directory")?;
        let bindings =
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(directory))
                .or_else(|_| Pdfium::bind_to_system_library())
                .context("PDF thumbnails require the server PDFium library")?;
        let pdfium = Pdfium::new(bindings);
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
        let mut reader = ImageReader::with_format(Cursor::new(bytes), expected);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(12000);
        limits.max_image_height = Some(12000);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        reader
            .decode()
            .map_err(|_| invalid("Invalid image or image exceeds decoding limits"))?
    };
    let mut output = Cursor::new(Vec::new());
    image
        .thumbnail(480, 480)
        .write_to(&mut output, ImageFormat::WebP)?;
    Ok(output.into_inner())
}

fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    Ok(())
}

impl TransactionAttachmentFiles {
    pub fn new(root: PathBuf, attachments: Arc<TransactionAttachmentsService>) -> Result<Self> {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&root)?;
        if std::fs::symlink_metadata(&root)?.file_type().is_symlink() {
            anyhow::bail!("Attachment root must not be a symlink");
        }
        Ok(Self {
            root,
            attachments,
            render_slots: Arc::new(Semaphore::new(2)),
        })
    }

    /// Hold before reading the upload body to bound both buffering and decoding.
    pub async fn upload_slot(&self) -> Result<OwnedSemaphorePermit> {
        self.render_slots
            .clone()
            .acquire_owned()
            .await
            .context("Attachment uploads unavailable")
    }

    fn activity_directory(&self, activity_id: &str) -> Result<PathBuf> {
        if activity_id.is_empty() || activity_id.len() > 120 {
            return Err(invalid("Invalid transaction ID"));
        }
        use std::fmt::Write as _;
        let mut encoded = String::with_capacity(activity_id.len() * 2);
        for byte in activity_id.bytes() {
            write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
        }
        Ok(self.root.join(encoded))
    }

    fn paths(&self, activity_id: &str, id: &str) -> Result<(PathBuf, PathBuf)> {
        let id = Uuid::parse_str(id)
            .map_err(|_| invalid("Invalid attachment ID"))?
            .to_string();
        let root = self.activity_directory(activity_id)?;
        Ok((
            root.join(format!("{id}.original")),
            root.join(format!("{id}.webp")),
        ))
    }

    pub async fn upload(
        &self,
        activity_id: &str,
        filename: &str,
        content_type: &str,
        bytes: Vec<u8>,
        slot: OwnedSemaphorePermit,
    ) -> Result<TransactionAttachment> {
        let attachments = self.attachments.list(activity_id).await?;
        if attachments.len() >= crate::transaction_attachments::MAX_ATTACHMENTS {
            return Err(invalid("Maximum 10 attachments per transaction"));
        }
        validate_filename(filename)?;
        if bytes.is_empty() || bytes.len() > MAX_ATTACHMENT_BYTES {
            return Err(invalid("Attachment must be between 1 byte and 20 MiB"));
        }
        let content_type = content_type.to_owned();
        let render_type = content_type.clone();
        let id = Uuid::new_v4().to_string();
        let (original_path, thumbnail_path) = self.paths(activity_id, &id)?;
        let original = original_path.clone();
        let thumb = thumbnail_path.clone();
        let size_bytes = bytes.len() as i64;
        let result = tokio::task::spawn_blocking(move || -> Result<()> {
            // Blocking jobs outlive a cancelled HTTP future; retain the permit
            // inside the job so a timeout cannot bypass the rendering limit.
            let _slot = slot;
            let preview = thumbnail(&bytes, &render_type)?;
            let parent = original.parent().expect("attachment paths have a parent");
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(parent)?;
            if std::fs::symlink_metadata(parent)?.file_type().is_symlink() {
                return Err(invalid("Invalid attachment directory"));
            }
            // UUID names are generated here; filenames are display metadata only.
            private_write(&original, &bytes)?;
            private_write(&thumb, &preview)?;
            Ok(())
        })
        .await?;
        let attachment = TransactionAttachment {
            id,
            activity_id: activity_id.into(),
            filename: filename.into(),
            content_type,
            size_bytes,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let result = match result {
            Ok(()) => self.attachments.add_attachment(attachment.clone()).await,
            Err(e) => Err(e),
        };
        if let Err(error) = result {
            let _ = tokio::fs::remove_file(original_path).await;
            let _ = tokio::fs::remove_file(thumbnail_path).await;
            return Err(error);
        }
        Ok(attachment)
    }

    pub async fn read(
        &self,
        activity_id: &str,
        id: &str,
        preview: bool,
    ) -> Result<(TransactionAttachment, Vec<u8>)> {
        let attachment = self.attachments.attachment(activity_id, id).await?;
        let (original, thumbnail) = self.paths(activity_id, &attachment.id)?;
        let path = if preview { thumbnail } else { original };
        if !tokio::fs::symlink_metadata(&path)
            .await?
            .file_type()
            .is_file()
        {
            return Err(invalid("Invalid attachment file"));
        }
        let bytes = tokio::fs::read(path).await?;
        Ok((attachment, bytes))
    }

    pub async fn delete(&self, activity_id: &str, id: &str) -> Result<()> {
        self.attachments.delete_attachment(activity_id, id).await?;
        self.remove_files(activity_id, id).await
    }

    /// Called after a successful transaction/account deletion. The directory
    /// contains only this transaction's server-generated UUID files.
    pub async fn remove_transaction(&self, activity_id: &str) -> Result<()> {
        let path = self.activity_directory(activity_id)?;
        match tokio::fs::remove_dir_all(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn remove_files(&self, activity_id: &str, id: &str) -> Result<()> {
        let (original, thumbnail) = self.paths(activity_id, id)?;
        for path in [original, thumbnail] {
            match tokio::fs::remove_file(path).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filenames_and_file_types_are_checked() {
        for name in [
            "../receipt.png",
            "a/b.png",
            "a\\b.png",
            "\r\nreceipt.png",
            "",
            "..",
        ] {
            assert!(validate_filename(name).is_err());
        }
        assert!(validate_filename("領収書.png").is_ok());
        assert!(thumbnail(b"<svg/>", "image/png").is_err());
        assert!(thumbnail(b"not a PDF", "application/pdf").is_err());
        assert!(thumbnail(b"GIF89a", "image/gif").is_err());
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
