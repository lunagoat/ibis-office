// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 印刷の設定 (PPTX-RECORD-016〜018): the colour of the printed slides
//! (カラー・グレースケール・白黒), a frame round each slide (スライドに枠を
//! 付けて印刷), and handouts (配布資料: slides on a portrait page, 1, 2, 3, 4,
//! 6 or 9 to a page), made of the slides' pictures.

use image::{Rgba, RgbaImage};

/// How the slides are printed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrintColor {
    #[default]
    Color,
    /// Shades of grey.
    Gray,
    /// Black and white only.
    BlackWhite,
}

/// The handout page, in points (A4, portrait), and its margin.
pub const HANDOUT_PAGE: (f32, f32) = (595.0, 842.0);

/// What a handout carries besides the slides (ヘッダーとフッター > ノートと配布資料).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HandoutMarks {
    pub header: Option<String>,
    pub date: Option<String>,
    pub footer: Option<String>,
    /// The number of the page, bottom right.
    pub number: bool,
    /// Where they go and how big: the file's handout master (the corners, 10 pt, if `None`).
    pub layout: Option<ibis_pptx::handout_master::HandoutLayout>,
}
/// The notes of a slide as a printed page carries them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoteText {
    pub paragraphs: Vec<NotePara>,
}

/// A paragraph of notes: its level (an indent), its bullet and its runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NotePara {
    pub level: u8,
    pub bullet: Option<String>,
    pub runs: Vec<NoteRun>,
}

/// A run of notes: bold, its size in points (12 if `None`) and colour.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoteRun {
    pub text: String,
    pub bold: bool,
    pub size_pt: Option<f32>,
    pub color: Option<(u8, u8, u8)>,
}

impl NoteText {
    /// Plain notes: a paragraph for each line.
    pub fn plain(text: &str) -> Self {
        NoteText { paragraphs: text.split('\n').map(|line| NotePara { level: 0, bullet: None, runs: vec![NoteRun { text: line.to_string(), ..Default::default() }] }).collect() }
    }
}

const MARGIN: f32 = 36.0;
/// Pixels per point of a handout page's picture.
const PX_PER_PT: f32 = 2.0;

/// `image` in the colour `mode`.
pub fn recolor(image: &mut RgbaImage, mode: PrintColor) {
    if mode == PrintColor::Color {
        return;
    }
    for pixel in image.pixels_mut() {
        // Rec. 601 luma, the weights a grey print is made with.
        let luma = (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32).round() as u8;
        let value = if mode == PrintColor::BlackWhite { if luma >= 128 { 255 } else { 0 } } else { luma };
        *pixel = Rgba([value, value, value, pixel[3]]);
    }
}

/// A one-pixel black line round `image`.
pub fn frame(image: &mut RgbaImage) {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return;
    }
    for x in 0..w {
        image.put_pixel(x, 0, Rgba([0, 0, 0, 255]));
        image.put_pixel(x, h - 1, Rgba([0, 0, 0, 255]));
    }
    for y in 0..h {
        image.put_pixel(0, y, Rgba([0, 0, 0, 255]));
        image.put_pixel(w - 1, y, Rgba([0, 0, 0, 255]));
    }
}

/// The columns and rows of a handout page with `per_page` slides (turned on its side: a row for a column).
pub fn handout_grid(per_page: u8, landscape: bool) -> (usize, usize) {
    let (columns, rows) = match per_page {
        1 => (1, 1),
        2 => (1, 2),
        3 => (1, 3),
        4 => (2, 2),
        6 => (2, 3),
        _ => (3, 3),
    };
    if landscape { (rows, columns) } else { (columns, rows) }
}

