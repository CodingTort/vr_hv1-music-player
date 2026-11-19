//! The synced-lyrics view: the line being sung is lit, the rest are dimmed,
//! and the text glides so the current line stays in place, as in Apple Music
//! or Spotify. Scroll to look around (it re-centres after a few seconds);
//! click a line to jump there; right-click the line being sung to re-sync
//! lyrics that were timed for another cut of the song.
//!
//! A custom widget rather than a `scrollable`, because following the song
//! needs the exact height of every wrapped line, which only exists after
//! layout, and gliding needs per-frame animation.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::text::{self, paragraph, Paragraph as _};
use iced::advanced::widget::{tree, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::event::{self, Event};
use iced::{alignment, mouse, window};
use iced::{Element, Length, Pixels, Point, Rectangle, Size};

use crate::app::Message;
use crate::lyrics::{LyricLine, Ruby};
use crate::ui::{icons, theme};

/// Vertical gap between lines.
const SPACING: f32 = 14.0;
/// Horizontal padding, and the top padding of unsynced lyrics.
const PADDING: f32 = 20.0;
/// Where the current line sits, as a fraction of the height from the top.
const ANCHOR: f32 = 0.38;
/// Lines fade out over this fraction of the height at the top and bottom.
const EDGE_FADE: f32 = 0.18;
/// Easing rates (per second) for the glide and for a line lighting up.
const GLIDE: f32 = 9.0;
const GLOW: f32 = 10.0;
/// How long a manual scroll holds before the view follows the song again.
const HOLD: Duration = Duration::from_secs(3);
/// Opacity of lines that are not being sung, and of a hovered one.
const DIM: f32 = 0.35;
const HOVER: f32 = 0.7;
/// Furigana size relative to the lyrics, and the row height that fits it.
const RUBY_SCALE: f32 = 0.5;
const RUBY_LINE_HEIGHT: f32 = 2.0;

pub struct LyricsView<'a> {
    lines: &'a [LyricLine],
    /// Furigana: the same lines split into pieces, kanji with readings.
    ruby: Option<&'a [Vec<Ruby>]>,
    synced: bool,
    position: Duration,
    /// Seconds the lyrics are delayed by (negative = sooner).
    offset: f32,
    text_size: f32,
    color: iced::Color,
}

impl<'a> LyricsView<'a> {
    pub fn new(
        lines: &'a [LyricLine],
        synced: bool,
        position: Duration,
        offset: f32,
        text_size: f32,
    ) -> Self {
        Self { lines, ruby: None, synced, position, offset, text_size, color: theme::text() }
    }

    /// Text colour (the theme's, unless adjusted for a bright cover).
    pub fn color(mut self, color: iced::Color) -> Self {
        self.color = color;
        self
    }

    /// Shows furigana above the kanji (ignored unless it matches the lines).
    pub fn furigana(mut self, ruby: Option<&'a [Vec<Ruby>]>) -> Self {
        self.ruby = ruby.filter(|r| r.len() == self.lines.len());
        self
    }

    /// Screen y of the content before scrolling. Synced lyrics scroll so the
    /// current line's centre lands here; unsynced ones simply start at the top.
    fn origin(&self, bounds: Rectangle) -> f32 {
        if self.synced {
            bounds.y + bounds.height * ANCHOR
        } else {
            bounds.y + PADDING
        }
    }

