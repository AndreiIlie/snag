use crate::geometry::PixelRect;
use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    InvalidDimensions,
    InvalidLength,
    CropOutsideImage,
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidDimensions => "invalid image dimensions",
            Self::InvalidLength => "pixel buffer length does not match the image dimensions",
            Self::CropOutsideImage => "crop extends outside the image",
        })
    }
}

impl Error for ImageError {}

/// Row-major, straight-alpha RGBA8 with no row padding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RgbaImage {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, ImageError> {
        let length = (width as usize)
            .checked_mul(height as usize)
            .and_then(|area| area.checked_mul(4))
            .filter(|&length| length > 0)
            .ok_or(ImageError::InvalidDimensions)?;
        if length != pixels.len() {
            return Err(ImageError::InvalidLength);
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub(crate) fn rows(&self, rect: PixelRect) -> Result<impl Iterator<Item = &[u8]>, ImageError> {
        if rect.right() > self.width || rect.bottom() > self.height {
            return Err(ImageError::CropOutsideImage);
        }
        let start = rect.left() as usize * 4;
        let end = rect.right() as usize * 4;
        Ok(self
            .pixels
            .chunks_exact(self.width as usize * 4)
            .skip(rect.top() as usize)
            .take(rect.height() as usize)
            .map(move |row| &row[start..end]))
    }

    pub fn crop(&self, rect: PixelRect) -> Result<Self, ImageError> {
        let rows = self.rows(rect)?;
        let mut pixels = Vec::with_capacity(rect.width() as usize * rect.height() as usize * 4);
        for row in rows {
            pixels.extend_from_slice(row);
        }
        Self::new(rect.width(), rect.height(), pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dimensions_and_length() {
        assert_eq!(
            RgbaImage::new(0, 1, vec![]),
            Err(ImageError::InvalidDimensions)
        );
        assert_eq!(
            RgbaImage::new(u32::MAX, u32::MAX, vec![]),
            Err(ImageError::InvalidDimensions)
        );
        assert_eq!(
            RgbaImage::new(2, 2, vec![0; 15]),
            Err(ImageError::InvalidLength)
        );
    }

    #[test]
    fn crop_copies_only_the_selected_columns_and_rows() {
        let image = RgbaImage::new(3, 3, (0..36).collect()).unwrap();
        let crop = image.crop(PixelRect::new(1, 1, 3, 3).unwrap()).unwrap();
        assert_eq!((crop.width(), crop.height()), (2, 2));
        assert_eq!(crop.pixels(), (16..24).chain(28..36).collect::<Vec<_>>());
        assert_eq!(
            image.crop(PixelRect::new(2, 1, 4, 2).unwrap()),
            Err(ImageError::CropOutsideImage)
        );
    }
}
