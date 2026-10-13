// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing slide content: backgrounds, shapes, pictures and text.
//!
//! These functions only draw; `slide_view.rs` adds selection and editing on
//! top, and `thumbnails.rs` reuses them for the slide list.

use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;
use std::sync::Arc;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{
    AutoShape, Color as DocColor, EMU_PER_POINT, EmuRect, Fill, Geometry, LineCap, LineEnd,
    LineJoin, LineSpacing, MediaFormat, MediaId, Picture, Placeholder, Presentation, Rgb, Shape,
    ShapeKind, Slide, Stroke, TextAlign, TextBody, TextStyle, VerticalAnchor,
};

/// Line height of single spacing, as a multiple of the font size. Shared by
/// display and the text field, so text does not jump when editing starts.
pub(crate) const LINE_HEIGHT: f32 = crate::render::font_metrics::POWERPOINT_SINGLE;

/// Decoded pictures and the fonts available to draw with, for one presentation.
/// Identifies one rotated drawing of a picture.
type RotatedKey = (MediaId, i32, bool, bool, ibis_doc_model::Crop, i64, i64);

pub(crate) struct Resources {
    pub(crate) images: HashMap<MediaId, Arc<Image>>,
    /// Encoded data of pictures that can be wrapped in SVG to rotate them.
    sources: std::cell::RefCell<HashMap<MediaId, (crate::render::picture_svg::Encoding, Arc<[u8]>)>>,
    /// Rotated pictures made so far, with their bounds relative to the frame.
    rotated: std::cell::RefCell<HashMap<RotatedKey, (Arc<Image>, EmuRect)>>,
    /// Pictures cut to the outlines of shapes they fill, by picture and a
    /// hash of the outline.
    clipped: std::cell::RefCell<HashMap<(MediaId, u64), Arc<Image>>>,
    /// Pictures recoloured as duotones, by picture and colours (and with a
    /// colour made clear, by picture and that colour twice).
    duotones: std::cell::RefCell<HashMap<(MediaId, ibis_doc_model::Rgb, ibis_doc_model::Rgb), Arc<Image>>>,
    /// Those SVG pictures (rotated, cut and recoloured) rasterized ahead,
    /// by the image's id: see `rasterize_svg_pictures`.
    rasters: std::cell::RefCell<HashMap<u64, Arc<RenderImage>>>,
    /// Decoding the pictures and fonts, on other threads.
    pub(crate) preload: crate::render::preload::Preload,
    /// Formats of all media, for labelling pictures that cannot be drawn.
    formats: HashMap<MediaId, MediaFormat>,
    sizes: HashMap<MediaId, (u32, u32)>,
    fonts: Fonts,
    text_system: Arc<TextSystem>,
    /// Ascent, descent and line gap of fonts, in ems.
    metrics: std::cell::RefCell<HashMap<Font, (f32, f32, f32)>>,
    /// Draw as printed: sheets without their grid lines.
    pub(crate) print: bool,
    /// The slide show is drawn: an animated GIF plays (elsewhere it shows its first frame).
    pub(crate) animate_pictures: std::cell::Cell<bool>,
    /// 印刷オプション: 背景の色とイメージを印刷する (DOCX-OPTIONS-010).
    pub(crate) print_backgrounds: bool,
    /// 印刷オプション: 隠し文字を印刷する (DOCX-OPTIONS-010).
    pub(crate) print_hidden_text: bool,
    /// Word documents: the headings (paragraph ids) shown collapsed; what is under them is
    /// not laid out (the navigation pane's 折りたたみ).
    pub(crate) collapsed_headings: Arc<std::collections::BTreeSet<u32>>,
    /// Word documents: the values the merge fields show (差し込み印刷's 結果のプレビュー); `None`: the field codes' own text.
    pub(crate) merge_values: Option<Arc<std::collections::HashMap<String, String>>>,
    /// The scale a printed sheet is drawn at on its paper (for where Excel
    /// starts its dashed borders), set by the export before each sheet.
    pub(crate) print_scale: std::cell::Cell<f64>,
    /// Draw shapes where they are meant to be, as a PDF rasterizer draws
    /// them (the export's pictures, which the coverage tests compare with
    /// PowerPoint's PDF rasterized): level and upright edges are not moved
    /// onto whole pixels as the screen has them (crisp, as PowerPoint draws
    /// its window), and rectangles are drawn as paths (GPUI puts the
    /// quads it paints on whole pixels).
    pub(crate) exact: bool,
    /// The document's body font set, for text with no typeface of its own.
    theme_fonts: Option<ibis_doc_model::FontSet>,
    /// The frame of each video playing now, drawn in place of its poster.
    live: std::cell::RefCell<HashMap<MediaId, crate::media_player::Frame>>,
    /// Worksheets in the workbook by sheet name (original and lower-case).
    pub(crate) sheets: HashMap<String, Arc<ibis_doc_model::Sheet>>,
}

impl Resources {
    pub(crate) fn new(presentation: &Presentation, cx: &App) -> Self {
        let t = crate::clock::debug_timing::start();
        let preload = crate::render::preload::Preload::for_presentation(presentation);
        crate::clock::debug_timing::report("resources: preload", t);
        let t = crate::clock::debug_timing::start();
        let mut images = HashMap::new();
        let mut sources = HashMap::new();
        for (id, (image, source)) in preload.prepared_now() {
            images.insert(id, image);
            if let Some(source) = source {
                sources.insert(id, source);
            }
        }
        crate::clock::debug_timing::report("resources: prepared", t);
        let t = crate::clock::debug_timing::start();
        let formats = presentation.media.iter().map(|m| (m.id, m.format)).collect();
        let sizes = presentation
            .media
            .iter()
            .filter_map(|m| crate::pptx::editing::picture_size(&m.data).map(|s| (m.id, s)))
            .collect();
        crate::clock::debug_timing::report("resources: sizes", t);
        let t = crate::clock::debug_timing::start();
        let mut fonts = Fonts::new(cx);
        crate::clock::debug_timing::report("resources: Fonts::new", t);
        let t = crate::clock::debug_timing::start();
        fonts.add_embedded(&presentation.fonts, &preload, cx);
        crate::clock::debug_timing::report("resources: add_embedded", t);
        let text_system = cx.text_system().clone();
        let mut sheets = HashMap::new();
        if let Some(ref wb) = presentation.workbook {
            for ws in &wb.sheets {
                let sheet_arc = if let Some(slide_id) = ws.slide {
                    presentation.slide(slide_id).and_then(|slide| {
                        slide.shapes.iter().find_map(|sh| {
                            if let ibis_doc_model::ShapeKind::Sheet(s) = &sh.kind {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                    })
                } else {
                    ws.hidden.clone()
                };
                if let Some(s) = sheet_arc {
                    sheets.insert(ws.name.clone(), s.clone());
                    sheets.insert(ws.name.to_lowercase(), s);
                }
            }
        }
        Self {
            images,
            sources: std::cell::RefCell::new(sources),
            preload,
            rotated: Default::default(),
            clipped: Default::default(),
            duotones: Default::default(),
            rasters: Default::default(),
            formats,
            sizes,
            fonts,
            text_system,
            metrics: Default::default(),
            print: false,
            animate_pictures: std::cell::Cell::new(false),
            print_backgrounds: false,
            print_hidden_text: false,
            collapsed_headings: Default::default(),
            merge_values: None,
            print_scale: std::cell::Cell::new(1.0),
            exact: false,
            theme_fonts: presentation.theme_fonts.as_ref().map(|t| t.minor.clone()),
            live: Default::default(),
            sheets,
        }
    }

    /// The frame of the video of clip `media` to draw now (`None`: its poster).
    pub(crate) fn set_live_frame(&self, media: MediaId, frame: Option<crate::media_player::Frame>) {
        match frame {
            Some(frame) => self.live.borrow_mut().insert(media, frame),
            None => self.live.borrow_mut().remove(&media),
        };
    }

    /// Whether a frame of the video of clip `media` is being drawn.
    pub(crate) fn has_live_frame(&self, media: MediaId) -> bool {
        self.live.borrow().contains_key(&media)
    }

    #[cfg(target_os = "macos")]
    fn live_frame(&self, media: MediaId) -> Option<crate::media_player::Frame> {
        self.live.borrow().get(&media).cloned()
    }

    /// A picture put in the document after it was opened (inserted), for
    /// GPUI to decode.
    pub(crate) fn add_picture(&mut self, media: MediaId, format: MediaFormat, data: &[u8]) {
        let image_format = match format {
            MediaFormat::Png => ImageFormat::Png,
            MediaFormat::Jpeg => ImageFormat::Jpeg,
            MediaFormat::Gif => ImageFormat::Gif,
            MediaFormat::Bmp => ImageFormat::Bmp,
            MediaFormat::Tiff => ImageFormat::Tiff,
            MediaFormat::Webp => ImageFormat::Webp,
            MediaFormat::Svg => ImageFormat::Svg,
            _ => return,
        };
        self.images.insert(media, Arc::new(Image::from_bytes(image_format, data.to_vec())));
        self.formats.insert(media, format);
        if let Some(s) = crate::pptx::editing::picture_size(data) {
            self.sizes.insert(media, s);
        }
    }

    /// The original pixel dimensions of `media`.
    pub(crate) fn image_size(&self, media: MediaId) -> Option<(u32, u32)> {
        self.sizes.get(&media).copied()
    }

    /// Whether `media` is a picture that can be drawn or is preparing.
    pub(crate) fn has_picture(&self, media: MediaId) -> bool {
        self.images.contains_key(&media) || self.preload.is_image_supported(media)
    }

    /// The encoded SVG source of `media`, on demand.
    pub(crate) fn source(&self, media: MediaId) -> Option<(crate::render::picture_svg::Encoding, Arc<[u8]>)> {
        if let Some(source) = self.sources.borrow().get(&media) {
            return Some(source.clone());
        }
        if let Some((_, Some(source))) = self.preload.prepared_one(media) {
            self.sources.borrow_mut().insert(media, source.clone());
            return Some(source);
        }
        None
    }

    /// The picture to draw for `media`: decoded, or left to GPUI to load.
    /// `None` while it is still decoding (it is drawn when it is ready) or
    /// when it cannot be drawn.
    pub(crate) fn picture(&self, media: MediaId) -> Option<ImageSource> {
        match self.preload.decoded(media) {
            Ok(Some(image)) => Some(ImageSource::Render(image)),
            Ok(None) => None,
            Err(()) => self
                .images
                .get(&media)
                .cloned()
                .or_else(|| self.preload.prepared_one(media).map(|(img, _)| img))
                .map(ImageSource::Image),
        }
    }

    /// An SVG picture (rotated, cut or recoloured) to draw: rasterized
    /// ahead, or left to GPUI to rasterize.
    pub(crate) fn svg_picture(&self, image: Arc<Image>) -> ImageSource {
        match self.rasters.borrow().get(&image.id()) {
            Some(raster) => ImageSource::Render(raster.clone()),
            None => ImageSource::Image(image),
        }
    }

    /// Rasterize the SVG pictures made so far, on several threads. An
    /// export does so before drawing: GPUI would rasterize them one after
    /// another (its headless executor runs on one thread), at the size of
    /// the picture itself.
    #[cfg(headless_export)]
    pub(crate) fn rasterize_svg_pictures(&self, renderer: &SvgRenderer) {
        let mut images: Vec<Arc<Image>> = self.rotated.borrow().values().map(|(image, _)| image.clone()).collect();
        images.extend(self.clipped.borrow().values().cloned());
        images.extend(self.duotones.borrow().values().cloned());
        images.extend(crate::docx::art_border::all_images());
        let done = self.rasters.borrow();
        images.retain(|image| !done.contains_key(&image.id()));
        drop(done);
        images.sort_by_key(|image| image.id());
        images.dedup_by_key(|image| image.id());
        if images.is_empty() {
            return;
        }
        #[cfg(target_family = "wasm")]
        let threads = 1;
        #[cfg(not(target_family = "wasm"))]
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8).min(images.len());
        if threads <= 1 {
            let rasters: Vec<(u64, Arc<RenderImage>)> = images
                .iter()
                .filter_map(|image| image.to_image_data(renderer.clone()).ok().map(|raster| (image.id(), raster)))
                .collect();
            self.rasters.borrow_mut().extend(rasters);
            return;
        }
        let next = std::sync::atomic::AtomicUsize::new(0);
        let rasters: Vec<(u64, Arc<RenderImage>)> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        let mut done = Vec::new();
                        while let Some(image) = images.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) {
                            if let Ok(raster) = image.to_image_data(renderer.clone()) {
                                done.push((image.id(), raster));
                            }
                        }
                        done
                    })
                })
                .collect();
            workers.into_iter().flat_map(|w| w.join().unwrap_or_default()).collect()
        });
        self.rasters.borrow_mut().extend(rasters);
    }

    /// The ascent and descent of `font`, in ems.
    pub(crate) fn metrics(&self, font: &Font) -> (f32, f32) {
        let (a, d, _) = self.line_metrics(font);
        (a, d)
    }

    /// The width of `text` in `font` at `size` pixels, from the characters'
    /// advances (text is laid out without kerning).
    pub(crate) fn text_width(&self, text: &str, font: &Font, size: f32) -> f32 {
        let id = self.text_system.resolve_font(font);
        text.chars()
            .map(|c| match self.text_system.advance(id, px(size), c) {
                Ok(advance) => f32::from(advance.width),
                Err(_) if c as u32 >= 0x2e80 => size,
                Err(_) => size * 0.5,
            })
            .sum()
    }

    /// The ascent, descent and line gap of `font`, in ems.
    pub(crate) fn line_metrics(&self, font: &Font) -> (f32, f32, f32) {
        if let Some(&m) = self.metrics.borrow().get(font) {
            return m;
        }
        let id = self.text_system.resolve_font(font);
        let em = px(1000.0);
        let line_gap = f32::from(self.text_system.line_gap(id, em)).max(0.0) / 1000.0;
        // The ascent and descent the Office apps take (`font_metrics`): from
        // the font's file, else those of the typeface it is (macOS's
        // YuGothic stands for 游ゴシック), else the text system's.
        let (ascent, descent) = self
            .text_system
            .font_data(id)
            .and_then(|(data, index)| crate::render::font_metrics::font_file_metrics(&data, index))
            .filter(|_| !STAND_INS.contains(&font.family.as_ref()))
            .or_else(|| crate::render::font_metrics::office_metrics(&font.family))
            .unwrap_or_else(|| (f32::from(self.text_system.ascent(id, em)).abs() / 1000.0, f32::from(self.text_system.descent(id, em)).abs() / 1000.0));
        let m = (ascent, descent, line_gap);
        if std::env::var_os("IBIS_DEBUG_METRICS").is_some() {
            eprintln!("metrics {} {:?}: {m:?}", font.family, font.weight);
        }
        self.metrics.borrow_mut().insert(font.clone(), m);
        m
    }

    /// How far to move a line of `font` at `size` down from where GPUI puts
    /// it in a line box `line` high (GPUI centres the font's height in the
    /// box) to where PowerPoint does (`rich_text::baseline`).
    fn baseline_shift(&self, font: &Font, size: f32, line: f32) -> f32 {
        let (a, d) = self.metrics(font);
        // GPUI's baseline: the font's height centred in the line.
        let gpui = (line - (a + d) * size) / 2.0 + a * size;
        crate::render::font_metrics::powerpoint_baseline(line, LINE_HEIGHT * size, a, d) - gpui
    }

    /// How much smaller to draw `font`, the font standing in for the
    /// typeface `requested` when that is not installed, so that it takes
    /// the room the typeface would: the ratio of their digits' widths. 1
    /// when the typeface is installed or its widths are not known.
    pub(crate) fn substitute_scale(&self, requested: Option<&str>, font: &Font) -> f32 {
        let Some(name) = requested else { return 1.0 };
        if self.fonts.available(name).is_some() {
            return 1.0;
        }
        // Digit widths in ems, as Office prints them. (Calibri, 0.507, is
        // left out: its substitute drawn to its width looks smaller than it,
        // and matched its PDF worse.)
        let expected = match name {
            n if n.starts_with("ＭＳ") || n.starts_with("MS ") => 0.54, // i18n-ok（フォント名の照合（ＭＳ で始まる名前の判定）。ファイルの中の名前。i18n-ok）
            _ => return 1.0,
        };
        let actual = self.text_width("0", font, 100.0) / 100.0;
        if actual <= 0.0 { 1.0 } else { (expected / actual).min(1.0) }
    }

    /// `substitute_scale` for a text of Japanese characters: the ratio of
    /// the widths of the text in the typeface (ＭＳ Ｐゴシック: kanji and
    /// fullwidth signs 1 em, kana 0.86, digits 0.54, other Latin 0.55, signs
    /// and spaces about 0.4; measured on Excel's PDF of a chart of titles and
    /// category names) and in the font standing in, not of their digits (which
    /// would draw kanji at 0.76 of their size).
    pub(crate) fn substitute_scale_for(&self, requested: Option<&str>, font: &Font, sample: &str) -> f32 {
        let Some(name) = requested else { return 1.0 };
        if !(name.starts_with("ＭＳ") || name.starts_with("MS ")) || sample.is_ascii() || self.fonts.available(name).is_some() { // i18n-ok（フォント名の照合（ＭＳ で始まる名前の判定）。i18n-ok）
            return self.substitute_scale(requested, font);
        }
        let expected: f32 = sample
            .chars()
            .map(|c| match c {
                '0'..='9' => 0.54,
                'a'..='z' | 'A'..='Z' => 0.55,
                ' ' | ',' | '.' | '/' | ':' | '-' | '(' | ')' | '%' => 0.4,
                '\u{3040}'..='\u{30ff}' => 0.86,
                '（' | '）' | '「' | '」' | '、' | '。' => 0.5,
                _ => 1.0,
            })
            .sum();
        let actual = self.text_width(sample, font, 100.0) / 100.0;
        if actual <= 0.0 { 1.0 } else { (expected / actual).clamp(0.5, 1.0) }
    }

    /// `style` for `text`, a run with no typeface of its own taking the
    /// theme's body font for its script (as PowerPoint's `+mn-lt` / `+mn-ea`).
    pub(crate) fn with_theme_font(&self, mut style: TextStyle, text: &str) -> TextStyle {
        if style.font.is_none()
            && let Some(fonts) = &self.theme_fonts
        {
            style.font = fonts.for_text(text).map(str::to_string);
            style.fallback_font = fonts.other_for_text(text).map(str::to_string);
        }
        style
    }

    /// The installed font to draw `style` with.
    pub(crate) fn font(&self, style: &TextStyle) -> Font {
        self.fonts.font(style)
    }

    /// The font a slide's East Asian characters are drawn in when their
    /// run (`style`) has no East Asian typeface: ＭＳ ゴシック, as PowerPoint
    /// draws them (measured on its PDF: runs of `lang="en-US"` with no
    /// `ea` font), when it is installed; `None` otherwise (the system's
    /// font draws them, as before).
    pub(crate) fn default_east_asian_font(&self, style: &TextStyle) -> Option<Font> {
        if style.font.as_deref().is_some_and(is_east_asian_typeface) {
            return None;
        }
        self.fonts.available("MS Gothic")?;
        Some(self.fonts.font(&TextStyle { font: Some("MS Gothic".into()), fallback_font: None, ..style.clone() }))
    }
}

/// Maps the typefaces a document asks for to installed fonts.
struct Fonts {
    installed: HashSet<String>,
    /// Used for East Asian typefaces that are not installed.
    east_asian_fallback: Option<SharedString>,
}

/// Weight words at the end of a typeface name, such as "Poppins SemiBold".
const WEIGHT_SUFFIXES: &[(&str, FontWeight)] = &[
    ("ExtraLight", FontWeight::EXTRA_LIGHT),
    ("UltraLight", FontWeight::EXTRA_LIGHT),
    ("Thin", FontWeight::THIN),
    ("Light", FontWeight::LIGHT),
    ("Regular", FontWeight::NORMAL),
    ("Medium", FontWeight::MEDIUM),
    ("SemiBold", FontWeight::SEMIBOLD),
    ("DemiBold", FontWeight::SEMIBOLD),
    ("ExtraBold", FontWeight::EXTRA_BOLD),
    ("UltraBold", FontWeight::EXTRA_BOLD),
    ("Bold", FontWeight::BOLD),
    ("Black", FontWeight::BLACK),
    ("Heavy", FontWeight::BLACK),
    // Hiragino's weights ("Hiragino Kaku Gothic Std W8").
    ("W0", FontWeight(100.0)),
    ("W1", FontWeight(200.0)),
    ("W2", FontWeight(250.0)),
    ("W3", FontWeight(300.0)),
    ("W4", FontWeight(400.0)),
    ("W5", FontWeight(500.0)),
    ("W6", FontWeight(600.0)),
    ("W7", FontWeight(700.0)),
    ("W8", FontWeight(800.0)),
    ("W9", FontWeight(900.0)),
];

/// Installed fonts that stand for a typeface Office has in other files,
/// whose ascent and descent are the typeface's (`font_metrics`), not their
/// own: macOS's 游ゴシック and 游明朝 (0.88 / 0.12 em against Windows' 0.985 /
/// 0.302).
const STAND_INS: &[&str] = &["YuGothic", "YuMincho"];

