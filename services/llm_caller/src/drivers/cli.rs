use crate::drivers::ImageArg;
use sdk::RpcError;
use tokio::process::Command;

fn ext_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" | "image/jpg" => ".jpg",
        "image/webp" => ".webp",
        "image/gif" => ".gif",
        _ => ".png",
    }
}

/// Writes each image as `image-N.<ext>` into a fresh temp folder (deleted on `Drop`, RAII like C# `using`),
/// runs `program` with that folder as its working directory, and returns stdout.
/// `build_args` receives the file names (relative to the folder) so the prompt can mention them.
pub async fn run_cli<F>(program: &str, images: &[ImageArg], build_args: F) -> Result<String, RpcError>
where
    F: FnOnce(&[String]) -> Vec<String>,
{
    let dir = tempfile::Builder::new()
        .prefix("wgic-llm-")
        .tempdir()
        .map_err(|e| RpcError::internal(format!("tempdir create failed: {e}")))?;

    let mut names = Vec::with_capacity(images.len());
    for (i, img) in images.iter().enumerate() {
        let name = format!("image-{}{}", i + 1, ext_for_mime(&img.mime_type));
        std::fs::write(dir.path().join(&name), &img.data)
            .map_err(|e| RpcError::internal(format!("image write failed: {e}")))?;
        names.push(name);
    }
    let args = build_args(&names);

    let output = Command::new(program)
        .args(&args)
        .current_dir(dir.path())
        .output()
        .await
        .map_err(|e| RpcError::internal(format!("failed to spawn '{program}': {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(RpcError::internal(format!("'{program}' exited with {}: {}", output.status, stderr.trim())));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// "The attached images are the files image-1.png, ... in the current folder." (empty without images)
pub fn images_note(names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    format!("\n\nThe attached images are the files {} in the current folder. Open only these files.", names.join(", "))
}
