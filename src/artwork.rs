//! Scale-aware artwork rendering for catalog covers and thumbnails.

use gpui::{App, Asset, ImageCacheError, ImageSource, RenderImage, Window};
use image::{DynamicImage, GenericImageView, imageops::FilterType};
use smallvec::SmallVec;
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Hash, PartialEq, Eq)]
struct ArtworkSource {
    path: PathBuf,
    target_pixels: u32,
}

#[derive(Clone)]
struct ArtworkLoader;

impl Asset for ArtworkLoader {
    type Source = ArtworkSource;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    fn load(
        source: Self::Source,
        _: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        async move {
            load_cover(&source.path, source.target_pixels)
                .map(Arc::new)
                .map_err(|error| ImageCacheError::Other(Arc::new(error)))
        }
    }
}

/// Returns artwork prefiltered for the current display's physical pixel density.
///
/// GPUI's atlas sampler is bilinear and has no mipmaps, so directly minifying a
/// large cover can skip narrow details such as text. Resizing in the background
/// with Lanczos first gives the GPU a texture it can paint at approximately 1:1.
pub fn source(path: PathBuf, logical_size: f32) -> ImageSource {
    ImageSource::from(move |window: &mut Window, cx: &mut App| {
        let target_pixels = (logical_size * window.scale_factor()).round().max(1.0) as u32;
        window.use_asset::<ArtworkLoader>(
            &ArtworkSource {
                path: path.clone(),
                target_pixels,
            },
            cx,
        )
    })
}

fn load_cover(path: &PathBuf, target_pixels: u32) -> anyhow::Result<RenderImage> {
    let source = image::open(path)?;
    let resized = resize_cover(source, target_pixels);
    let mut pixels = resized.into_rgba8();

    // GPUI's polychrome atlas stores pixels as BGRA.
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    Ok(RenderImage::new(SmallVec::from_elem(
        image::Frame::new(pixels),
        1,
    )))
}

fn resize_cover(source: DynamicImage, target_pixels: u32) -> DynamicImage {
    let (width, height) = source.dimensions();
    let side = width.min(height);
    let x = (width - side) / 2;
    let y = (height - side) / 2;
    source.crop_imm(x, y, side, side).resize_exact(
        target_pixels,
        target_pixels,
        FilterType::Lanczos3,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn cover_resize_center_crops_before_filtering() {
        let source = RgbaImage::from_fn(6, 2, |x, _| {
            if x < 2 {
                Rgba([255, 0, 0, 255])
            } else if x < 4 {
                Rgba([0, 255, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });

        let resized = resize_cover(DynamicImage::ImageRgba8(source), 2).into_rgba8();

        assert_eq!(resized.dimensions(), (2, 2));
        assert!(
            resized
                .pixels()
                .all(|pixel| pixel[1] > pixel[0] && pixel[1] > pixel[2])
        );
    }
}