/// Office's names of typefaces and the names other systems install them
/// under (macOS calls 游ゴシック "YuGothic").
const TYPEFACE_ALIASES: &[(&str, &str)] = &[
    // Windows' 游ゴシック first: it is what Office draws with (Office for
    // Mac brings it), before macOS' own (heavier) one.
    ("游ゴシック", "Yu Gothic"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("游ゴシック", "YuGothic"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("Yu Gothic", "YuGothic"),
    ("游明朝", "Yu Mincho"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("游明朝", "YuMincho"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ角ゴ Pro", "Hiragino Kaku Gothic Pro"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ角ゴ ProN", "Hiragino Kaku Gothic ProN"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ角ゴ Std", "Hiragino Kaku Gothic Std"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ角ゴ StdN", "Hiragino Kaku Gothic StdN"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ角ゴシック", "Hiragino Sans"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ丸ゴ Pro", "Hiragino Maru Gothic Pro"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ丸ゴ ProN", "Hiragino Maru Gothic ProN"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ明朝 Pro", "Hiragino Mincho Pro"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ明朝 ProN", "Hiragino Mincho ProN"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("Yu Mincho", "YuMincho"),
    // Century is not on macOS; Times New Roman is the nearest serif.
    ("Century", "Times New Roman"),
    // Calibri is not on macOS; Carlito, bundled (`BUNDLED_FONTS`), has its
    // widths.
    ("Calibri", "Carlito"),
    ("メイリオ", "Meiryo"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ＭＳ Ｐゴシック", "MS PGothic"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ＭＳ ゴシック", "MS Gothic"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ＭＳ Ｐ明朝", "MS PMincho"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ＭＳ 明朝", "MS Mincho"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    // Linux has none of Office's typefaces unless the user installs them.
    // The fonts made with their widths stand in, so lines break where they
    // do in Office: Liberation (and Croscore's Arimo, Tinos, Cousine) for
    // Arial, Times New Roman and Courier New, Caladea for Cambria (Carlito
    // for Calibri is above), and the IPA fonts, of ＭＳ ゴシック's and
    // ＭＳ 明朝's fixed widths.
    ("Arial", "Liberation Sans"),
    ("Arial", "Arimo"),
    ("Helvetica", "Liberation Sans"),
    ("Helvetica", "Arimo"),
    ("Times New Roman", "Liberation Serif"),
    ("Times New Roman", "Tinos"),
    ("Century", "Liberation Serif"),
    ("Century", "Tinos"),
    ("Courier New", "Liberation Mono"),
    ("Courier New", "Cousine"),
    ("Cambria", "Caladea"),
    ("ＭＳ ゴシック", "IPAGothic"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("MS Gothic", "IPAGothic"),
    ("ＭＳ Ｐゴシック", "IPAPGothic"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("MS PGothic", "IPAPGothic"),
    ("ＭＳ 明朝", "IPAMincho"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("MS Mincho", "IPAMincho"),
    ("ＭＳ Ｐ明朝", "IPAPMincho"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("MS PMincho", "IPAPMincho"),
];

/// For the web build: the typefaces the document asks for, under the names
/// browsers know them by, for the text the bundled fonts lack (Japanese),
/// which the browser draws with its own fonts. Without them it takes its
/// default Japanese font, whose widths differ from the document's
/// (a company name on a slide of a user's private presentation, sized to
/// Hiragino, wrapped).
#[cfg(target_family = "wasm")]
fn browser_fallbacks(style: &TextStyle) -> Option<FontFallbacks> {
    let mut names: Vec<String> = Vec::new();
    for name in [style.font.as_deref(), style.fallback_font.as_deref()].into_iter().flatten() {
        let base = WEIGHT_SUFFIXES.iter().find_map(|(suffix, _)| name.strip_suffix(suffix)).map_or(name, str::trim_end);
        for n in [name, base] {
            if !names.iter().any(|x| x == n) {
                names.push(n.to_string());
            }
            for (_, other) in TYPEFACE_ALIASES.iter().filter(|(office, _)| *office == n) {
                if !names.iter().any(|x| x == other) {
                    names.push(other.to_string());
                }
            }
        }
        if is_east_asian_typeface(name) {
            // Close in widths to Office's Japanese typefaces.
            for n in ["Hiragino Kaku Gothic ProN", "Hiragino Sans", "Yu Gothic", "YuGothic", "Meiryo", "Noto Sans JP"] {
                if !names.iter().any(|x| x == n) {
                    names.push(n.to_string());
                }
            }
        }
    }
    (!names.is_empty()).then(|| FontFallbacks::from_fonts(names))
}

#[cfg(not(target_family = "wasm"))]
fn browser_fallbacks(_: &TextStyle) -> Option<FontFallbacks> {
    None
}

/// Typefaces made for Japanese (or other East Asian) text.
fn is_east_asian_typeface(name: &str) -> bool {
    const HINTS: &[&str] = &[
        "M PLUS", "Noto Sans JP", "Noto Serif JP", "Noto Sans CJK", "Yu Gothic", "Yu Mincho",
        "Meiryo", "Hiragino", "MS Gothic", "MS Mincho", "MS PGothic", "BIZ UD", "Kosugi",
        "Sawarabi", "IPA", "Source Han",
    ];
    name.chars().any(|c| c as u32 >= 0x2e80) || HINTS.iter().any(|h| name.contains(h))
}

/// Fonts bundled with the desktop app (`fonts/carlito/`, OFL-1.1): Carlito,
/// with Calibri's widths, stands in for it. Not in the web build, whose
/// download it would add 2.7 MB to.
#[cfg(not(target_family = "wasm"))]
const BUNDLED_FONTS: &[&[u8]] = &[
    include_bytes!("../../fonts/carlito/Carlito-Regular.ttf"),
    include_bytes!("../../fonts/carlito/Carlito-Bold.ttf"),
    include_bytes!("../../fonts/carlito/Carlito-Italic.ttf"),
    include_bytes!("../../fonts/carlito/Carlito-BoldItalic.ttf"),
];
#[cfg(target_family = "wasm")]
const BUNDLED_FONTS: &[&[u8]] = &[];
/// The family of `BUNDLED_FONTS`.
const BUNDLED_FAMILY: &str = "Carlito";

/// For tests only: the fonts in the folders `IBIS_EXTRA_FONT_DIRS` names
/// (separated by `:`), e.g. the ones the real Office apps ship, so the
/// coverage tests (test/coverage/) draw with the same font files as the
/// reference and compare the drawing alone. Nothing happens without the
/// variable; the fonts are only held in memory.
#[cfg(not(target_family = "wasm"))]
fn add_extra_font_dirs(cx: &App, names: &mut HashSet<String>) {
    let Some(dirs) = std::env::var_os("IBIS_EXTRA_FONT_DIRS") else { return };
    let mut fonts = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
            if matches!(ext.as_deref(), Some("ttf" | "otf" | "ttc"))
                && let Ok(data) = std::fs::read(&path)
            {
                fonts.push(std::borrow::Cow::Owned(data));
            }
        }
    }
    if fonts.is_empty() {
        return;
    }
    match cx.text_system().add_fonts(fonts) {
        Ok(()) => names.extend(cx.text_system().all_font_names()),
        Err(err) => log::warn!("IBIS_EXTRA_FONT_DIRS: fonts could not be loaded: {err}"),
    }
}

#[cfg(target_family = "wasm")]
fn add_extra_font_dirs(_: &App, _: &mut HashSet<String>) {}

/// Embedded fonts already given to the text system, by a hash of their
/// data, with their family. The text system cannot drop fonts, so each is
/// added once per run of the app.
static REGISTERED_FONTS: std::sync::Mutex<Vec<(u64, String)>> = std::sync::Mutex::new(Vec::new());

fn data_hash(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
}

impl Fonts {
    /// Make the presentation's embedded fonts available for drawing it.
    ///
    /// Fonts are only held in memory, never installed or written out. A
    /// typeface already installed is used as installed; a font whose
    /// licence forbids embedded use is not used.
    fn add_embedded(&mut self, fonts: &[ibis_doc_model::EmbeddedFont], preload: &crate::render::preload::Preload, cx: &App) {
        let mut registered = REGISTERED_FONTS.lock().unwrap_or_else(|e| e.into_inner());
        let mut new_fonts = Vec::new();
        for font in fonts {
            let installed = self.available(&font.typeface).is_some_and(|(family, _)| !registered.iter().any(|(_, f)| **f == *family));
            if installed {
                continue; // installed on this system
            }
            let hash = data_hash(&font.data);
            if let Some((_, family)) = registered.iter().find(|(h, _)| *h == hash) {
                self.installed.insert(family.clone());
                continue;
            }
            // Decoded on another thread since the file was read (none when
            // its licence forbids using it).
            match preload.font(font) {
                Some((family, ttf)) => {
                    registered.push((hash, family.clone()));
                    self.installed.insert(family);
                    new_fonts.push(std::borrow::Cow::Owned(ttf));
                }
                None => log::warn!("embedded font {} could not be used", font.typeface),
            }
        }
        if !new_fonts.is_empty()
            && let Err(err) = cx.text_system().add_fonts(new_fonts)
        {
            log::warn!("embedded fonts could not be loaded: {err}");
        }
    }

    fn new(cx: &App) -> Self {
        // The system's fonts, listed once per run (it takes a while), with
        // the fonts bundled with the app; fonts embedded in documents are
        // added apart, by `add_embedded`.
        static SYSTEM_FONTS: std::sync::OnceLock<HashSet<String>> = std::sync::OnceLock::new();
        // Another text system than the first one (each scenario of the tests has its own)
        // is given the bundled fonts too: the list above is kept, they are not.
        let first = SYSTEM_FONTS.get().is_none();
        let installed = SYSTEM_FONTS
            .get_or_init(|| {
                let mut names: HashSet<String> = cx.text_system().all_font_names().into_iter().collect();
                if !BUNDLED_FONTS.is_empty() && !names.contains(BUNDLED_FAMILY) {
                    let fonts = BUNDLED_FONTS.iter().map(|data| std::borrow::Cow::Borrowed(*data)).collect();
                    match cx.text_system().add_fonts(fonts) {
                        Ok(()) => {
                            names.insert(BUNDLED_FAMILY.to_string());
                        }
                        Err(err) => log::warn!("bundled fonts could not be loaded: {err}"),
                    }
                }
                add_extra_font_dirs(cx, &mut names);
                names
            })
            .clone();
        // Listing the fonts takes a while: once for each text system (the app has one).
        static CHECKED_SYSTEMS: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
        let system = Arc::as_ptr(cx.text_system()) as usize;
        let new_system = {
            let mut checked = CHECKED_SYSTEMS.lock().unwrap_or_else(|e| e.into_inner());
            if first || checked.contains(&system) {
                checked.push(system);
                false
            } else {
                checked.push(system);
                true
            }
        };
        if new_system && !BUNDLED_FONTS.is_empty() && !cx.text_system().all_font_names().iter().any(|n| n == BUNDLED_FAMILY) {
            let fonts = BUNDLED_FONTS.iter().map(|data| std::borrow::Cow::Borrowed(*data)).collect();
            if let Err(err) = cx.text_system().add_fonts(fonts) {
                log::warn!("bundled fonts could not be loaded: {err}");
            }
        }
        // The last five are what a Linux system has for Japanese.
        let east_asian_fallback = ["Hiragino Sans", "Hiragino Kaku Gothic ProN", "Yu Gothic", "Noto Sans CJK JP", "Noto Sans JP", "Source Han Sans JP", "IPAexGothic", "IPAGothic"]
            .into_iter()
            .find(|f| installed.contains(*f))
            .map(SharedString::from);
        Self { installed, east_asian_fallback }
    }

    fn font(&self, style: &TextStyle) -> Font {
        let (family, mut weight) = match &style.font {
            // A typeface that is not installed gives way to the run's other one.
            Some(name) if self.available(name).is_none() && style.fallback_font.as_deref().is_some_and(|f| self.available(f).is_some()) => {
                self.family(style.fallback_font.as_deref().unwrap_or(name))
            }
            Some(name) => self.family(name),
            None => (SharedString::from(".SystemUIFont"), FontWeight::NORMAL),
        };
        // A typeface named for its weight ("Poppins SemiBold") is its own
        // family to PowerPoint, whose bold member is usually the same face,
        // so bold only applies to plain typefaces.
        let named_weight = style.font.as_deref().is_some_and(|n| WEIGHT_SUFFIXES.iter().any(|(suffix, _)| n.ends_with(suffix)));
        if style.bold && !named_weight {
            weight = FontWeight::BOLD;
        }
        Font {
            family,
            weight,
            style: if style.italic { FontStyle::Italic } else { FontStyle::Normal },
            features: FontFeatures(Arc::new(vec![("kern".into(), 0)])),
            fallbacks: browser_fallbacks(style),
            ..font(".SystemUIFont")
        }
    }

    /// The available family and weight for a typeface name, if any: the
    /// name itself, or the name without a weight word ("Poppins SemiBold"
    /// is Poppins at 600).
    fn available(&self, name: &str) -> Option<(SharedString, FontWeight)> {
        let installed = |name: &str| -> Option<SharedString> {
            if self.installed.contains(name) {
                return Some(name.to_string().into());
            }
            // The same typeface under the name this system gives it.
            TYPEFACE_ALIASES
                .iter()
                .filter(|(office, _)| *office == name)
                .find(|(_, other)| self.installed.contains(*other))
                .map(|(_, other)| other.to_string().into())
        };
        if let Some(family) = installed(name) {
            return Some((family, FontWeight::NORMAL));
        }
        WEIGHT_SUFFIXES
            .iter()
            .find_map(|(suffix, weight)| Some((name.strip_suffix(suffix)?.trim_end(), *weight)))
            .and_then(|(base, weight)| Some((installed(base)?, weight)))
    }

    /// The family and weight to draw a typeface with, falling back to a
    /// system font.
    fn family(&self, name: &str) -> (SharedString, FontWeight) {
        if let Some(found) = self.available(name) {
            return found;
        }
        let weight = WEIGHT_SUFFIXES.iter().find(|(suffix, _)| name.ends_with(suffix)).map(|(_, w)| *w).unwrap_or(FontWeight::NORMAL);
        // A Mincho (serif) typeface gives way to an installed Mincho.
        if (name.contains("明朝") || name.contains("Mincho")) // i18n-ok（フォント名の照合（部分一致で明朝を判定）。i18n-ok）
            && let Some(mincho) = ["Hiragino Mincho ProN", "YuMincho", "BIZ UDMincho", "Noto Serif CJK JP", "Noto Serif JP", "Source Han Serif JP", "IPAexMincho", "IPAMincho"].into_iter().find(|f| self.installed.contains(*f))
        {
            return (mincho.into(), weight);
        }
        match &self.east_asian_fallback {
            Some(fallback) if is_east_asian_typeface(name) => (fallback.clone(), weight),
            _ => (".SystemUIFont".into(), weight),
        }
    }
}

pub(crate) fn hsla(c: DocColor) -> Hsla {
    let Rgb(v) = c.rgb;
    Rgba {
        r: ((v >> 16) & 0xff) as f32 / 255.0,
        g: ((v >> 8) & 0xff) as f32 / 255.0,
        b: (v & 0xff) as f32 / 255.0,
        a: c.alpha as f32 / 255.0,
    }
    .into()
}

fn rgb_hsla(rgb: Rgb) -> Hsla {
    hsla(DocColor::opaque(rgb))
}

/// A fill as a GPUI background, for fills that are not pictures.
pub(crate) fn background(fill: &Fill) -> Option<Background> {
    match fill {
        Fill::Solid(c) => Some(hsla(*c).into()),
        Fill::Linear { angle, stops, .. } => {
            let (first, last) = (stops.first()?, stops.last()?);
            // OOXML's 0° runs left to right; CSS's runs bottom to top.
            let degrees = *angle as f32 / 60_000.0 + 90.0;
            Some(linear_gradient(
                degrees,
                linear_color_stop(hsla(first.color), first.position as f32 / 100_000.0),
                linear_color_stop(hsla(last.color), last.position as f32 / 100_000.0),
            ))
        }
        // The background colour; the foreground's cells are painted over it
        // (`paint_pattern`).
        Fill::Pattern { background, .. } => Some(hsla(*background).into()),
        // (Painted band by band where it can be: `gradient::paint_gradient`.)
        Fill::Path { stops, .. } => Some(hsla(stops.last()?.color).into()),
        Fill::None | Fill::Image(_) | Fill::TileImage { .. } | Fill::TileLinear { .. } => None,
    }
}

/// The dashes of a stroke along a polyline (window pixels), in its dash
/// pattern (`LineDash::pattern`, in line widths).
pub(crate) fn dash_pieces(points: &[Point<Pixels>], closed: bool, stroke: &Stroke, width: f32) -> Vec<Vec<Point<Pixels>>> {
    let pattern = match stroke.dash.pattern() {
        [] => &[4.0, 3.0][..],
        p => p,
    };
    let w = width;
    let step_len = |index: usize, _is_on: bool| pattern[index] * w;
    let mut pts: Vec<(f32, f32)> = points.iter().map(|p| (f32::from(p.x), f32::from(p.y))).collect();
    if closed && let Some(&first) = pts.first() {
        pts.push(first);
    }
    let mut out: Vec<Vec<Point<Pixels>>> = Vec::new();
    let mut current: Vec<Point<Pixels>> = Vec::new();
    let mut index = 0usize;
    let mut on = true;
    let mut left = step_len(index, on);
    for seg in pts.windows(2) {
        let ((x0, y0), (x1, y1)) = (seg[0], seg[1]);
        let len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        if len == 0.0 {
            continue;
        }
        let at = |t: f32| point(px(x0 + (x1 - x0) * t / len), px(y0 + (y1 - y0) * t / len));
        let mut t = 0.0;
        while t < len {
            let step = left.min(len - t);
            if on {
                if current.is_empty() {
                    current.push(at(t));
                }
                current.push(at(t + step));
            }
            t += step;
            left -= step;
            if left <= 1e-4 {
                if on && current.len() >= 2 {
                    out.push(std::mem::take(&mut current));
                }
                current.clear();
                on = !on;
                index = (index + 1) % pattern.len();
                left = step_len(index, on);
            }
        }
    }
    if on && current.len() >= 2 {
        out.push(current);
    }
    if stroke.cap == ibis_doc_model::LineCap::Round {
        for piece in &mut out {
            let n = piece.len();
            if n >= 2 {
                let (first, last) = (piece[0], piece[n - 1]);
                let (dx, dy) = (f32::from(last.x - first.x), f32::from(last.y - first.y));
                let len = (dx * dx + dy * dy).sqrt();
                if len <= w {
                    piece[0] = first;
                    piece[n - 1] = first;
                } else {
                    let (ux, uy) = (dx / len * w, dy / len * w);
                    piece[0] = first;
                    piece[n - 1] = point(last.x - px(ux), last.y - px(uy));
                }
            }
        }
    }
    out
}

/// Round caps: a disc of the line's width at each end of each piece.
pub(crate) fn paint_round_caps(window: &mut Window, pieces: &[Vec<Point<Pixels>>], width: f32, color: Hsla) {
    let r = width / 2.0;
    for piece in pieces {
        let ends: Vec<&Point<Pixels>> = match (piece.first(), piece.last()) {
            (Some(f), Some(l)) if (f32::from(f.x - l.x)).hypot(f32::from(f.y - l.y)) <= 1e-4 => vec![f],
            (Some(f), Some(l)) => vec![f, l],
            (Some(f), None) => vec![f],
            _ => vec![],
        };
        for end in ends {
            let disc: Vec<Point<Pixels>> = (0..16).map(|i| {
                let t = std::f32::consts::TAU * i as f32 / 16.0;
                point(end.x + px(r * t.cos()), end.y + px(r * t.sin()))
            }).collect();
            if let Some(p) = polygon(&disc, None, true) {
                window.paint_path(p, color);
            }
        }
    }
}

/// Round caps on a compound line: an outer semicircular arch connecting the
/// outer bands, plus discs for inner bands, so the line ends in concentric
/// arcs rather than separate tiny dots.
pub(crate) fn paint_compound_round_caps(
    window: &mut Window,
    pieces: &[Vec<Point<Pixels>>],
    width: f32,
    compound: ibis_doc_model::LineCompound,
    color: Hsla,
    head: bool,
    tail: bool,
) {
    if compound == ibis_doc_model::LineCompound::Single || width < 1.5 {
        return;
    }
    let n_pieces = pieces.len();
    for (idx, piece) in pieces.iter().enumerate() {
        if piece.len() < 2 {
            continue;
        }
        let apply_head = if idx == 0 { head } else { true };
        let apply_tail = if idx + 1 == n_pieces { tail } else { true };
        let p0 = piece[0];
        let p1 = piece[1];
        let pn = *piece.last().unwrap();
        let pn_prev = piece[piece.len() - 2];

        for (at, neighbor, apply, is_head) in [(p0, p1, apply_head, true), (pn, pn_prev, apply_tail, false)] {
            if !apply {
                continue;
            }
            let (dx, dy) = if is_head {
                (f32::from(neighbor.x - at.x), f32::from(neighbor.y - at.y))
            } else {
                (f32::from(at.x - neighbor.x), f32::from(at.y - neighbor.y))
            };
            let len = dx.hypot(dy);
            if len <= 1e-4 {
                continue;
            }
            let (dir_x, dir_y) = (dx / len, dy / len);
            let (nx, ny) = (-dir_y, dir_x);
            let (out_x, out_y) = if is_head { (-dir_x, -dir_y) } else { (dir_x, dir_y) };

            let make_arc = |window: &mut Window, radius: f32, stroke_w: f32| {
                let pts: Vec<Point<Pixels>> = (0..=16)
                    .map(|i| {
                        let theta = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 16.0;
                        let (sin_t, cos_t) = (theta.sin(), theta.cos());
                        point(
                            at.x + px(radius * (sin_t * nx + cos_t * out_x)),
                            at.y + px(radius * (sin_t * ny + cos_t * out_y)),
                        )
                    })
                    .collect();
                if let Some(p) = polygon(&pts, Some(px(stroke_w.max(0.5))), false) {
                    window.paint_path(p, color);
                }
            };

            let make_disc = |window: &mut Window, center: Point<Pixels>, disc_w: f32| {
                let r = disc_w / 2.0;
                let disc: Vec<Point<Pixels>> = (0..16)
                    .map(|i| {
                        let t = std::f32::consts::TAU * i as f32 / 16.0;
                        point(center.x + px(r * t.cos()), center.y + px(r * t.sin()))
                    })
                    .collect();
                if let Some(p) = polygon(&disc, None, true) {
                    window.paint_path(p, color);
                }
            };

            match compound {
                ibis_doc_model::LineCompound::Double => {
                    make_arc(window, width / 3.0, width / 3.0);
                    make_disc(window, point(at.x - px(width / 3.0 * nx), at.y - px(width / 3.0 * ny)), width / 3.0);
                    make_disc(window, point(at.x + px(width / 3.0 * nx), at.y + px(width / 3.0 * ny)), width / 3.0);
                }
                ibis_doc_model::LineCompound::Triple => {
                    make_arc(window, 5.0 / 12.0 * width, width / 6.0);
                    make_disc(window, point(at.x - px(5.0 / 12.0 * width * nx), at.y - px(5.0 / 12.0 * width * ny)), width / 6.0);
                    make_disc(window, at, width / 3.0);
                    make_disc(window, point(at.x + px(5.0 / 12.0 * width * nx), at.y + px(5.0 / 12.0 * width * ny)), width / 6.0);
                }
                ibis_doc_model::LineCompound::ThickThin => {
                    make_arc(window, 0.4 * width, 0.2 * width);
                    make_disc(window, point(at.x - px(0.2 * width * nx), at.y - px(0.2 * width * ny)), 0.6 * width);
                    make_disc(window, point(at.x + px(0.4 * width * nx), at.y + px(0.4 * width * ny)), 0.2 * width);
                }
                ibis_doc_model::LineCompound::ThinThick => {
                    make_arc(window, 0.4 * width, 0.2 * width);
                    make_disc(window, point(at.x - px(0.4 * width * nx), at.y - px(0.4 * width * ny)), 0.2 * width);
                    make_disc(window, point(at.x + px(0.2 * width * nx), at.y + px(0.2 * width * ny)), 0.6 * width);
                }
                ibis_doc_model::LineCompound::Single => {}
            }
        }
    }
}

/// The pieces of an open line with square caps: half a width longer at
/// specified ends.
pub(crate) fn square_capped(piece: &[Point<Pixels>], width: f32, head: bool, tail: bool) -> Vec<Point<Pixels>> {
    let mut out = piece.to_vec();
    let n = out.len();
    if n < 2 {
        return out;
    }
    let extend = |a: Point<Pixels>, b: Point<Pixels>| {
        let (dx, dy) = (f32::from(a.x - b.x), f32::from(a.y - b.y));
        let len = (dx * dx + dy * dy).sqrt().max(1e-4);
        point(a.x + px(dx / len * width / 2.0), a.y + px(dy / len * width / 2.0))
    };
    if head {
        out[0] = extend(piece[0], piece[1]);
    }
    if tail {
        out[n - 1] = extend(piece[n - 1], piece[n - 2]);
    }
    out
}

#[derive(Clone, Copy, Default)]
struct PatternRun {
    offset: i8,
    len: u8,
}

#[derive(Clone, Copy, Default)]
struct TileRuns {
    count: u8,
    runs: [PatternRun; 4],
}

/// Precomputed runs of 1-bits within an 8-cell tile for each byte.
const RUN_TABLE: [TileRuns; 256] = {
    let mut table = [TileRuns { count: 0, runs: [PatternRun { offset: 0, len: 0 }; 4] }; 256];
    let mut b = 0usize;
    while b < 256 {
        let bits = b as u8;
        if bits != 0 && bits != 0xff {
            // Find a zero bit to ensure runs do not wrap around the scan boundary.
            let mut zero_c = 0;
            while zero_c < 8 {
                if (bits & (0x80 >> zero_c)) == 0 {
                    break;
                }
                zero_c += 1;
            }
            let mut count = 0;
            let mut runs = [PatternRun { offset: 0, len: 0 }; 4];
            let mut i = 1;
            while i <= 8 {
                let c = (zero_c + i) % 8;
                if (bits & (0x80 >> c)) != 0 {
                    let start = (zero_c + i) % 8;
                    let mut len = 1;
                    while i + 1 <= 8 && (bits & (0x80 >> ((zero_c + i + 1) % 8))) != 0 {
                        i += 1;
                        len += 1;
                    }
                    runs[count] = PatternRun {
                        offset: start as i8,
                        len,
                    };
                    count += 1;
                }
                i += 1;
            }
            table[b] = TileRuns { count: count as u8, runs };
        }
        b += 1;
    }
    table
};

/// Check if `subpaths` represents a single axis-aligned rectangle.
/// Returns `Some((min_x, min_y, max_x, max_y))` if so.
fn as_axis_aligned_rect(subpaths: &[Vec<Point<Pixels>>]) -> Option<(f32, f32, f32, f32)> {
    if subpaths.len() != 1 {
        return None;
    }
    let pts = &subpaths[0];
    let pts = if pts.len() == 5 && pts[0] == pts[4] {
        &pts[..4]
    } else if pts.len() == 4 {
        &pts[..]
    } else {
        return None;
    };

    let xs = [f32::from(pts[0].x), f32::from(pts[1].x), f32::from(pts[2].x), f32::from(pts[3].x)];
    let ys = [f32::from(pts[0].y), f32::from(pts[1].y), f32::from(pts[2].y), f32::from(pts[3].y)];

    let min_x = xs.iter().copied().fold(f32::MAX, f32::min);
    let max_x = xs.iter().copied().fold(f32::MIN, f32::max);
    let min_y = ys.iter().copied().fold(f32::MAX, f32::min);
    let max_y = ys.iter().copied().fold(f32::MIN, f32::max);

    if max_x <= min_x || max_y <= min_y {
        return None;
    }

    for i in 0..4 {
        let (x0, y0) = (xs[i], ys[i]);
        let (x1, y1) = (xs[(i + 1) % 4], ys[(i + 1) % 4]);
        if (x0 - x1).abs() > 0.001 && (y0 - y1).abs() > 0.001 {
            return None;
        }
    }

    for i in 0..4 {
        let (x, y) = (xs[i], ys[i]);
        let at_x = (x - min_x).abs() <= 0.001 || (x - max_x).abs() <= 0.001;
        let at_y = (y - min_y).abs() <= 0.001 || (y - max_y).abs() <= 0.001;
        if !at_x || !at_y {
            return None;
        }
    }

    Some((min_x, min_y, max_x, max_y))
}

#[inline]
fn paint_pattern_span(
    window: &mut Window,
    bits: u8,
    left: f32,
    right: f32,
    top_px: f32,
    bottom_px: f32,
    ox: f32,
    cell: f32,
    color: Hsla,
) {
    if bits == 0 || right <= left || bottom_px <= top_px {
        return;
    }
    if bits == 0xff {
        window.paint_quad(gpui_kit::fill(
            Bounds::new(point(px(left), px(top_px)), size(px(right - left), px(bottom_px - top_px))),
            color,
        ));
        return;
    }
    let tile_width = 8.0 * cell;
    let first_tile = ((left - ox) / tile_width).floor() as i64 - 1;
    let last_tile = ((right - ox) / tile_width).ceil() as i64;
    let tile_runs = &RUN_TABLE[bits as usize];
    for t in first_tile..=last_tile {
        for run in &tile_runs.runs[..tile_runs.count as usize] {
            let col_start = t * 8 + run.offset as i64;
            let col_end = col_start + run.len as i64;
            let x0 = (ox + col_start as f32 * cell).max(left);
            let x1 = (ox + col_end as f32 * cell).min(right);
            if x1 > x0 {
                window.paint_quad(gpui_kit::fill(
                    Bounds::new(point(px(x0), px(top_px)), size(px(x1 - x0), px(bottom_px - top_px))),
                    color,
                ));
            }
        }
    }
}

/// Paint the foreground cells of pattern `fill` inside the closed polygons
/// `subpaths` (window pixels, even–odd), its 8 × 8 tile of 1 pt cells
/// anchored at `origin` (the slide's corner, as PowerPoint anchors it),
/// `scale` pixels per EMU. Each row of cells is cut to where the row's
/// middle line crosses the polygons.
pub(crate) fn paint_pattern(window: &mut Window, fill: &Fill, subpaths: &[Vec<Point<Pixels>>], origin: Point<Pixels>, scale: f32) {
    let Fill::Pattern { preset, foreground, .. } = fill else { return };
    let Some(tile) = ibis_doc_model::pattern_tile(preset) else { return };
    let cell = ibis_doc_model::EMU_PER_POINT as f32 * scale;
    if cell <= 0.0 || subpaths.is_empty() {
        return;
    }
    let color = hsla(*foreground);
    let (ox, oy) = (f32::from(origin.x), f32::from(origin.y));

    // Fast path: axis-aligned rectangle (covers slide background, table cells, and regular rectangular shapes).
    if let Some((left, top, right, bottom)) = as_axis_aligned_rect(subpaths) {
        if right <= left || bottom <= top {
            return;
        }

        // Fast path for vertical patterns (all 8 rows have identical bit patterns):
        // Merge the entire column into single vertical quads spanning from top to bottom.
        let is_vertical = tile.windows(2).all(|w| w[0] == w[1]);
        if is_vertical {
            paint_pattern_span(window, tile[0], left, right, top, bottom, ox, cell, color);
            return;
        }

        let first = ((top - oy) / cell).floor() as i64;
        let last = ((bottom - oy) / cell).ceil() as i64;
        let mut row = first;
        while row < last {
            let bits = tile[row.rem_euclid(8) as usize];
            if bits == 0 {
                row += 1;
                continue;
            }
            if bits == 0xff {
                // Merge consecutive 0xff rows into a single vertical quad.
                let row_start = row;
                while row + 1 < last && tile[(row + 1).rem_euclid(8) as usize] == 0xff {
                    row += 1;
                }
                let y0 = oy + row_start as f32 * cell;
                let y1 = oy + (row + 1) as f32 * cell;
                let top_px = y0.max(top);
                let bottom_px = y1.min(bottom);
                if bottom_px > top_px {
                    window.paint_quad(gpui_kit::fill(
                        Bounds::new(point(px(left), px(top_px)), size(px(right - left), px(bottom_px - top_px))),
                        color,
                    ));
                }
                row += 1;
                continue;
            }
            let y0 = oy + row as f32 * cell;
            let top_px = y0.max(top);
            let bottom_px = (y0 + cell).min(bottom);
            paint_pattern_span(window, bits, left, right, top_px, bottom_px, ox, cell, color);
            row += 1;
        }
        return;
    }

    // General polygon path. Reuses `xs` buffer across rows to eliminate heap allocations.
    let ys = subpaths.iter().flatten().map(|p| f32::from(p.y));
    let (top, bottom) = ys.clone().fold((f32::MAX, f32::MIN), |(a, b), y| (a.min(y), b.max(y)));
    let first = ((top - oy) / cell).floor() as i64;
    let last = ((bottom - oy) / cell).ceil() as i64;
    let mut xs: Vec<f32> = Vec::with_capacity(16);
    for row in first..last {
        let bits = tile[row.rem_euclid(8) as usize];
        if bits == 0 {
            continue;
        }
        let y0 = oy + row as f32 * cell;
        let middle = y0 + cell / 2.0;
        xs.clear();
        for points in subpaths {
            for i in 0..points.len() {
                let (a, b) = (points[i], points[(i + 1) % points.len()]);
                let (ay, by) = (f32::from(a.y), f32::from(b.y));
                if (ay <= middle) != (by <= middle) {
                    let t = (middle - ay) / (by - ay);
                    xs.push(f32::from(a.x) + t * (f32::from(b.x) - f32::from(a.x)));
                }
            }
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        let top_px = y0.max(top);
        let bottom_px = (y0 + cell).min(bottom);
        if bottom_px <= top_px {
            continue;
        }
        for span in xs.chunks_exact(2) {
            paint_pattern_span(window, bits, span[0], span[1], top_px, bottom_px, ox, cell, color);
        }
    }
}

/// The picture `media` cut to `outlines` (frame-relative EMU, flipped as
/// the shape is), if its format can be wrapped in SVG.
fn clipped_fill(media: MediaId, shape: &Shape, outlines: &[ibis_doc_model::Outline], frame: EmuRect, res: &Resources) -> Option<Arc<Image>> {
    let (w, h) = (frame.width as f64, frame.height as f64);
    let subpaths: Vec<Vec<(f64, f64)>> = outlines
        .iter()
        .filter(|o| o.fill != ibis_doc_model::PathFill::None)
        .flat_map(|o| &o.subpaths)
        .map(|s| {
            s.points
                .iter()
                .map(|&(x, y)| (if shape.flip_h { w - x } else { x }, if shape.flip_v { h - y } else { y }))
                .collect()
        })
        .collect();
    let key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (frame.width, frame.height).hash(&mut hasher);
        for p in subpaths.iter().flatten() {
            (p.0.to_bits(), p.1.to_bits()).hash(&mut hasher);
        }
        (media, hasher.finish())
    };
    if let Some(found) = res.clipped.borrow().get(&key) {
        return Some(found.clone());
    }
    let (encoding, data) = res.source(media)?;
    let svg = crate::render::picture_svg::clipped_picture(&data, encoding, frame.width, frame.height, &subpaths);
    let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg));
    res.clipped.borrow_mut().insert(key, image.clone());
    Some(image)
}

/// `fill` turned with its shape: a gradient's direction follows the shape's
/// flips and rotation (OOXML's `rotWithShape`, on by default).
fn turned_fill(fill: &Fill, shape: &Shape) -> Fill {
    match fill {
        Fill::Linear { angle, scaled, stops } => {
            let mut angle = *angle;
            if shape.flip_v {
                angle = -angle;
            }
            if shape.flip_h {
                angle = 10_800_000 - angle;
            }
            Fill::Linear { angle: (angle + shape.rotation).rem_euclid(21_600_000), scaled: *scaled, stops: stops.clone() }
        }
        Fill::TileLinear { angle, scaled, stops, tile, flip } => {
            let mut angle = *angle;
            if shape.flip_v {
                angle = -angle;
            }
            if shape.flip_h {
                angle = 10_800_000 - angle;
            }
            Fill::TileLinear { angle: (angle + shape.rotation).rem_euclid(21_600_000), scaled: *scaled, stops: stops.clone(), tile: *tile, flip: *flip }
        }
        other => other.clone(),
    }
}

/// A shadow's offset, blur radius (pixels) and colour.
fn shadow_parts(shadow: ibis_doc_model::Shadow, scale: f32) -> (f32, f32, f32, Hsla) {
    let angle = (shadow.direction as f32 / 60_000.0).to_radians();
    let distance = shadow.distance as f32 * scale;
    (distance * angle.cos(), distance * angle.sin(), shadow.blur as f32 * scale, hsla(shadow.color))
}

fn shadow_anchor(align: ibis_doc_model::TileAlign, w: f64, h: f64, stroke_half: f64) -> (f64, f64) {
    let s = stroke_half;
    let w_ext = w + 2.0 * s;
    let h_ext = h + 2.0 * s;
    let (ax, ay) = match align {
        ibis_doc_model::TileAlign::TopLeft => (0.0, 0.0),
        ibis_doc_model::TileAlign::Top => (w_ext / 2.0, 0.0),
        ibis_doc_model::TileAlign::TopRight => (w_ext, 0.0),
        ibis_doc_model::TileAlign::Left => (0.0, h_ext / 2.0),
        ibis_doc_model::TileAlign::Center => (w_ext / 2.0, h_ext / 2.0),
        ibis_doc_model::TileAlign::Right => (w_ext, h_ext / 2.0),
        ibis_doc_model::TileAlign::BottomLeft => (0.0, h_ext),
        ibis_doc_model::TileAlign::Bottom => (w_ext / 2.0, h_ext),
        ibis_doc_model::TileAlign::BottomRight => (w_ext, h_ext),
    };
    (ax - s, ay - s)
}

fn render_transformed_shadow(
    window: &mut Window,
    outlines: &[ibis_doc_model::Outline],
    anchor: (f64, f64),
    shadow: &ibis_doc_model::Shadow,
    blur: f32,
    color: Hsla,
    exact: Point<Pixels>,
    scale: f32,
) {
    let (xa, ya) = anchor;
    let sx = shadow.sx as f64 / 100_000.0;
    let sy = shadow.sy as f64 / 100_000.0;
    let kx_rad = (shadow.kx as f64 / 60_000.0).to_radians();
    let ky_rad = (shadow.ky as f64 / 60_000.0).to_radians();
    let tan_kx = kx_rad.tan();
    let tan_ky = ky_rad.tan();

    let dir_rad = (shadow.direction as f64 / 60_000.0).to_radians();
    let dist = shadow.distance as f64;
    let off_x = dist * dir_rad.cos();
    let off_y = dist * dir_rad.sin();

    let transform_pt = |p: (f64, f64)| -> (f64, f64) {
        let dx_rel = p.0 - xa;
        let dy_rel = p.1 - ya;
        let x_prime = sx * dx_rel + dy_rel * tan_kx;
        let y_prime = sy * dy_rel + dx_rel * tan_ky;
        (xa + x_prime + off_x, ya + y_prime + off_y)
    };

    let spots: Vec<(f32, f32, f32)> = if blur < 0.5 {
        vec![(0.0, 0.0, 1.0)]
    } else {
        let mut s = Vec::with_capacity(37);
        s.push((0.0, 0.0, 1.0));
        let r1 = blur * 0.30;
        for i in 0..8 {
            let (sin, cos) = (i as f32 * std::f32::consts::FRAC_PI_4).sin_cos();
            s.push((cos * r1, sin * r1, 0.7));
        }
        let r2 = blur * 0.70;
        for i in 0..12 {
            let (sin, cos) = (i as f32 * std::f32::consts::TAU / 12.0).sin_cos();
            s.push((cos * r2, sin * r2, 0.6));
        }
        let r3 = blur * 1.05;
        for i in 0..16 {
            let (sin, cos) = (i as f32 * std::f32::consts::TAU / 16.0).sin_cos();
            s.push((cos * r3, sin * r3, 0.25));
        }
        s
    };

    let total_weight: f32 = spots.iter().map(|(_, _, w)| *w).sum();

    for &(ox, oy, weight) in &spots {
        let faint = Hsla { a: (color.a * (weight / total_weight) * 1.25).min(1.0), ..color };
        for outline in outlines.iter().filter(|o| o.fill != ibis_doc_model::PathFill::None) {
            let subpaths: Vec<(Vec<Point<Pixels>>, bool)> = outline
                .subpaths
                .iter()
                .map(|s| {
                    let pts: Vec<Point<Pixels>> = s
                        .points
                        .iter()
                        .map(|&p| {
                            let (tx, ty) = transform_pt(p);
                            point(
                                exact.x + px((tx * scale as f64) as f32 + ox),
                                exact.y + px((ty * scale as f64) as f32 + oy),
                            )
                        })
                        .collect();
                    (pts, s.closed)
                })
                .collect();
            if let Some(p) = compound_path(&subpaths, None) {
                window.paint_path(p, faint);
            }
        }
    }
}

/// Where a shape `w` × `h` pixels tilted by `tilt` appears in its frame, as
/// x, y, width and height: an orthographic view shortens it by the cosine
/// of each turn, and its lift off the slide moves it by the sine
/// (measured against PowerPoint's PDF output: a lime band under a heading
/// turned 75° about the horizontal axis and lifted 6 pt).
fn tilted_frame(tilt: ibis_doc_model::Tilt, w: f32, h: f32, scale: f32) -> (f32, f32, f32, f32) {
    let lat = (tilt.lat as f32 / 60_000.0).to_radians();
    let lon = (tilt.lon as f32 / 60_000.0).to_radians();
    let depth = tilt.depth as f32 * scale;
    let (tw, th) = (w * lon.cos().abs(), h * lat.cos().abs());
    let x = (w - tw) / 2.0 + depth * lon.sin();
    let y = (h - th) / 2.0 + depth * lat.sin();
    // PowerPoint draws the tilted shape's edges too, about a point around it.
    let edge = 0.75 * scale * EMU_PER_POINT as f32;
    (x - edge, y, tw + edge * 2.0, th + edge)
}

/// Place an element at `frame` on a slide drawn at `scale`.
pub(crate) fn place<E: Styled>(element: E, frame: EmuRect, scale: f32) -> E {
    let x0 = frame.x as f32 * scale;
    let y0 = frame.y as f32 * scale;
    let x1 = (frame.x + frame.width) as f32 * scale;
    let y1 = (frame.y + frame.height) as f32 * scale;
    let snap = |v: f32| {
        if (v - v.trunc()).abs() == 0.5 { v.trunc() } else { v.round() }
    };
    let w = (snap(x1) - snap(x0)).max(0.0);
    let h = (snap(y1) - snap(y0)).max(0.0);
    element
        .absolute()
        .left(px(x0))
        .top(px(y0))
        .w(px(w))
        .h(px(h))
}

/// Paint `fill` over the whole of `element`.
pub(crate) fn with_fill(element: Div, fill: &Fill, res: &Resources) -> Div {
    match fill {
        // The background colour (`slide_backdrop` paints the cells).
        Fill::Pattern { background: bg, .. } => element.bg(hsla(*bg)),
        Fill::Image(media) | Fill::TileImage { media, .. } => match res.picture(*media) {
            Some(image) => element.child(img(image).absolute().size_full().object_fit(ObjectFit::Fill)),
            None => element,
        },
        other => match background(other) {
            Some(bg) => element.bg(bg),
            None => element,
        },
    }
}

/// Paint a tiled picture over `bounds_emu` (in EMU), at `scale` pixels per EMU.
pub(crate) fn tile_fill_element(
    media: MediaId,
    tx: i64,
    ty: i64,
    sx: i32,
    sy: i32,
    align: ibis_doc_model::TileAlign,
    bounds_emu: (i64, i64),
    scale: f32,
    res: &Resources,
) -> Div {
    let container = div().absolute().size_full().overflow_hidden();
    let Some(image) = res.picture(media) else { return container };
    let Some((img_w_px, img_h_px)) = res.image_size(media) else { return container };
    if img_w_px == 0 || img_h_px == 0 {
        return container;
    }

    // In Office, picture original dimensions are 1 px = 1 pt = 12700 EMU.
    let orig_w_emu = img_w_px as i64 * ibis_doc_model::EMU_PER_POINT;
    let orig_h_emu = img_h_px as i64 * ibis_doc_model::EMU_PER_POINT;

    let sx_f = (sx.max(1) as f64) / 100_000.0;
    let sy_f = (sy.max(1) as f64) / 100_000.0;

    let tile_w_emu = ((orig_w_emu as f64 * sx_f).round() as i64).max(ibis_doc_model::EMU_PER_POINT);
    let tile_h_emu = ((orig_h_emu as f64 * sy_f).round() as i64).max(ibis_doc_model::EMU_PER_POINT);

    let (dest_w_emu, dest_h_emu) = bounds_emu;

    let (base_x, base_y) = match align {
        ibis_doc_model::TileAlign::TopLeft => (0, 0),
        ibis_doc_model::TileAlign::Top => ((dest_w_emu - tile_w_emu) / 2, 0),
        ibis_doc_model::TileAlign::TopRight => (dest_w_emu - tile_w_emu, 0),
        ibis_doc_model::TileAlign::Left => (0, (dest_h_emu - tile_h_emu) / 2),
        ibis_doc_model::TileAlign::Center => ((dest_w_emu - tile_w_emu) / 2, (dest_h_emu - tile_h_emu) / 2),
        ibis_doc_model::TileAlign::Right => (dest_w_emu - tile_w_emu, (dest_h_emu - tile_h_emu) / 2),
        ibis_doc_model::TileAlign::BottomLeft => (0, dest_h_emu - tile_h_emu),
        ibis_doc_model::TileAlign::Bottom => ((dest_w_emu - tile_w_emu) / 2, dest_h_emu - tile_h_emu),
        ibis_doc_model::TileAlign::BottomRight => (dest_w_emu - tile_w_emu, dest_h_emu - tile_h_emu),
    };

    let anchor_x_emu = base_x + tx;
    let anchor_y_emu = base_y + ty;

    let tile_w_px = tile_w_emu as f32 * scale;
    let tile_h_px = tile_h_emu as f32 * scale;
    let anchor_x_px = anchor_x_emu as f32 * scale;
    let anchor_y_px = anchor_y_emu as f32 * scale;
    let dest_w_px = dest_w_emu as f32 * scale;
    let dest_h_px = dest_h_emu as f32 * scale;

    let first_col = ((-anchor_x_px) / tile_w_px).floor() as i32;
    let last_col = ((dest_w_px - anchor_x_px) / tile_w_px).ceil() as i32;
    let first_row = ((-anchor_y_px) / tile_h_px).floor() as i32;
    let last_row = ((dest_h_px - anchor_y_px) / tile_h_px).ceil() as i32;

    let col_count = (last_col - first_col).max(0) as usize;
    let row_count = (last_row - first_row).max(0) as usize;
    if col_count * row_count > 10_000 {
        return container;
    }

    let mut grid = div().absolute().size_full();
    for row in first_row..last_row {
        for col in first_col..last_col {
            let x = anchor_x_px + col as f32 * tile_w_px;
            let y = anchor_y_px + row as f32 * tile_h_px;
            grid = grid.child(
                img(image.clone())
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(tile_w_px))
                    .h(px(tile_h_px))
                    .object_fit(ObjectFit::Fill),
            );
        }
    }
    container.child(grid)
}

/// The slide background and the layout's and master's shapes.
pub(crate) fn slide_backdrop(slide: &Slide, scale: f32, res: &Resources) -> Div {
    let base = match &slide.background {
        Fill::TileImage { media, tx, ty, sx, sy, align, .. } => {
            let size = slide.size.unwrap_or(ibis_doc_model::EmuSize {
                width: 12_192_000,
                height: 6_858_000,
            });
            div().absolute().size_full().top_0().left_0().child(tile_fill_element(
                *media,
                *tx,
                *ty,
                *sx,
                *sy,
                *align,
                (size.width, size.height),
                scale,
                res,
            ))
        }
        _ => with_fill(div().absolute().size_full().top_0().left_0(), &slide.background, res),
    };
    // A pattern's cells over the whole slide, from its corner.
    let base = match &slide.background {
        pattern @ Fill::Pattern { .. } => {
            let pattern = pattern.clone();
            base.child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let (o, s) = (bounds.origin, bounds.size);
                        let rect = vec![o, point(o.x + s.width, o.y), point(o.x + s.width, o.y + s.height), point(o.x, o.y + s.height)];
                        paint_pattern(window, &pattern, &[rect], o, scale);
                    },
                )
                .absolute()
                .size_full(),
            )
        }
        gradient if crate::render::gradient::is_gradient(gradient) => {
            let gradient = gradient.clone();
            base.child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let (o, s) = (bounds.origin, bounds.size);
                        let (w, h) = (f32::from(s.width), f32::from(s.height));
                        let rect = vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
                        crate::render::gradient::paint_gradient(window, &gradient, &[rect], (w, h), 1.0, &|pts| {
                            pts.iter().map(|&(x, y)| point(o.x + px(x), o.y + px(y))).collect()
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
        }
        _ => base,
    };
    base
        .overflow_hidden()
        .children(slide.shown_backdrop().iter().map(|shape| shape_element(shape, shape.frame, scale, res, None)))
}

/// A shape drawn at `frame` (which may differ from `shape.frame` while it is
/// dragged). `text` replaces the drawn text, for the in-place text field.
pub(crate) fn shape_element(
    shape: &Shape,
    frame: EmuRect,
    scale: f32,
    res: &Resources,
    text: Option<AnyElement>,
) -> AnyElement {
    shape_element_with_positions(shape, frame, scale, res, text, None)
}

/// `shape_element`, recording where the shape's text is painted in
/// `positions` (for the caret and clicks while the text is edited).
pub(crate) fn shape_element_with_positions(
    shape: &Shape,
    frame: EmuRect,
    scale: f32,
    res: &Resources,
    text: Option<AnyElement>,
    positions: Option<&crate::render::rich_text::TextPositions>,
) -> AnyElement {
    let container = place(div(), frame, scale);
    // The text rectangle: the geometry's (the frame for a text box),
    // flipped with the shape.
    let (fw, fh) = (frame.width as f64, frame.height as f64);
    let (mut l, mut t, mut r, mut b) = match &shape.kind {
        ShapeKind::Auto(auto) => auto.geometry.text_rect(fw, fh),
        _ => (0.0, 0.0, fw, fh),
    };
    if shape.flip_h {
        (l, r) = (fw - r, fw - l);
    }
    if shape.flip_v {
        (t, b) = (fh - b, fh - t);
    }
    let body = shape.text_body();
    let text_turn = body.and_then(|body| text_turn(shape, body, (l, t, r, b), frame, scale));
    // Turned a quarter turn (`vert`), the text is laid out in the rectangle
    let sideways = body.is_some_and(|b| (quarter_turn(b).rem_euclid(10_800_000) == 5_400_000) != (b.upright && upright_swaps(shape.rotation)));
    let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
    let (w, h) = if sideways { (b - t, r - l) } else { (r - l, b - t) };
    let is_1d_line = match &shape.kind {
        ShapeKind::Auto(auto) => match &auto.geometry {
            Geometry::Formula(f) => matches!(f.preset.as_deref(), Some("line" | "lineInv" | "straightConnector1")),
            _ => false,
        },
        ShapeKind::Line(_) => true,
        _ => false,
    };
    let is_point_text = w <= 0.0 && h <= 0.0;
    let text = text.or_else(|| {
        // Edited text is laid out even when empty, for its caret.
        let body = body.filter(|b| !b.is_empty() || positions.is_some())?;
        let (body, max_w) = if is_point_text {
            let mut modified = body.clone();
            modified.insets = ibis_doc_model::Insets::default();
            modified.wrap = false;
            (std::borrow::Cow::Owned(modified), Some(0.0))
        } else if is_1d_line {
            let mut modified = body.clone();
            modified.wrap = false;
            (std::borrow::Cow::Owned(modified), None)
        } else {
            // Turned a quarter turn by `vert`, the insets stay on the
            // shape's sides (PowerPoint's PDF: `lIns` is still at the left
            // of the shape), so they are given to the turned layout turned
            // back.
            let quarter = quarter_turn(body).rem_euclid(21_600_000);
            let turned_insets = (quarter == 5_400_000 || quarter == 16_200_000) && !(body.upright && upright_swaps(shape.rotation));
            let turned = turned_insets.then(|| {
                let mut b = body.clone();
                let i = body.insets;
                b.insets = if quarter == 5_400_000 {
                    ibis_doc_model::Insets { left: i.top, top: i.right, right: i.bottom, bottom: i.left }
                } else {
                    ibis_doc_model::Insets { left: i.bottom, top: i.left, right: i.top, bottom: i.right }
                };
                b
            });
            let fitted = fit_insets(turned.as_ref().unwrap_or(body), w, h);
            let b = match (fitted, turned) {
                (Some(f), _) => std::borrow::Cow::Owned(f),
                (None, Some(t)) => std::borrow::Cow::Owned(t),
                (None, None) => std::borrow::Cow::Borrowed(body),
            };
            (b, Some(w))
        };
        let has_stroke = match &shape.kind {
            ShapeKind::Auto(auto) => auto.stroke.is_some(),
            ShapeKind::Line(_) => true,
            _ => false,
        };
        let origin_x = if has_stroke {
            None
        } else {
            let text_left = (cx - w / 2.0) as f32 * scale;
            Some(frame.x as f32 * scale + text_left)
        };
        Some(text_body_element(&body, max_w, scale, res, positions, text_turn, origin_x).into_any_element())
    });
    // The text goes in the text rectangle.
    let text = text.map(|text| {
        div()
            .absolute()
            .left(px((cx - w / 2.0) as f32 * scale))
            .top(px((cy - h / 2.0) as f32 * scale))
            .w(px(w.max(0.0) as f32 * scale))
            .h(px(h.max(0.0) as f32 * scale))
            .child(text)
    });
    match &shape.kind {
        ShapeKind::TextBox(_) => container.children(text).into_any_element(),
        ShapeKind::Auto(auto) => auto_shape(container, shape, auto, frame, scale, res).children(text).into_any_element(),
        ShapeKind::Picture(picture) => match res.has_picture(picture.media) {
            true => picture_element(container, shape, picture, frame, scale, res).into_any_element(),
            // A picture that cannot be drawn keeps its place visible.
            false => {
                let format = res.formats.get(&picture.media).copied().unwrap_or(MediaFormat::Other);
                placeholder_element(container, &Placeholder::Picture(format), frame, scale).into_any_element()
            }
        },
        ShapeKind::Line(stroke) => line_element(shape, stroke.clone(), frame, scale).into_any_element(),
        // A SmartArt graphic's shapes are drawn on their own.
        ShapeKind::Placeholder(Placeholder::Diagram) => container.into_any_element(),
        ShapeKind::Placeholder(kind) => placeholder_element(container, kind, frame, scale).into_any_element(),
        ShapeKind::Chart(chart) => container.child(crate::render::chart_view::chart_element(chart, frame, scale, res)).into_any_element(),
        ShapeKind::Table(table) => {
            // PowerPoint hides tables that have rotation or flip applied.
            if shape.rotation != 0 || shape.flip_h || shape.flip_v {
                div().into_any_element()
            } else {
                container.child(crate::render::table_view::table_element(table, frame, scale, res)).into_any_element()
            }
        }
        ShapeKind::Sheet(sheet) => container.child(crate::xlsx::sheet_view::sheet_element(sheet, frame, scale, res)).into_any_element(),
        ShapeKind::Page { document, index } => container.child(crate::docx::page_view::page_element(document, *index, frame, scale, res)).into_any_element(),
        ShapeKind::Slicer(slicer) => container.child(crate::xlsx::slicer_view::slicer_element(slicer, frame, scale)).into_any_element(),
    }
}

/// `body` with insets that fit a `w` by `h` text rectangle, if its own do
/// not: insets wider than the rectangle keep the middle of the space between
/// them and shrink until the text has the whole rectangle, as PowerPoint
/// centres text in a box narrower than its insets (a 0.14 in box with 0.1 in
/// insets). `None` if the insets fit.
fn fit_insets(body: &TextBody, w: f64, h: f64) -> Option<TextBody> {
    let fit = |a: i64, b: i64, size: f64| -> Option<(i64, i64)> {
        if (a + b) as f64 <= size || size <= 0.0 {
            return None;
        }
        let middle = ((a as f64 + size - b as f64) / 2.0).clamp(0.0, size);
        let half = middle.min(size - middle);
        Some(((middle - half).round() as i64, (size - middle - half).round() as i64))
    };
    let i = body.insets;
    let across = fit(i.left, i.right, w);
    let down = fit(i.top, i.bottom, h);
    if across.is_none() && down.is_none() {
        return None;
    }
    let mut body = body.clone();
    if let Some((l, r)) = across {
        (body.insets.left, body.insets.right) = (l, r);
    }
    if let Some((t, b)) = down {
        (body.insets.top, body.insets.bottom) = (t, b);
    }
    Some(body)
}

/// How a shape's text is turned, as PowerPoint turns it (measured on its
/// PDF: the glyphs' angles in the shapes of the PXFM coverage tests): with
/// the shape's rotation, about the shape's middle; a shape flipped upside
/// down has its text turned half a turn more (a flip left to right does
/// not turn it), and `bodyPr/@rot` and `vert` turn it more, about the
/// middle of its rectangle (`rect`, frame EMU, already flipped). Upright
/// text (`upright`) turns with neither the shape nor its flips: its
/// rectangle's middle goes round with the shape, the text stays level
/// there (laid out in the rectangle turned a quarter turn when the shape
/// is turned nearer a quarter turn than level, `upright_swaps`). `None`
/// when it is not turned.
/// Whether upright text in a shape turned `rotation` is laid out in its
/// rectangle turned a quarter turn: when the shape is nearer upright on its
/// side (measured: 90° and 270° do, 30°, 135° and 180° do not).
fn upright_swaps(rotation: i32) -> bool {
    let a = rotation.rem_euclid(10_800_000);
    (2_700_000..8_100_000).contains(&a)
}

/// How far the text is turned as a whole for `vert`: East Asian vertical
/// writing is laid out as the text turned a quarter turn clockwise (`vert`),
/// with its East Asian characters drawn upright.
fn quarter_turn(body: &TextBody) -> i32 {
    if body.vertical && matches!(body.vertical_kind, ibis_doc_model::VerticalKind::EastAsian | ibis_doc_model::VerticalKind::Mongolian) {
        5_400_000
    } else {
        body.sideways
    }
}

fn text_turn(shape: &Shape, body: &TextBody, (l, t, r, b): (f64, f64, f64, f64), frame: EmuRect, scale: f32) -> Option<crate::render::rich_text::TextTurn> {
    let degrees = |v: i32| v.rem_euclid(21_600_000) as f32 / 60_000.0;
    let shape_turn = degrees(shape.rotation);
    // Upright text is turned back as much as the shape turns it.
    let flip_turn = if body.upright { 360.0 - shape_turn } else if shape.flip_v { 180.0 } else { 0.0 };
    let text = (degrees(body.rotation) + degrees(quarter_turn(body)) + flip_turn) % 360.0;
    if shape_turn == 0.0 && text == 0.0 {
        return None;
    }
    let (fw, fh) = (frame.width as f64, frame.height as f64);
    let frame_offset = (((fw - l - r) / 2.0) as f32 * scale, ((fh - t - b) / 2.0) as f32 * scale);
    let shape_turned = (shape_turn != 0.0 || shape.flip_v) && !body.upright;
    Some(crate::render::rich_text::TextTurn { text, shape: shape_turn, frame: frame_offset, shape_turned })
}

/// The spread and blur, in pixels, of the box shadow drawn for a glow
/// `r` pixels wide. PowerPoint's glow is its full colour at the shape's edge
/// and fades out at the radius (measured against its PDF: a 15 pt glow is
/// full colour a third of the way out, half at half the radius and gone
/// at the radius).
fn glow_spread_blur(r: f32) -> (f32, f32) {
    (r * 0.5, r * 0.2)
}

/// The outline of a rectangle, rounded rectangle or ellipse `w` by `h`
/// pixels: centred on its edge, half of it outside, as PowerPoint draws it.
/// A dashed rectangle's dashes are PowerPoint's "dash", four line widths
/// with gaps of three, starting at each corner (a dashed box on a slide of a
/// user's private presentation, against the PDF).
/// `at` is where the frame is meant to be on the slide (pixels, before
/// GPUI snaps the element to whole pixels).
fn outline_element(stroke: &Stroke, w: f32, h: f32, radius: Pixels, scale: f32, rect: bool, at: (f32, f32)) -> AnyElement {
    let width = f32::from(stroke_px(stroke, scale));
    let half = width / 2.0;
    let color = hsla(stroke.color);
    if stroke.dashed && rect {
        let (dash, gap) = ((width * 4.0).max(3.0), (width * 3.0).max(2.0));
        return canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let o = bounds.origin;
                // Each edge from its corner, a dash at a time.
                let mut edge = |x0: f32, y0: f32, len: f32, across: bool| {
                    let mut t = 0.0;
                    while t < len {
                        let l = dash.min(len - t);
                        let r = if across {
                            Bounds::new(point(o.x + px(x0 + t - half), o.y + px(y0 - half)), size(px(l + if t + l >= len { width } else { 0.0 }), px(width)))
                        } else {
                            Bounds::new(point(o.x + px(x0 - half), o.y + px(y0 + t - half)), size(px(width), px(l + if t + l >= len { width } else { 0.0 })))
                        };
                        window.paint_quad(fill(r, color));
                        t += dash + gap;
                    }
                };
                edge(0.0, 0.0, w, true);
                edge(0.0, h, w, true);
                edge(0.0, 0.0, h, false);
                edge(w, 0.0, h, false);
            },
        )
        .absolute()
        .left_0()
        .top_0()
        .w(px(w))
        .h(px(h))
        .into_any_element();
    }
    let inset = stroke.alignment == ibis_doc_model::LineAlignment::Inset;
    if rect && !stroke.dashed {
        // Each edge of the outline on the pixels nearest to where it is
        // meant to be, as PowerPoint puts it, not from the element's
        // snapped corner (a pixel off on one side or the other).
        return canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let sf = window.scale_factor();
                let dev = |v: f32| (v * sf).round() / sf;
                let exact = exact_origin(bounds.origin, at, sf);
                let (ox, oy) = (f32::from(exact.x), f32::from(exact.y));
                let (l, r) = if inset {
                    (dev(ox), dev(ox + w))
                } else {
                    (dev(ox - half), dev(ox + w + half))
                };
                let (t, b) = if inset {
                    (dev(oy), dev(oy + h))
                } else {
                    (dev(oy - half), dev(oy + h + half))
                };
                let th = dev(width).max(1.0 / sf).min((r - l) / 2.0).min((b - t) / 2.0);
                let quad = |x: f32, y: f32, qw: f32, qh: f32| fill(Bounds::new(point(px(x), px(y)), size(px(qw), px(qh))), color);
                window.paint_quad(quad(l, t, r - l, th));
                window.paint_quad(quad(l, b - th, r - l, th));
                window.paint_quad(quad(l, t + th, th, b - t - 2.0 * th));
                window.paint_quad(quad(r - th, t + th, th, b - t - 2.0 * th));
            },
        )
        .absolute()
        .left_0()
        .top_0()
        .w(px(w))
        .h(px(h))
        .into_any_element();
    }
    if !stroke.dashed {
        let stroke_half = if inset { px(0.) } else { px(half) };
        return canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let sf = window.scale_factor();
                let dev = |v: f32| (v * sf).round() / sf;
                let exact = exact_origin(bounds.origin, at, sf);
                let (ox, oy) = (f32::from(exact.x), f32::from(exact.y));
                let (l, r) = if inset {
                    (dev(ox), dev(ox + w))
                } else {
                    (dev(ox - half), dev(ox + w + half))
                };
                let (t, b) = if inset {
                    (dev(oy), dev(oy + h))
                } else {
                    (dev(oy - half), dev(oy + h + half))
                };
                let quad_bounds = Bounds::new(point(px(l), px(t)), size(px(r - l), px(b - t)));
                let border_widths = Edges::all(px(dev(width).max(1.0 / sf)));
                window.paint_quad(quad(quad_bounds, radius + stroke_half, transparent_black(), border_widths, color, BorderStyle::default()));
            },
        )
        .absolute()
        .left_0()
        .top_0()
        .w(px(w))
        .h(px(h))
        .into_any_element();
    }
    div()
        .absolute()
        .left(px(-half))
        .top(px(-half))
        .w(px(w + width))
        .h(px(h + width))
        .rounded(radius + px(half))
        .border(px(width))
        .border_color(color)
        .when(stroke.dashed, |d| d.border_dashed())
        .into_any_element()
}

