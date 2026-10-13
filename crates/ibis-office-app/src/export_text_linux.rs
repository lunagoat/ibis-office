// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `export_text` for Linux: a line of text drawn on a page picture (the
//! header, date, footer and number of a printed handout) and the notes of a
//! notes page, with the system's sans-serif font and fontconfig's fallback
//! for the other scripts (cosmic-text, the text engine of GPUI's Linux
//! platform, rasterized by swash).

use std::ops::Range;
use std::sync::{Mutex, OnceLock};

use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, LayoutRun, Metrics, Shaping, SwashCache, Weight};
use image::{Rgba, RgbaImage};

/// A line's height, in its font's size.
const LINE_HEIGHT: f32 = 1.2;
/// The size of notes text that has none of its own, in points.
const NOTE_PT: f32 = 12.0;

/// Where a line sits against the `x` given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Center,
    Right,
}

/// The system's fonts (reading their list takes a moment: once for the
/// process) and the glyphs rasterized so far.
fn with_fonts<R>(f: impl FnOnce(&mut FontSystem, &mut SwashCache) -> R) -> R {
    static FONTS: OnceLock<Mutex<(FontSystem, SwashCache)>> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| Mutex::new((FontSystem::new(), SwashCache::new())));
    let mut fonts = fonts.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (system, cache) = &mut *fonts;
    f(system, cache)
}

/// The glyphs of the laid out line `run` on `image`, the line's box at
/// (`x`, `y`), in `color` where a glyph has none of its own.
fn draw_run(image: &mut RgbaImage, run: &LayoutRun, (x, y): (i64, i64), color: Color, system: &mut FontSystem, cache: &mut SwashCache) {
    for glyph in run.glyphs {
        let physical = glyph.physical((0.0, 0.0), 1.0);
        let (gx, gy) = (x + physical.x as i64, y + run.line_y.round() as i64 + physical.y as i64);
        cache.with_pixels(system, physical.cache_key, glyph.color_opt.unwrap_or(color), |dx, dy, c| {
            let alpha = c.a() as u32;
            let (px, py) = (gx + dx as i64, gy + dy as i64);
            if alpha == 0 || px < 0 || py < 0 || px >= image.width() as i64 || py >= image.height() as i64 {
                return;
            }
            // The colour with its coverage over what is there.
            let under = image.get_pixel(px as u32, py as u32).0;
            let mix = |u: u8, c: u8| ((u as u32 * (255 - alpha) + c as u32 * alpha) / 255) as u8;
            image.put_pixel(px as u32, py as u32, Rgba([mix(under[0], c.r()), mix(under[1], c.g()), mix(under[2], c.b()), 255]));
        });
    }
}

/// `text`, `size` pixels high, in black on `image`: its left (or right) end at `x`, the top of its letters
/// at `top`.
pub(crate) fn draw_text(image: &mut RgbaImage, text: &str, x: f32, top: f32, size: f32, align: Align) {
    if text.is_empty() {
        return;
    }
    with_fonts(|system, cache| {
        let attrs = Attrs::new().family(Family::SansSerif);
        let mut buffer = Buffer::new(system, Metrics::new(size, size * LINE_HEIGHT));
        // One line, however long.
        buffer.set_size(None, None);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(system, false);
        let Some(run) = buffer.layout_runs().next() else { return };
        let left = match align {
            Align::Left => x,
            Align::Center => x - run.line_w / 2.0,
            Align::Right => x - run.line_w,
        };
        draw_run(image, &run, (left.round() as i64, top.round() as i64), Color::rgb(0, 0, 0), system, cache);
    });
}

/// How a stretch of the notes' text is drawn.
#[derive(Clone, Copy)]
struct Style {
    bold: bool,
    /// In pixels.
    size: f32,
    color: Option<(u8, u8, u8)>,
}

impl Style {
    fn attrs(&self) -> Attrs<'static> {
        let mut attrs = Attrs::new().family(Family::SansSerif).metrics(Metrics::new(self.size, self.size * LINE_HEIGHT));
        if self.bold {
            attrs = attrs.weight(Weight::BOLD);
        }
        if let Some((r, g, b)) = self.color {
            attrs = attrs.color(Color::rgb(r, g, b));
        }
        attrs
    }
}

/// The notes of one slide as one text to lay out: paragraphs (a bullet, a level's indent) of
/// runs (bold, size, colour), broken into lines, and over as many boxes (pages) as it takes.
pub(crate) struct NoteFlow {
    plain: String,
    /// The text's stretches (byte ranges of `plain`, in order, all of it) and their styles.
    spans: Vec<(Range<usize>, Style)>,
    base: Style,
    /// The byte of `plain` the next box starts at.
    at: usize,
}

