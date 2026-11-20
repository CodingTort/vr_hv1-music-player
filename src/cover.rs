//! Album covers shared by the GUI and the terminal client.

/// Fits a cover into a square without cutting it up. Covers from YouTube are
/// 16:9 thumbnails: either square artwork between flat bars (the bars are
/// trimmed off), or a full-width picture, which is shown whole over a
/// blurred, dimmed copy of itself rather than centre-cropped.
pub fn square_cover(img: &image::DynamicImage, size: u32) -> image::RgbaImage {
    use image::imageops::{self, FilterType};
    let rgba = img.to_rgba8();
    let (x, y, w, h) = content_bounds(&rgba);
    let content = imageops::crop_imm(&rgba, x, y, w, h).to_image();

    let aspect = w as f32 / h as f32;
    if (0.85..=1.18).contains(&aspect) {
        // Square enough: trim the last sliver and use it as it is.
        let side = w.min(h);
        let square = imageops::crop_imm(&content, (w - side) / 2, (h - side) / 2, side, side).to_image();
        return imageops::resize(&square, size, size, FilterType::Triangle);
    }

    // Backdrop: the picture filling the square, blurred and dimmed.
    let fill = image::DynamicImage::ImageRgba8(content.clone()).resize_to_fill(size / 8, size / 8, FilterType::Triangle);
    let mut canvas = imageops::resize(&imageops::blur(&fill, 2.0), size, size, FilterType::Triangle);
    for p in canvas.pixels_mut() {
        for c in 0..3 {
            p[c] = (p[c] as f32 * 0.55) as u8;
        }
    }
    // The whole picture, as large as fits, centred.
    let scale = (size as f32 / w as f32).min(size as f32 / h as f32);
    let (fw, fh) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
    let fitted = imageops::resize(&content, fw, fh, FilterType::Triangle);
    imageops::overlay(&mut canvas, &fitted, ((size - fw) / 2) as i64, ((size - fh) / 2) as i64);
    canvas
}

/// The picture inside flat bars along the edges (pillar- or letterboxing):
/// edge columns/rows whose pixels nearly all match the outermost one are
/// trimmed, as long as at least half of the image is left.
pub fn content_bounds(px: &image::RgbaImage) -> (u32, u32, u32, u32) {
    let (w, h) = px.dimensions();
    let close = |a: &image::Rgba<u8>, b: &image::Rgba<u8>| (0..3).all(|c| (a[c] as i32 - b[c] as i32).abs() <= 20);
    // A line is flat when ≥ 97% of its pixels match the bar's colour.
    let flat = |pixels: &mut dyn Iterator<Item = &image::Rgba<u8>>, bar: &image::Rgba<u8>| {
        let (mut n, mut same) = (0u32, 0u32);
        for p in pixels {
            n += 1;
            same += close(p, bar) as u32;
        }
        n > 0 && same * 100 >= n * 97
    };
    let column = |x: u32| (0..h).map(move |y| px.get_pixel(x, y));
    let row = |y: u32| (0..w).map(move |x| px.get_pixel(x, y));

    let left_bar = *px.get_pixel(0, h / 2);
    let left = (0..w / 4).take_while(|&x| flat(&mut column(x), &left_bar)).count() as u32;
    let right_bar = *px.get_pixel(w - 1, h / 2);
    let right = (0..w / 4).take_while(|&i| flat(&mut column(w - 1 - i), &right_bar)).count() as u32;
    let top_bar = *px.get_pixel(w / 2, 0);
    let top = (0..h / 4).take_while(|&y| flat(&mut row(y), &top_bar)).count() as u32;
    let bottom_bar = *px.get_pixel(w / 2, h - 1);
    let bottom = (0..h / 4).take_while(|&i| flat(&mut row(h - 1 - i), &bottom_bar)).count() as u32;

    // Bars come in matching pairs (same colour on both sides); one flat
    // edge, or two different colours, is part of the picture.
    let (left, right) = if left > 0 && right > 0 && close(&left_bar, &right_bar) { (left, right) } else { (0, 0) };
    let (top, bottom) = if top > 0 && bottom > 0 && close(&top_bar, &bottom_bar) { (top, bottom) } else { (0, 0) };
    let (cw, ch) = (w - left - right, h - top - bottom);
    // Only square artwork framed by bars counts; anything else (a dark or
    // plain picture with flat areas at its edges) is kept whole.
    let aspect = cw as f32 / ch.max(1) as f32;
    if (left + right + top + bottom) == 0 || !(0.85..=1.18).contains(&aspect) {
        return (0, 0, w, h);
    }
    (left, top, cw, ch)
}

#[cfg(test)]
mod tests {
    #[test]
    fn square_art_between_bars_is_trimmed() {
        use image::{Rgba, RgbaImage};
        // 1280×720 thumbnail: a 720×720 checkered square between black bars.
        let mut thumb = RgbaImage::from_pixel(1280, 720, Rgba([0, 0, 0, 255]));
        for y in 0..720 {
            for x in 280..1000 {
                let v = if (x / 40 + y / 40) % 2 == 0 { 230 } else { 40 };
                thumb.put_pixel(x, y, Rgba([v, 90, 160, 255]));
            }
        }
        assert_eq!(super::content_bounds(&thumb), (280, 0, 720, 720));
        let out = super::square_cover(&image::DynamicImage::ImageRgba8(thumb), 256);
        // Corners show the artwork, not black bars.
        assert!(out.get_pixel(2, 2)[1] > 60);
    }

    #[test]
    fn wide_pictures_are_shown_whole() {
        use image::{Rgba, RgbaImage};
        // Full-width 16:9 picture: left half red, right half blue.
        let mut wide = RgbaImage::new(1280, 720);
        for (x, _, p) in wide.enumerate_pixels_mut() {
            *p = if x < 640 { Rgba([250, 20, 20, 255]) } else { Rgba([20, 20, 250, 255]) };
        }
        let out = super::square_cover(&image::DynamicImage::ImageRgba8(wide), 256);
        // Both edges of the picture survive (a centre crop would lose them).
        let (left, right) = (out.get_pixel(3, 128), out.get_pixel(252, 128));
        assert!(left[0] > 200 && left[2] < 60, "left edge {left:?}");
        assert!(right[2] > 200 && right[0] < 60, "right edge {right:?}");
        // Above the picture: the dimmed backdrop, darker than the picture.
        assert!(out.get_pixel(3, 10)[0] < 200, "backdrop {:?}", out.get_pixel(3, 10));
    }
}
