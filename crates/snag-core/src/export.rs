use crate::{
    geometry::PixelRect,
    image::{ImageError, RgbaImage},
};
use png::{BitDepth, ColorType, Compression, Encoder, EncodingError, Filter};
use std::{error::Error, fmt, io::Write};

#[derive(Debug)]
pub enum ExportError {
    Image(ImageError),
    Png(EncodingError),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Image(e) => e.fmt(f),
            Self::Png(e) => write!(f, "could not encode PNG: {e}"),
        }
    }
}
impl Error for ExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Image(e) => Some(e),
            Self::Png(e) => Some(e),
        }
    }
}

/// Encode straight-alpha source rows. A crop uses the same row iterator as image cropping.
pub fn encode_png(image: &RgbaImage, crop: Option<PixelRect>) -> Result<Vec<u8>, ExportError> {
    let rect = crop.unwrap_or_else(|| {
        PixelRect::new(0, 0, image.width(), image.height()).expect("nonempty image")
    });
    let rows = image.rows(rect).map_err(ExportError::Image)?;
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, rect.width(), rect.height());
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    encoder.set_compression(Compression::Fast);
    encoder.set_filter(Filter::Sub);
    let mut writer = encoder.write_header().map_err(ExportError::Png)?;
    let mut stream = writer.stream_writer().map_err(ExportError::Png)?;
    for row in rows {
        stream
            .write_all(row)
            .map_err(|e| ExportError::Png(e.into()))?;
    }
    stream.finish().map_err(ExportError::Png)?;
    writer.finish().map_err(ExportError::Png)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn whole_image_and_crop_round_trip_with_straight_alpha() {
        let mut pixels: Vec<u8> = (0..24).collect();
        pixels[4..8].copy_from_slice(&[255, 0, 0, 128]);
        let image = RgbaImage::new(3, 2, pixels).unwrap();
        for crop in [None, PixelRect::new(1, 0, 3, 2)] {
            let expected = crop
                .map(|rect| image.crop(rect).unwrap())
                .unwrap_or_else(|| image.clone());
            let png = encode_png(&image, crop).unwrap();
            assert_eq!(png, encode_png(&image, crop).unwrap());
            let mut reader = png::Decoder::new(Cursor::new(png)).read_info().unwrap();
            let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut decoded).unwrap();
            assert_eq!(
                (info.width, info.height),
                (expected.width(), expected.height())
            );
            assert_eq!(info.color_type, ColorType::Rgba);
            assert_eq!(info.bit_depth, BitDepth::Eight);
            assert_eq!(decoded, expected.pixels());
        }
    }
}
