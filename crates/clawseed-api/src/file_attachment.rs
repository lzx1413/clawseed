//! Client-owned documents. The gateway retains metadata, never arbitrary client paths.
use serde::{Deserialize, Serialize};

pub const MAX_FILES: usize = 4;
pub const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_MESSAGE_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_EXCERPT_CHARS: usize = 8_000;
pub const MAX_MESSAGE_CHARS: usize = 16_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentReadResult {
    pub attachment_id: String,
    pub unit: String,
    pub start: usize,
    pub next: usize,
    pub total: usize,
    pub eof: bool,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileAttachment {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub format: String,
    #[serde(default = "ready_status")]
    pub status: String,
    pub unit: String,
    pub total: usize,
    pub excerpt: AttachmentReadResult,
}

fn ready_status() -> String {
    "ready".into()
}

pub fn validate_files(files: &[FileAttachment]) -> anyhow::Result<()> {
    anyhow::ensure!(files.len() <= MAX_FILES, "At most 4 files may be attached");
    let mut ids = std::collections::HashSet::new();
    let mut bytes = 0u64;
    let mut chars = 0usize;
    for file in files {
        anyhow::ensure!(
            file.id.len() == 37
                && file.id.starts_with("file_")
                && file.id[5..].bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid file attachment ID"
        );
        anyhow::ensure!(ids.insert(&file.id), "Duplicate file attachment ID");
        anyhow::ensure!(
            !file.name.is_empty() && file.name.chars().count() <= 255,
            "Invalid file name"
        );
        anyhow::ensure!(
            file.size_bytes > 0 && file.size_bytes <= MAX_FILE_BYTES,
            "File exceeds 20 MiB or is empty"
        );
        let (mime, unit) = match file.format.as_str() {
            "md" => ("text/markdown", "character"),
            "txt" => ("text/plain", "character"),
            "csv" => ("text/csv", "record"),
            "pdf" => ("application/pdf", "page"),
            "docx" => (
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "character",
            ),
            _ => anyhow::bail!("Unsupported file format"),
        };
        anyhow::ensure!(
            file.mime_type == mime && file.unit == unit && file.status == "ready",
            "Invalid file metadata or parse status"
        );
        anyhow::ensure!(
            file.total > 0 && file.total <= 2_000_000 && (unit != "page" || file.total <= 300),
            "Invalid file unit count"
        );
        let excerpt = &file.excerpt;
        anyhow::ensure!(
            excerpt.attachment_id == file.id
                && excerpt.unit == file.unit
                && excerpt.total == file.total
                && excerpt.start == 0
                && excerpt.next <= file.total
                && excerpt.eof == (excerpt.next == file.total),
            "Invalid file excerpt range"
        );
        let length = excerpt.content.chars().count();
        anyhow::ensure!(
            length <= MAX_EXCERPT_CHARS && (excerpt.next != 0 || length == 0),
            "File excerpt exceeds budget or has invalid range"
        );
        anyhow::ensure!(
            unit != "character" || length == excerpt.next,
            "Text excerpt offsets must count Unicode characters"
        );
        bytes += file.size_bytes;
        chars += length;
    }
    anyhow::ensure!(bytes <= MAX_MESSAGE_BYTES, "Message files exceed 50 MiB");
    anyhow::ensure!(
        chars <= MAX_MESSAGE_CHARS,
        "Message file excerpts exceed 16000 characters"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn omitted_default_status_is_compatible_but_explicit_failure_is_rejected() {
        let mut json = serde_json::to_value(sample()).unwrap();
        json.as_object_mut().unwrap().remove("status");
        let file: FileAttachment = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(file.status, "ready");
        validate_files(&[file]).unwrap();
        json["status"] = "failed".into();
        let file: FileAttachment = serde_json::from_value(json).unwrap();
        assert!(validate_files(&[file]).is_err());
    }
    fn sample() -> FileAttachment {
        FileAttachment {
            id: format!("file_{}", "a".repeat(32)),
            name: "中文 file.md".into(),
            mime_type: "text/markdown".into(),
            size_bytes: 9,
            format: "md".into(),
            status: "ready".into(),
            unit: "character".into(),
            total: 3,
            excerpt: AttachmentReadResult {
                attachment_id: format!("file_{}", "a".repeat(32)),
                unit: "character".into(),
                start: 0,
                next: 3,
                total: 3,
                eof: true,
                content: "甲😀乙".into(),
            },
        }
    }
    #[test]
    fn validates_unicode_ranges_and_rejects_forged_metadata() {
        let file = sample();
        validate_files(std::slice::from_ref(&file)).unwrap();
        assert!(validate_files(&[file.clone(), file.clone()]).is_err());
        let mut invalid = file.clone();
        invalid.excerpt.next = 2;
        assert!(validate_files(&[invalid]).is_err());
        let mut invalid = file;
        invalid.id = "../../etc/passwd".into();
        assert!(validate_files(&[invalid]).is_err());
    }
}
