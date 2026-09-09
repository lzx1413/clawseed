//! Durable image files with opaque identifiers and session ownership checks.
use std::io::{Cursor, Write};

use anyhow::{Context, ensure};
use clawseed_api::provider::ImageAttachment;
use image::{ImageFormat, ImageReader, Limits};
use rusqlite::params;

use crate::session_sqlite::SqliteSessionBackend;

pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_MESSAGE_IMAGES: usize = 4;
pub const MAX_IMAGE_DIMENSION: u32 = 8192;

fn inspect_image(bytes: &[u8]) -> anyhow::Result<(String, u32, u32)> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_IMAGE_BYTES,
        "Image must be between 1 byte and 5 MiB"
    );
    let format = image::guess_format(bytes).context("Unrecognized image format")?;
    let mime = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::WebP => "image/webp",
        _ => anyhow::bail!("Only JPEG, PNG, GIF and WebP images are supported"),
    };
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(128 * 1024 * 1024);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .context("Invalid image or image dimensions exceed limits")?;
    ensure!(
        decoded.width() > 0 && decoded.height() > 0,
        "Image dimensions must be nonzero"
    );
    Ok((mime.into(), decoded.width(), decoded.height()))
}

impl SqliteSessionBackend {
    pub(super) fn store_image(
        &self,
        session_key: &str,
        user_id: &str,
        bytes: &[u8],
    ) -> anyhow::Result<ImageAttachment> {
        let (mime_type, width, height) = inspect_image(bytes)?;
        let conn = self.conn.lock();
        let owner: Option<String> = conn
            .query_row(
                "SELECT user_id FROM sessions WHERE session_key = ?1",
                [session_key],
                |row| row.get(0),
            )
            .context("Create a session before uploading images")?;
        ensure!(
            owner.as_deref() == Some(user_id),
            "Session is not owned by this user"
        );
        let pending: usize = conn.query_row(
            "SELECT COUNT(*) FROM image_attachments a WHERE a.session_key = ?1 AND NOT EXISTS (
                SELECT 1 FROM messages m, json_each(m.attachments_json) j WHERE json_extract(j.value, '$.id') = a.id
            )", [session_key], |row| row.get(0),
        )?;
        ensure!(
            pending < 32,
            "Too many unsent images; remove drafts or retry after cleanup"
        );
        let id = format!("att_{}", uuid::Uuid::new_v4().simple());
        let metadata = ImageAttachment {
            id,
            mime_type,
            size_bytes: bytes.len() as u64,
            width,
            height,
            resolved_path: None,
        };
        std::fs::create_dir_all(&self.image_dir)?;
        let path = self.image_dir.join(&metadata.id);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let result = (|| -> anyhow::Result<()> {
            file.write_all(bytes)?;
            file.sync_all()?;
            conn.execute("INSERT INTO image_attachments (id, session_key, user_id, metadata_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![metadata.id, session_key, user_id, serde_json::to_string(&metadata)?, chrono::Utc::now().timestamp()])?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(path);
        }
        result?;
        Ok(metadata)
    }

    pub(super) fn find_images(
        &self,
        session_key: &str,
        user_id: &str,
        ids: &[String],
    ) -> anyhow::Result<Vec<ImageAttachment>> {
        let conn = self.conn.lock();
        let mut images = Vec::with_capacity(ids.len());
        for id in ids {
            ensure!(
                id.starts_with("att_")
                    && id.len() == 36
                    && id[4..].bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid attachment ID"
            );
            let json: String = conn.query_row(
                "SELECT a.metadata_json FROM image_attachments a JOIN sessions s ON s.session_key = a.session_key
                 WHERE a.id = ?1 AND a.session_key = ?2 AND a.user_id = ?3 AND s.user_id = ?3",
                params![id, session_key, user_id], |row| row.get(0),
            ).context("Image attachment is unavailable or belongs to another session")?;
            let mut image: ImageAttachment = serde_json::from_str(&json)?;
            let path = self.image_dir.join(id);
            ensure!(path.is_file(), "Image attachment file is unavailable");
            image.resolved_path = Some(path);
            images.push(image);
        }
        Ok(images)
    }

    pub(super) fn collect_images(&self) -> anyhow::Result<usize> {
        let conn = self.conn.lock();
        let cutoff = chrono::Utc::now().timestamp() - 24 * 60 * 60;
        let ids: Vec<String> = conn.prepare(
            "SELECT a.id FROM image_attachments a WHERE
             (a.created_at < ?1 OR NOT EXISTS (SELECT 1 FROM sessions s WHERE s.session_key = a.session_key))
             AND NOT EXISTS (SELECT 1 FROM messages m, json_each(m.attachments_json) j
                             WHERE json_extract(j.value, '$.id') = a.id)",
        )?.query_map([cutoff], |row| row.get(0))?.collect::<Result<_, _>>()?;
        for id in &ids {
            match std::fs::remove_file(self.image_dir.join(id)) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
            conn.execute("DELETE FROM image_attachments WHERE id = ?1", [id])?;
        }
        Ok(ids.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_backend::SessionBackend;
    use clawseed_api::provider::ChatMessage;

    fn png() -> Vec<u8> {
        let image = image::RgbaImage::new(2, 3);
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn image_validation_checks_actual_content() {
        assert_eq!(inspect_image(&png()).unwrap(), ("image/png".into(), 2, 3));
        assert!(inspect_image(b"not a png").is_err());
        assert!(inspect_image(&png()[..20]).is_err());
        assert!(inspect_image(&vec![0; MAX_IMAGE_BYTES + 1]).is_err());
    }

    #[test]
    fn images_are_durable_scoped_and_collected_only_when_unreferenced() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = SqliteSessionBackend::new(tmp.path()).unwrap();
        backend.bind_session_user("gw_a", "alice").unwrap();
        backend.bind_session_user("gw_b", "bob").unwrap();
        assert!(backend.upload_image("gw_a", "bob", &png()).is_err());
        let image = backend.upload_image("gw_a", "alice", &png()).unwrap();
        assert!(
            backend
                .resolve_images("gw_b", "alice", std::slice::from_ref(&image.id))
                .is_err()
        );
        assert!(
            backend
                .resolve_images("gw_a", "bob", std::slice::from_ref(&image.id))
                .is_err()
        );
        assert!(
            backend
                .resolve_images("gw_a", "alice", &["../../etc/passwd".into()])
                .is_err()
        );
        let mut message = ChatMessage::user("");
        message.attachments = vec![image.clone()];
        backend.append("gw_a", &message).unwrap();
        let unsent = backend.upload_image("gw_a", "alice", &png()).unwrap();
        backend
            .conn
            .lock()
            .execute("UPDATE image_attachments SET created_at = 0", [])
            .unwrap();
        assert_eq!(backend.cleanup_images().unwrap(), 1);
        assert!(
            backend
                .resolve_images("gw_a", "alice", &[unsent.id])
                .is_err()
        );
        drop(backend);
        let backend = SqliteSessionBackend::new(tmp.path()).unwrap();
        let resolved = backend
            .resolve_images("gw_a", "alice", std::slice::from_ref(&image.id))
            .unwrap();
        let path = resolved[0].resolved_path.as_ref().unwrap();
        assert_eq!(std::fs::read(path).unwrap(), png());
        backend.delete_session("gw_a").unwrap();
        assert_eq!(backend.cleanup_images().unwrap(), 1);
        assert!(!path.exists());
    }
}
