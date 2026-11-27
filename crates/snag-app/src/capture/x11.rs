use snag_core::{geometry::ScaleFactor, image::RgbaImage, platform::Screenshot};
use std::error::Error;
use x11rb::{
    connection::Connection,
    image::{BitsPerPixel, Image, ImageOrder, PixelLayout},
    protocol::xproto::{ConnectionExt, VisualClass, Visualtype},
};

pub fn capture() -> Result<Screenshot, Box<dyn Error>> {
    let (conn, screen) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen];
    let root = conn.get_geometry(screen.root)?.reply()?;
    let (frame, visual_id) = Image::get(&conn, screen.root, 0, 0, root.width, root.height)?;
    let visual = screen
        .allowed_depths
        .iter()
        .flat_map(|d| &d.visuals)
        .find(|v| v.visual_id == visual_id)
        .ok_or("unknown root visual")?;
    Ok(Screenshot {
        image: rgba(&frame, *visual)?,
        scale: ScaleFactor::ONE,
    })
}

fn rgba(frame: &Image<'_>, visual: Visualtype) -> Result<RgbaImage, Box<dyn Error>> {
    if visual.class != VisualClass::TRUE_COLOR {
        return Err("unsupported root visual (expected TrueColor)".into());
    }
    let masks = (visual.red_mask, visual.green_mask, visual.blue_mask);
    let pixels = if frame.bits_per_pixel() == BitsPerPixel::B32
        && frame.byte_order() == ImageOrder::LsbFirst
        && masks == (0xff0000, 0x00ff00, 0x0000ff)
    {
        let len = usize::from(frame.width()) * usize::from(frame.height()) * 4;
        let (bgrx, _) = frame.data()[..len].as_chunks::<4>();
        bgrx.iter()
            .map(|&[b, g, r, _]| [r, g, b, 255])
            .collect::<Vec<_>>()
            .into_flattened()
    } else {
        decode_pixels(frame, PixelLayout::from_visual_type(visual)?)
    };
    Ok(RgbaImage::new(
        frame.width().into(),
        frame.height().into(),
        pixels,
    )?)
}

fn decode_pixels(frame: &Image<'_>, layout: PixelLayout) -> Vec<u8> {
    let mut pixels =
        Vec::with_capacity(usize::from(frame.width()) * usize::from(frame.height()) * 4);
    for y in 0..frame.height() {
        for x in 0..frame.width() {
            let (r, g, b) = layout.decode(frame.get_pixel(x, y));
            pixels.extend_from_slice(&[(r >> 8) as u8, (g >> 8) as u8, (b >> 8) as u8, 255]);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11rb::image::ScanlinePad;

    #[test]
    fn converts_standard_masks_in_both_byte_orders() {
        let visual = Visualtype {
            class: VisualClass::TRUE_COLOR,
            red_mask: 0xff0000,
            green_mask: 0x00ff00,
            blue_mask: 0x0000ff,
            ..Default::default()
        };
        for (order, bytes) in [
            (ImageOrder::LsbFirst, [3, 2, 1, 99, 6, 5, 4, 98]),
            (ImageOrder::MsbFirst, [99, 1, 2, 3, 98, 4, 5, 6]),
        ] {
            let mut data = bytes.to_vec();
            data.extend_from_slice(&[7, 8, 9, 10]);
            let frame = Image::new(
                2,
                1,
                ScanlinePad::Pad32,
                24,
                BitsPerPixel::B32,
                order,
                data.into(),
            )
            .unwrap();
            let image = rgba(&frame, visual).unwrap();
            assert_eq!(image.pixels(), &[1, 2, 3, 255, 4, 5, 6, 255]);
            let layout = PixelLayout::from_visual_type(visual).unwrap();
            assert_eq!(image.pixels(), decode_pixels(&frame, layout));
        }
    }

    #[test]
    fn converts_padded_rgb565_rows() {
        let frame = Image::new(
            1,
            2,
            ScanlinePad::Pad32,
            16,
            BitsPerPixel::B16,
            ImageOrder::MsbFirst,
            vec![0xf8, 0, 0xab, 0xcd, 0, 0x1f, 0xab, 0xcd].into(),
        )
        .unwrap();
        let mut visual = Visualtype {
            class: VisualClass::TRUE_COLOR,
            red_mask: 0xf800,
            green_mask: 0x07e0,
            blue_mask: 0x001f,
            ..Default::default()
        };
        assert_eq!(
            rgba(&frame, visual).unwrap().pixels(),
            &[255, 0, 0, 255, 0, 0, 255, 255]
        );
        visual.class = VisualClass::PSEUDO_COLOR;
        assert!(rgba(&frame, visual).is_err());
    }
}
