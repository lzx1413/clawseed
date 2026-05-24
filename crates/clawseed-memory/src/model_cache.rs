//! Model download and cache for local ONNX embedding models.
//!
//! Models are stored in `{workspace_dir}/models/{model_name}/`.
//! If missing, they are downloaded from HuggingFace.
//! ONNX Runtime shared library (`libonnxruntime.so`) is also cached here on desktop,
//! downloaded from the ort-sys CDN. On Android, the .so is bundled in jniLibs.

use anyhow::Context;
use std::path::{Path, PathBuf};

/// Known model configurations: download URLs and file names.
struct ModelSpec {
    /// HuggingFace model repo ID (e.g. "clawseed/gte-multilingual-base-onnx-int8")
    repo_id: &'static str,
    /// ONNX model file name in the repo
    onnx_file: &'static str,
    /// Tokenizer file name in the repo
    tokenizer_file: &'static str,
}

/// Registry of supported local embedding models.
fn model_spec(model_name: &str) -> Option<ModelSpec> {
    match model_name {
        "gte-multilingual-base" => Some(ModelSpec {
            repo_id: "clawseed/gte-multilingual-base-onnx-int8",
            onnx_file: "model_int8.onnx",
            tokenizer_file: "tokenizer.json",
        }),
        "gte-multilingual-base-full" => Some(ModelSpec {
            repo_id: "clawseed/gte-multilingual-base-onnx",
            onnx_file: "model.onnx",
            tokenizer_file: "tokenizer.json",
        }),
        _ => None,
    }
}

/// Ensure model files are available in `model_dir`.
///
/// If the model directory doesn't exist or is missing files, downloads them
/// from HuggingFace. Returns the model directory path on success.
pub async fn ensure_model_available(model_name: &str, model_dir: &Path) -> anyhow::Result<PathBuf> {
    let spec = model_spec(model_name).with_context(|| {
        format!("Unknown local embedding model '{model_name}'. Supported: gte-multilingual-base, gte-multilingual-base-full")
    })?;

    let onnx_path = model_dir.join(spec.onnx_file);
    let tokenizer_path = model_dir.join(spec.tokenizer_file);

    if onnx_path.exists() && tokenizer_path.exists() {
        tracing::info!(
            "Local embedding model already available at {}",
            model_dir.display()
        );
        return Ok(model_dir.to_path_buf());
    }

    // Create model directory
    std::fs::create_dir_all(model_dir)?;

    tracing::info!(
        "Downloading local embedding model '{}' to {}",
        model_name,
        model_dir.display()
    );

    let client = clawseed_config::schema::build_runtime_proxy_client_with_timeouts(
        "model-download",
        300, // 5 min timeout for large model downloads
        30,
    );

    let base_url = format!("https://huggingface.co/{}/resolve/main", spec.repo_id);

    // Download ONNX model
    download_file(
        &client,
        &format!("{}/{}", base_url, spec.onnx_file),
        &onnx_path,
    )
    .await?;

    // Download tokenizer
    download_file(
        &client,
        &format!("{}/{}", base_url, spec.tokenizer_file),
        &tokenizer_path,
    )
    .await?;

    tracing::info!(
        "Local embedding model downloaded successfully ({} bytes)",
        onnx_path.metadata().map(|m| m.len()).unwrap_or(0)
    );

    Ok(model_dir.to_path_buf())
}

/// Platform-specific ONNX Runtime download URL from ort-sys CDN.
#[cfg(not(target_os = "android"))]
fn ort_dist_url() -> &'static str {
    // URLs from ort-sys v2.0.0-rc.12 dist.txt (ONNX Runtime 1.24.2)
    match () {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        () => "https://cdn.pyke.io/0/pyke:ort-rs/ms@1.24.2/x86_64-unknown-linux-gnu.tar.lzma2",
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        () => "https://cdn.pyke.io/0/pyke:ort-rs/ms@1.24.2/aarch64-unknown-linux-gnu.tar.lzma2",
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        () => "https://cdn.pyke.io/0/pyke:ort-rs/ms@1.24.2/aarch64-apple-darwin.tar.lzma2",
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        () => "https://cdn.pyke.io/0/pyke:ort-rs/ms@1.24.2/x86_64-apple-darwin+wgpu.tar.lzma2",
    }
}