    fn line_under<P: text::Paragraph>(
        &self,
        state: &State<P>,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<usize> {
        if !self.synced {
            return None;
        }
        let y = cursor.position_over(bounds)?.y - self.origin(bounds) + state.scroll;
        (0..state.tops.len()).find(|&i| {
            y >= state.tops[i] - SPACING / 2.0 && y < state.tops[i] + state.heights[i] + SPACING / 2.0
        })
    }
}

/// One lyric line, laid out: the text, and the readings over its kanji.
#[derive(Default)]
struct LineText<P: text::Paragraph> {
    base: P,
    /// What `base` was laid out from, to skip redoing it every frame.
    key: u64,
    /// (index of the kanji piece in `base`, its reading).
    readings: Vec<(usize, paragraph::Plain<P>)>,
}

#[derive(Default)]
struct State<P: text::Paragraph> {
    lines: Vec<LineText<P>>,
    /// Top and height of each line, relative to the first line.
    tops: Vec<f32>,
    heights: Vec<f32>,
    /// Identifies the lyrics on screen, to start over when the song changes.
    fingerprint: u64,
    /// 0‥1 brightness of each line; eases towards 1 for the current line.
    glow: Vec<f32>,
    active: Option<usize>,
    hovered: Option<usize>,
    scroll: f32,
    target: f32,
    /// While set, the user is looking around and the view does not follow.
    hold_until: Option<Instant>,
    last_frame: Option<Instant>,
}

impl<P: text::Paragraph> State<P> {
    fn centre_of(&self, line: usize) -> f32 {
        self.tops.get(line).map_or(0.0, |top| top + self.heights[line] / 2.0)
    }

    fn content_height(&self) -> f32 {
        self.tops.last().map_or(0.0, |top| top + self.heights[self.heights.len() - 1])
    }
}

impl<'a, Theme, Renderer> Widget<Message, Theme, Renderer> for LyricsView<'a>
where
    Renderer: text::Renderer<Font = iced::Font>,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<Renderer::Paragraph>::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();

        let mut hasher = DefaultHasher::new();
        (self.lines, self.synced).hash(&mut hasher);
        let fingerprint = hasher.finish();
        if fingerprint != state.fingerprint {
            *state = State { fingerprint, ..State::default() };
        }

