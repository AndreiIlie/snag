use tiny_skia::{FillRule, Paint, Path, PixmapMut, Stroke, Transform};

/// At capture resolution tiny-skia must see the same clip in the frame, the crop
/// and a mosaic sample. Clipping curves to different pixmap edges changes their
/// subdivision and hence coverage even a few pixels away from the clipped edge.
pub(super) fn draw(
    target: &mut PixmapMut<'_>,
    transform: Transform,
    path: Path,
    paint: &Paint<'_>,
    stroke: Option<&Stroke>,
) {
    if transform.sx != 1.0 || transform.sy != 1.0 {
        if let Some(stroke) = stroke {
            target.stroke_path(&path, paint, stroke, transform, None);
        } else {
            target.fill_path(&path, paint, FillRule::Winding, transform, None);
        }
        return;
    }
    let path = match stroke {
        Some(stroke) => match path.stroke(stroke, 1.) {
            Some(outline) => outline,
            None => return,
        },
        None => path,
    };
    let bounds = path.bounds();
    let origin_x = -(transform.tx as i64);
    let origin_y = -(transform.ty as i64);
    let width = target.width() as usize;
    let left = (bounds.left().floor() as i64).max(origin_x);
    let top = (bounds.top().floor() as i64).max(origin_y);
    let right = (bounds.right().ceil() as i64).min(origin_x + width as i64);
    let bottom = (bounds.bottom().ceil() as i64).min(origin_y + i64::from(target.height()));
    if left >= right || top >= bottom {
        return;
    }

    const SIDE: usize = 128;
    let side = SIDE as i64;
    let mut storage = [0u8; SIDE * SIDE * 4];
    let mut tile = PixmapMut::from_bytes(&mut storage, SIDE as u32, SIDE as u32).unwrap();
    let pixels = target.pixels_mut();
    for y in (top.div_euclid(side) * side..bottom).step_by(SIDE) {
        for x in (left.div_euclid(side) * side..right).step_by(SIDE) {
            let x0 = x.max(left);
            let len = ((x + side).min(right) - x0) as usize;
            let rows = y.max(top)..(y + side).min(bottom);
            let offsets = rows.map(|row| {
                let dest = (row - origin_y) as usize * width + (x0 - origin_x) as usize;
                let local = (row - y) as usize * SIDE + (x0 - x) as usize;
                (dest..dest + len, local..local + len)
            });
            for (dest, local) in offsets.clone() {
                tile.pixels_mut()[local].copy_from_slice(&pixels[dest]);
            }
            tile.fill_path(
                &path,
                paint,
                FillRule::Winding,
                Transform::from_translate(-(x as f32), -(y as f32)),
                None,
            );
            for (dest, local) in offsets {
                pixels[dest].copy_from_slice(&tile.as_ref().pixels()[local]);
            }
        }
    }
}