/// Ensure ONNX Runtime shared library is available in `model_dir`.
///
/// On desktop: downloads the ort-sys CDN archive and extracts `libonnxruntime.so`.
/// On Android: skipped (the .so is bundled in nativeLibraryDir by the build system).
/// The .so is needed for ort's `load-dynamic` feature to work at runtime.
#[cfg(not(target_os = "android"))]
pub async fn ensure_ort_lib_available(model_dir: &Path) -> anyhow::Result<PathBuf> {
    let ort_lib_path = model_dir.join("libonnxruntime.so");
    if ort_lib_path.exists() {
        tracing::info!("ONNX Runtime .so already available at {}", ort_lib_path.display());
        return Ok(ort_lib_path);
    }

    std::fs::create_dir_all(model_dir)?;

    tracing::info!("Downloading ONNX Runtime shared library to {}", model_dir.display());

    let client = clawseed_config::schema::build_runtime_proxy_client_with_timeouts(
        "ort-download",
        300,
        30,
    );

    let url = ort_dist_url();
    let resp = client.get(url).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Failed to download ONNX Runtime from {url}: HTTP {status} — {text}");
    }

    let compressed = resp.bytes().await?;

    // Decompress lzma2 and extract tar archive.
    // lzma_rust2::Lzma2Reader implements std::io::Read for lzma2 streams.
    let mut decompressed = Vec::new();
    {
        let mut reader = lzma_rust2::Lzma2Reader::new(&compressed[..], 1 << 26, None);
        std::io::Read::read_to_end(&mut reader, &mut decompressed)?;
    }
    let mut archive = tar::Archive::new(decompressed.as_slice());
    archive.unpack(model_dir)?;

    // Verify .so exists after extraction
    if !ort_lib_path.exists() {
        // Walk subdirectories recursively
        let found = find_file_recursive(model_dir, "libonnxruntime.so");
        if let Some(found_path) = found {
            std::fs::copy(&found_path, &ort_lib_path)?;
        } else {
            tracing::warn!(
                "libonnxruntime.so not found in extracted archive at {}. \
                 ONNX Runtime will attempt to load from system paths.",
                model_dir.display()
            );
        }
    }

    if ort_lib_path.exists() {
        tracing::info!(
            "ONNX Runtime .so available at {} ({} bytes)",
            ort_lib_path.display(),
            ort_lib_path.metadata().map(|m| m.len()).unwrap_or(0)
        );
    }

    Ok(ort_lib_path)
}

#[cfg(not(target_os = "android"))]
fn find_file_recursive(dir: &Path, filename: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_recursive(&path, filename) {
                return Some(found);
            }
        } else if path.file_name().map(|n| n == filename).unwrap_or(false) {
            return Some(path);
        }
    }
    None
}

async fn download_file(client: &reqwest::Client, url: &str, dest: &Path) -> anyhow::Result<()> {
    use futures_util::StreamExt;
    use std::io::Write;

    tracing::info!("Downloading {} → {}", url, dest.display());
    let resp = client.get(url).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let filename = dest.file_name().unwrap_or_default().to_string_lossy();
        tracing::info!(
            "EMBEDDING_DOWNLOAD_ERROR:{}:HTTP {}",
            filename,
            status.as_u16()
        );
        anyhow::bail!("Failed to download {url}: HTTP {status} — {text}");
    }

    let total_size = resp.content_length();
    let filename = dest.file_name().unwrap_or_default().to_string_lossy();

    tracing::info!(
        "EMBEDDING_DOWNLOAD_START:{}:{}",
        filename,
        total_size.map(|s| s.to_string()).unwrap_or_else(|| "-1".to_string())
    );

    let mut file = std::fs::File::create(dest)?;
    let mut downloaded: u64 = 0;
    let mut last_reported_percent: u32 = 0;
    let mut stream = resp.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                tracing::info!(
                    "EMBEDDING_DOWNLOAD_ERROR:{}:{}",
                    filename,
                    e.to_string().chars().take(200).collect::<String>()
                );
                anyhow::bail!("Download stream error for {url}: {e}");
            }
        };

        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;

        let should_report = match total_size {
            Some(total) if total > 0 => {
                let percent = ((downloaded as f64 / total as f64) * 100.0) as u32;
                percent >= last_reported_percent + 5 || downloaded >= total
            }
            _ => {
                // Unknown size: report every 1MB milestone
                downloaded / 1_000_000 > (downloaded - chunk.len() as u64) / 1_000_000
            }
        };

        if should_report {
            let percent = total_size
                .filter(|&t| t > 0)
                .map(|total| ((downloaded as f64 / total as f64) * 100.0) as u32)
                .unwrap_or(0);
            last_reported_percent = percent;

            tracing::info!(
                "EMBEDDING_DOWNLOAD_PROGRESS:{}:{}:{}:{}",
                percent,
                downloaded,
                total_size.map(|s| s.to_string()).unwrap_or_else(|| "-1".to_string()),
                filename
            );
        }
    }

    tracing::info!(
        "EMBEDDING_DOWNLOAD_COMPLETE:{}:{}",
        filename,
        downloaded
    );

    Ok(())
}