/// The slides as handout pages: `per_page` to a page, in rows (a column for
/// 2 and 3), each as large as its place takes at its own proportions, with a
/// frame if `framed`. Each page with its size in points.
pub fn handouts(slides: &[RgbaImage], per_page: u8, framed: bool, landscape: bool, marks: &HandoutMarks) -> Vec<(RgbaImage, (f32, f32))> {
    let (columns, rows) = handout_grid(per_page, landscape);
    let per_page = columns * rows;
    let (page_w, page_h) = if landscape { (HANDOUT_PAGE.1, HANDOUT_PAGE.0) } else { HANDOUT_PAGE };
    let cell_w = (page_w - 2.0 * MARGIN) / columns as f32;
    let cell_h = (page_h - 2.0 * MARGIN) / rows as f32;
    // Room between slides.
    let gap = if per_page > 1 { 12.0 } else { 0.0 };
    slides
        .chunks(per_page)
        .enumerate()
        .map(|(page_number, group)| {
            let mut page = RgbaImage::from_pixel((page_w * PX_PER_PT) as u32, (page_h * PX_PER_PT) as u32, Rgba([255, 255, 255, 255]));
            for (i, slide) in group.iter().enumerate() {
                let (column, row) = (i % columns, i / columns);
                let (avail_w, avail_h) = (cell_w - gap, cell_h - gap);
                let scale = (avail_w / slide.width().max(1) as f32 * PX_PER_PT).min(avail_h / slide.height().max(1) as f32 * PX_PER_PT);
                let (w, h) = (((slide.width() as f32 * scale).round() as u32).max(1), ((slide.height() as f32 * scale).round() as u32).max(1));
                let mut picture = image::imageops::resize(slide, w, h, image::imageops::FilterType::Triangle);
                if framed {
                    frame(&mut picture);
                }
                // Centred in its cell.
                let x = (MARGIN + column as f32 * cell_w) * PX_PER_PT + (cell_w * PX_PER_PT - w as f32) / 2.0;
                let y = (MARGIN + row as f32 * cell_h) * PX_PER_PT + (cell_h * PX_PER_PT - h as f32) / 2.0;
                image::imageops::overlay(&mut page, &picture, x.round() as i64, y.round() as i64);
            }
            draw_marks(&mut page, marks, page_number + 1, (page_w, page_h));
            (page, (page_w, page_h))
        })
        .collect()
}

/// ノート ページ: each slide's picture on top of an A4 portrait page (with a thin
/// frame, as PowerPoint draws it), its notes below, and the header, date,
/// footer and page number the notes carry. Notes that do not fit go on further pages
/// (the text only, from the top). Each page with its size in points.
pub fn notes_pages(slides: &[RgbaImage], notes: &[NoteText], marks: &HandoutMarks) -> Vec<(RgbaImage, (f32, f32))> {
    let (page_w, page_h) = HANDOUT_PAGE;
    // The slide as wide as the page's text allows, from the top; the notes under it.
    let side = 72.0;
    let top = 60.0;
    let blank = || RgbaImage::from_pixel((page_w * PX_PER_PT) as u32, (page_h * PX_PER_PT) as u32, Rgba([255, 255, 255, 255]));
    let mut pages = Vec::new();
    for (i, slide) in slides.iter().enumerate() {
        let mut page = blank();
        let avail_w = page_w - 2.0 * side;
        let scale = avail_w / slide.width().max(1) as f32 * PX_PER_PT;
        let (w, h) = (((slide.width() as f32 * scale).round() as u32).max(1), ((slide.height() as f32 * scale).round() as u32).max(1));
        let mut picture = image::imageops::resize(slide, w, h, image::imageops::FilterType::Triangle);
        frame(&mut picture);
        image::imageops::overlay(&mut page, &picture, (side * PX_PER_PT) as i64, (top * PX_PER_PT) as i64);
        let text_top = top * PX_PER_PT + h as f32 + 28.0 * PX_PER_PT;
        let mut flow = start_notes(notes.get(i));
        draw_notes(&mut flow, &mut page, (side * PX_PER_PT, text_top, avail_w * PX_PER_PT, (page_h - 56.0) * PX_PER_PT - text_top));
        pages.push(page);
        // What did not fit: pages of text, from the top.
        while notes_left(&flow) {
            let mut more = blank();
            draw_notes(&mut flow, &mut more, (side * PX_PER_PT, top * PX_PER_PT, avail_w * PX_PER_PT, (page_h - 56.0 - top) * PX_PER_PT));
            pages.push(more);
        }
    }
    pages
        .into_iter()
        .enumerate()
        .map(|(n, mut page)| {
            draw_marks(&mut page, marks, n + 1, (page_w, page_h));
            (page, (page_w, page_h))
        })
        .collect()
}