/// Stroke width on screen. GPUI's path rasterizer drops paths thinner than a
/// pixel, so thin lines are drawn one pixel wide (as PowerPoint does too).
fn stroke_px(stroke: &Stroke, scale: f32) -> Pixels {
    px((stroke.width as f32 * scale).max(1.0))
}

/// Stroke width for painting: at least a pixel of the screen, so a thin line
/// on a high-density screen is one device pixel wide, as PowerPoint draws it.
fn painted_stroke_px(stroke: &Stroke, scale: f32, window: &Window) -> Pixels {
    px((stroke.width as f32 * scale).max(1.0 / window.scale_factor()))
}

fn auto_shape(container: Div, shape: &Shape, auto: &AutoShape, frame: EmuRect, scale: f32, res: &Resources) -> Div {
    let rotated = shape.rotation.rem_euclid(21_600_000) != 0;
    // Soft edges (`a:softEdge`) on a plain filled rectangle: the shape fades
    // out over the radius inside its edge, and its outline goes with it
    // (PEFF-00081〜00083 against PowerPoint's PDF). Drawn as the blurred
    // shadow of a box inside the shape: fitting a Gaussian edge to the
    // reference's fades puts it 0.8 of the radius in (8.5 px for a 10 px
    // radius, 15.6 for 20) with a deviation of a third of the radius
    // (3.4 px, 6.8 px).
    if let (Some(soft), Fill::Solid(color), Geometry::Rect | Geometry::RoundRect { .. }) = (auto.soft_edge, &auto.fill, &auto.geometry)
        && !rotated
        && auto.text.as_ref().is_none_or(|t| t.paragraphs.iter().all(|p| p.runs.is_empty()))
    {
        let w = frame.width as f32 * scale;
        let h = frame.height as f32 * scale;
        let r = (soft as f32 * scale).min(w / 2.0).min(h / 2.0);
        let corner = match auto.geometry {
            Geometry::RoundRect { radius } => w.min(h) * radius as f32 / 100_000.0,
            _ => 0.0,
        };
        let inset = r * 0.8;
        return container.child(
            div()
                .absolute()
                .left(px(inset))
                .top(px(inset))
                .w(px((w - 2.0 * inset).max(0.0)))
                .h(px((h - 2.0 * inset).max(0.0)))
                .rounded(px((corner - inset).max(0.0)))
                .shadow(vec![BoxShadow { color: hsla(*color), offset: point(px(0.), px(0.)), blur_radius: px(r / 3.0), spread_radius: px(0.), inset: false }]),
        );
    }
    // (A pattern is painted cell by cell along the outline, so as a path.)
    let simple = !rotated
        && !matches!(auto.fill, Fill::Pattern { .. })
        && !crate::render::gradient::is_gradient(&auto.fill)
        && !auto.stroke.as_ref().is_some_and(|s| s.dashed || s.compound != ibis_doc_model::LineCompound::Single)
        && matches!(auto.geometry, Geometry::Rect | Geometry::RoundRect { .. })
        // Drawn exactly, as paths (the quads would be put on whole pixels),
        // but for what only the elements draw (a picture fill, shadows).
        && (!res.exact || matches!(auto.fill, Fill::Image(_)) || auto.shadow.is_some() || auto.glow.is_some() || auto.inner_shadow.is_some() || auto.reflection.is_some());

    if simple {
        let w = frame.width as f32 * scale;
        let h = frame.height as f32 * scale;
        let radius = match auto.geometry {
            Geometry::RoundRect { radius } => px(w.min(h) * radius as f32 / 100_000.0),
            Geometry::Ellipse => px(w.max(h)),
            _ => px(0.),
        };
        let paint = |container: Div| {
            let mut container = container;
            let stroke_half = auto.stroke.as_ref().map(|s| stroke_px(s, scale) / 2.0).unwrap_or(px(0.));
            let half = f32::from(stroke_half);
            let at = (frame.x as f32 * scale, frame.y as f32 * scale);

            // 1. Hard shadows (blur < 0.5) drawn exactly onto the background via paint_quad
            if let Some(s) = auto.shadow.filter(|s| auto.fill != Fill::None && !s.has_transform()) {
                let (dx, dy, blur, color) = shadow_parts(s, scale);
                if blur < 0.5 {
                    container = container.child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                let sf = window.scale_factor();
                                let dev = |v: f32| (v * sf).round() / sf;
                                let exact = exact_origin(bounds.origin, at, sf);
                                let (ox, oy) = (f32::from(exact.x), f32::from(exact.y));
                                let frac = |v: f32| {
                                    let s = v * sf;
                                    s - s.floor()
                                };
                                let pad_l = if dx.abs() < 0.5 { 1.0 / sf } else { 0.0 };
                                let pad_r = if dx.abs() < 0.5 && frac(ox + w + half) < 0.3 { 1.0 / sf } else { 0.0 };
                                let pad_t = if dy.abs() < 0.5 && frac(oy - half) > 0.85 { 1.0 / sf } else { 0.0 };
                                let pad_b = if dy.abs() < 0.5 && frac(oy + h + half) > 0.85 { 1.0 / sf } else { 0.0 };
                                let (l, r) = (dev(ox + dx - half - pad_l), dev(ox + dx + w + half + pad_r));
                                let (t, b) = (dev(oy + dy - half - pad_t), dev(oy + dy + h + half + pad_b));
                                let quad_bounds = Bounds::new(point(px(l), px(t)), size(px(r - l), px(b - t)));
                                window.paint_quad(quad(quad_bounds, radius + stroke_half + px(pad_l.max(pad_r).max(pad_t).max(pad_b)), color, Edges::default(), transparent_black(), BorderStyle::default()));
                            },
                        )
                        .absolute()
                        .size_full(),
                    );
                }
            }

            // 1b. Transformed shadows (outerShdw with sx, sy, kx, ky, algn)
            if let Some(s) = auto.shadow.filter(|s| auto.fill != Fill::None && s.has_transform()) {
                let stroke_half_emu = auto.stroke.as_ref().map(|st| st.width / 2).unwrap_or(0);
                let (w_emu, h_emu) = (frame.width as f64, frame.height as f64);
                let s_emu = stroke_half_emu as f64;
                let anchor = shadow_anchor(s.align, w_emu, h_emu, s_emu);
                let outlines = shadow_outlines(&auto.geometry, frame, s_emu);
                let blur = s.blur as f32 * scale;
                let color = hsla(s.color);
                container = container.child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            let sf = window.scale_factor();
                            let exact = exact_origin(bounds.origin, at, sf);
                            render_transformed_shadow(
                                window,
                                &outlines,
                                anchor,
                                &s,
                                blur,
                                color,
                                exact,
                                scale,
                            );
                        },
                    )
                    .absolute()
                    .size_full(),
                );
            }

            // 2. Shape body fill
            let round_rect_exact = matches!(auto.geometry, Geometry::RoundRect { .. })
                && matches!(auto.fill, Fill::Solid(_) | Fill::None)
                && !auto.stroke.as_ref().is_some_and(|s| s.dashed);

            let mut body = div().size_full().rounded(radius);
            body = match &auto.fill {
                _ if round_rect_exact => {
                    let fill_color = match &auto.fill {
                        Fill::Solid(color) => hsla(*color),
                        _ => transparent_black(),
                    };
                    let stroke_info = if auto.inner_shadow.is_none() {
                        auto.stroke.as_ref().map(|s| (stroke_px(s, scale), hsla(s.color)))
                    } else {
                        None
                    };
                    body.child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                let sf = window.scale_factor();
                                let dev = |v: f32| (v * sf).round() / sf;
                                let exact = exact_origin(bounds.origin, at, sf);
                                let (ox, oy) = (f32::from(exact.x), f32::from(exact.y));
                                let (l, r) = (dev(ox - half), dev(ox + w + half));
                                let (t, b) = (dev(oy - half), dev(oy + h + half));
                                let quad_bounds = Bounds::new(point(px(l), px(t)), size(px(r - l), px(b - t)));
                                let (border_widths, border_color) = match stroke_info {
                                    Some((spx, sclr)) => (Edges::all(spx), sclr),
                                    None => (Edges::default(), transparent_black()),
                                };
                                window.paint_quad(quad(quad_bounds, radius + stroke_half, fill_color, border_widths, border_color, BorderStyle::default()));
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                }
                Fill::Solid(color) if auto.geometry == Geometry::Rect => {
                    let color = hsla(*color);
                    body.child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                let sf = window.scale_factor();
                                let dev = |v: f32| (v * sf).round() / sf;
                                let exact = exact_origin(bounds.origin, at, sf);
                                let (ox, oy) = (f32::from(exact.x), f32::from(exact.y));
                                let (l, r) = (dev(ox), dev(ox + w));
                                let (t, b) = (dev(oy), dev(oy + h));
                                window.paint_quad(fill(Bounds::new(point(px(l), px(t)), size(px(r - l), px(b - t))), color));
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                }
                Fill::TileImage { media, tx, ty, sx, sy, align, .. } => body.child(tile_fill_element(
                    *media,
                    *tx,
                    *ty,
                    *sx,
                    *sy,
                    *align,
                    (frame.width, frame.height),
                    scale,
                    res,
                )),
                _ => with_fill(body, &auto.fill, res),
            };
            if matches!(auto.fill, Fill::Image(_) | Fill::TileImage { .. }) {
                body = body.overflow_hidden();
            }

            // 3. Blurred shadow or glow
            let mut shadows = Vec::new();
            if let Some(s) = auto.shadow.filter(|s| auto.fill != Fill::None && !s.has_transform()) {
                let (dx, dy, blur, color) = shadow_parts(s, scale);
                if blur >= 0.5 {
                    // GPUI's blur radius is the Gaussian's standard deviation;
                    // PowerPoint's shadow fades over about its blur radius
                    // either side of the edge, a deviation of a third of it
                    // (measured on its PDF: an 8 pt blur fades from 10 % to
                    // 90 % over 7 pt). PEFF 23 → 46.
                    shadows.push(BoxShadow { color, offset: point(px(dx), px(dy)), blur_radius: px(blur / 3.0), spread_radius: stroke_half, inset: false });
                }
            }
            // The glow apart, on a box grown by its spread with its corners
            // rounded as much more: PowerPoint's glow follows the outline
            // out, round at the corners (PEFF-00079), where a spread box
            // shadow keeps the shape's corners.
            let glow = auto.glow.map(|(color, glow_radius)| {
                let r = glow_radius as f32 * scale;
                let (spread, blur) = glow_spread_blur(r);
                div()
                    .absolute()
                    .left(px(-spread))
                    .top(px(-spread))
                    .w(px(w + 2.0 * spread))
                    .h(px(h + 2.0 * spread))
                    .rounded(radius + px(spread))
                    .shadow(vec![BoxShadow { color: hsla(color), offset: point(px(0.), px(0.)), blur_radius: px(blur), spread_radius: px(0.), inset: false }])
            });
            if !shadows.is_empty() {
                body = body.shadow(shadows);
            }
            if let Some(glow) = glow {
                container = container.child(glow);
            }
            // A reflection mirrored down under the shape (`sy` −100 %, put
            // under it, fading downwards): the shape's fill, outline
            // included in its size, fading from `stA` at `stPos` to `endA`
            // at `endPos` of its height, `dist` below the outline
            // (PEFF-00084・00085). Its blur is not drawn.
            if let (Some(r), Fill::Solid(fill_color)) = (auto.reflection, &auto.fill)
                && r.sy == -100_000
                && r.fade_direction == 5_400_000
                && (r.distance == 0 || r.direction == 5_400_000)
                && matches!(r.align, ibis_doc_model::TileAlign::Bottom | ibis_doc_model::TileAlign::BottomLeft)
            {
                let alpha = |a: i32| {
                    let mut c = hsla(*fill_color);
                    c.a *= (a as f32 / 100_000.0).clamp(0.0, 1.0);
                    c
                };
                let gap = r.distance as f32 * scale;
                container = container.child(
                    div()
                        .absolute()
                        .left(px(-half))
                        .top(px(h + half + gap))
                        .w(px(w + 2.0 * half))
                        .h(px(h + 2.0 * half))
                        .rounded(radius + stroke_half)
                        .bg(linear_gradient(
                            180.0,
                            linear_color_stop(alpha(r.start_alpha), r.start_pos as f32 / 100_000.0),
                            linear_color_stop(alpha(r.end_alpha), r.end_pos as f32 / 100_000.0),
                        )),
                );
            }

            // 4. Add body and outline
            container = container.child(body);
            // A shadow inside (`a:innerShdw`), over the fill (which the body
            // may paint in a child), blurred as the outer one.
            if let Some(s) = auto.inner_shadow.filter(|_| auto.fill != Fill::None) {
                let (dx, dy, blur, color) = shadow_parts(s, scale);
                // GPUI moves the hole by the offset, so the shadow falls on the
                // other side: PowerPoint's falls towards its direction.
                let inner = BoxShadow { color, offset: point(px(-dx), px(-dy)), blur_radius: px(blur / 2.0), spread_radius: px(0.), inset: true };

                container = container.child(div().absolute().top_0().left_0().size_full().rounded(radius).shadow(vec![inner]));
            }

            // 3D bevel effect (面取りの陰影): top/left highlight and bottom/right shadow inside the shape.
            if let Some(bevel) = auto.bevel.filter(|_| auto.fill != Fill::None) {
                let bw = (bevel.width as f32 * scale).max(1.0).min(w / 4.0).min(h / 4.0);
                let light = BoxShadow {
                    color: hsla(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0xffffff), alpha: 120 }),
                    offset: point(px(bw * 0.5), px(bw * 0.5)),
                    blur_radius: px(bw * 0.4),
                    spread_radius: px(0.),
                    inset: true,
                };
                let dark = BoxShadow {
                    color: hsla(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0x000000), alpha: 90 }),
                    offset: point(px(-bw * 0.5), px(-bw * 0.5)),
                    blur_radius: px(bw * 0.4),
                    spread_radius: px(0.),
                    inset: true,
                };
                container = container.child(div().absolute().top_0().left_0().size_full().rounded(radius).shadow(vec![light, dark]));
            }


            if let Some(stroke) = &auto.stroke {
                if round_rect_exact && auto.inner_shadow.is_none() {
                    // Handled inside body canvas above
                } else if matches!(auto.fill, Fill::Image(_) | Fill::TileImage { .. }) {
                    container = container.child(
                        div()
                            .size_full()
                            .rounded(radius)
                            .border(stroke_px(stroke, scale))
                            .border_color(hsla(stroke.color))
                            .when(stroke.dashed, |d| d.border_dashed()),
                    );
                } else {
                    container = container.child(outline_element(stroke, w, h, radius, scale, auto.geometry == Geometry::Rect, at));
                }
            }
            container

        };
        return match auto.tilt {
            // The tilted body is drawn apart from the frame, which keeps the text.
            Some(tilt) => {
                let (x, y, tw, th) = tilted_frame(tilt, w, h, scale);
                container.child(paint(div().absolute().left(px(x)).top(px(y)).w(px(tw)).h(px(th))))
            }
            None => paint(container),
        };
    }

    // Everything else is drawn as paths, which can also be rotated and flipped.
    // A picture fill is cut to the outline (photos in rounded frames); when
    // it cannot be, or the shape is rotated, it covers the frame.
    let outlines = geometry_outlines(&auto.geometry, frame);
    let container = match &auto.fill {
        Fill::Image(media) => match (!rotated).then(|| clipped_fill(*media, shape, &outlines, frame, res)).flatten() {
            Some(image) => container.child(img(res.svg_picture(image)).absolute().size_full().object_fit(ObjectFit::Fill)),
            None => with_fill(container.overflow_hidden(), &auto.fill, res),
        },
        Fill::TileImage { media, tx, ty, sx, sy, align, .. } => container.child(tile_fill_element(
            *media,
            *tx,
            *ty,
            *sx,
            *sy,
            *align,
            (frame.width, frame.height),
            scale,
            res,
        )),
        _ => container,
    };
    let fill = turned_fill(&auto.fill, shape);
    // A gradient is painted in the shape's own coordinates, so it turns
    // and flips with the shape (`rotWithShape`).
    let own_fill = auto.fill.clone();
    let box_size = (frame.width as f32, frame.height as f32);
    let stroke = auto.stroke.clone();
    let place = Placement::new(shape, frame);
    // The slide's corner, from the shape's place on it (patterns start there).
    let at = (shape.frame.x as f32 * scale, shape.frame.y as f32 * scale);
    // (Shapes are laid out on whole pixels; the slide's corner is on one too.)
    let slide_origin = move |o: Point<Pixels>| point(px((f32::from(o.x) - at.0).round()), px((f32::from(o.y) - at.1).round()));
    let stroke_half_emu = auto.stroke.as_ref().map(|st| st.width / 2).unwrap_or(0);
    let shadow_outlines = shadow_outlines(&auto.geometry, frame, stroke_half_emu as f64);
    let shadow_spec = auto.shadow.filter(|_| auto.fill != Fill::None);
    let at_px = (frame.x as f32 * scale, frame.y as f32 * scale);
    // Level and upright edges on whole pixels, as the screen has them; a
    // closed outline's of two pixels or more even when drawn exactly, as
    // poppler adjusts them (at 2 px a point: a rectangle's 1.5 pt outline
    // on whole pixels, its 0.75 pt one and open lines smoothed across them).
    let on_pixels = !res.exact;
    container.child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                // Where the frame is meant to be (GPUI put the element on
                // whole pixels): the outline is drawn there, as PowerPoint
                // draws it.
                let exact = exact_origin(bounds.origin, at_px, window.scale_factor());
                // The shadow: the shape's outline moved by the offset, painted
                // a few times around it, faintly, in place of a blur.
                if let Some(s) = shadow_spec {
                    if s.has_transform() {
                        let (w_emu, h_emu) = (frame.width as f64, frame.height as f64);
                        let anchor = shadow_anchor(s.align, w_emu, h_emu, stroke_half_emu as f64);
                        let blur = s.blur as f32 * scale;
                        let color = hsla(s.color);
                        render_transformed_shadow(
                            window,
                            &shadow_outlines,
                            anchor,
                            &s,
                            blur,
                            color,
                            exact,
                            scale,
                        );
                    } else {
                        let (dx, dy, blur, color) = shadow_parts(s, scale);
                        let spots: Vec<(f32, f32)> = if blur < 0.5 {
                            vec![(0.0, 0.0)]
                        } else {
                            (0..8).map(|i| (i as f32 * std::f32::consts::FRAC_PI_4).sin_cos()).map(|(s, c)| (c * blur / 2.0, s * blur / 2.0)).chain([(0.0, 0.0)]).collect()
                        };
                        let faint = Hsla { a: color.a / spots.len() as f32 * 1.6, ..color };
                        for (sx, sy) in &spots {
                            let origin = point(exact.x + px(dx + sx), exact.y + px(dy + sy));
                            for outline in outlines.iter().filter(|o| o.fill != ibis_doc_model::PathFill::None) {
                                let subpaths: Vec<(Vec<Point<Pixels>>, bool)> = outline
                                    .subpaths
                                    .iter()
                                    .map(|s| {
                                        let points: Vec<(f32, f32)> = s.points.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
                                        (place.map(&points, origin, scale), s.closed)
                                    })
                                    .collect();
                                if let Some(p) = compound_path(&subpaths, None) {
                                    window.paint_path(p, faint);
                                }
                            }
                        }
                    }
                }
                for outline in &outlines {
                    let subpaths: Vec<(Vec<Point<Pixels>>, bool)> = outline
                        .subpaths
                        .iter()
                        .map(|s| {
                            let points: Vec<(f32, f32)> = s.points.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
                            (place.map(&points, exact, scale), s.closed)
                        })
                        .collect();
                    if outline.fill == ibis_doc_model::PathFill::Normal && crate::render::gradient::is_gradient(&own_fill) {
                        let own: Vec<Vec<(f32, f32)>> = outline.subpaths.iter().map(|s| s.points.iter().map(|&(x, y)| (x as f32, y as f32)).collect()).collect();
                        let origin = exact;
                        crate::render::gradient::paint_gradient(window, &own_fill, &own, box_size, scale, &|pts| place.map(pts, origin, scale));
                    } else if let Some(bg) = shaded_background(&fill, outline.fill)
                        && let Some(p) = compound_path(&subpaths, None)
                    {
                        window.paint_path(p, bg);
                        if matches!(fill, Fill::Pattern { .. }) {
                            let closed: Vec<Vec<Point<Pixels>>> = subpaths.iter().filter(|(_, c)| *c).map(|(p, _)| p.clone()).collect();
                            paint_pattern(window, &fill, &closed, slide_origin(bounds.origin), scale);
                        }
                    }
                    if outline.stroke
                        && let Some(stroke) = &stroke
                    {
                        let width = painted_stroke_px(stroke, scale, window);
                        let inset_offset = if stroke.alignment == ibis_doc_model::LineAlignment::Inset {
                            f32::from(width) / 2.0
                        } else {
                            0.0
                        };
                        let subpaths: Vec<(Vec<Point<Pixels>>, bool)> = subpaths
                            .iter()
                            .map(|(p, c)| {
                                let pts = if *c && inset_offset > 0.0 {
                                    offset_polyline(p, true, inset_offset)
                                } else {
                                    p.clone()
                                };
                                (if on_pixels || *c { stroke_adjusted(&pts, *c, f32::from(width), window.scale_factor()) } else { pts }, *c)
                            })
                            .collect();
                        // The line stops inside a filled head, so its end
                        // does not poke out past the head's point (as a
                        // straight line's, `line_element`).
                        let tips = subpaths.clone();
                        let mut subpaths = subpaths;
                        if stroke.head != LineEnd::None || stroke.tail != LineEnd::None {
                            let head_base = END_BASE_POINTS * EMU_PER_POINT as f32 * scale;
                            if let Some((points, _)) = subpaths.iter_mut().find(|(p, closed)| !*closed && p.len() >= 2) {
                                let n = points.len();
                                for (i, j, end) in [(0, 1, stroke.head), (n - 1, n - 2, stroke.tail)] {
                                    let back = end_setback(end, f32::from(width), head_base);
                                    let (dx, dy) = (f32::from(points[j].x - points[i].x), f32::from(points[j].y - points[i].y));
                                    let len = (dx * dx + dy * dy).sqrt();
                                    if back > 0.0 && len > back {
                                        points[i] = point(points[i].x + px(dx / len * back), points[i].y + px(dy / len * back));
                                    }
                                }
                            }
                        }
                        if stroke.compound != ibis_doc_model::LineCompound::Single && !stroke.dashed {
                            for (points, closed) in &subpaths {
                                for (line, w) in compound_lines(points, *closed, f32::from(width), stroke.compound) {
                                    if let Some(p) = polygon_styled(&line, Some(px(w)), *closed, stroke.cap, stroke.join) {
                                        window.paint_path(p, hsla(stroke.color));
                                    }
                                }
                                if !*closed && stroke.cap == ibis_doc_model::LineCap::Round && f32::from(width) >= 1.5 {
                                    paint_compound_round_caps(window, &[points.clone()], f32::from(width), stroke.compound, hsla(stroke.color), true, true);
                                }
                            }
                        } else if stroke.dashed {
                            for (points, closed) in &subpaths {
                                let pieces = dash_pieces(points, *closed, stroke, f32::from(width));
                                for piece in &pieces {
                                    for (line, w) in compound_lines(piece, false, f32::from(width), stroke.compound) {
                                        if let Some(p) = polygon_styled(&line, Some(px(w)), false, stroke.cap, stroke.join) {
                                            window.paint_path(p, hsla(stroke.color));
                                        }
                                    }
                                }
                                if stroke.cap == ibis_doc_model::LineCap::Round {
                                    let w_px = f32::from(width);
                                    if stroke.compound != ibis_doc_model::LineCompound::Single && w_px >= 1.5 {
                                        let is_dot = |p: &[Point<Pixels>]| match (p.first(), p.last()) {
                                            (Some(f), Some(l)) => (f32::from(l.x - f.x)).hypot(f32::from(l.y - f.y)) <= 1e-4,
                                            _ => false,
                                        };
                                        let dot_pieces: Vec<Vec<Point<Pixels>>> = pieces.iter().filter(|p| is_dot(p)).cloned().collect();
                                        if !dot_pieces.is_empty() {
                                            paint_round_caps(window, &dot_pieces, w_px, hsla(stroke.color));
                                        }
                                        paint_compound_round_caps(window, &pieces, w_px, stroke.compound, hsla(stroke.color), true, true);
                                    } else {
                                        paint_round_caps(window, &pieces, w_px, hsla(stroke.color));
                                    }
                                }
                            }
                        } else if let Some(p) = compound_path_styled(&subpaths, Some(width), stroke.cap, stroke.join) {
                            window.paint_path(p, hsla(stroke.color));
                        }
                        // Arrowheads at the ends of an open outline (bent
                        // and curved connectors, arcs).
                        if stroke.head != LineEnd::None || stroke.tail != LineEnd::None {
                            let head_base = END_BASE_POINTS * EMU_PER_POINT as f32 * scale;
                            if let Some((points, _)) = tips.iter().find(|(p, closed)| !*closed && p.len() >= 2) {
                                // The direction from a point a little way along, for a curve.
                                let toward = |tip: Point<Pixels>, rest: &mut dyn Iterator<Item = &Point<Pixels>>| {
                                    let mut last = tip;
                                    for &p in rest {
                                        last = p;
                                        if (f32::from(p.x - tip.x)).hypot(f32::from(p.y - tip.y)) >= f32::from(width) * 2.0 {
                                            break;
                                        }
                                    }
                                    last
                                };
                                let (first, last) = (points[0], points[points.len() - 1]);
                                let from_first = toward(first, &mut points.iter().skip(1));
                                let from_last = toward(last, &mut points.iter().rev().skip(1));
                                let color = hsla(stroke.color);
                                paint_line_end(stroke.head, first, from_first, f32::from(width), head_base, color, window);
                                paint_line_end(stroke.tail, last, from_last, f32::from(width), head_base, color, window);
                            }
                        }
                    }
                }
            },
        )
        .absolute()
        .size_full(),
    )
}