        let bounds = Size::new((size.width - 2.0 * PADDING).max(1.0), f32::INFINITY);
        state.lines.resize_with(self.lines.len(), Default::default);
        state.glow.resize(self.lines.len(), 0.0);
        state.tops.clear();
        state.heights.clear();
        let mut y = 0.0;
        for (i, (line, laid)) in self.lines.iter().zip(&mut state.lines).enumerate() {
            let pieces = self.ruby.and_then(|r| r.get(i)).filter(|p| p.iter().any(|x| x.reading.is_some()));
            let content = display_text(line, self.synced);

            let mut hasher = DefaultHasher::new();
            (content, pieces, bounds.width.to_bits(), self.text_size.to_bits()).hash(&mut hasher);
            let key = hasher.finish();
            if laid.key != key {
                let spans: Vec<text::Span<'_, (), iced::Font>> = match pieces {
                    Some(p) => p.iter().map(|x| text::Span::new(x.text.as_str())).collect(),
                    None => vec![text::Span::new(content)],
                };
                laid.base = Renderer::Paragraph::with_spans(text::Text {
                    content: &spans[..],
                    bounds,
                    size: Pixels(self.text_size),
                    // Rows with furigana are taller: the readings sit in the
                    // extra room above the text.
                    line_height: text::LineHeight::Relative(if pieces.is_some() { RUBY_LINE_HEIGHT } else { 1.3 }),
                    font: icons::UI_FONT_BOLD,
                    horizontal_alignment: alignment::Horizontal::Left,
                    vertical_alignment: alignment::Vertical::Top,
                    // Advanced shaping falls back to other installed fonts, so
                    // Japanese, Korean etc. render instead of empty boxes.
                    shaping: text::Shaping::Advanced,
                    wrapping: text::Wrapping::WordOrGlyph,
                });
                laid.readings = pieces
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .filter_map(|(j, piece)| {
                        let reading = piece.reading.as_deref()?;
                        Some((j, paragraph::Plain::new(text::Text {
                            content: reading,
                            bounds: Size::INFINITY,
                            size: Pixels(self.text_size * RUBY_SCALE),
                            line_height: text::LineHeight::Relative(1.0),
                            font: icons::UI_FONT,
                            horizontal_alignment: alignment::Horizontal::Left,
                            vertical_alignment: alignment::Vertical::Top,
                            shaping: text::Shaping::Advanced,
                            wrapping: text::Wrapping::None,
                        })))
                    })
                    .collect();
                laid.key = key;
            }
            let height = laid.base.min_bounds().height;
            state.tops.push(y);
            state.heights.push(height);
            y += height + SPACING;
        }
        layout::Node::new(size)
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> event::Status {
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();
        let bounds = layout.bounds();

        match event {
            Event::Window(window::Event::RedrawRequested(now)) => {
                let first_frame = state.last_frame.is_none();
                let dt = state
                    .last_frame
                    .map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f32().min(0.05));
                state.last_frame = Some(now);

                if state.hold_until.is_some_and(|t| now >= t) {
                    state.hold_until = None;
                }
                if self.synced {
                    state.active = active_line(self.lines, shift(self.position, -self.offset));
                    if state.hold_until.is_none() {
                        state.target = state.centre_of(state.active.unwrap_or(0));
                    }
                }
                if first_frame {
                    // A new song, or the panel just opened: start in place
                    // rather than gliding in from the top.
                    state.scroll = state.target;
                }

                let mut moving = ease(&mut state.scroll, state.target, GLIDE, dt, 0.5);
                for (i, glow) in state.glow.iter_mut().enumerate() {
                    let want = if state.active == Some(i) { 1.0 } else { 0.0 };
                    moving |= ease(glow, want, GLOW, dt, 0.01);
                }
                state.hovered = self.line_under(state, bounds, cursor);

                if moving {
                    shell.request_redraw(window::RedrawRequest::NextFrame);
                } else if let Some(t) = state.hold_until {
                    shell.request_redraw(window::RedrawRequest::At(t));
                }
                event::Status::Ignored
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let dy = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => y * 60.0,
                    mouse::ScrollDelta::Pixels { y, .. } => y,
                };
                let max = if self.synced {
                    state.content_height()
                } else {
                    (state.content_height() + 2.0 * PADDING - bounds.height).max(0.0)
                };
                state.target = (state.target - dy).clamp(0.0, max);
                if self.synced {
                    state.hold_until = Some(Instant::now() + HOLD);
                }
                shell.request_redraw(window::RedrawRequest::NextFrame);
                event::Status::Captured
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let hovered = self.line_under(state, bounds, cursor);
                if hovered != state.hovered {
                    state.hovered = hovered;
                    shell.request_redraw(window::RedrawRequest::NextFrame);
                }
                event::Status::Ignored
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                let Some(i) = self.line_under(state, bounds, cursor) else {
                    return event::Status::Ignored;
                };
                let time = self.lines[i].time;
                match button {
                    mouse::Button::Left => shell.publish(Message::Seek(shift(time, self.offset))),
                    // "This line is being sung now."
                    mouse::Button::Right => shell.publish(Message::LyricsSyncLine(time)),
                    _ => return event::Status::Ignored,
                }
                state.hold_until = None;
                event::Status::Captured
            }
            _ => event::Status::Ignored,
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<Renderer::Paragraph>>();
        let bounds = layout.bounds();
        let Some(clip) = bounds.intersection(viewport) else { return };
        let origin = self.origin(bounds);
        let fade = bounds.height * EDGE_FADE;

        for (i, laid) in state.lines.iter().enumerate() {
            let (Some(&top), Some(&height)) = (state.tops.get(i), state.heights.get(i)) else {
                break;
            };
            let y = origin + top - state.scroll;
            if y + height < bounds.y || y > bounds.y + bounds.height {
                continue;
            }

            let alpha = if self.synced {
                let rest = if state.hovered == Some(i) { HOVER } else { DIM };
                let glow = state.glow.get(i).copied().unwrap_or(0.0);
                let centre = y + height / 2.0;
                let edge = (centre - bounds.y).min(bounds.y + bounds.height - centre);
                (rest + (1.0 - rest) * glow) * (edge / fade).clamp(0.0, 1.0)
            } else {
                0.9
            };
            if alpha > 0.01 {
                let at = Point::new(bounds.x + PADDING, y);
                let color = theme::with_alpha(self.color, alpha);
                renderer.fill_paragraph(&laid.base, at, color, clip);
                // Each reading centred over its kanji, just above the
                // glyphs (which sit in the middle of their tall row).
                for (piece, reading) in &laid.readings {
                    let Some(kanji) = laid.base.span_bounds(*piece).into_iter().next() else { continue };
                    let size = reading.min_bounds();
                    let glyph_top = kanji.y + (kanji.height - self.text_size) / 2.0;
                    let x = at.x + kanji.x + (kanji.width - size.width) / 2.0;
                    renderer.fill_paragraph(reading.raw(), Point::new(x, at.y + glyph_top - size.height), color, clip);
                }
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State<Renderer::Paragraph>>();
        if self.line_under(state, layout.bounds(), cursor).is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}

impl<'a, Theme, Renderer> From<LyricsView<'a>> for Element<'a, Message, Theme, Renderer>
where
    Theme: 'a,
    Renderer: text::Renderer<Font = iced::Font> + 'a,
{
    fn from(view: LyricsView<'a>) -> Self {
        Element::new(view)
    }
}

/// The line being sung at `position`: the last one that has started.
fn active_line(lines: &[LyricLine], position: Duration) -> Option<usize> {
    lines.partition_point(|l| l.time <= position).checked_sub(1)
}

/// `t` moved by `secs`, stopping at zero.
fn shift(t: Duration, secs: f32) -> Duration {
    Duration::from_secs_f64((t.as_secs_f64() + secs as f64).max(0.0))
}

fn display_text(line: &LyricLine, synced: bool) -> &str {
    match line.text.as_str() {
        // An empty timed line marks an instrumental break.
        "" if synced => "♪",
        // A blank line between stanzas still needs height.
        "" => " ",
        text => text,
    }
}

/// Moves `value` towards `target` with exponential smoothing.
/// Returns whether it is still on its way.
fn ease(value: &mut f32, target: f32, rate: f32, dt: f32, epsilon: f32) -> bool {
    *value += (target - *value) * (1.0 - (-rate * dt).exp());
    if (target - *value).abs() <= epsilon {
        *value = target;
        false
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(secs: u64) -> LyricLine {
        LyricLine { time: Duration::from_secs(secs), text: String::new() }
    }

    #[test]
    fn active_line_is_the_last_one_started() {
        let lines = [line(5), line(10), line(10), line(20)];
        assert_eq!(active_line(&lines, Duration::from_secs(0)), None);
        assert_eq!(active_line(&lines, Duration::from_secs(5)), Some(0));
        assert_eq!(active_line(&lines, Duration::from_millis(9_999)), Some(0));
        assert_eq!(active_line(&lines, Duration::from_secs(10)), Some(2));
        assert_eq!(active_line(&lines, Duration::from_secs(99)), Some(3));
        assert_eq!(active_line(&[], Duration::from_secs(1)), None);
    }

    #[test]
    fn offset_shifts_and_clamps_at_zero() {
        let s = Duration::from_secs;
        assert_eq!(shift(s(10), 2.5), Duration::from_millis(12_500));
        assert_eq!(shift(s(10), -2.5), Duration::from_millis(7_500));
        assert_eq!(shift(s(1), -5.0), Duration::ZERO);
        // Lyrics delayed by 12 s: at 0:20 the line timed 0:08 is the one sung.
        let lines = [line(5), line(8), line(15)];
        assert_eq!(active_line(&lines, shift(s(20), -12.0)), Some(1));
    }

    #[test]
    fn easing_settles_on_target() {
        let mut v = 0.0;
        let mut frames = 0;
        while ease(&mut v, 100.0, GLIDE, 1.0 / 60.0, 0.5) {
            frames += 1;
            assert!(frames < 120, "should settle within two seconds");
        }
        assert_eq!(v, 100.0);
    }
}
