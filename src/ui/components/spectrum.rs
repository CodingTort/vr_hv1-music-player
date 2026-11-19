use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path};
use iced::{Color, Element, Length, Point, Rectangle, Size};

use crate::app::Message;
use crate::ui::theme;

struct SpectrumCanvas {
    bins: Vec<f32>,
    accent: Color,
    /// Output volume (0.0–1.0). Bars show the music's own loudness, not the
    /// slider's (a low volume made them too small to read); muted = flat.
    gain: f32,
    /// Bars grow both up and down from a centre line instead of up from
    /// the bottom.
    mirrored: bool,
}

impl canvas::Program<Message> for SpectrumCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry<iced::Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());

        let src = &self.bins;
        if src.is_empty() || self.gain <= 0.0 {
            return vec![frame.into_geometry()];
        }

        const BAR_W: f32 = 1.5;
        const GAP: f32 = 3.0;
        const SLOT: f32 = BAR_W + GAP;

        let n_bars = ((bounds.width / SLOT).floor() as usize).clamp(1, src.len());
        let x_offset = (bounds.width - n_bars as f32 * SLOT) / 2.0;

        let bottom_y = bounds.height;
        let centre_y = bounds.height / 2.0;
        for i in 0..n_bars {
            // Evenly resample src into n_bars
            let src_idx = i * src.len() / n_bars;
            let amp = src[src_idx];

            if amp < 0.005 {
                continue;
            }

            let x = x_offset + i as f32 * SLOT + (SLOT - BAR_W) / 2.0;
            let bar_h = amp.clamp(0.0, 1.0) * bounds.height * 0.88;

            let path = if self.mirrored {
                // Same overall height, split evenly above and below.
                Path::rectangle(Point::new(x, centre_y - bar_h / 2.0), Size::new(BAR_W, bar_h))
            } else {
                Path::rectangle(Point::new(x, bottom_y - bar_h), Size::new(BAR_W, bar_h))
            };
            frame.fill(&path, self.accent);
        }

        vec![frame.into_geometry()]
    }
}

pub fn spectrum_view(
    bins: &[f32],
    gain: f32,
    mirrored: bool,
    width: Length,
    height: Length,
) -> Element<'_, Message> {
    Canvas::new(SpectrumCanvas {
        bins: bins.to_vec(),
        accent: theme::accent(),
        gain: gain.clamp(0.0, 1.0),
        mirrored,
    })
    .width(width)
    .height(height)
    .into()
}