/// `points` (a polyline stroked `width` wide) with its level and upright
/// segments moved so that the stroke's edges fall on device pixels, as
/// PowerPoint's outlines are drawn (crisp, a pixel off where they are
/// meant to be at most); slanting ones are left where they are.
fn stroke_adjusted(points: &[Point<Pixels>], closed: bool, width: f32, scale_factor: f32) -> Vec<Point<Pixels>> {
    let mut out = points.to_vec();
    let n = points.len();
    if n < 2 {
        return out;
    }
    let half = width / 2.0;
    let snap = |v: f32| (((v - half) * scale_factor).round() / scale_factor + ((v + half) * scale_factor).round() / scale_factor) / 2.0;
    let segments = if closed { n } else { n - 1 };
    let level = |i: usize| {
        let (a, b) = (points[i % n], points[(i + 1) % n]);
        (f32::from(a.y - b.y)).abs() < 0.01 || (f32::from(a.x - b.x)).abs() < 0.01
    };
    for i in 0..segments {
        let j = (i + 1) % n;
        let (a, b) = (points[i], points[j]);
        // Only between level or upright segments (or the line's ends): a
        // slanting one would move with the corner.
        let before = if i == 0 && !closed { true } else { level((i + segments - 1) % segments) };
        let after = if j == n - 1 && !closed { true } else { level((i + 1) % segments) };
        if !(before && after) {
            continue;
        }
        if (f32::from(a.y - b.y)).abs() < 0.01 && (f32::from(a.x - b.x)).abs() >= 0.01 {
            let y = px(snap(f32::from(a.y)));
            out[i].y = y;
            out[j].y = y;
        } else if (f32::from(a.x - b.x)).abs() < 0.01 && (f32::from(a.y - b.y)).abs() >= 0.01 {
            let x = px(snap(f32::from(a.x)));
            out[i].x = x;
            out[j].x = x;
        }
    }
    out
}