#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "export"))]
type NotesFlow = Option<crate::export_text::NoteFlow>;
#[cfg(not(all(any(target_os = "macos", target_os = "linux"), feature = "export")))]
type NotesFlow = ();

#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "export"))]
fn start_notes(text: Option<&NoteText>) -> NotesFlow {
    text.and_then(|t| crate::export_text::NoteFlow::new(t, PX_PER_PT))
}

#[cfg(not(all(any(target_os = "macos", target_os = "linux"), feature = "export")))]
fn start_notes(_: Option<&NoteText>) -> NotesFlow {}

#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "export"))]
fn draw_notes(flow: &mut NotesFlow, page: &mut RgbaImage, rect: (f32, f32, f32, f32)) {
    if let Some(flow) = flow {
        flow.draw_next(page, rect);
    }
}

#[cfg(not(all(any(target_os = "macos", target_os = "linux"), feature = "export")))]
fn draw_notes(_: &mut NotesFlow, _: &mut RgbaImage, _: (f32, f32, f32, f32)) {}

#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "export"))]
fn notes_left(flow: &NotesFlow) -> bool {
    flow.as_ref().is_some_and(|f| f.has_more())
}

#[cfg(not(all(any(target_os = "macos", target_os = "linux"), feature = "export")))]
fn notes_left(_: &NotesFlow) -> bool {
    false
}

/// The header, date, footer and page number of a handout page: where the
/// handout master puts them, or in the margins' corners.
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "export"))]
fn draw_marks(page: &mut RgbaImage, marks: &HandoutMarks, number: usize, (page_w, page_h): (f32, f32)) {
    use crate::export_text::{draw_text, Align};
    use ibis_pptx::handout_master::Mark;
    if let Some(layout) = &marks.layout {
        // The master's boxes, scaled from the notes page to this one; the text inside as its box says.
        let (kx, ky) = (page_w * PX_PER_PT / layout.page.0.max(1) as f32, page_h * PX_PER_PT / layout.page.1.max(1) as f32);
        let inset_x = 7.2 * PX_PER_PT;
        let inset_y = 3.6 * PX_PER_PT;
        let number_text = number.to_string();
        for place in &layout.places {
            let text = match place.mark {
                Mark::Header => marks.header.as_deref(),
                Mark::Date => marks.date.as_deref(),
                Mark::Footer => marks.footer.as_deref(),
                Mark::Number => marks.number.then_some(number_text.as_str()),
            };
            let Some(text) = text else { continue };
            let (x, y, w, h) = (place.frame.0 as f32 * kx, place.frame.1 as f32 * ky, place.frame.2 as f32 * kx, place.frame.3 as f32 * ky);
            let size = place.size_pt * PX_PER_PT;
            let (align, at) = match place.align.as_str() {
                "r" => (Align::Right, x + w - inset_x),
                "ctr" => (Align::Center, x + w / 2.0),
                _ => (Align::Left, x + inset_x),
            };
            let top = if place.bottom { y + h - inset_y - size * 1.2 } else { y + inset_y };
            draw_text(page, text, at, top, size, align);
        }
        return;
    }
    let size = 10.0 * PX_PER_PT;
    let (left, right) = (MARGIN * PX_PER_PT, (page_w - MARGIN) * PX_PER_PT);
    let (top, bottom) = (14.0 * PX_PER_PT, (page_h - 14.0 - 12.0) * PX_PER_PT);
    if let Some(text) = &marks.header {
        draw_text(page, text, left, top, size, Align::Left);
    }
    if let Some(text) = &marks.date {
        draw_text(page, text, right, top, size, Align::Right);
    }
    if let Some(text) = &marks.footer {
        draw_text(page, text, left, bottom, size, Align::Left);
    }
    if marks.number {
        draw_text(page, &number.to_string(), right, bottom, size, Align::Right);
    }
}