impl NoteFlow {
    /// `None` for notes with no text. A size is `base_pt` unless a run has its own (`px` per point).
    pub(crate) fn new(text: &crate::export_print::NoteText, px_per_pt: f32) -> Option<Self> {
        let base = Style { bold: false, size: NOTE_PT * px_per_pt, color: None };
        let mut plain = String::new();
        let mut spans: Vec<(Range<usize>, Style)> = Vec::new();
        let mut push = |plain: &mut String, text: &str, style: Style| {
            let start = plain.len();
            plain.push_str(text);
            if !text.is_empty() {
                spans.push((start..plain.len(), style));
            }
        };
        let mut first = true;
        for paragraph in &text.paragraphs {
            if !first {
                push(&mut plain, "\n", base);
            }
            first = false;
            let lead = format!("{}{}", "    ".repeat(paragraph.level as usize), paragraph.bullet.as_ref().map(|b| format!("{b} ")).unwrap_or_default());
            push(&mut plain, &lead, base);
            for run in &paragraph.runs {
                push(&mut plain, &run.text, Style { bold: run.bold, size: run.size_pt.unwrap_or(NOTE_PT) * px_per_pt, color: run.color });
            }
        }
        if plain.trim().is_empty() {
            return None;
        }
        Some(Self { plain, spans, base, at: 0 })
    }

    /// Whether there is text not drawn yet.
    pub(crate) fn has_more(&self) -> bool {
        self.at < self.plain.len()
    }

    /// The text from where the last box ended, in the box `(x, y, width, height)` of `image`, as much
    /// as fits (the next box takes the rest).
    pub(crate) fn draw_next(&mut self, image: &mut RgbaImage, (x, y, width, height): (f32, f32, f32, f32)) {
        let (w, h) = (width.round(), height.round());
        if !self.has_more() || w < 8.0 || h < 8.0 {
            return;
        }
        let at = self.at;
        // What is left, in its stretches.
        let rest: Vec<(&str, Attrs)> = self
            .spans
            .iter()
            .filter(|(range, _)| range.end > at)
            .map(|(range, style)| (&self.plain[range.start.max(at)..range.end], style.attrs()))
            .collect();
        let base = self.base.attrs();
        let taken = with_fonts(|system, cache| {
            let mut buffer = Buffer::new(system, Metrics::new(self.base.size, self.base.size * LINE_HEIGHT));
            // As wide as the box; its height is kept to line by line below.
            buffer.set_size(Some(w), None);
            buffer.set_rich_text(rest, &base, Shaping::Advanced, None);
            buffer.shape_until_scroll(system, false);
            // Where each paragraph (a line of the buffer) starts in what is left: a line feed parts them.
            let mut starts = Vec::with_capacity(buffer.lines.len());
            let mut start = 0;
            for line in &buffer.lines {
                starts.push(start);
                start += line.text().len() + 1;
            }
            let runs: Vec<LayoutRun> = buffer.layout_runs().collect();
            let mut taken = 0;
            for (i, run) in runs.iter().enumerate() {
                // Always a line: a box too small for one takes it all the same, so the text ends.
                if i > 0 && run.line_top + run.line_height > h {
                    break;
                }
                draw_run(image, run, (x.round() as i64, y.round() as i64), Color::rgb(0, 0, 0), system, cache);
                let paragraph = starts[run.line_i];
                taken = match runs.get(i + 1) {
                    // A line broken within its paragraph ends where the next one starts: the space
                    // it was broken at belongs to neither.
                    Some(next) if next.line_i == run.line_i => {
                        let end = run.glyphs.iter().map(|g| g.end).max().unwrap_or(0);
                        paragraph + next.glyphs.iter().map(|g| g.start).min().unwrap_or(end).max(end)
                    }
                    // The last line of its paragraph takes the line feed after it too.
                    _ => paragraph + run.text.len() + 1,
                };
            }
            taken
        });
        // Nothing laid out (no font at all): the text ends here rather than never.
        self.at = if taken == 0 { self.plain.len() } else { (at + taken).min(self.plain.len()) };
    }
}

#[cfg(test)]
mod tests {
    use super::{draw_text, Align, NoteFlow};
    use image::{Rgba, RgbaImage};

    fn dark(image: &RgbaImage) -> Vec<(u32, u32)> {
        image.enumerate_pixels().filter(|(_, _, p)| p.0[0] < 128).map(|(x, y, _)| (x, y)).collect()
    }