/// Where an element meant to be at `at` (pixels) is drawn exactly, from
/// its `origin` as GPUI placed it: GPUI snaps authored positions to device
/// pixels (halves toward zero), which moves it by up to half a pixel.
pub(crate) fn exact_origin(origin: Point<Pixels>, at: (f32, f32), scale_factor: f32) -> Point<Pixels> {
    let snap = |v: f32| {
        let d = v * scale_factor;
        (if (d - d.trunc()).abs() == 0.5 { d.trunc() } else { d.round() }) / scale_factor
    };
    point(origin.x + px(at.0 - snap(at.0)), origin.y + px(at.1 - snap(at.1)))
}

/// The fill of a path with its `fill` mode: none, as is, or lightened or
/// darkened (the shading of 3-D looking presets such as `can` and `cube`).
fn shaded_background(fill: &Fill, mode: ibis_doc_model::PathFill) -> Option<Background> {
    use ibis_doc_model::PathFill;
    let factor = match mode {
        PathFill::None => return None,
        PathFill::Normal => return background(fill),
        PathFill::Lighten => 1.4,
        PathFill::LightenLess => 1.2,
        PathFill::Darken => 0.6,
        PathFill::DarkenLess => 0.8,
    };
    match fill {
        Fill::Solid(c) => {
            let mut color = hsla(*c);
            color.l = (color.l * factor).clamp(0.0, 1.0);
            Some(color.into())
        }
        other => background(other),
    }
}

