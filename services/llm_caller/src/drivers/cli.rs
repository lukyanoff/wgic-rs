use crate::drivers::ImageArg;
use sdk::RpcError;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use tokio::process::Command;

fn ext_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" | "image/jpg" => ".jpg",
        "image/webp" => ".webp",
        "image/gif" => ".gif",
        _ => ".png",
    }
}

/// Automatically deletes all temporary image files when dropped!
pub struct TempImageGuard {
    _files: Vec<NamedTempFile>,
    pub paths: Vec<PathBuf>,
}

pub fn stage_temp_images(images: &[ImageArg]) -> Result<TempImageGuard, RpcError> {
    let mut files = Vec::with_capacity(images.len());
    let mut paths = Vec::with_capacity(images.len());

    for img in images {
        let tmp = tempfile::Builder::new()
            .prefix("wgic-img-")
            .suffix(ext_for_mime(&img.mime_type))
            .tempfile()
            .map_err(|e| RpcError::internal(format!("tempfile create failed: {e}")))?;

        std::fs::write(tmp.path(), &img.data)
            .map_err(|e| RpcError::internal(format!("tempfile write failed: {e}")))?;

        paths.push(tmp.path().to_path_buf());
        files.push(tmp);
    }

    Ok(TempImageGuard { _files: files, paths })
}

pub async fn run_cli<F>(
    program: &str,
    images: &[ImageArg],
    build_args: F,
) -> Result<String, RpcError>
where
    F: FnOnce(&[PathBuf]) -> Vec<String>,
{
    let guard = stage_temp_images(images)?;
    let args = build_args(&guard.paths);

    let output = Command::new(program)
        .args(&args)
        .output()
        .await
        .map_err(|e| RpcError::internal(format!("failed to spawn '{program}': {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(RpcError::internal(format!(
            "'{program}' exited with {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
