//! Bounded, off-thread image decoding and a size-limited disk derivative cache.
use madari_model::{Error, ErrorCode, Result};
use madari_native::NativeHttp;
use sha2::{Digest, Sha256};
use std::{
    io::Cursor,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::Semaphore;

const DISK_LIMIT: u64 = 192 * 1024 * 1024;
pub struct Pixels {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}
pub struct ArtworkCache {
    http: NativeHttp,
    slots: Arc<Semaphore>,
    directory: PathBuf,
}
fn error(message: &str) -> Error {
    Error::new(ErrorCode::InvalidResponse, message)
}
impl ArtworkCache {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            http: NativeHttp::default(),
            slots: Arc::new(Semaphore::new(2)),
            directory,
        }
    }
    pub async fn load(&self, url: url::Url, allow_local: bool, edge: u32) -> Result<Pixels> {
        let permit = self
            .slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| error("Artwork loader closed"))?;
        let edge = edge.clamp(180, 1600);
        let name = format!(
            "{:x}.png",
            Sha256::digest(format!("v1:{allow_local}:{edge}:{url}"))
        );
        let path = self.directory.join(name);
        let fresh = tokio::fs::metadata(&path).await.ok().is_some_and(|m| {
            m.len() < 16 * 1024 * 1024
                && m.modified()
                    .ok()
                    .and_then(|t| SystemTime::now().duration_since(t).ok())
                    .is_some_and(|age| age < Duration::from_secs(7 * 86400))
        });
        let cached = if fresh {
            tokio::fs::read(&path).await.ok()
        } else {
            None
        };
        let from_disk = cached.is_some();
        let bytes = if let Some(bytes) = cached {
            bytes
        } else {
            self.http.get_artwork(url, allow_local).await?
        };
        let directory = self.directory.clone();
        tokio::task::spawn_blocking(move || {
            // Retain the permit inside the worker even if its awaiting task is cancelled.
            let _permit = permit;
            let pixels = decode(&bytes, edge)?;
            if !from_disk && madari_native::private_directory(&directory).is_ok() {
                let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
                if image::save_buffer_with_format(
                    &temporary,
                    &pixels.data,
                    pixels.width,
                    pixels.height,
                    image::ColorType::Rgba8,
                    image::ImageFormat::Png,
                )
                .is_ok()
                {
                    let _ = std::fs::rename(&temporary, &path);
                } else {
                    let _ = std::fs::remove_file(&temporary);
                }
                prune(&directory);
            }
            Ok(pixels)
        })
        .await
        .map_err(|_| error("Artwork worker stopped"))?
    }
}
fn decode(bytes: &[u8], edge: u32) -> Result<Pixels> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| error("Unknown image format"))?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(32 * 1024 * 1024);
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| error("Artwork is invalid or exceeds image limits"))?;
    let rgba = image.thumbnail(edge, edge).into_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(Pixels {
        data: rgba.into_raw(),
        width,
        height,
    })
}
fn prune(directory: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            if !m.is_file() || e.path().extension()?.to_str()? != "png" {
                return None;
            }
            Some((
                m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                m.len(),
                e.path(),
            ))
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    if total <= DISK_LIMIT {
        return;
    }
    files.sort_by_key(|(modified, _, _)| *modified);
    for (_, size, path) in files {
        if total <= DISK_LIMIT * 3 / 4 {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_decode_produces_small_texture_and_rejects_bad_data() {
        let image = image::RgbaImage::from_pixel(1200, 1800, image::Rgba([20, 30, 40, 255]));
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let pixels = decode(bytes.get_ref(), 360).unwrap();
        assert_eq!((pixels.width, pixels.height), (240, 360));
        assert_eq!(pixels.data.len(), 240 * 360 * 4);
        assert!(decode(b"not an image", 360).is_err());
    }
}
