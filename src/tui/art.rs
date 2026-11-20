//! Album art in the terminal: each cell shows two pixels, the top one as the
//! foreground of "▀" and the bottom one as its background. Needs a terminal
//! with 24-bit colour, which nearly all have (including SSH apps on phones).

use image::imageops::{self, FilterType};
use image::RgbaImage;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// `img` drawn `cols` cells wide and `rows` cells tall, from `cols` × `2·rows`
/// pixels. Terminal cells are about twice as tall as they are wide, so a
/// square picture looks square at `cols = 2 · rows`.
pub fn half_blocks(img: &RgbaImage, cols: u16, rows: u16) -> Vec<Line<'static>> {
    if cols == 0 || rows == 0 {
        return Vec::new();
    }
    let px = imageops::resize(img, cols as u32, rows as u32 * 2, FilterType::Triangle);
    let rgb = |x: u32, y: u32| {
        let p = px.get_pixel(x, y);
        Color::Rgb(p[0], p[1], p[2])
    };
    (0..rows as u32)
        .map(|r| {
            Line::from(
                (0..cols as u32)
                    .map(|c| Span::styled("▀", Style::default().fg(rgb(c, 2 * r)).bg(rgb(c, 2 * r + 1))))
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_and_bottom_pixels_become_foreground_and_background() {
        // Top half red, bottom half blue.
        let img = RgbaImage::from_fn(4, 4, |_, y| if y < 2 { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 0, 255, 255]) });
        let lines = half_blocks(&img, 2, 2);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans.len(), 2);
        assert_eq!(lines[0].spans[0].style.fg, Some(Color::Rgb(255, 0, 0)));
        assert_eq!(lines[1].spans[0].style.bg, Some(Color::Rgb(0, 0, 255)));
        assert!(half_blocks(&img, 0, 3).is_empty());
    }
}