/// A path of several subpaths, filled together (so inner subpaths make
/// holes) or stroked with `stroke` width.
pub(crate) fn compound_path_styled(
    subpaths: &[(Vec<Point<Pixels>>, bool)],
    stroke: Option<Pixels>,
    cap: LineCap,
    join: LineJoin,
) -> Option<Path<Pixels>> {
    let mut builder = match stroke {
        Some(width) => {
            let lyon_cap = match cap {
                LineCap::Round => lyon_tessellation::LineCap::Round,
                LineCap::Square => lyon_tessellation::LineCap::Square,
                LineCap::Flat => lyon_tessellation::LineCap::Butt,
            };
            let lyon_join = match join {
                LineJoin::Round => lyon_tessellation::LineJoin::Round,
                LineJoin::Bevel => lyon_tessellation::LineJoin::Bevel,
                LineJoin::Miter => lyon_tessellation::LineJoin::Miter,
            };
            let opts = lyon_tessellation::StrokeOptions::default()
                .with_line_width(f32::from(width))
                .with_line_cap(lyon_cap)
                .with_line_join(lyon_join);
            PathBuilder::default().with_style(gpui_kit::PathStyle::Stroke(opts))
        }
        None => PathBuilder::fill(),
    };
    let mut any = false;
    for (points, closed) in subpaths {
        let Some((first, rest)) = points.split_first() else { continue };
        builder.move_to(*first);
        for p in rest {
            builder.line_to(*p);
        }
        // Fills close every subpath; strokes only the closed ones.
        if *closed || stroke.is_none() {
            builder.close();
        }
        any = true;
    }
    if any { builder.build().ok() } else { None }
}

pub(crate) fn compound_path(subpaths: &[(Vec<Point<Pixels>>, bool)], stroke: Option<Pixels>) -> Option<Path<Pixels>> {
    compound_path_styled(subpaths, stroke, LineCap::Flat, LineJoin::Miter)
}

/// Rotation and flips of a shape, applied to points in its frame.
#[derive(Clone, Copy)]
struct Placement {
    width: f32,
    height: f32,
    flip_h: bool,
    flip_v: bool,
    sin: f32,
    cos: f32,
}

impl Placement {
    fn new(shape: &Shape, frame: EmuRect) -> Self {
        let angle = shape.rotation as f32 / 60_000.0 * PI / 180.0;
        Self {
            width: frame.width as f32,
            height: frame.height as f32,
            flip_h: shape.flip_h,
            flip_v: shape.flip_v,
            sin: angle.sin(),
            cos: angle.cos(),
        }
    }

    /// Frame-relative EMU points to window pixels, the frame at `origin`.
    fn map(&self, points: &[(f32, f32)], origin: Point<Pixels>, scale: f32) -> Vec<Point<Pixels>> {
        let (cx, cy) = (self.width / 2.0, self.height / 2.0);
        points
            .iter()
            .map(|&(x, y)| {
                let x = if self.flip_h { self.width - x } else { x } - cx;
                let y = if self.flip_v { self.height - y } else { y } - cy;
                let (x, y) = (x * self.cos - y * self.sin + cx, x * self.sin + y * self.cos + cy);
                point(origin.x + px(x * scale), origin.y + px(y * scale))
            })
            .collect()
    }
}

/// `points` moved `d` pixels to the right of their direction (a negative
/// `d` to the left), each corner on the meeting of its two sides' moved
/// lines (not further than four times `d` from it).
fn offset_polyline(points: &[Point<Pixels>], closed: bool, d: f32) -> Vec<Point<Pixels>> {
    let n = points.len();
    if n < 2 || d == 0.0 {
        return points.to_vec();
    }
    let normal = |i: usize, j: usize| {
        let (a, b) = (points[i], points[j]);
        let (dx, dy) = (f32::from(b.x - a.x), f32::from(b.y - a.y));
        let len = dx.hypot(dy);
        if len == 0.0 { None } else { Some((-dy / len, dx / len)) }
    };
    (0..n)
        .map(|i| {
            let before = if i > 0 { normal(i - 1, i) } else if closed { normal(n - 1, 0) } else { None };
            let after = if i + 1 < n { normal(i, i + 1) } else if closed { normal(n - 1, 0) } else { None };
            let (nx, ny) = match (before, after) {
                (Some(a), Some(b)) => {
                    let (sx, sy) = (a.0 + b.0, a.1 + b.1);
                    let len = sx.hypot(sy);
                    if len < 1e-3 {
                        a
                    } else {
                        let (mx, my) = (sx / len, sy / len);
                        let k = (1.0 / (mx * a.0 + my * a.1)).min(4.0);
                        (mx * k, my * k)
                    }
                }
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => (0.0, 0.0),
            };
            point(points[i].x + px(nx * d), points[i].y + px(ny * d))
        })
        .collect()
}

/// The lines a compound stroke `width` wide is drawn as, along `points`:
/// each moved across and its width.
fn compound_lines(points: &[Point<Pixels>], closed: bool, width: f32, compound: ibis_doc_model::LineCompound) -> Vec<(Vec<Point<Pixels>>, f32)> {
    if compound == ibis_doc_model::LineCompound::Single || width < 1.5 {
        return vec![(points.to_vec(), width)];
    }
    compound
        .bands()
        .iter()
        .map(|&(a, b)| (offset_polyline(points, closed, ((a + b) / 2.0 - 0.5) * width), ((b - a) * width).max(0.5)))
        .collect()
}

pub(crate) fn polygon_styled(
    points: &[Point<Pixels>],
    stroke: Option<Pixels>,
    closed: bool,
    cap: LineCap,
    join: LineJoin,
) -> Option<Path<Pixels>> {
    let (first, rest) = points.split_first()?;
    let mut builder = match stroke {
        Some(width) => {
            let lyon_cap = match cap {
                LineCap::Round => lyon_tessellation::LineCap::Round,
                LineCap::Square => lyon_tessellation::LineCap::Square,
                LineCap::Flat => lyon_tessellation::LineCap::Butt,
            };
            let lyon_join = match join {
                LineJoin::Round => lyon_tessellation::LineJoin::Round,
                LineJoin::Bevel => lyon_tessellation::LineJoin::Bevel,
                LineJoin::Miter => lyon_tessellation::LineJoin::Miter,
            };
            let opts = lyon_tessellation::StrokeOptions::default()
                .with_line_width(f32::from(width))
                .with_line_cap(lyon_cap)
                .with_line_join(lyon_join);
            PathBuilder::default().with_style(gpui_kit::PathStyle::Stroke(opts))
        }
        None => PathBuilder::fill(),
    };
    builder.move_to(*first);
    for p in rest {
        builder.line_to(*p);
    }
    if closed {
        builder.close();
    }
    builder.build().ok()
}

pub(crate) fn polygon(points: &[Point<Pixels>], stroke: Option<Pixels>, closed: bool) -> Option<Path<Pixels>> {
    polygon_styled(points, stroke, closed, LineCap::Flat, LineJoin::Miter)
}

/// Outlines of a geometry for `frame`, in frame-relative EMU.
fn geometry_outlines(geometry: &Geometry, frame: EmuRect) -> Vec<ibis_doc_model::Outline> {
    use ibis_doc_model::{Outline, PathFill, Subpath};
    let (w, h) = (frame.width as f64, frame.height as f64);
    let shape = |points: Vec<(f64, f64)>| vec![Outline { subpaths: vec![Subpath { points, closed: true }], fill: PathFill::Normal, stroke: true }];
    let ellipse = |cx: f64, cy: f64, rx: f64, ry: f64, start: f64, steps: usize| {
        (0..=steps).map(move |i| {
            let t = start + std::f64::consts::FRAC_PI_2 * i as f64 / steps as f64;
            (cx + rx * t.cos(), cy + ry * t.sin())
        })
    };
    match geometry {
        Geometry::Rect => shape(vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]),
        Geometry::Ellipse => shape((0..64).map(|i| {
            let t = std::f64::consts::TAU * i as f64 / 64.0;
            (w / 2.0 + w / 2.0 * t.cos(), h / 2.0 + h / 2.0 * t.sin())
        }).collect()),
        Geometry::RoundRect { radius } => {
            let r = w.min(h) * *radius as f64 / 100_000.0;
            let mut points = Vec::new();
            for (cx, cy, start) in [(w - r, r, -std::f64::consts::FRAC_PI_2), (w - r, h - r, 0.0), (r, h - r, std::f64::consts::FRAC_PI_2), (r, r, std::f64::consts::PI)] {
                // 8 steps a corner, as before cce4268e: with 32, Word's
                // rounded rectangles' straight edges came out on whole
                // pixels, where Word's PDF has them between (DSHP-00076).
                points.extend(ellipse(cx, cy, r, r, start, 8));
            }
            shape(points)
        }
        Geometry::Formula(g) => g.outline(w, h),
    }
}

/// Outlines of a geometry expanded by `stroke_half` for shadow rendering, in frame-relative EMU.
fn shadow_outlines(geometry: &Geometry, frame: EmuRect, stroke_half: f64) -> Vec<ibis_doc_model::Outline> {
    use ibis_doc_model::{Outline, PathFill, Subpath};
    let s = stroke_half;
    let (w, h) = (frame.width as f64, frame.height as f64);
    let shape = |points: Vec<(f64, f64)>| vec![Outline { subpaths: vec![Subpath { points, closed: true }], fill: PathFill::Normal, stroke: true }];
    let ellipse = |cx: f64, cy: f64, rx: f64, ry: f64, start: f64, steps: usize| {
        (0..=steps).map(move |i| {
            let t = start + std::f64::consts::FRAC_PI_2 * i as f64 / steps as f64;
            (cx + rx * t.cos(), cy + ry * t.sin())
        })
    };
    match geometry {
        Geometry::Rect => shape(vec![(-s, -s), (w + s, -s), (w + s, h + s), (-s, h + s)]),
        Geometry::Ellipse => shape((0..64).map(|i| {
            let t = std::f64::consts::TAU * i as f64 / 64.0;
            let rx = w / 2.0 + s;
            let ry = h / 2.0 + s;
            (w / 2.0 + rx * t.cos(), h / 2.0 + ry * t.sin())
        }).collect()),
        Geometry::RoundRect { radius } => {
            let r = w.min(h) * *radius as f64 / 100_000.0;
            let r_ext = r + s;
            let mut points = Vec::new();
            for (cx, cy, start) in [(w - r, r, -std::f64::consts::FRAC_PI_2), (w - r, h - r, 0.0), (r, h - r, std::f64::consts::FRAC_PI_2), (r, r, std::f64::consts::PI)] {
                points.extend(ellipse(cx, cy, r_ext, r_ext, start, 32));
            }
            shape(points)
        }
        Geometry::Formula(g) => g.outline(w, h),
    }
}