#[cfg(not(all(any(target_os = "macos", target_os = "linux"), feature = "export")))]
fn draw_marks(_: &mut RgbaImage, _: &HandoutMarks, _: usize, _: (f32, f32)) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([rgb[0], rgb[1], rgb[2], 255]))
    }

    #[test]
    fn colours() {
        let mut gray = solid(2, 2, [255, 0, 0]);
        recolor(&mut gray, PrintColor::Gray);
        assert_eq!(gray.get_pixel(0, 0), &Rgba([76, 76, 76, 255]));
        let mut bw = solid(2, 2, [255, 0, 0]);
        recolor(&mut bw, PrintColor::BlackWhite);
        assert_eq!(bw.get_pixel(0, 0), &Rgba([0, 0, 0, 255]));
        let mut light = solid(2, 2, [250, 250, 200]);
        recolor(&mut light, PrintColor::BlackWhite);
        assert_eq!(light.get_pixel(1, 1), &Rgba([255, 255, 255, 255]));
        let mut same = solid(2, 2, [10, 20, 30]);
        recolor(&mut same, PrintColor::Color);
        assert_eq!(same.get_pixel(0, 0), &Rgba([10, 20, 30, 255]));
    }

    /// ノート ページ: the slide on top, framed, with the notes' text below it.
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "export"))]
    #[test]
    fn notes_pages_have_the_slide_above_and_its_notes_below() {
        let slides = vec![solid(160, 90, [200, 0, 0]), solid(160, 90, [0, 0, 200]), solid(160, 90, [0, 200, 0])];
        let long: String = (1..=90).map(|i| format!("Line {i} of very long notes")).collect::<Vec<_>>().join("\n");
        let notes = vec![NoteText::plain("Speaker notes for the first slide.\n二行目のメモ"), NoteText::default(), NoteText::plain(&long)];
        let pages = notes_pages(&slides, &notes, &HandoutMarks::default());
        // The third slide's notes take more pages, with nothing lost: text on each, and no slide on the later ones.
        assert!(pages.len() >= 5, "{}", pages.len());
        let green = |i: usize| pages[i].0.pixels().filter(|p| p[1] == 200 && p[0] == 0).count();
        assert!(green(2) > 10_000 && green(3) == 0);
        assert!(pages[3..].iter().all(|(p, _)| p.pixels().filter(|q| q[0] < 100 && q[1] < 100 && q[2] < 100).count() > 300));
        let (page, size) = &pages[0];
        assert_eq!(*size, HANDOUT_PAGE);
        let (w, h) = (page.width(), page.height());
        // The slide's colour in the upper half only; dark text in the lower half.
        let red = |y0: u32, y1: u32| (y0..y1).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| page.get_pixel(x, y)[0] == 200 && page.get_pixel(x, y)[2] == 0).count();
        // The slide ends 627 px down (60 pt and 253.7 pt, 2 px a point).
        assert!(red(0, 640) > 10_000 && red(640, h) == 0, "the slide above");
        let dark = |y0: u32, y1: u32| (y0..y1).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| page.get_pixel(x, y).0[..3].iter().all(|c| *c < 100)).count();
        assert!(dark(640, h) > 300, "the notes below");
        // A slide without notes has nothing below it (but the page's blank).
        assert_eq!(pages[1].0.pixels().filter(|p| p[0] < 100 && p[1] < 100 && p[2] > 150).count() > 10_000, true);
        let below = (640..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| pages[1].0.get_pixel(x, y)[0] < 100).count();
        assert_eq!(below, 0);
    }

    #[test]
    fn handout_pages() {
        let slides: Vec<RgbaImage> = (0..7).map(|_| solid(160, 90, [200, 0, 0])).collect();
        // 7 slides, 3 to a page: 3 pages (3, 3, 1).
        let pages = handouts(&slides, 3, true, false, &HandoutMarks::default());
        assert_eq!(pages.len(), 3);
        let (page, size) = &pages[0];
        assert_eq!(*size, HANDOUT_PAGE);
        assert_eq!((page.width(), page.height()), (1190, 1684));
        // The page is white at its corner, and has the slides' colour (or the frame) inside.
        assert_eq!(page.get_pixel(0, 0), &Rgba([255, 255, 255, 255]));
        assert!(page.pixels().any(|p| p[0] == 200 && p[1] == 0));
        assert!(page.pixels().any(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
        // Four to a page: 2 pages, and the second holds three slides (of 4).
        let four = handouts(&slides, 4, false, false, &HandoutMarks::default());
        assert_eq!(four.len(), 2);
        let second = &four[1].0;
        let red = second.pixels().filter(|p| p[0] == 200 && p[1] == 0).count();
        let first_red = four[0].0.pixels().filter(|p| p[0] == 200 && p[1] == 0).count();
        assert!(red * 4 > first_red * 2 && red * 4 < first_red * 4, "three slides, not four: {red} of {first_red}");
        assert_eq!(handout_grid(6, false), (2, 3));
        assert_eq!(handout_grid(9, false), (3, 3));
        // On its side.
        assert_eq!(handout_grid(6, true), (3, 2));
        let wide = handouts(&slides, 6, false, true, &HandoutMarks::default());
        assert_eq!((wide[0].0.width(), wide[0].0.height()), (1684, 1190));
        assert_eq!(wide[0].1, (842.0, 595.0));
        assert_eq!(wide.len(), 2);
    }

    /// The handout master's places: the header where the master puts it, big, the footer gone.
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "export"))]
    #[test]
    fn marks_go_where_the_handout_master_puts_them() {
        use ibis_pptx::handout_master::{HandoutLayout, Mark, Place};
        let slides = vec![solid(160, 90, [255, 255, 255])];
        let layout = HandoutLayout {
            page: (6_858_000, 9_144_000),
            // The header in the middle of the page, 24 pt, centred; the date bottom right.
            places: vec![
                Place { mark: Mark::Header, frame: (1_714_500, 4_000_000, 3_429_000, 600_000), size_pt: 24.0, align: "ctr".into(), bottom: false },
                Place { mark: Mark::Date, frame: (3_884_613, 8_685_213, 2_971_800, 458_787), size_pt: 12.0, align: "r".into(), bottom: true },
            ],
        };
        let marks = HandoutMarks { header: Some("Header".into()), date: Some("2026/10/11".into()), footer: Some("Footer".into()), number: true, layout: Some(layout) };
        let pages = handouts(&slides, 6, false, false, &marks);
        let page = &pages[0].0;
        let dark = |x0: u32, y0: u32, x1: u32, y1: u32| (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| page.get_pixel(x, y)[0] < 128).count();
        let (w, h) = (page.width(), page.height());
        // The header: in the middle band (4.0M to 4.6M of 9.144M EMU), across the middle of the page.
        let band = (h as f32 * 4_000_000.0 / 9_144_000.0) as u32..(h as f32 * 4_700_000.0 / 9_144_000.0) as u32;
        assert!(dark(w / 3, band.start, w * 2 / 3, band.end) > 100, "the header in its place");
        // Not in the corners it used to be in, and no footer (the master has none): the bottom left is clean.
        assert_eq!(dark(0, h * 9 / 10, w / 2, h), 0, "no footer");
        // The date: bottom right.
        assert!(dark(w / 2, h * 9 / 10, w, h) > 50, "the date at the bottom right");
    }
}