    #[test]
    fn notes_are_broken_into_lines_and_boxes() {
        use crate::export_print::{NotePara, NoteRun, NoteText};
        let run = |text: &str, bold: bool, size: Option<f32>, color: Option<(u8, u8, u8)>| NoteRun { text: text.into(), bold, size_pt: size, color };
        let text = NoteText {
            paragraphs: vec![
                NotePara { level: 0, bullet: None, runs: vec![run("The quick brown fox jumps over the lazy dog and runs on and on across the whole field. ", false, None, None), run("二つ目の段落です。", true, Some(16.0), Some((200, 0, 0)))] },
                NotePara { level: 1, bullet: Some("•".into()), runs: vec![run("A bulleted line, one level in, with more words to fill a few lines of the box.", false, None, None)] },
            ],
        };
        let mut flow = NoteFlow::new(&text, 1.0).unwrap();
        // A box with room for part of it: the rest is for the next box.
        let mut first = RgbaImage::from_pixel(300, 200, Rgba([255, 255, 255, 255]));
        flow.draw_next(&mut first, (20.0, 20.0, 200.0, 40.0));
        let marks = dark(&first);
        assert!(marks.len() > 100 && flow.has_more(), "part of it fits: {} {}", marks.len(), flow.has_more());
        let max_y = marks.iter().map(|m| m.1).max().unwrap();
        assert!(max_y <= 62, "inside the box: {max_y}");
        let max_x = marks.iter().map(|m| m.0).max().unwrap();
        assert!(max_x <= 222, "no wider than the box: {max_x}");
        let mut second = RgbaImage::from_pixel(300, 200, Rgba([255, 255, 255, 255]));
        flow.draw_next(&mut second, (20.0, 20.0, 200.0, 150.0));
        assert!(!flow.has_more(), "all of it is drawn");
        assert!(dark(&second).len() > 100);
        // The red bold run: red pixels somewhere in the two.
        let red = |i: &RgbaImage| i.pixels().filter(|p| p[0] > 150 && p[1] < 80 && p[2] < 80).count();
        assert!(red(&first) + red(&second) > 20, "coloured");
        assert!(NoteFlow::new(&NoteText::default(), 1.0).is_none());
    }

    #[test]
    fn a_box_takes_up_where_the_last_one_ended() {
        use crate::export_print::{NotePara, NoteRun, NoteText};
        let words: Vec<String> = (1..=60).map(|n| format!("word{n}")).collect();
        let text = NoteText { paragraphs: vec![NotePara { level: 0, bullet: None, runs: vec![NoteRun { text: words.join(" "), bold: false, size_pt: None, color: None }] }] };
        let mut flow = NoteFlow::new(&text, 1.0).unwrap();
        // Boxes of two lines each: every one takes some of the text, none of it twice, until none is left.
        let mut starts = Vec::new();
        while flow.has_more() {
            starts.push(flow.at);
            let mut page = RgbaImage::from_pixel(240, 60, Rgba([255, 255, 255, 255]));
            flow.draw_next(&mut page, (10.0, 10.0, 200.0, 30.0));
            assert!(dark(&page).len() > 50, "a box with text in it");
            assert!(starts.len() < 60, "the text ends");
        }
        assert!(starts.len() > 2, "over several boxes: {}", starts.len());
        assert!(starts.windows(2).all(|w| w[0] < w[1]), "further on in each: {starts:?}");
        // Each box starts at the start of a word.
        assert!(starts.iter().all(|&s| flow.plain[s..].starts_with("word")), "{starts:?}");
    }

    #[test]
    fn text_is_drawn_where_it_is_put() {
        let mut page = RgbaImage::from_pixel(400, 100, Rgba([255, 255, 255, 255]));
        draw_text(&mut page, "Header 123", 20.0, 30.0, 24.0, Align::Left);
        let marks = dark(&page);
        assert!(marks.len() > 100, "{}", marks.len());
        let (min_x, max_x) = (marks.iter().map(|m| m.0).min().unwrap(), marks.iter().map(|m| m.0).max().unwrap());
        assert!((15..=30).contains(&min_x), "left {min_x}");
        assert!(max_x < 200, "right {max_x}");
        let (min_y, max_y) = (marks.iter().map(|m| m.1).min().unwrap(), marks.iter().map(|m| m.1).max().unwrap());
        assert!(min_y >= 25 && max_y < 75, "{min_y}..{max_y}");
        // Right-aligned: it ends at the x given; Japanese is drawn too.
        let mut right = RgbaImage::from_pixel(400, 100, Rgba([255, 255, 255, 255]));
        draw_text(&mut right, "社外秘 2026/10/11", 380.0, 30.0, 24.0, Align::Right);
        let marks = dark(&right);
        assert!(marks.len() > 100);
        let max_x = marks.iter().map(|m| m.0).max().unwrap();
        assert!((370..=384).contains(&max_x), "{max_x}");
        // The right way up: a T has its bar at the top.
        let mut t = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        draw_text(&mut t, "T", 40.0, 30.0, 40.0, Align::Left);
        let marks = dark(&t);
        let (top, bottom) = (marks.iter().map(|m| m.1).min().unwrap(), marks.iter().map(|m| m.1).max().unwrap());
        let third = (bottom - top) / 3;
        let wide = |from: u32, to: u32| marks.iter().filter(|m| m.1 >= from && m.1 < to).count();
        assert!(wide(top, top + third) > 2 * wide(bottom - third, bottom + 1), "the bar of the T is at the top");
        // Nothing for nothing.
        let mut empty = RgbaImage::from_pixel(10, 10, Rgba([255, 255, 255, 255]));
        draw_text(&mut empty, "", 0.0, 0.0, 12.0, Align::Left);
        assert!(dark(&empty).is_empty());
    }
}