fn line_element(shape: &Shape, stroke: Stroke, frame: EmuRect, scale: f32) -> Div {
    let place = Placement::new(shape, frame);
    let (w, h) = (frame.width as f32, frame.height as f32);

    // Horizontal and vertical lines have an empty frame, and GPUI skips
    // painting empty bounds, so the element extends past the frame on all
    // sides (by enough for the arrowheads).
    let head_base = END_BASE_POINTS * EMU_PER_POINT as f32 * scale;
    let margin = f32::from(stroke_px(&stroke, scale)).max(head_base) * 6.0 + 1.0;
    div()
        .absolute()
        .left(px(frame.x as f32 * scale - margin))
        .top(px(frame.y as f32 * scale - margin))
        .w(px(w * scale + margin * 2.0))
        .h(px(h * scale + margin * 2.0))
        .child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                // Where the frame is meant to be (not where GPUI snapped
                // the element to): the line is drawn there, as PowerPoint
                // draws it, smoothed across the pixels it crosses.
                let corner = exact_origin(bounds.origin, (frame.x as f32 * scale - margin, frame.y as f32 * scale - margin), window.scale_factor());
                let origin = point(corner.x + px(margin), corner.y + px(margin));
                let mut ends = place.map(&[(0.0, 0.0), (w, h)], origin, scale);
                let width = painted_stroke_px(&stroke, scale, window);
                // The line stops inside a filled head, so its square end
                // does not poke out past the head's point.
                let (tip0, tip1) = (ends[0], ends[1]);
                for (i, end, toward) in [(0, stroke.head, tip1), (1, stroke.tail, tip0)] {
                    let back = end_setback(end, f32::from(width), head_base);
                    let (dx, dy) = (f32::from(toward.x - ends[i].x), f32::from(toward.y - ends[i].y));
                    let len = (dx * dx + dy * dy).sqrt();
                    if back > 0.0 && len > back * 2.0 {
                        ends[i] = point(ends[i].x + px(dx / len * back), ends[i].y + px(dy / len * back));
                    }
                }
                let color = hsla(stroke.color);
                let head_plain = stroke.head == LineEnd::None;
                let tail_plain = stroke.tail == LineEnd::None;
                // Dashes in the line's pattern; the ends in its caps.
                let pieces: Vec<Vec<Point<Pixels>>> = if stroke.dashed {
                    let mut pieces = dash_pieces(&[ends[0], ends[1]], false, &stroke, f32::from(width));
                    // A head that the line is drawn short of (`end_setback`:
                    // triangle, stealth and arrow, 8cc4cb7e) starts the
                    // dash pattern at the shortened line's start, so the
                    // round-capped dashes do not reach half a width back
                    // past it as they do on a plain start: PowerPoint's are
                    // there (PARW-00221, stealth: 2 px later). Diamond and
                    // oval heads do not shorten the line, and their dashes
                    // stay where they were (PARW-00242, 00260 measured).
                    if stroke.cap == ibis_doc_model::LineCap::Round && end_setback(stroke.head, f32::from(width), head_base) > 0.0 {
                        let (dx, dy) = (f32::from(ends[1].x - ends[0].x), f32::from(ends[1].y - ends[0].y));
                        let len = (dx * dx + dy * dy).sqrt();
                        if len > 0.0 {
                            let (ux, uy) = (dx / len * f32::from(width) / 2.0, dy / len * f32::from(width) / 2.0);
                            for piece in &mut pieces {
                                for p in piece.iter_mut() {
                                    *p = point(p.x + px(ux), p.y + px(uy));
                                }
                            }
                        }
                    }
                    if stroke.cap == ibis_doc_model::LineCap::Square && !pieces.is_empty() {
                        let (dx, dy) = (f32::from(tip1.x - tip0.x), f32::from(tip1.y - tip0.y));
                        let len = (dx * dx + dy * dy).sqrt();
                        if len > 0.0 {
                            let (ux, uy) = (dx / len * f32::from(width) / 2.0, dy / len * f32::from(width) / 2.0);
                            if head_plain && let Some(first) = pieces.first_mut().and_then(|p| p.first_mut()) {
                                *first = point(first.x - px(ux), first.y - px(uy));
                            }
                            if tail_plain && let Some(last) = pieces.last_mut().and_then(|p| p.last_mut()) {
                                *last = point(last.x + px(ux), last.y + px(uy));
                            }
                        }
                    }
                    pieces
                } else if stroke.cap == ibis_doc_model::LineCap::Square && (head_plain || tail_plain) {
                    vec![square_capped(&[ends[0], ends[1]], f32::from(width), head_plain, tail_plain)]
                } else {
                    vec![vec![ends[0], ends[1]]]
                };
                if stroke.cap == ibis_doc_model::LineCap::Round {
                    let w_px = f32::from(width);
                    if stroke.dashed {
                        let is_dot = |p: &[Point<Pixels>]| match (p.first(), p.last()) {
                            (Some(f), Some(l)) => (f32::from(l.x - f.x)).hypot(f32::from(l.y - f.y)) <= 1e-4,
                            _ => false,
                        };
                        let dot_pieces: Vec<Vec<Point<Pixels>>> = pieces.iter().filter(|p| is_dot(p)).cloned().collect();
                        if stroke.compound == ibis_doc_model::LineCompound::Single || w_px < 1.5 {
                            paint_round_caps(window, &pieces, w_px, color);
                        } else if !dot_pieces.is_empty() {
                            paint_round_caps(window, &dot_pieces, w_px, color);
                        }
                    } else if stroke.compound == ibis_doc_model::LineCompound::Single || w_px < 1.5 {
                        let r = w_px / 2.0;
                        for (pt, plain) in [(ends[0], head_plain), (ends[1], tail_plain)] {
                            if plain {
                                let disc: Vec<Point<Pixels>> = (0..16)
                                    .map(|i| {
                                        let t = std::f32::consts::TAU * i as f32 / 16.0;
                                        point(pt.x + px(r * t.cos()), pt.y + px(r * t.sin()))
                                    })
                                    .collect();
                                if let Some(p) = polygon(&disc, None, true) {
                                    window.paint_path(p, color);
                                }
                            }
                        }
                    }
                }
                for piece in &pieces {
                    for (line, w) in compound_lines(piece, false, f32::from(width), stroke.compound) {
                        if let Some(p) = polygon(&line, Some(px(w)), false) {
                            window.paint_path(p, color);
                        }
                    }
                }
                if stroke.cap == ibis_doc_model::LineCap::Round
                    && stroke.compound != ibis_doc_model::LineCompound::Single
                    && f32::from(width) >= 1.5
                {
                    paint_compound_round_caps(window, &pieces, f32::from(width), stroke.compound, color, head_plain, tail_plain);
                }
                for (at, from, end) in [(tip1, tip0, stroke.tail), (tip0, tip1, stroke.head)] {
                    paint_line_end(end, at, from, f32::from(width), head_base, color, window);
                }
            },
        )
        .absolute()
        .size_full(),
    )
}

/// The narrowest line width, in points, that line ends are sized by: a
/// thinner line has the heads of a 2 pt one (PowerPoint's small head on a
/// 1 pt line is 4 pt wide and long, measured against its PDF).
const END_BASE_POINTS: f32 = 2.0;

/// The width and length, in pixels, of the decoration `end` of a line
/// `width` pixels wide: 2, 3 or 5 times the line's width (small, medium,
/// large), but at least those of a line `base` pixels wide.
fn line_end_size(end: LineEnd, width: f32, base: f32) -> Option<(f32, f32)> {
    let size = match end {
        LineEnd::None => return None,
        LineEnd::Arrow(s) => {
            let bw = width.max(base).max(1.0);
            let fw = [3.0, 3.0, 5.0][s.width.min(2) as usize] * bw;
            let fl = [2.5, 3.5, 5.0][s.length.min(2) as usize] * bw;
            return Some((fw, fl));
        }
        LineEnd::Triangle(s) | LineEnd::Stealth(s) | LineEnd::Diamond(s) | LineEnd::Oval(s) => s,
    };
    let factor = |v: u8| [2.0, 3.0, 5.0][v.min(2) as usize] * width.max(base).max(1.0);
    Some((factor(size.width), factor(size.length)))
}

/// How far a line stops short of its end so that it ends inside the end's
/// filled head (half the head's length); 0 for the other ends.
fn end_setback(end: LineEnd, width: f32, base: f32) -> f32 {
    match (end, line_end_size(end, width, base)) {
        (LineEnd::Triangle(_) | LineEnd::Stealth(_), Some((_, long))) => long / 2.0,
        (LineEnd::Arrow(_), _) => width,
        _ => 0.0,
    }
}

/// Paint the decoration `end` of a line `width` pixels wide that ends at
/// `at`, coming from `from`, sized as [`line_end_size`] says.
fn paint_line_end(end: LineEnd, at: Point<Pixels>, from: Point<Pixels>, width: f32, base: f32, color: Hsla, window: &mut Window) {
    let Some((wide, long)) = line_end_size(end, width, base) else { return };
    let half = wide / 2.0;
    let (dx, dy) = (f32::from(at.x - from.x), f32::from(at.y - from.y));
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    // A point `back` behind the end along the line and `side` to its left.
    let p = |back: f32, side: f32| point(at.x - px(ux * back) - px(uy * side), at.y - px(uy * back) + px(ux * side));
    let shape: Vec<Point<Pixels>> = match end {
        LineEnd::Triangle(_) => vec![p(0.0, 0.0), p(long, half), p(long, -half)],
        LineEnd::Stealth(_) => vec![p(0.0, 0.0), p(long, half), p(long * 0.6, 0.0), p(long, -half)],
        LineEnd::Diamond(_) => vec![p(-long / 2.0, 0.0), p(0.0, half), p(long / 2.0, 0.0), p(0.0, -half)],
        LineEnd::Oval(_) => (0..24)
            .map(|i| (i as f32 / 24.0 * 2.0 * PI).sin_cos())
            .map(|(s, c)| p(c * long / 2.0, s * half))
            .collect(),
        LineEnd::Arrow(_) => {
            let stroke = px(width.max(1.0));
            for side in [half, -half] {
                if let Some(path) = polygon(&[p(long, side), p(0.0, 0.0)], Some(stroke), false) {
                    window.paint_path(path, color);
                }
            }
            return;
        }
        LineEnd::None => return,
    };
    if let Some(path) = polygon(&shape, None, true) {
        window.paint_path(path, color);
    }
}

fn picture_element(container: Div, shape: &Shape, picture: &Picture, frame: EmuRect, scale: f32, res: &Resources) -> Div {
    let (w, h) = (frame.width as f32 * scale, frame.height as f32 * scale);
    let radius = px(w.min(h) * picture.corner as f32 / 100_000.0);
    // The outline, over the picture.
    let outline = picture.stroke.as_ref().map(|stroke| {
        let width = px((stroke.width as f32 * scale).max(0.5));
        div().absolute().size_full().rounded(radius).border(width).border_color(hsla(stroke.color))
    });

    // A video playing: its newest frame, in place of the poster, preserving corner, outline and shadow.
    #[cfg(target_os = "macos")]
    if let Some(ref clip) = picture.clip
        && let Some(live) = res.live_frame(clip.media)
    {
        let mut el = container
            .rounded(radius)
            .overflow_hidden()
            .child(gpui::surface(live).absolute().size_full().object_fit(ObjectFit::Fill))
            .children(outline);
        if let Some(s) = picture.shadow {
            let (dx, dy, blur, color) = shadow_parts(s, scale);
            el = el.shadow(vec![BoxShadow { color, offset: point(px(dx), px(dy)), blur_radius: px(blur / 3.0), spread_radius: px(0.), inset: false }]);
        }
        return el;
    }
    let turned = shape.rotation.rem_euclid(21_600_000) != 0 || shape.flip_h || shape.flip_v;
    if turned && let Some((image, bounds)) = rotated_image(shape, picture, frame, res) {
        // The rotated picture covers its bounding box, around the frame.
        return container.child(
            place(img(res.svg_picture(image)), bounds, scale).object_fit(ObjectFit::Fill),
        );
    }
    if !res.has_picture(picture.media) {
        return container;
    }
    let recoloured = picture.duotone.and_then(|(dark, light)| {
        let key = (picture.media, dark, light);
        if let Some(found) = res.duotones.borrow().get(&key) {
            return Some(found.clone());
        }
        let (encoding, data) = res.source(picture.media)?;
        let rgb = |c: ibis_doc_model::Rgb| (c.r(), c.g(), c.b());
        let image = Arc::new(Image::from_bytes(ImageFormat::Svg, crate::render::picture_svg::duotone_picture(&data, encoding, rgb(dark), rgb(light))));
        res.duotones.borrow_mut().insert(key, image.clone());
        Some(image)
    });
    // A colour set transparent (not combined with a duotone).
    let cleared = picture.clear.filter(|_| recoloured.is_none()).and_then(|clear| {
        let key = (picture.media, clear, clear);
        if let Some(found) = res.duotones.borrow().get(&key) {
            return Some(found.clone());
        }
        let (encoding, data) = res.source(picture.media)?;
        let image = Arc::new(Image::from_bytes(ImageFormat::Svg, crate::render::picture_svg::clear_colour_picture(&data, encoding, (clear.r(), clear.g(), clear.b()))));
        res.duotones.borrow_mut().insert(key, image.clone());
        Some(image)
    });
    // Recoloured pictures are loaded by GPUI; the picture itself once decoded.
    let Some(image) = recoloured.or(cleared).map(|image| res.svg_picture(image)).or_else(|| res.picture(picture.media)) else { return container };
    let c = picture.crop;
    let frac = |v: i32| v as f32 / 100_000.0;
    let visible_w = (1.0 - frac(c.left) - frac(c.right)).max(0.01);
    let visible_h = (1.0 - frac(c.top) - frac(c.bottom)).max(0.01);
    // A brightness overlay reflecting luminance.
    let lum_overlay = picture.luminance.and_then(|(bright, _)| {
        if bright > 0 {
            let alpha = ((bright as f32 / 100_000.0).clamp(0.0, 0.85) * 255.0) as u8;
            Some(div().absolute().size_full().bg(hsla(DocColor { rgb: ibis_doc_model::Rgb(0xffffff), alpha })))
        } else if bright < 0 {
            let alpha = (((-bright) as f32 / 100_000.0).clamp(0.0, 0.85) * 255.0) as u8;
            Some(div().absolute().size_full().bg(hsla(DocColor { rgb: ibis_doc_model::Rgb(0x000000), alpha })))
        } else {
            None
        }
    });

    let mut cont = container.rounded(radius);
    if let Some(s) = picture.shadow {
        let (dx, dy, blur, color) = shadow_parts(s, scale);
        cont = cont.shadow(vec![BoxShadow { color, offset: point(px(dx), px(dy)), blur_radius: px(blur / 3.0), spread_radius: px(0.), inset: false }]);
    }
    if let Some(r) = picture.reflection
        && r.sy == -100_000
        && (r.distance == 0 || r.direction == 5_400_000)
    {
        let gap = r.distance as f32 * scale;
        cont = cont.child(
            div()
                .absolute()
                .left(px(0.))
                .top(px(h + gap))
                .w(px(w))
                .h(px(h))
                .rounded(radius)
                .overflow_hidden()
                .opacity(0.35)
                .child(
                    img(image.clone())
                        .absolute()
                        .size_full()
                        .object_fit(ObjectFit::Fill),
                ),
        );
    }

    // The whole picture, sized and offset so the cropped part fills the frame.
    let whole = img(image.clone())
        .absolute()
        .left(relative(-frac(c.left) / visible_w))
        .top(relative(-frac(c.top) / visible_h))
        .w(relative(1.0 / visible_w))
        .h(relative(1.0 / visible_h))
        .object_fit(ObjectFit::Fill);
    // An animated GIF plays in the slide show: an image with an id keeps its
    // frame (GPUI advances it by the frames' delays).
    let animated = res.animate_pictures.get() && matches!(&image, ImageSource::Render(data) if data.frame_count() > 1);
    if animated {
        cont.overflow_hidden().child(whole.id(ElementId::Name(format!("gif-{}", shape.id.0).into())))
    } else {
        cont.overflow_hidden().child(whole)
    }
    .children(lum_overlay)
    .children(outline)
}

/// The picture of `shape` rotated and flipped, with its bounds relative to
/// the frame's top-left corner; `None` if its format cannot be wrapped.
fn rotated_image(shape: &Shape, picture: &Picture, frame: EmuRect, res: &Resources) -> Option<(Arc<Image>, EmuRect)> {
    let key = (picture.media, shape.rotation, shape.flip_h, shape.flip_v, picture.crop, frame.width, frame.height);
    if let Some(found) = res.rotated.borrow().get(&key) {
        return Some(found.clone());
    }
    let (encoding, data) = res.source(picture.media)?;
    let local = EmuRect { x: 0, y: 0, ..frame };
    let wrapped = crate::render::picture_svg::rotated_picture(&data, encoding, local, shape.rotation, shape.flip_h, shape.flip_v, picture.crop);
    let entry = (Arc::new(Image::from_bytes(ImageFormat::Svg, wrapped.svg)), wrapped.bounds);
    res.rotated.borrow_mut().insert(key, entry.clone());
    Some(entry)
}

fn placeholder_element(container: Div, kind: &Placeholder, frame: EmuRect, scale: f32) -> Div {
    let label = match kind {
        Placeholder::Chart => "_render.chart-unsupported",
        Placeholder::Table => "_render.table-unsupported",
        Placeholder::Picture(MediaFormat::Emf) => "_render.emf-image-unsupported",
        Placeholder::Picture(MediaFormat::Wmf) => "_render.wmf-image-unsupported",
        Placeholder::Picture(_) => "_render.image-unsupported",
        Placeholder::Diagram => "",
    };
    let side = frame.width.min(frame.height) as f32 * scale;
    container
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .bg(rgba(0x8080800f))
        .border_1()
        .border_dashed()
        .border_color(rgba(0x80808080))
        .text_color(rgb(0x808080))
        .text_size(px((side * 0.12).clamp(4.0, 14.0)))
        .when(side > 24.0, |d| d.child(if label.is_empty() { SharedString::default() } else { crate::shell::i18n::text(label) }))
}

