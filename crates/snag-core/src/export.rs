use crate::{
    annotation::Document,
    geometry::PixelRect,
    image::{ImageError, RgbaImage},
    render::{
        annotations::draw_layer,
        mosaic::{Grid, Mosaic, capture_area, local_transform, premultiply},
    },
};
use png::{BitDepth, ColorType, Compression, Encoder, EncodingError, Filter};
use std::{error::Error, fmt, io::Write};

#[derive(Debug)]
pub enum ExportError {
    Image(ImageError),
    Decode(png::DecodingError),
    Png(EncodingError),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Image(e) => e.fmt(f),
            Self::Decode(e) => write!(f, "could not decode PNG: {e}"),
            Self::Png(e) => write!(f, "could not encode PNG: {e}"),
        }
    }
}
impl Error for ExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Image(e) => Some(e),
            Self::Decode(e) => Some(e),
            Self::Png(e) => Some(e),
        }
    }
}

/// Decode the first PNG frame to straight-alpha RGBA8.
pub fn decode_png(bytes: &[u8]) -> Result<RgbaImage, ExportError> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().map_err(ExportError::Decode)?;
    let size = reader
        .output_buffer_size()
        .ok_or(ExportError::Image(ImageError::InvalidDimensions))?;
    let mut pixels = vec![0; size];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(ExportError::Decode)?;
    reader.finish().map_err(ExportError::Decode)?;
    pixels.truncate(frame.buffer_size());
    if frame.color_type == ColorType::GrayscaleAlpha {
        let count = pixels.len() / 2;
        pixels.resize(
            count
                .checked_mul(4)
                .ok_or(ExportError::Image(ImageError::InvalidDimensions))?,
            0,
        );
        for i in (0..count).rev() {
            let gray = pixels[2 * i];
            let alpha = pixels[2 * i + 1];
            pixels[4 * i..4 * i + 4].copy_from_slice(&[gray, gray, gray, alpha]);
        }
    }
    RgbaImage::new(frame.width, frame.height, pixels).map_err(ExportError::Image)
}

/// Encode straight-alpha source rows. A crop uses the same row iterator as image cropping.
pub fn encode_png(image: &RgbaImage, crop: Option<PixelRect>) -> Result<Vec<u8>, ExportError> {
    let rect = crop.unwrap_or_else(|| {
        PixelRect::new(0, 0, image.width(), image.height()).expect("nonempty image")
    });
    let rows = image.rows(rect).map_err(ExportError::Image)?;
    encode_rows(rect, rows)
}

fn encode_rows<'a>(
    rect: PixelRect,
    rows: impl Iterator<Item = &'a [u8]>,
) -> Result<Vec<u8>, ExportError> {
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

// A later mosaic can depend on whole blocks from an earlier one outside the crop.
fn required_blocks(
    image: &RgbaImage,
    doc: &Document,
    crop: PixelRect,
) -> (PixelRect, Vec<Option<Grid>>) {
    let mut area = crop;
    let mut grids: Vec<_> = doc
        .layers
        .iter()
        .rev()
        .map(|shape| {
            let grid = Grid::new(shape, image)?.reaching(area)?;
            area = area.union(grid.rect);
            Some(grid)
        })
        .collect();
    grids.reverse();
    (area, grids)
}

/// Flatten the document in capture coordinates and encode only the requested rows.
pub fn encode_annotated_png(
    image: &RgbaImage,
    doc: &Document,
    crop: Option<PixelRect>,
) -> Result<Vec<u8>, ExportError> {
    if doc.layers.is_empty() {
        return encode_png(image, crop);
    }
    let crop = crop.unwrap_or_else(|| PixelRect::new(0, 0, image.width(), image.height()).unwrap());
    let source_rows = image.rows(crop).map_err(ExportError::Image)?;
    let (area, grids) = required_blocks(image, doc, crop);
    let mut target =
        capture_area(image, area).ok_or(ExportError::Image(ImageError::InvalidDimensions))?;
    let ts = local_transform(area);
    let mut count = 0;
    for (shape, grid) in doc.layers.iter().zip(&grids) {
        let mosaic = grid.map(|grid| {
            Mosaic::sample(grid, |x, y| {
                target.pixel(x - area.left(), y - area.top()).unwrap()
            })
        });
        draw_layer(&mut target.as_mut(), ts, shape, &mut count, mosaic.as_ref());
    }
    let stride = area.width() as usize * 4;
    let left = (crop.left() - area.left()) as usize * 4;
    let right = left + crop.width() as usize * 4;
    let mut data = target.take();
    let rows = data
        .chunks_exact_mut(stride)
        .skip((crop.top() - area.top()) as usize)
        .zip(source_rows)
        .zip(crop.top()..crop.bottom())
        .map(|((row, src), y)| {
            let row = &mut row[left..right];
            for ((out, original), x) in row
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(src.as_chunks::<4>().0)
                .zip(crop.left()..crop.right())
            {
                if out[3] == 255 {
                    continue;
                }
                let in_mosaic = grids.iter().flatten().any(|g| {
                    (g.rect.top()..g.rect.bottom()).contains(&y)
                        && (g.rect.left()..g.rect.right()).contains(&x)
                });
                let [r, g, b, a] = *out;
                let composed = tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, a).unwrap();
                if !in_mosaic && composed == premultiply(original) {
                    *out = *original;
                } else {
                    let c = composed.demultiply();
                    *out = [c.red(), c.green(), c.blue(), c.alpha()];
                }
            }
            &row[..]
        });
    encode_rows(crop, rows)
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
            assert_eq!(decode_png(&png).unwrap(), expected);
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
    #[test]
    fn export_composes_only_the_mosaic_blocks_reaching_the_crop() {
        use crate::{
            annotation::{Annotation, Style, Tool},
            geometry::PixelPoint,
        };
        let image = RgbaImage::new(400, 400, vec![0; 400 * 400 * 4]).unwrap();
        let mut patch = Annotation::start(
            Tool::Pixelate,
            Style::default(),
            1.,
            PixelPoint { x: 200, y: 100 },
        );
        patch.extend(PixelPoint { x: 390, y: 390 });
        let doc = Document {
            layers: vec![patch],
        };
        let crop = PixelRect::new(2, 2, 8, 8).unwrap();
        assert_eq!(required_blocks(&image, &doc, crop).0, crop);
        let crop = PixelRect::new(202, 102, 205, 105).unwrap();
        assert_eq!(
            required_blocks(&image, &doc, crop).0,
            PixelRect::new(200, 100, 212, 112).unwrap()
        );
    }

    #[test]
    fn decodes_greyscale_and_rejects_invalid_png() {
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(&mut bytes, 2, 1);
        encoder.set_color(ColorType::Grayscale);
        encoder.set_depth(BitDepth::Sixteen);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[32, 0, 210, 0]).unwrap();
        writer.finish().unwrap();
        assert_eq!(
            decode_png(&bytes).unwrap().pixels(),
            &[32, 32, 32, 255, 210, 210, 210, 255]
        );
        assert!(decode_png(b"invalid").is_err());
    }
}