/// Text of a body laid out inside its frame at `scale` pixels per EMU;
/// `width` is the width of the frame's text area in EMU when known (the
/// lines break at it exactly, the same at every zoom). `positions`, when
/// given, gets where each paragraph's characters and lines are painted
/// (for a caret, a selection and clicks).
pub(crate) fn text_body_element(
    body: &TextBody,
    width: Option<f64>,
    scale: f32,
    res: &Resources,
    positions: Option<&crate::render::rich_text::TextPositions>,
    turn: Option<crate::render::rich_text::TextTurn>,
    origin_x: Option<f32>,
) -> Div {
    if body.vertical && matches!(body.vertical_kind, ibis_doc_model::VerticalKind::WordArt | ibis_doc_model::VerticalKind::WordArtRtl) {
        return vertical_text_element(body, scale, res);
    }
    let px_per_point = scale * EMU_PER_POINT as f32;
    let font_scale = body.font_scale as f32 / 100_000.0;
    let size_px = |style: &TextStyle| px(style.font_size_pt() * font_scale * px_per_point);

    // The paragraphs are placed together, from their unrounded heights.
    let block = crate::render::rich_text::TextBlock::default();
    block.heights.borrow_mut().resize(body.paragraphs.len(), 0.0);
    let slots = positions.map(|p| p.slots(body.paragraphs.len()));
    let paragraphs = body.paragraphs.iter().enumerate().map(|(index, paragraph)| {
        let align = paragraph.align.unwrap_or(body.style.align);
        let styles: Vec<TextStyle> = paragraph.runs.iter().map(|r| res.with_theme_font(body.style.with_run(&r.style), &r.text)).collect();
        let end_style = res.with_theme_font(body.style.with_run(&paragraph.end_style), "");
        let first = styles.first().unwrap_or(&end_style);
        let line_height = match paragraph.line_spacing {
            Some(LineSpacing::Percent(p)) => relative(LINE_HEIGHT * p as f32 / 100_000.0),
            Some(LineSpacing::Points(p)) => px(p as f32 / 100.0 * px_per_point).into(),
            None => relative(LINE_HEIGHT),
        };
        let first_size = f32::from(size_px(first));
        let line_px = match paragraph.line_spacing {
            Some(LineSpacing::Percent(p)) => LINE_HEIGHT * p as f32 / 100_000.0 * first_size,
            Some(LineSpacing::Points(p)) => p as f32 / 100.0 * px_per_point,
            None => LINE_HEIGHT * first_size,
        };
        let shift = px(res.baseline_shift(&res.fonts.font(first), first_size, line_px));

        // Superscript and subscript are drawn at two thirds of the size,
        // raised or lowered by `baseline` of the full size.
        let script_scale = |r: &ibis_doc_model::Run| if r.style.baseline != 0 { 2.0 / 3.0 } else { 1.0 };
        // Without a bullet the first line starts at the indent and the others
        // at the margin; with one, the text starts at the margin.
        let first_line = if paragraph.bullet.is_some() {
            0
        } else {
            (paragraph.margin_left + paragraph.indent).max(0) - paragraph.margin_left
        };
        // PowerPoint puts no space above the first paragraph of a body, nor
        // below the last (measured: at the bottom of its box).
        let space_before = if index == 0 { 0 } else { paragraph.space_before };
        let space_after = if index + 1 == body.paragraphs.len() { 0 } else { paragraph.space_after };
        // An empty paragraph still takes a line at its mark's size.
        let mark = [ibis_doc_model::Run { text: String::new(), style: paragraph.end_style.clone() }];
        let (runs, run_styles) = if paragraph.runs.is_empty() {
            (&mark[..], std::slice::from_ref(&end_style))
        } else {
            (&paragraph.runs[..], &styles[..])
        };
        // An empty paragraph shows no bullet (it keeps one for the text typed into it).
        let has_text = paragraph.runs.iter().any(|r| !r.text.is_empty());
        // Turned text paints its (text) bullet with its lines.
        let turned_bullet = turn.and_then(|_| {
            let b = paragraph.bullet.as_ref().filter(|b| has_text && b.picture.is_none())?;
            let font = res.fonts.font(first);
            let size = f32::from(size_px(first)) * (b.size as f32 / 100_000.0);
            // As the unturned bullet: its font's height centred in the first
            // line, moved as the first line's text is (`baseline_shift`).
            let (ba, bd) = res.metrics(&font);
            let (fa, fd) = res.metrics(&font);
            let centred = |a: f32, d: f32, s: f32| (line_px - (a + d) * s) / 2.0 + a * s;
            Some(crate::render::rich_text::BulletMark {
                text: b.text.clone(),
                font,
                size,
                color: rgb_hsla(b.color.unwrap_or(first.color)),
                x: paragraph.indent as f32 * scale,
                drop: centred(ba, bd, size) - centred(fa, fd, first_size),
            })
        });
        let content: AnyElement = {
            // The ascent and descent Office gives typeface `name`: the
            // font's that draws it, unless another font stands in for it.
            let typeface = |name: Option<&str>, style: &TextStyle| -> (f32, f32) {
                let font = res.fonts.font(&TextStyle { font: name.map(str::to_string), ..style.clone() });
                name.filter(|name| *name != font.family.as_ref())
                    .and_then(crate::render::font_metrics::office_metrics)
                    .unwrap_or_else(|| res.metrics(&font))
            };
            let pieces = runs
                .iter()
                .zip(run_styles)
                .flat_map(|(r, s)| {
                    let full = f32::from(size_px(s));
                    let size = full * script_scale(r);
                    let font = res.fonts.font(s);
                    let default_east_asian = res.default_east_asian_font(s);
                    // A run's East Asian characters are in its East Asian
                    // typeface (`s.font` when it has them) and the others in
                    // its Latin one (`s.fallback_font`). A line is as high as
                    // its highest typeface, so the run is split where the
                    // script changes. (Without an East Asian typeface,
                    // PowerPoint for Mac draws them with ＭＳ ゴシック
                    // (`default_east_asian_font`), and the lines are as its
                    // Latin typeface's in most cases measured, so the run's
                    // typeface's height is kept.)
                    let has_east_asian = r.text.chars().any(crate::render::rich_text::is_cjk);
                    let east_asian = s
                        .font
                        .as_deref()
                        .filter(|name| has_east_asian && is_east_asian_typeface(name))
                        .map(|name| typeface(Some(name), s));
                    let latin = match (has_east_asian, s.fallback_font.as_deref()) {
                        (true, Some(latin)) if s.font.as_deref().is_some_and(is_east_asian_typeface) => typeface(Some(latin), s),
                        _ => typeface(s.font.as_deref(), s),
                    };
                    // Segments: text, ascent and descent, and whether in the
                    // default East Asian font.
                    let mut segments: Vec<(String, (f32, f32), bool)> = Vec::new();
                    for c in r.text.chars() {
                        let cjk = crate::render::rich_text::is_cjk(c);
                        let m = match east_asian {
                            Some(m) if cjk => m,
                            _ => latin,
                        };
                        let other = cjk && default_east_asian.is_some();
                        match segments.last_mut() {
                            Some((text, last, o)) if *last == m && *o == other => text.push(c),
                            _ => segments.push((c.to_string(), m, other)),
                        }
                    }
                    if segments.is_empty() {
                        segments.push((String::new(), latin, false));
                    }
                    // Capitals (`a:rPr/@cap`): each letter is drawn as its
                    // capital (one character for one, so that the caret's
                    // positions stay), small capitals smaller.
                    let segments: Vec<(String, (f32, f32), bool, f32)> = segments
                        .into_iter()
                        .flat_map(|(text, m, other)| crate::render::rich_text::capitals(&text, r.style.caps).into_iter().map(move |(t, k)| (t, m, other, k)))
                        .collect();
                    segments
                        .into_iter()
                        .map(|(text, (a, d), other, k)| {
                            let piece_font = if other { default_east_asian.clone().unwrap_or_else(|| font.clone()) } else { font.clone() };
                            crate::render::rich_text::Piece {
                                text,
                                font: piece_font,
                                size: size * k,
                                color: if r.style.invisible { Hsla::transparent_black() } else { rgb_hsla(s.color) },
                                underline: r.style.underline && !r.style.invisible,
                                underline_kind: r.style.underline_kind,
                                raise: r.style.baseline as f32 / 100_000.0 * full,
                                spacing: r.style.spacing as f32 / 100.0 * px_per_point,
                                highlight: r.style.highlight.filter(|_| !r.style.invisible).map(rgb_hsla),
                                outline: r.style.outline.filter(|_| !r.style.invisible).map(|(c, w)| (rgb_hsla(c), w as f32 * scale)),
                                shadow: r.style.shadow.filter(|_| !r.style.invisible).map(|s| {
                                    let rad = (s.direction as f32 / 60_000.0).to_radians();
                                    let dist = s.distance as f32 * scale;
                                    (hsla(s.color), dist * rad.cos(), dist * rad.sin())
                                }),
                                fill: r.style.fill.clone().filter(|_| !r.style.invisible),
                                ascent: a * size,
                                descent: d * size,
                                strike: if r.style.invisible { ibis_doc_model::Strike::None } else { r.style.strike },
                                strike_size: size,
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            let line = match paragraph.line_spacing {
                Some(LineSpacing::Percent(p)) => crate::render::rich_text::LineRule::Multiple(LINE_HEIGHT * p as f32 / 100_000.0),
                Some(LineSpacing::Points(p)) => crate::render::rich_text::LineRule::Fixed(p as f32 / 100.0 * px_per_point),
                None => crate::render::rich_text::LineRule::Multiple(LINE_HEIGHT),
            };
            crate::render::rich_text::rich_paragraph(crate::render::rich_text::RichParagraph {
                pieces,
                align,
                font_align: paragraph.font_align,
                line,
                wrap: body.wrap,
                first_line: first_line as f32 * scale,
                room: width.map(|w| {
                    let total_w = (w as f32 - (body.insets.left + body.insets.right) as f32).max(0.0);
                    let col_w = if body.columns > 1 && !body.vertical {
                        let cols = body.columns.clamp(1, 16) as f32;
                        let total_spc = (cols - 1.0) * body.column_spacing as f32;
                        ((total_w - total_spc) / cols).max(0.0)
                    } else {
                        total_w
                    };
                    (col_w - paragraph.margin_left as f32).max(0.0) * scale
                }),
                px_per_point,
                text_top: Some(crate::render::rich_text::TextTop {
                    block: block.clone(),
                    index,
                    before: space_before as f32 / 100.0 * px_per_point,
                    after: space_after as f32 / 100.0 * px_per_point,
                }),
                bullet: turned_bullet.clone(),
                positions: slots.as_ref().map(|s| s[index].clone()),
                last: index + 1 == body.paragraphs.len(),
                upright: body.vertical,
                mirror: body.vertical && body.vertical_kind == ibis_doc_model::VerticalKind::Mongolian,
                at_x: origin_x.filter(|_| turn.is_none() && !body.vertical && paragraph.align.unwrap_or(body.style.align) == TextAlign::Left).map(|x0| {
                    x0 + (body.insets.left + paragraph.margin_left) as f32 * scale
                }),
            })
            .into_any_element()
        };

        let above = px(space_before as f32 / 100.0 * px_per_point);
        let bullet = paragraph.bullet.as_ref().filter(|b| has_text && (turn.is_none() || b.picture.is_some())).map(|b| {
            let color = b.color.unwrap_or(first.color);
            let size = size_px(first) * (b.size as f32 / 100_000.0);
            let left = px((paragraph.margin_left + paragraph.indent) as f32 * scale);
            match b.picture.and_then(|m| res.picture(m)) {
                // A picture bullet is a square in the middle of the first line,
                // 0.7 of the bullet's size (measured against PowerPoint's PDF:
                // a check mark at 142 % of 16 pt).
                Some(image) => {
                    let size = size * 0.7;
                    div()
                    .absolute()
                    .top(above + (px(line_px) - size) / 2.0)
                    .left(left)
                    .size(size)
                    .child(img(image).size_full().object_fit(ObjectFit::Contain))
                }
                None => div()
                    .absolute()
                    .top(above + shift)
                    .left(left)
                    .text_size(size)
                    // A smaller bullet sits in the middle of the first line.
                    .line_height(px(line_px))
                    .text_color(rgb_hsla(color))
                    .child(SharedString::from(b.text.clone())),
            }
        });

        div()
            .relative()
            .w_full()
            .pl(px(paragraph.margin_left as f32 * scale))
            .pt(above)
            .pb(px(space_after as f32 / 100.0 * px_per_point))
            .line_height(line_height)
            .map(|d| align_text(d, align))
            .children(bullet)
            .child(content)
    });

    div()
        .size_full()
        .flex()
        .flex_col()
        .map(|d| match body.anchor {
            VerticalAnchor::Top => d.justify_start(),
            VerticalAnchor::Middle => d.justify_center(),
            VerticalAnchor::Bottom => d.justify_end(),
        })
        .pl(px(body.insets.left as f32 * scale))
        .pr(px(body.insets.right as f32 * scale))
        .pt(px(body.insets.top as f32 * scale))
        .pb(px(body.insets.bottom as f32 * scale))
        .when(!body.wrap, |d| d.whitespace_nowrap())
        .child({
            if body.columns > 1 && !body.vertical {
                let cols = (body.columns as usize).clamp(1, 16);
                let mut paras: Vec<Option<gpui_kit::Div>> = paragraphs.map(Some).collect();
                let total = paras.len();
                let per_col = if total == 0 { 1 } else { (total + cols - 1) / cols };
                let gap_px = (body.column_spacing as f32 * scale).max(0.0);
                let col_elements = (0..cols).map(|c| {
                    let start = c * per_col;
                    let end = (start + per_col).min(total);
                    let col_paras: Vec<gpui_kit::Div> = (start..end).filter_map(|i| paras[i].take()).collect();
                    div().flex_1().flex().flex_col().children(col_paras)
                });
                div().w_full().flex().flex_row().gap(px(gap_px)).children(col_elements).into_any_element()
            } else {
                div().w_full().flex().flex_col().children(paragraphs).into_any_element()
            }
        })
        // After the paragraphs are prepainted (their heights known), the
        // text's place in the whole area (the padding is the insets).
        .child({
            let insets = (body.insets.top as f32 * scale, body.insets.bottom as f32 * scale);
            let anchor = match body.anchor {
                VerticalAnchor::Top => 0.0,
                VerticalAnchor::Middle => 0.5,
                VerticalAnchor::Bottom => 1.0,
            };
            canvas(
                move |bounds, _, _| {
                    let (top, height) = (f32::from(bounds.origin.y), f32::from(bounds.size.height));
                    block.place(top + insets.0, height - insets.0 - insets.1, anchor);
                    let middle = bounds.center();
                    block.turn(f32::from(middle.x), f32::from(middle.y), turn);
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        })
}

/// WordArt vertical writing (`wordArtVert`, `wordArtVertRtl`): every
/// character upright in a cell of its own, stacked down a column, the
/// columns from left to right (from right to left for `wordArtVertRtl`),
/// a paragraph starting a column and a column wrapping at the frame's
/// height. Measured in PowerPoint's PDF at 14 pt: a Latin character's cell
/// is 1.42 em square, an East Asian one's 1.29 em wide and 1.17 em high. The
/// anchor places the columns across the frame (top: where they start) and
/// a paragraph's alignment its characters along its column.
fn vertical_text_element(body: &TextBody, scale: f32, res: &Resources) -> Div {
    let px_per_point = scale * EMU_PER_POINT as f32;
    let font_scale = body.font_scale as f32 / 100_000.0;
    let rtl = body.vertical_kind == ibis_doc_model::VerticalKind::WordArtRtl;
    let mut cells: Vec<AnyElement> = Vec::new();
    for (n, paragraph) in body.paragraphs.iter().enumerate() {
        if n > 0 {
            // A new paragraph starts a column.
            cells.push(div().w(px(0.0)).h_full().into_any_element());
        }
        for run in &paragraph.runs {
            let style = res.with_theme_font(body.style.with_run(&run.style), &run.text);
            let font = res.font(&style);
            let size = style.font_size_pt() * font_scale * px_per_point;
            let color = rgb_hsla(style.color);
            for c in run.text.chars() {
                let (w, h) = if crate::render::rich_text::is_cjk(c) { (1.2857 * size, 1.1657 * size) } else { (1.4229 * size, 1.4229 * size) };
                cells.push(
                    div()
                        .flex_shrink_0()
                        .w(px(w))
                        .h(px(h))
                        .flex()
                        .justify_center()
                        .items_center()
                        .font_family(font.family.clone())
                        .font_weight(font.weight)
                        .font_features(font.features.clone())
                        .text_size(px(size))
                        .line_height(px(h))
                        .text_color(color)
                        .child(SharedString::from(c.to_string()))
                        .into_any_element(),
                );
            }
        }
    }
    let align = body.paragraphs.first().and_then(|p| p.align).unwrap_or(body.style.align);
    div()
        .size_full()
        .flex()
        .flex_col()
        .map(|d| if rtl { d.flex_wrap_reverse() } else { d.flex_wrap() })
        .map(|d| match body.anchor {
            VerticalAnchor::Top => d.content_start(),
            VerticalAnchor::Middle => d.content_center(),
            VerticalAnchor::Bottom => d.content_end(),
        })
        .map(|d| match align {
            TextAlign::Left | TextAlign::Justify | TextAlign::Distributed => d.justify_start(),
            TextAlign::Center => d.justify_center(),
            TextAlign::Right => d.justify_end(),
        })
        .pl(px(body.insets.left as f32 * scale))
        .pr(px(body.insets.right as f32 * scale))
        .pt(px(body.insets.top as f32 * scale))
        .pb(px(body.insets.bottom as f32 * scale))
        .children(cells)
}

pub(crate) fn align_text<E: Styled>(element: E, align: TextAlign) -> E {
    match align {
        TextAlign::Left | TextAlign::Justify | TextAlign::Distributed => element.text_left(),
        TextAlign::Center => element.text_center(),
        TextAlign::Right => element.text_right(),
    }
}

/// The font of a body's style, for the text field that edits it.
pub(crate) fn body_font(style: &TextStyle, res: &Resources) -> Font {
    res.fonts.font(style)
}

thread_local! {
    /// Pair kerning by (font, first, second), in ems.
    static PAIR_KERNING: std::cell::RefCell<HashMap<(FontId, char, char), f32>> = std::cell::RefCell::new(HashMap::new());
    /// A font's shaping data, built once: building it (the font's tables,
    /// tens of milliseconds for a Japanese font) for every new pair made
    /// opening a 144-page Japanese document take 10 s.
    static SHAPER_DATA: std::cell::RefCell<HashMap<FontId, Option<std::rc::Rc<(std::sync::Arc<Vec<u8>>, u32, harfrust::ShaperData)>>>> = std::cell::RefCell::new(HashMap::new());
}

impl Resources {
    /// How much the pair `a` `b` is kerned in `font` at `size` (points;
    /// negative: closer), from the kerning of the font file drawn (its kern
    /// table or GPOS pair adjustments, through harfrust): the advance of the
    /// two shaped with kerning less that without. 0 when the font's data is
    /// not available. (Word's own Calibri was used only to check that
    /// kerning explains Word's line breaks: DBDY-00007.)
    pub(crate) fn pair_kerning(&self, font: &Font, size: f32, a: char, b: char) -> f32 {
        let id = self.text_system.resolve_font(font);
        if let Some(em) = PAIR_KERNING.with(|c| c.borrow().get(&(id, a, b)).copied()) {
            return em * size;
        }
        let entry = SHAPER_DATA.with(|c| {
            c.borrow_mut()
                .entry(id)
                .or_insert_with(|| {
                    let (data, index) = self.text_system.font_data(id)?;
                    let font = harfrust::FontRef::from_index(&data, index).ok()?;
                    let shaper_data = harfrust::ShaperData::new(&font);
                    Some(std::rc::Rc::new((data, index, shaper_data)))
                })
                .clone()
        });
        let em = entry
            .and_then(|entry| {
                let (data, index, shaper_data) = &*entry;
                let font = harfrust::FontRef::from_index(data, *index).ok()?;
                let shaper = shaper_data.shaper(&font).build();
                let advance = |kern: u32| {
                    let mut buffer = harfrust::UnicodeBuffer::new();
                    let mut text = [0u8; 8];
                    let mut s = String::new();
                    s.push_str(a.encode_utf8(&mut text));
                    s.push_str(b.encode_utf8(&mut text));
                    buffer.push_str(&s);
                    buffer.guess_segment_properties();
                    let features = [harfrust::Feature::new(harfrust::Tag::new(b"kern"), kern, ..)];
                    let shaped = shaper.shape(buffer, &features);
                    shaped.glyph_positions().iter().map(|p| p.x_advance).sum::<i32>()
                };
                let upem = shaper.units_per_em().max(1) as f32;
                Some((advance(1) - advance(0)) as f32 / upem)
            })
            .unwrap_or(0.0);
        PAIR_KERNING.with(|c| c.borrow_mut().insert((id, a, b), em));
        em * size
    }
}


#[cfg(test)]
mod font_tests {
    // Not `super::*`: GPUI's own `test` attribute would take the place of `#[test]`.
    use super::{FontWeight, Fonts};

    fn fonts(installed: &[&str]) -> Fonts {
        Fonts { installed: installed.iter().map(|f| f.to_string()).collect(), east_asian_fallback: None }
    }

    #[test]
    fn a_linux_system_draws_office_typefaces_with_the_fonts_of_their_widths() {
        let linux = fonts(&["Liberation Sans", "Liberation Serif", "Liberation Mono", "Carlito", "Caladea", "IPAGothic", "IPAMincho", "Noto Sans CJK JP"]);
        let family = |name: &str| linux.available(name).map(|(family, _)| family.to_string());
        assert_eq!(family("Arial").as_deref(), Some("Liberation Sans"));
        assert_eq!(family("Times New Roman").as_deref(), Some("Liberation Serif"));
        assert_eq!(family("Century").as_deref(), Some("Liberation Serif"));
        assert_eq!(family("Courier New").as_deref(), Some("Liberation Mono"));
        assert_eq!(family("Calibri").as_deref(), Some("Carlito"));
        assert_eq!(family("Cambria").as_deref(), Some("Caladea"));
        // Both of the names Office writes for it.
        assert_eq!(family("ＭＳ ゴシック").as_deref(), Some("IPAGothic")); // i18n-ok（フォント名。訳さない）
        assert_eq!(family("MS Gothic").as_deref(), Some("IPAGothic"));
        assert_eq!(family("MS Mincho").as_deref(), Some("IPAMincho"));
        // A weight in the name stays a weight.
        assert_eq!(linux.available("Arial Bold").map(|(family, weight)| (family.to_string(), weight)), Some(("Liberation Sans".to_string(), FontWeight::BOLD)));
        // No font of its widths: not available (the caller falls back).
        assert_eq!(family("Meiryo"), None);
    }

    #[test]
    fn an_installed_typeface_is_drawn_with_itself() {
        let both = fonts(&["Arial", "Liberation Sans", "MS Gothic", "IPAGothic"]);
        assert_eq!(both.available("Arial").map(|(family, _)| family.to_string()).as_deref(), Some("Arial"));
        assert_eq!(both.available("MS Gothic").map(|(family, _)| family.to_string()).as_deref(), Some("MS Gothic"));
        // The file's name for it finds the installed one before a stand-in.
        assert_eq!(both.available("ＭＳ ゴシック").map(|(family, _)| family.to_string()).as_deref(), Some("MS Gothic")); // i18n-ok（フォント名。訳さない）
    }

    #[test]
    fn a_mincho_typeface_gives_way_to_a_serif_japanese_font() {
        let linux = fonts(&["Noto Sans CJK JP", "Noto Serif CJK JP"]);
        assert_eq!(linux.family("游明朝").0.as_ref(), "Noto Serif CJK JP"); // i18n-ok（フォント名。訳さない）
        assert_eq!(linux.family("HG明朝E").0.as_ref(), "Noto Serif CJK JP"); // i18n-ok（フォント名。訳さない）
    }
}
