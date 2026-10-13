// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The presentation editor window: state, editing operations and layout.
//!
//! Rendering is split by area: `chrome.rs`, `status_bar.rs`, `workspace.rs` and
//! `thumbnails.rs` add `impl SlideEditor` blocks for their part of the window.

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState, TextareaState};
use gpui_kit::component::slider::{SliderEvent, SliderScale, SliderState};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use ibis_doc_model::{
    CellRange, Command, Document, EmuPoint, EmuRect, Presentation, ResizeHandle, Shape, ShapeId, ShapeKind, Sheet,
    Slide, SlideId, TextStyle, step_font_size,
};

use crate::render::shape_view::Resources;
use crate::shell::format::{FormatState, with_format};
use crate::{CancelTool, CloseFile, DeleteSelection, NextSlide, OpenFile, PreviousSlide, Redo, Undo};

/// Key context of the editor; see the bindings in [`crate::init`].
pub(crate) const CONTEXT: &str = "SlideEditor";
/// Key context of the workspace with the slide (the editor's focus).
pub(crate) const WORKSPACE_CONTEXT: &str = "SlideWorkspace";
/// Key context of the slide list.
pub(crate) const LIST_CONTEXT: &str = "SlideList";
/// Key context of the workspace when it shows a worksheet: the arrow keys
/// move the selected cell instead of turning pages.
pub(crate) const SHEET_CONTEXT: &str = "SheetWorkspace";
/// Key context of the workspace when it shows a flowing document: the keys
/// scroll through its pages, as in Word.
pub(crate) const DOCUMENT_CONTEXT: &str = "DocumentWorkspace";

/// Scrolling of a trackpad that turns one page, in pixels.
const WHEEL_PAGE: f32 = 80.0;
/// In the browser, a pixel scroll that follows this long a pause starts a new
/// scroll and turns a page at once, in milliseconds. See `on_workspace_scroll`.
#[cfg(target_family = "wasm")]
const WHEEL_PAUSE_MS: f64 = 100.0;

/// The lines Windows scrolls for a notch of the wheel (the user's setting,
/// which GPUI multiplies a notch by), at least one.
#[cfg(target_os = "windows")]
fn wheel_scroll_lines() -> f32 {
    const SPI_GETWHEELSCROLLLINES: u32 = 0x0068;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, value: *mut std::ffi::c_void, win_ini: u32) -> i32;
    }
    let mut lines: u32 = 3;
    // SAFETY: SPI_GETWHEELSCROLLLINES writes one u32 to `value`.
    let ok = unsafe { SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, (&raw mut lines).cast(), 0) } != 0;
    if ok { lines.max(1) as f32 } else { 3.0 }
}

/// The lines GPUI's Linux platform reports for a notch of the wheel (its
/// `SCROLL_LINES`, on Wayland and on X11 alike).
#[cfg(target_os = "linux")]
fn wheel_scroll_lines() -> f32 {
    3.0
}

/// How often to look for newly decoded pictures, in milliseconds: a frame.
const PICTURE_POLL_MS: u64 = 16;

/// Space kept around the slide inside the workspace, in pixels.
const WORKSPACE_MARGIN: f32 = 40.0;
/// Space kept around a slide fitted to the workspace, in pixels: room for
/// its shadow and little more, as in PowerPoint.
const FIT_MARGIN: f32 = 8.0;

/// Pixels per inch at 100% zoom: the slide at its real size on a 96 dpi
/// screen (a point is 4/3 px), as in PowerPoint.
const PX_PER_INCH_AT_100: f32 = 96.0;
/// A worksheet's: Excel for Mac draws a point as one pixel at 100%, 0.75 of
/// the size a slide has there (the user's measure: ibisOffice's 70% looked
/// like Excel's 100%).
const SHEET_PX_PER_INCH_AT_100: f32 = 72.0;
/// The zoom range of the status bar, 1.0 being 100%.
pub(crate) const MIN_ZOOM: f32 = 0.1;
pub(crate) const MAX_ZOOM: f32 = 4.0;
/// Ratio of one − / + step, 2^(1/4): four steps double or halve the zoom.
const ZOOM_STEP: f32 = 1.189_207_1;
/// Ratios between the zooms of the status bar's list: over 100% each is
/// 1.2 times the one under it, under 100% 0.8 times the one over it.
const ZOOM_LIST_UP: f32 = 1.2;
const ZOOM_LIST_DOWN: f32 = 0.8;
/// Zooming with the wheel (the zoom modifier held): a notch of a wheel
/// zooms by this ratio, and a trackpad doubles the zoom every this many pixels.
const WHEEL_ZOOM_STEP: f32 = 1.1;
const WHEEL_ZOOM_DOUBLING: f32 = 150.0;
/// A press that moves less than this is a click, not a drag, in pixels.
const DRAG_THRESHOLD: f32 = 3.0;
/// The smallest side a text box can be resized to, in EMU (0.1 in).
const MIN_SHAPE_SIDE: i64 = 91_440;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    Select,
    TextBox,
}

/// A pointer drag in progress. Frames are previewed while dragging and
/// committed as one command on release.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Drag {
    Move { shape: ShapeId, start: Point<Pixels>, current: Point<Pixels>, origin: EmuRect },
    Resize {
        shape: ShapeId,
        handle: ResizeHandle,
        start: Point<Pixels>,
        current: Point<Pixels>,
        origin: EmuRect,
    },
    Create { start: Point<Pixels>, current: Point<Pixels> },
}

/// Text editing in place, with the text field laid over the shape.
pub(crate) struct TextEdit {
    pub(crate) shape: ShapeId,
    pub(crate) state: Entity<TextareaState>,
    /// The shape was created by this edit; an empty result removes it.
    is_new: bool,
    _subscription: Subscription,
}

/// Mapping between slide EMU and window pixels for the current layout.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SlideLayout {
    /// Top-left of the slide, in window coordinates.
    pub(crate) origin: Point<Pixels>,
    /// Pixels per EMU.
    pub(crate) scale: f32,
}

impl SlideLayout {
    /// The zoom this scale shows, 1.0 being 100%.
    pub(crate) fn zoom(&self, px_per_inch_at_100: f32) -> f32 {
        self.scale * ibis_doc_model::EMU_PER_INCH as f32 / px_per_inch_at_100
    }

    pub(crate) fn to_px(&self, emu: i64) -> Pixels {
        px(emu as f32 * self.scale)
    }

    pub(crate) fn to_emu_len(&self, len: Pixels) -> i64 {
        (f32::from(len) / self.scale).round() as i64
    }

    pub(crate) fn to_emu(&self, p: Point<Pixels>) -> EmuPoint {
        EmuPoint::new(self.to_emu_len(p.x - self.origin.x), self.to_emu_len(p.y - self.origin.y))
    }
}

/// What the workspace scrolls: the slide with its margin, and at least the
/// workspace itself so a smaller slide stays centred.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WorkspaceContent {
    pub(crate) size: Size<Pixels>,
    /// Top-left of the slide in the content.
    pub(crate) slide_origin: Point<Pixels>,
    /// Pixels per EMU.
    pub(crate) scale: f32,
}

/// 編集記号の表示設定 (DOCX-OPTIONS-008: Word Options > 表示 > 常に画面に表示する編集記号).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct FormattingMarksOptions {
    /// 常に表示する: タブ (→)
    pub show_tabs: bool,
    /// 常に表示する: スペース (·, □)
    pub show_spaces: bool,
    /// 常に表示する: 段落記号 (¶)
    pub show_paragraph_marks: bool,
    /// 常に表示する: 任意指定の改行 / 手動改行 (↵)
    pub show_line_breaks: bool,
    /// 常に表示する: 隠し文字 (下線点線)
    pub show_hidden_text: bool,
}

impl FormattingMarksOptions {
    pub fn is_tabs_visible(&self, all_open: bool) -> bool {
        all_open || self.show_tabs
    }
    pub fn is_spaces_visible(&self, all_open: bool) -> bool {
        all_open || self.show_spaces
    }
    pub fn is_paragraph_marks_visible(&self, all_open: bool) -> bool {
        all_open || self.show_paragraph_marks
    }
    pub fn is_line_breaks_visible(&self, all_open: bool) -> bool {
        all_open || self.show_line_breaks
    }
    pub fn is_hidden_text_visible(&self, all_open: bool) -> bool {
        all_open || self.show_hidden_text
    }
    pub fn any_visible(&self, all_open: bool) -> bool {
        all_open || self.show_tabs || self.show_spaces || self.show_paragraph_marks || self.show_line_breaks || self.show_hidden_text
    }
}

/// Word Options > 詳細設定 > 切り取り、コピー、貼り付け: 貼付の既定動作 (DOCX-OPTIONS-006).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DefaultPasteMode {
    /// 元の書式を保持 (Keep Source Formatting, Word既定値).
    #[default]
    KeepSourceFormatting,
    /// 書式を統合 (Merge Formatting / 貼り付け先の書式に合わせる).
    MergeFormatting,
    /// テキストのみ保持 (Keep Text Only).
    KeepTextOnly,
}

/// Word Options > 詳細設定 > 表示: ルーラーの測定単位 (DOCX-VIEW-013).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RulerUnit {
    /// センチメートル (Centimeters, Word 日本語既定値).
    #[default]
    Centimeters,
    /// ミリメートル (Millimeters).
    Millimeters,
    /// インチ (Inches).
    Inches,
    /// ポイント (Points).
    Points,
    /// ピカ (Picas, 1 pica = 12 pt).
    Picas,
}

impl RulerUnit {
    /// 1 単位あたりの EMU 値。
    pub(crate) fn emu_per_unit(self) -> f32 {
        match self {
            Self::Centimeters => 360_000.0,
            Self::Millimeters => 36_000.0,
            Self::Inches => 914_400.0,
            Self::Points => 12_700.0,
            Self::Picas => 152_400.0,
        }
    }

    /// 単位の略称（ルーラーのコーナー表示等）。
    pub(crate) fn abbreviation(self) -> &'static str {
        match self {
            Self::Centimeters => "cm",
            Self::Millimeters => "mm",
            Self::Inches => "in",
            Self::Points => "pt",
            Self::Picas => "pi",
        }
    }

    /// ルーラーの基本刻み間隔（EMU）。
    pub(crate) fn tick_step_emu(self) -> f32 {
        match self {
            Self::Centimeters => 180_000.0, // 0.5 cm
            Self::Millimeters => 36_000.0,  // 1 mm
            Self::Inches => 114_300.0,      // 1/8 in
            Self::Points => 228_600.0,      // 18 pt
            Self::Picas => 76_200.0,        // 0.5 pica
        }
    }

    /// ラベル数値の間隔候補（単位数）。
    pub(crate) fn label_candidates(self) -> &'static [i32] {
        match self {
            Self::Centimeters => &[1, 2, 5, 10, 20],
            Self::Millimeters => &[5, 10, 20, 50, 100],
            Self::Inches => &[1, 2, 5, 10],
            Self::Points => &[36, 72, 144, 288, 720],
            Self::Picas => &[1, 2, 5, 10, 20],
        }
    }

    /// 刻みインデックスから、大目盛り（数値表示候補）・中目盛り・表示数値を判定。
    /// 返り値: (is_major, is_medium, unit_label_number)
    pub(crate) fn tick_info(self, index: i32) -> (bool, bool, i32) {
        match self {
            Self::Centimeters => {
                let is_major = index % 2 == 0;
                (is_major, false, index.abs() / 2)
            }
            Self::Millimeters => {
                let is_major = index % 10 == 0;
                let is_medium = index % 5 == 0 && !is_major;
                (is_major, is_medium, index.abs())
            }
            Self::Inches => {
                let is_major = index % 8 == 0;
                let is_medium = (index % 4 == 0 || index % 2 == 0) && !is_major;
                (is_major, is_medium, index.abs() / 8)
            }
            Self::Points => {
                let pt = index.abs() * 18;
                let is_major = pt % 72 == 0;
                let is_medium = pt % 36 == 0 && !is_major;
                (is_major, is_medium, pt)
            }
            Self::Picas => {
                let is_major = index % 2 == 0;
                (is_major, false, index.abs() / 2)
            }
        }
    }

    /// 次の測定単位を返す（トグル送り用）。
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Centimeters => Self::Millimeters,
            Self::Millimeters => Self::Inches,
            Self::Inches => Self::Points,
            Self::Points => Self::Picas,
            Self::Picas => Self::Centimeters,
        }
    }
}

pub struct SlideEditor {
    /// Focus of the workspace; key presses reach the editor through it.
    pub(crate) focus_handle: FocusHandle,
    /// Focus of the slide list.
    pub(crate) list_focus: FocusHandle,
    pub(crate) list_scroll: ScrollHandle,
    /// Trackpad scrolling not yet turned into a page, in pixels.
    wheel: f32,
    /// The menu of the menu bar that is open (shell/menu_bar.rs).
    pub(crate) open_menu: Option<crate::shell::menu_bar::OpenMenu>,
    /// Where the pointer was when a menu closed, so that the press that closed it does not open it again.
    pub(crate) menu_dismissed_at: Option<Point<Pixels>>,
    /// Time of the last pixel scroll, in milliseconds (browser only).
    #[cfg(target_family = "wasm")]
    wheel_at: f64,
    /// Parts of a wheel notch not yet turned into a page (Windows and Linux only).
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    wheel_notches: f32,
    pub(crate) document: Document,
    pub(crate) current_slide: SlideId,
    pub(crate) selection: Option<ShapeId>,
    pub(crate) tool: Tool,
    pub(crate) drag: Option<Drag>,
    pub(crate) editing: Option<TextEdit>,
    /// Bounds of the workspace around the slide, from the last prepaint.
    pub(crate) workspace_bounds: Option<Bounds<Pixels>>,
    /// Scrolling of the workspace when the zoomed slide does not fit.
    pub(crate) workspace_scroll: ScrollHandle,
    /// Zoom chosen in the status bar, 1.0 being 100%; `None` fits the slide
    /// to the workspace.
    pub(crate) zoom: Option<f32>,
    /// The zoom slider of the status bar, in percent.
    pub(crate) zoom_slider: Entity<SliderState>,
    /// Where the status bar was drawn last, in window coordinates: its left
    /// and right edges and the zoom slider's left edge (the sheet tabs'
    /// scrollbar lies between the tabs' left edge and the slider).
    pub(crate) status_layout: std::rc::Rc<std::cell::Cell<[f32; 3]>>,
    _zoom_subscription: Subscription,
    /// The status bar's list of zooms is open.
    pub(crate) zoom_menu_open: bool,
    /// 「指定」 of the list of zooms: a zoom typed in percent.
    pub(crate) zoom_input: Entity<InputState>,
    _zoom_input_subscription: Subscription,
    /// Whether the zoom dialog is open.
    pub(crate) zoom_dialog_open: bool,
    /// The input state inside the zoom dialog.
    pub(crate) zoom_dialog_input: Entity<InputState>,
    _zoom_dialog_subscription: Subscription,
    /// The slide show, while the presentation is shown full screen.
    pub(crate) slideshow: Option<crate::pptx::slideshow::Slideshow>,
    /// Decoded pictures and fonts for the open presentation.
    pub(crate) resources: Resources,
    /// Redraws the editor as pictures decode (`watch_pictures`).
    picture_watch: Option<Task<()>>,
    /// Name of the open file; `None` for a new presentation.
    pub(crate) file_name: Option<SharedString>,
    /// Progress or error of the last file operation, for the status bar.
    pub(crate) status: Option<SharedString>,
    /// A chart's workbook (データの編集): the ranges its series take their
    /// data from, framed on the sheet.
    pub(crate) chart_ranges: Option<Vec<ibis_pptx::SeriesRefs>>,
    /// Recently opened files, most recent first, for ファイル > 履歴.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) recent_files: Vec<std::path::PathBuf>,
    /// The password the open file is protected with (パスワードを使用して暗号化):
    /// what is saved is protected again with it.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) protection_password: Option<String>,
    /// The colours picked lately from a palette, the last first (for the palette's row of them).
    pub(crate) recent_colors: Vec<ibis_doc_model::Rgb>,
    /// The colour last picked from each toolbar palette button (by its id), for the split button's left half.
    pub(crate) last_colors: std::collections::HashMap<&'static str, ibis_doc_model::Rgb>,
    /// Scrolling of the recent files list of the start view.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) recent_scroll: ScrollHandle,
    /// A file is open; until one is, the window shows the start view.
    pub(crate) has_document: bool,
    /// How many times a file was closed (`close_file`): a file still being
    /// read when the count changes is not opened.
    pub(crate) closed: u32,
    /// An empty document stands in for the file being read (`begin_loading`).
    pub(crate) loading: bool,
    /// What only the open document's format has (see `shell/format.rs`).
    pub(crate) format: FormatState,
    /// The sidebar is shown (the title row's toggle).
    pub(crate) sidebar_open: bool,
    /// 記号と特殊文字 (挿入 > 特殊文字…), while open in the sidebar.
    pub(crate) symbol: Option<crate::shell::symbols::SymbolState>,
    /// A document's outline (概要) is shown.
    pub(crate) outline_open: bool,
    /// A document's ruler (ルーラー) is shown.
    pub(crate) ruler_open: bool,
    /// A document's vertical ruler (垂直ルーラー) is shown (DOCX-VIEW-012).
    pub(crate) vertical_ruler_open: bool,
    /// A document's ruler measurement unit (ルーラーの測定単位, DOCX-VIEW-013).
    pub(crate) ruler_unit: RulerUnit,
    /// A document's gridlines (グリッド線) are shown.
    pub(crate) gridlines_open: bool,
    /// A document's formatting marks (編集記号) are shown.
    pub(crate) formatting_marks_open: bool,
    /// ページ間の余白の表示 (DOCX-VIEW-017). True displays normal page gaps and margins; false collapses page gaps into a thin dividing line.
    pub(crate) show_white_space: bool,
    /// ウィンドウの分割比率 (DOCX-VIEW-019). None = 分割なし, Some(ratio) = 上下分割 (例: 0.5).
    pub(crate) doc_split: Option<f32>,
    /// 分割時の上部ペインの独立スクロールハンドル (DOCX-VIEW-019).
    pub(crate) doc_split_scroll: ScrollHandle,
    /// 分割境界バーのドラッグ中フラグ (DOCX-VIEW-019).
    pub(crate) doc_split_dragging: bool,
    /// 分割時に直近でアクティブになったペイン (DOCX-VIEW-019: true = 上部, false = 下部).
    pub(crate) doc_split_active: bool,
    /// ページの左右移動モード (DOCX-VIEW-008). false = 縦方向スクロール (既定), true = 左右ページ移動 (Side to Side).
    pub(crate) page_movement_side_to_side: bool,
    /// 閲覧モード (DOCX-VIEW-005).
    pub(crate) doc_read_mode: bool,
    /// フォーカス表示モード (DOCX-VIEW-006).
    pub(crate) doc_focus_mode: bool,
    /// 常に画面に表示する編集記号オプション (DOCX-OPTIONS-008).
    pub(crate) formatting_marks_options: FormattingMarksOptions,
    /// 貼付の既定動作 (DOCX-OPTIONS-006).
    pub(crate) default_paste_mode: DefaultPasteMode,
    /// ユーザー名設定 (DOCX-OPTIONS-003).
    pub(crate) custom_user_name: Option<String>,
    /// ユーザー頭文字設定 (DOCX-OPTIONS-003).
    pub(crate) custom_user_initials: Option<String>,
    /// 印刷前にリンクされたデータを更新する (DOCX-OPTIONS-009).
    pub(crate) update_links_before_printing: bool,
    /// 印刷オプション: 背景の色とイメージを印刷する (DOCX-OPTIONS-010).
    pub(crate) print_backgrounds: bool,
    /// 印刷オプション: 隠し文字を印刷する (DOCX-OPTIONS-010).
    pub(crate) print_hidden_text: bool,
    /// 変更履歴の特定の人物フィルタ (DOCX-REVIEW-005). None = すべての校閲者.
    pub(crate) reviewer_filter: Option<std::collections::HashSet<String>>,
    /// The title row and the menu bar are hidden (the toolbar's ^).
    pub(crate) chrome_compact: bool,
    /// Where the open file came from, for saving it (`shell/save.rs`);
    /// `None` for a new document or one that cannot be saved.
    pub(crate) source: Option<std::sync::Arc<ibis_pptx::Source>>,
    /// スライド マスター表示, while it is open.
    pub(crate) master_view: Option<crate::pptx::master_view::MasterView>,
    /// ノートの書式編集, while it is open.
    pub(crate) notes_format: Option<crate::pptx::notes_format::NotesFormat>,
    /// The file's path, for ⌘S (none in the browser, or for a new document).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) path: Option<std::path::PathBuf>,
    /// A unique identifier for the workbook session, shared across windows viewing the same workbook.
    pub(crate) workbook_id: u64,
    /// A unique identifier for the document session, shared across windows viewing the same document.
    pub(crate) document_id: u64,
    /// Sequence number when multiple windows view the same file (e.g. 1 for Book1:1 / Doc:1, 2 for Book1:2 / Doc:2).
    pub(crate) window_number: Option<usize>,
    /// Whether the save confirmation dialog is open (shown when closing the last window with unsaved edits).
    pub(crate) save_confirm_dialog_open: bool,
    /// Whether side-by-side comparison mode (並べて比較 / XLSX-VIEW-013) is active.
    pub(crate) side_by_side: bool,
    /// Whether synchronous scrolling (同期スクロール / XLSX-VIEW-014) is active.
    pub(crate) sync_scrolling: bool,
    /// Paired window and editor entity for side-by-side mode.
    pub(crate) side_by_side_peer: Option<(AnyWindowHandle, Entity<SlideEditor>)>,
    /// Guard against recursive scroll syncing between paired windows.
    pub(crate) is_syncing_scroll: std::cell::Cell<bool>,
    /// Last scroll offset synced with peer window.
    pub(crate) last_synced_scroll: std::cell::Cell<Point<Pixels>>,
    /// Whether this document was opened from an AutoRecover snapshot (XLSX-FILE-015).
    pub(crate) is_recovered: bool,
    /// Path of the autorecover backup file associated with this open document (if any).
    pub(crate) active_autorecover_file: Option<std::path::PathBuf>,
    /// Whether this document was opened after repairing corruption (XLSX-FILE-020).
    pub(crate) is_repaired: bool,
    /// Summary text of repairs applied (XLSX-FILE-020).
    pub(crate) repair_summary: Option<String>,
    /// Detailed report of repairs applied (XLSX-FILE-020).
    pub(crate) repair_report: Option<ibis_pptx::repair::RepairReport>,
}

static NEXT_WORKBOOK_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
static NEXT_DOCUMENT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl SlideEditor {
    pub fn new(mut presentation: Presentation, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // NOW, TODAY and a date typed without its year read the local time.
        ibis_calc::eval::convert::set_clock(crate::clock::calc_clock);
        let resources = Resources::new(&presentation, cx);
        crate::docx::flow_layout::expand_pages(&mut presentation, &resources, None);
        crate::xlsx::pivot::fit_built_pivots(&mut presentation, &resources, &window.text_system().clone());
        let current_slide = presentation.slides[0].id;
        // Logarithmic, so equal distances on the slider are equal zoom ratios.
        let zoom_slider = cx.new(|_| {
            SliderState::new()
                .min(MIN_ZOOM * 100.0)
                .max(MAX_ZOOM * 100.0)
                .scale(SliderScale::Logarithmic)
                .default_value(100.0)
        });
        let zoom_subscription = cx.subscribe_in(&zoom_slider, window, |this, _, event: &SliderEvent, window, cx| {
            if let SliderEvent::Change(value) = event {
                this.set_zoom(Some(value.end() / 100.0), window, cx);
            }
        });
        let format = FormatState::for_model(&presentation, window, cx);
        let zoom_input = cx.new(|cx| InputState::new(window, cx));
        let zoom_input_subscription = cx.subscribe_in(&zoom_input, window, |this, input, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                // A number that cannot be read leaves the zoom as it is.
                if let Some(percent) = parse_percent(&input.read(cx).value()) {
                    this.set_zoom(Some(percent / 100.0), window, cx);
                }
                this.zoom_menu_open = false;
                this.focus_handle.focus(window, cx);
                cx.notify();
            }
        });
        let zoom_dialog_input = cx.new(|cx| InputState::new(window, cx));
        let zoom_dialog_subscription = cx.subscribe_in(&zoom_dialog_input, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.apply_zoom_dialog(window, cx);
            }
        });
        Self {
            focus_handle: cx.focus_handle(),
            list_focus: cx.focus_handle(),
            list_scroll: ScrollHandle::new(),
            wheel: 0.0,
            open_menu: None,
            menu_dismissed_at: None,
            #[cfg(target_family = "wasm")]
            wheel_at: f64::NEG_INFINITY,
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            wheel_notches: 0.0,
            document: Document::new(presentation),
            current_slide,
            selection: None,
            tool: Tool::Select,
            drag: None,
            editing: None,
            workspace_bounds: None,
            workspace_scroll: ScrollHandle::new(),
            zoom: None,
            zoom_slider,
            status_layout: Default::default(),
            _zoom_subscription: zoom_subscription,
            zoom_menu_open: false,
            zoom_input,
            _zoom_input_subscription: zoom_input_subscription,
            zoom_dialog_open: false,
            zoom_dialog_input,
            _zoom_dialog_subscription: zoom_dialog_subscription,
            slideshow: None,
            resources,
            picture_watch: None,
            file_name: None,
            status: None,
            chart_ranges: None,
            #[cfg(not(target_family = "wasm"))]
            recent_files: crate::shell::recent_files::load(),
            #[cfg(not(target_family = "wasm"))]
            protection_password: None,
            recent_colors: Vec::new(),
            last_colors: Default::default(),
            #[cfg(not(target_family = "wasm"))]
            recent_scroll: ScrollHandle::new(),
            has_document: false,
            closed: 0,
            loading: false,
            format,
            sidebar_open: true,
            symbol: None,
            outline_open: false,
            ruler_open: true,
            vertical_ruler_open: true,
            ruler_unit: RulerUnit::default(),
            gridlines_open: false,
            formatting_marks_open: false,
            show_white_space: true,
            doc_split: None,
            doc_split_scroll: ScrollHandle::new(),
            doc_split_dragging: false,
            doc_split_active: false,
            page_movement_side_to_side: false,
            doc_read_mode: false,
            doc_focus_mode: false,
            formatting_marks_options: FormattingMarksOptions::default(),
            default_paste_mode: DefaultPasteMode::default(),
            custom_user_name: None,
            custom_user_initials: None,
            update_links_before_printing: false,
            print_backgrounds: false,
            print_hidden_text: false,
            reviewer_filter: None,
            chrome_compact: false,
            source: None,
            master_view: None,
            notes_format: None,
            #[cfg(not(target_family = "wasm"))]
            path: None,
            workbook_id: NEXT_WORKBOOK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            document_id: NEXT_DOCUMENT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            window_number: None,
            save_confirm_dialog_open: false,
            side_by_side: false,
            sync_scrolling: false,
            side_by_side_peer: None,
            is_syncing_scroll: std::cell::Cell::new(false),
            last_synced_scroll: std::cell::Cell::new(Point::default()),
            is_recovered: false,
            active_autorecover_file: None,
            is_repaired: false,
            repair_summary: None,
            repair_report: None,
        }
    }

    /// Replace the open presentation, dropping undo history and selection.
    pub(crate) fn load(&mut self, mut presentation: Presentation, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        self.end_slideshow(window, cx);
        self.zoom_menu_open = false;
        self.editing = None;
        self.drag = None;
        self.selection = None;
        self.tool = Tool::Select;
        self.symbol = None;
        let timing = crate::clock::debug_timing::start();
        self.resources = Resources::new(&presentation, cx);
        self.watch_pictures(window, cx);
        crate::clock::debug_timing::report("load: resources", timing);
        let timing = crate::clock::debug_timing::start();
        crate::docx::flow_layout::update_fields_on_open(&mut presentation, &self.resources);
        crate::clock::debug_timing::report("load: update_fields_on_open", timing);
        let timing = crate::clock::debug_timing::start();
        crate::docx::flow_layout::expand_pages(&mut presentation, &self.resources, Some(&name));
        crate::clock::debug_timing::report("load: expand_pages", timing);
        crate::xlsx::pivot::fit_built_pivots(&mut presentation, &self.resources, &window.text_system().clone());
        // A presentation's date fields show today, as PowerPoint's do.
        if presentation.flow.is_none() && presentation.workbook.is_none() {
            presentation.refresh_dates(&crate::clock::local_now(), &ibis_pptx::script_pieces);
        }
        self.current_slide = presentation.slides[0].id;
        self.format = FormatState::for_model(&presentation, window, cx);
        // A workbook's and a presentation's right pane is there when asked
        // for (a workbook's tabs have no panel to show until one is chosen;
        // either would take the room of the sheet or the slide, which is
        // fitted to the width without it).
        self.sidebar_open = self.format.kind() == ibis_pptx::Kind::Document;
        let is_reading = presentation.flow.as_ref().is_some_and(|f| f.view_mode == ibis_doc_model::DocumentViewMode::Reading);
        let is_outline = presentation.flow.as_ref().is_some_and(|f| f.view_mode == ibis_doc_model::DocumentViewMode::Outline);
        self.document = Document::new(presentation);
        self.source = None;
        #[cfg(not(target_family = "wasm"))]
        {
            self.path = None;
            self.protection_password = None;
        }
        self.zoom = with_format!(self, initial_zoom(self));
        self.workspace_scroll.set_offset(Point::default());
        self.doc_split = None;
        self.doc_split_scroll.set_offset(Point::default());
        self.doc_split_dragging = false;
        self.doc_split_active = false;
        self.page_movement_side_to_side = false;
        self.doc_read_mode = is_reading;
        if is_outline {
            self.outline_open = true;
        }
        self.doc_focus_mode = false;
        window.set_window_title(&format!("{name} - ibisOffice"));
        self.file_name = Some(name);
        self.has_document = true;
        self.loading = false;
        self.workbook_id = NEXT_WORKBOOK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.document_id = NEXT_DOCUMENT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.window_number = None;
        self.save_confirm_dialog_open = false;
        // A file with macros (.docm, .xlsm, .pptm) opens and saves with
        // them as they are; they never run.
        self.status = self
            .file_name
            .as_ref()
            .and_then(|n| n.rsplit_once('.'))
            .filter(|(_, ext)| ibis_pptx::Kind::has_macros(ext))
            .map(|_| crate::shell::i18n::text("_shell.macros-in-this-file-will-not-run-they-are-preser"));
        // A presentation opens with the keys on the slide list (Home, End,
        // the arrows move between slides without a click first).
        if self.format.kind() == ibis_pptx::Kind::Presentation {
            self.list_focus.focus(window, cx);
        } else {
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    /// Redraw as the pictures decode, until all are.
    fn watch_pictures(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let preload = self.resources.preload.clone();
        self.picture_watch = Some(cx.spawn_in(window, async move |this, cx| {
            let mut seen = preload.generation();
            let mut slide_ready = false;
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(PICTURE_POLL_MS)).await;
                let finished = preload.finished();
                let generation = preload.generation();
                let changed = generation != seen;
                seen = generation;
                let alive = this.update(cx, |this, cx| {
                    // The empty document shown while a file loads is not timed.
                    if !slide_ready && !this.loading && this.resources.preload.slide_ready(this.slide()) {
                        slide_ready = true;
                        #[cfg(not(target_family = "wasm"))]
                        crate::timing::mark("slide-pictures");
                    }
                    if changed {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
                if finished {
                    #[cfg(not(target_family = "wasm"))]
                    if slide_ready {
                        crate::timing::mark("all-pictures");
                    }
                    break;
                }
            }
        }));
    }

    /// Ask for a .pptx, .xlsx or .docx file and open it.
    pub(crate) fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_shell.open").into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update_in(cx, |this, window, cx| this.open_path(path, None, window, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        cx.spawn_in(window, async move |this, cx| {
            let (name, data) = match crate::web::pick_file().await {
                Ok(Some(file)) => file,
                Ok(None) => return,
                Err(err) => {
                    this.update(cx, |this, cx| this.set_status(crate::shell::i18n::format("file.pick-failed", &[("error", &err)]), cx)).ok();
                    return;
                }
            };
            let name: SharedString = name.into();
            let closed = this.update(cx, |this, _| this.closed).unwrap_or_default();
            let kind = name.rsplit_once('.').and_then(|(_, e)| ibis_pptx::Kind::from_extension(e));
            this.update_in(cx, |this, window, cx| match kind {
                Some(kind) if !this.has_document => this.begin_loading(kind, name.clone(), window, cx),
                _ => this.set_status(crate::shell::i18n::format("file.opening", &[("name", &name)]), cx),
            })
            .ok();
            // Let the empty document show before reading the file, which
            // holds the page while it runs.
            cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
            let result = ibis_pptx::open(data).map_err(|e| e.to_string());
            this.update_in(cx, |this, window, cx| match result {
                // Closed (ファイル > 閉じる) while it was read.
                _ if this.closed != closed => {}
                Ok(opened) => {
                    let is_ppsx = name.rsplit_once('.').is_some_and(|(_, e)| ibis_pptx::is_slideshow(e));
                    let is_tmpl = name.rsplit_once('.').is_some_and(|(_, e)| ibis_pptx::is_template(e));
                    let doc_name = if is_tmpl {
                        crate::shell::file_actions::new_file_name(ibis_pptx::Kind::of(&opened.presentation))
                    } else {
                        name
                    };
                    this.load(opened.presentation, doc_name, window, cx);
                    this.source = Some(std::sync::Arc::new(opened.source));
                    if is_ppsx {
                        this.start_slideshow(window, cx);
                    }
                }
                Err(err) => this.fail_loading(crate::shell::i18n::format("file.open-failed", &[("name", &name), ("error", &err)]), cx),
            })
            .ok();
        })
        .detach();
    }

    /// Open the .pptx, .xlsx or .docx at `path` and put it first in the
    /// recent files, then show slide `slide` (1-based) if given. A window
    /// with no document shows an empty one of the file's kind while it
    /// loads (`begin_loading`).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn open_path(&mut self, path: std::path::PathBuf, slide: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let read_path = path.clone();
        self.open_read(path, move || crate::read_file(&read_path), slide, window, cx);
    }

    /// Open `path` as `open_path` does, with `read` reading it (on a
    /// background thread; it may wait for a read started earlier).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn open_read(
        &mut self,
        path: std::path::PathBuf,
        read: impl FnOnce() -> Result<crate::Loaded, String> + Send + 'static,
        slide: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_edit(window, cx);
        let name: SharedString = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into();
        let kind = path.extension().and_then(|e| e.to_str()).and_then(ibis_pptx::Kind::from_extension);
        match kind {
            Some(kind) if !self.has_document => self.begin_loading(kind, name.clone(), window, cx),
            _ => self.set_status(crate::shell::i18n::format("file.opening", &[("name", &name)]), cx),
        }
        let closed = self.closed;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move { read() }).await;
            this.update_in(cx, |this, window, cx| match result {
                // Closed (ファイル > 閉じる) while it was read.
                _ if this.closed != closed => {}
                Ok(loaded) => {
                    let is_tmpl = path.extension().and_then(|e| e.to_str()).is_some_and(ibis_pptx::is_template);
                    if is_tmpl {
                        let tmpl_name = crate::shell::file_actions::new_file_name(ibis_pptx::Kind::of(&loaded.presentation));
                        this.load(loaded.presentation, tmpl_name, window, cx);
                        this.source = Some(loaded.source);
                        this.path = None;
                    } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xls")) {
                        // An old workbook is read as a new .xlsx: saving asks where, and leaves the .xls as it is.
                        let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        this.load(loaded.presentation, format!("{stem}.xlsx").into(), window, cx);
                        this.source = Some(loaded.source);
                        this.path = None;
                    } else {
                        this.load(loaded.presentation, name, window, cx);
                        this.source = Some(loaded.source);
                        this.path = Some(path.clone());
                        this.protection_password = loaded.password;
                    }
                    if let Some(number) = slide {
                        this.show_slide_number(number, window, cx);
                    }
                    this.recent_files = crate::shell::recent_files::add(&path);
                    crate::timing::document_loaded(window);
                    if path.extension().and_then(|e| e.to_str()).is_some_and(ibis_pptx::is_slideshow) {
                        this.start_slideshow(window, cx);
                    }
                    // A presentation with a password to modify: asked for, or read as it is.
                    if this.is_pptx_modify_locked() {
                        this.ask_modify_password_to_unlock(window, cx);
                    }
                }
                // A protected file: its password is asked for.
                Err(err) if err == crate::NEEDS_PASSWORD || err == crate::WRONG_PASSWORD => {
                    this.fail_loading(String::new(), cx);
                    crate::shell::password_window::ask_to_open(cx.entity().downgrade(), path.clone(), slide, err == crate::WRONG_PASSWORD, window, cx);
                }
                Err(err) => this.fail_loading(crate::shell::i18n::format("file.open-failed", &[("name", &name), ("error", &err)]), cx),
            })
            .ok();
        })
        .detach();
    }

    /// Open protected file `path` with `password` (`open_path` for a file that asked for it).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn open_path_with_password(&mut self, path: std::path::PathBuf, password: String, slide: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let read_path = path.clone();
        self.open_read(path, move || crate::read_file_with(&read_path, Some(&password)), slide, window, cx);
    }

    /// Show an empty document of `kind`, titled `name`, with 「読み込み中...」
    /// over it until the file is loaded (`load`) or cannot be
    /// (`fail_loading`).
    pub(crate) fn begin_loading(&mut self, kind: ibis_pptx::Kind, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        self.load(ibis_pptx::blank(kind), name, window, cx);
        self.loading = true;
    }

    /// The file could not be opened: back to the start view if an empty
    /// document stood in for it, with `status` saying why.
    pub(crate) fn fail_loading(&mut self, status: String, cx: &mut Context<Self>) {
        if self.loading {
            self.loading = false;
            self.has_document = false;
            self.file_name = None;
        }
        self.set_status(status, cx);
    }

    /// ファイル > 閉じる: close the open file and show the start view
    /// (スタート画面) in the window, as Office does.
    pub(crate) fn close_file(&mut self, _: &CloseFile, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_document {
            return;
        }
        if self.is_workbook() && self.workbook_id != 0 {
            let my_id = self.workbook_id;
            let my_entity_id = cx.entity_id();
            let other_matching = Self::find_other_matching_windows(my_id, my_entity_id, cx);
            if !other_matching.is_empty() {
                if self.side_by_side {
                    if let Some((_, peer_entity)) = self.side_by_side_peer.take() {
                        let _ = peer_entity.update(cx, |peer, cx| {
                            peer.side_by_side = false;
                            peer.sync_scrolling = false;
                            peer.side_by_side_peer = None;
                            cx.notify();
                        });
                    }
                    self.side_by_side = false;
                    self.sync_scrolling = false;
                }
                self.workbook_id = 0;
                self.window_number = None;
                window.remove_window();
                Self::renumber_workbook_windows(my_id, cx);
                return;
            } else if self.document.can_undo() {
                self.save_confirm_dialog_open = true;
                cx.notify();
                return;
            }
        } else if self.is_document() && self.document_id != 0 {
            let my_id = self.document_id;
            let my_entity_id = cx.entity_id();
            let other_matching = Self::find_other_matching_document_windows(my_id, my_entity_id, cx);
            if !other_matching.is_empty() {
                self.document_id = 0;
                self.window_number = None;
                window.remove_window();
                Self::renumber_document_windows(my_id, cx);
                return;
            } else if self.document.can_undo() {
                self.save_confirm_dialog_open = true;
                cx.notify();
                return;
            }
        }
        self.force_close_file(window, cx);
    }

    pub(crate) fn force_close_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.side_by_side {
            if let Some((_, peer_entity)) = self.side_by_side_peer.take() {
                let _ = peer_entity.update(cx, |peer, cx| {
                    peer.side_by_side = false;
                    peer.sync_scrolling = false;
                    peer.side_by_side_peer = None;
                    cx.notify();
                });
            }
            self.side_by_side = false;
            self.sync_scrolling = false;
        }
        self.save_confirm_dialog_open = false;
        self.end_slideshow(window, cx);
        self.editing = None;
        if self.is_workbook() || self.is_presentation() {
            self.cleanup_active_autorecover_file();
            self.is_recovered = false;
            self.is_repaired = false;
            self.repair_summary = None;
            self.repair_report = None;
        }
        // An empty presentation, as a new window has, with the state of the
        // file reset (`load`) and then the start view.
        self.load(Presentation::new(), SharedString::default(), window, cx);
        self.list_scroll.set_offset(Point::default());
        self.has_document = false;
        self.closed += 1;
        self.file_name = None;
        self.workbook_id = 0;
        self.document_id = 0;
        self.window_number = None;
        window.set_window_title(&crate::shell::start_view::start_title());
        // The file just closed is first, as other windows may have opened
        // files since this one's list was read.
        #[cfg(not(target_family = "wasm"))]
        {
            self.recent_files = crate::shell::recent_files::load();
            self.recent_scroll.set_offset(Point::default());
        }
    }

    /// 「読み込み中...」in the middle of the workspace while the file loads.
    pub(crate) fn render_loading(&self, cx: &Context<Self>) -> Option<Div> {
        if !self.loading {
            return None;
        }
        let theme = cx.theme();
        Some(
            div().absolute().inset_0().flex().items_center().justify_center().child(
                div()
                    .px_4()
                    .py_2()
                    .rounded_md()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_md()
                    .text_color(theme.muted_foreground)
                    .child(crate::shell::i18n::text("_shell.loading")),
            ),
        )
    }

    pub(crate) fn set_status(&mut self, status: String, cx: &mut Context<Self>) {
        self.status = Some(status.into());
        cx.notify();
    }

    // ----- Reading state -------------------------------------------------

    pub(crate) fn presentation(&self) -> &Presentation {
        self.document.presentation()
    }

    pub(crate) fn slide(&self) -> &Slide {
        self.presentation()
            .slide(self.current_slide)
            .unwrap_or(&self.presentation().slides[0])
    }

    pub(crate) fn selected_shape(&self) -> Option<&Shape> {
        self.selection.and_then(|id| self.slide().shape(id))
    }

    /// The worksheet of the current page and the shape that holds it.
    pub(crate) fn current_sheet(&self) -> Option<(&Shape, &std::sync::Arc<Sheet>)> {
        sheet_of(self.slide())
    }

    /// The selected cell of the current worksheet: (column, row) of the cell
    /// or of the top-left cell of its merged range.
    pub(crate) fn current_cell(&self) -> Option<(u32, u32)> {
        let (_, sheet) = self.current_sheet()?;
        let (column, row) = self.sheet_state()?.cells.get(&self.current_slide).copied().unwrap_or((0, 0));
        Some(sheet.merge_at(row, column).map_or((column, row), |m| (m.first_column, m.first_row)))
    }

    /// The cells the selection covers: from the selected cell to the other
    /// corner of the range, grown to take in whole merged ranges.
    pub(crate) fn selected_range(&self) -> Option<CellRange> {
        let (_, sheet) = self.current_sheet()?;
        let (column, row) = self.current_cell()?;
        let (end_column, end_row) = self.sheet_state()?.cell_ends.get(&self.current_slide).copied().unwrap_or((column, row));
        let mut range = CellRange {
            first_row: row.min(end_row),
            first_column: column.min(end_column),
            last_row: row.max(end_row),
            last_column: column.max(end_column),
        };
        // Merged ranges it cuts into are taken in whole, until none is left.
        loop {
            let grown = sheet.merges.iter().fold(range, |r, m| {
                let meets = m.first_row <= r.last_row && m.last_row >= r.first_row && m.first_column <= r.last_column && m.last_column >= r.first_column;
                if !meets {
                    return r;
                }
                CellRange {
                    first_row: r.first_row.min(m.first_row),
                    first_column: r.first_column.min(m.first_column),
                    last_row: r.last_row.max(m.last_row),
                    last_column: r.last_column.max(m.last_column),
                }
            });
            if grown == range {
                return Some(range);
            }
            range = grown;
        }
    }

    /// The scrolled content of the workspace at the current zoom, as the
    /// open document's format lays it out (`FormatEditor::workspace_content`).
    pub(crate) fn workspace_content(&self) -> Option<WorkspaceContent> {
        let ws = self.workspace_bounds?;
        Some(with_format!(self, workspace_content(self, ws.size)))
    }

    /// A page `page` big in a workspace `view` big: fitted into it (less the
    /// space `before` and `after` it on each axis, in pixels), or at the
    /// chosen zoom; in the top-left corner after `before` (`top_left`), or
    /// centred while it fits.
    pub(crate) fn page_content(&self, view: Size<Pixels>, page: ibis_doc_model::EmuSize, before: Point<f32>, after: Point<f32>, top_left: bool) -> WorkspaceContent {
        let (view_w, view_h) = (f32::from(view.width), f32::from(view.height));
        let scale = match self.zoom {
            Some(zoom) => zoom * self.px_per_inch_at_100() / ibis_doc_model::EMU_PER_INCH as f32,
            None => {
                let avail_w = (view_w - before.x - after.x).max(40.0);
                let avail_h = (view_h - before.y - after.y).max(40.0);
                (avail_w / page.width.max(1) as f32).min(avail_h / page.height.max(1) as f32)
            }
        };
        let slide_w = page.width as f32 * scale;
        let slide_h = page.height as f32 * scale;
        let content_w = view_w.max(slide_w + before.x + after.x);
        let content_h = view_h.max(slide_h + before.y + after.y);
        let slide_origin = if top_left {
            point(px(before.x), px(before.y))
        } else {
            point(px((content_w - slide_w) / 2.0), px((content_h - slide_h) / 2.0))
        };
        WorkspaceContent { size: size(px(content_w), px(content_h)), slide_origin, scale }
    }

    /// The margin kept around a slide: little when it is fitted.
    pub(crate) fn slide_margin(&self) -> f32 {
        if self.zoom.is_none() { FIT_MARGIN } else { WORKSPACE_MARGIN }
    }

    /// Where the slide sits in the window, as the workspace is scrolled.
    pub(crate) fn slide_layout(&self) -> Option<SlideLayout> {
        let ws = self.workspace_bounds?;
        let content = self.workspace_content()?;
        let origin = ws.origin + self.workspace_scroll.offset() + content.slide_origin;
        Some(SlideLayout { origin, scale: content.scale })
    }

    /// Pixels per inch at 100%: a slide's, or a worksheet's (the screen's size
    /// only: a PDF and a print keep the real size).
    pub(crate) fn px_per_inch_at_100(&self) -> f32 {
        if self.is_workbook() { SHEET_PX_PER_INCH_AT_100 } else { PX_PER_INCH_AT_100 }
    }

    /// The zoom shown, 1.0 being 100%.
    pub(crate) fn current_zoom(&self) -> f32 {
        self.zoom.or_else(|| self.slide_layout().map(|l| l.zoom(self.px_per_inch_at_100()))).unwrap_or(1.0)
    }

    /// Zoom to `zoom` (clamped to the range), or fit the slide with `None`.
    /// The point of the slide in the middle of the workspace stays there.
    pub(crate) fn set_zoom(&mut self, zoom: Option<f32>, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom_at(zoom, None, window, cx);
    }

    /// Zoom as `set_zoom` does, keeping the point of the slide at `anchor`
    /// (from the workspace's top-left corner) where it is, or the one in the
    /// middle of the workspace with `None`.
    pub(crate) fn set_zoom_at(&mut self, zoom: Option<f32>, anchor: Option<Point<Pixels>>, _: &mut Window, cx: &mut Context<Self>) {
        let zoom = zoom.map(|z| z.clamp(MIN_ZOOM, MAX_ZOOM));
        let before = self.workspace_content();
        self.zoom = zoom;
        let offset = match (zoom, before, self.workspace_content(), self.workspace_bounds) {
            (Some(_), Some(before), Some(after), Some(ws)) => {
                let old = self.workspace_scroll.offset();
                let anchor = anchor.unwrap_or(point(ws.size.width / 2.0, ws.size.height / 2.0));
                let ratio = after.scale / before.scale;
                let axis = |anchor: Pixels, view: Pixels, old: Pixels, before_origin: Pixels, after_origin: Pixels, content: Pixels| {
                    let f = f32::from;
                    px(zoom_axis_offset(f(anchor), f(view), f(old), f(before_origin), f(after_origin), ratio, f(content)))
                };
                let y = with_format!(self, zoom_scroll_y(self, &before, &after, old.y, ws.size.height, anchor.y))
                    .unwrap_or_else(|| axis(anchor.y, ws.size.height, old.y, before.slide_origin.y, after.slide_origin.y, after.size.height));
                point(axis(anchor.x, ws.size.width, old.x, before.slide_origin.x, after.slide_origin.x, after.size.width), y)
            }
            // Fitting a document keeps its place: only the sideways
            // scrolling goes, as the pages then fit across.
            (None, Some(before), Some(after), Some(ws)) => {
                let old = self.workspace_scroll.offset().y;
                match with_format!(self, zoom_scroll_y(self, &before, &after, old, ws.size.height, ws.size.height / 2.0)) {
                    Some(y) => point(px(0.0), y),
                    None => Point::default(),
                }
            }
            _ => Point::default(),
        };
        self.workspace_scroll.set_offset(offset);
        if self.sheet_state().is_some() {
            if let Some((_, sheet)) = self.current_sheet() {
                let percent = zoom.map(|z| (z * 100.0).round() as u16).filter(|&z| z != 100);
                if sheet.zoom != percent {
                    let slide = self.current_slide;
                    let _ = self.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetZoom { slide, zoom: percent })]);
                }
            }
        }
        cx.notify();
    }

    /// Excel: Zoom to selection (表示 > 選択範囲に合わせて拡大/縮小).
    /// Fits the currently selected cell range into the workspace.
    pub(crate) fn zoom_to_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some((_, sheet)), Some(range)) = (self.current_sheet(), self.selected_range()) else { return };
        let xs = sheet.column_offsets();
        let ys = sheet.row_offsets();
        let first_col = range.first_column as usize;
        let last_col = (range.last_column + 1) as usize;
        let first_row = range.first_row as usize;
        let last_row = (range.last_row + 1) as usize;
        let width_emu = if last_col < xs.len() && first_col < xs.len() {
            xs[last_col] - xs[first_col]
        } else if first_col < xs.len() {
            *xs.last().unwrap_or(&0) - xs[first_col]
        } else {
            0
        };
        let height_emu = if last_row < ys.len() && first_row < ys.len() {
            ys[last_row] - ys[first_row]
        } else if first_row < ys.len() {
            *ys.last().unwrap_or(&0) - ys[first_row]
        } else {
            0
        };
        if width_emu <= 0 || height_emu <= 0 {
            return;
        }

        // Available area inside workspace (excluding row/column headers and margins)
        let avail_w = self.workspace_bounds.map_or(800.0, |ws| f32::from(ws.size.width) - 60.0).max(100.0);
        let avail_h = self.workspace_bounds.map_or(600.0, |ws| f32::from(ws.size.height) - 50.0).max(100.0);
        let scale_100 = self.px_per_inch_at_100() / ibis_doc_model::EMU_PER_INCH as f32;
        let px_at_100_w = width_emu as f32 * scale_100;
        let px_at_100_h = height_emu as f32 * scale_100;
        let zoom_w = avail_w / px_at_100_w;
        let zoom_h = avail_h / px_at_100_h;
        let zoom = zoom_w.min(zoom_h).clamp(MIN_ZOOM, MAX_ZOOM);

        self.set_zoom(Some(zoom), window, cx);
        self.scroll_range_into_view(true);
        cx.notify();
    }

    /// The wheel over the workspace with the zoom modifier held (see
    /// `wheel_zooms`) zooms about the pointer: in with the wheel turned up
    /// (away from you), a notch by `WHEEL_ZOOM_STEP`, a trackpad smoothly.
    /// Whether the wheel was taken for zooming.
    pub(crate) fn on_wheel_zoom(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !wheel_zooms(&event.modifiers, zoom_modifier_is_mac()) || !self.has_document {
            return false;
        }
        let Some(ws) = self.workspace_bounds else { return false };
        let factor = match event.delta {
            // A notch at a time: some systems send several lines for one.
            ScrollDelta::Lines(delta) if delta.y > 0.0 => WHEEL_ZOOM_STEP,
            ScrollDelta::Lines(delta) if delta.y < 0.0 => 1.0 / WHEEL_ZOOM_STEP,
            ScrollDelta::Lines(_) => 1.0,
            ScrollDelta::Pixels(delta) => 2f32.powf(f32::from(delta.y) / WHEEL_ZOOM_DOUBLING),
        };
        if factor != 1.0 {
            let anchor = event.position - ws.origin;
            self.set_zoom_at(Some(self.current_zoom() * factor), Some(anchor), window, cx);
        }
        true
    }

    /// Zoom in (`grow`) or out by one step of the status bar's − / + buttons.
    pub(crate) fn step_zoom(&mut self, grow: bool, window: &mut Window, cx: &mut Context<Self>) {
        let zoom = self.current_zoom();
        let zoom = if grow { zoom * ZOOM_STEP } else { zoom / ZOOM_STEP };
        self.set_zoom(Some(zoom), window, cx);
    }

    /// Open the Zoom dialog.
    pub(crate) fn open_zoom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let percent = (self.current_zoom() * 100.0).round() as u32;
        self.zoom_dialog_input.update(cx, |input, cx| input.set_value(percent.to_string(), window, cx));
        self.zoom_dialog_open = true;
        self.zoom_dialog_input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// Close the Zoom dialog.
    pub(crate) fn close_zoom_dialog(&mut self, cx: &mut Context<Self>) {
        self.zoom_dialog_open = false;
        cx.notify();
    }

    /// Apply the zoom specified in the zoom dialog.
    pub(crate) fn apply_zoom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.zoom_dialog_input.read(cx).value().to_string();
        if let Some(percent) = parse_percent(&text) {
            let clamped = (percent / 100.0).clamp(MIN_ZOOM, MAX_ZOOM);
            self.set_zoom(Some(clamped), window, cx);
        }
        self.zoom_dialog_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Set zoom to fit to window from the zoom dialog.
    pub(crate) fn fit_zoom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(None, window, cx);
        self.zoom_dialog_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Render the Zoom modal dialog.
    pub(crate) fn render_zoom_dialog(&self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let input = self.zoom_dialog_input.clone();
        let current_val = self.zoom_dialog_input.read(cx).value().to_string();
        let is_fit = self.zoom.is_none();

        let presets = [400, 200, 100, 66, 50, 33];

        let preset_buttons = presets.into_iter().map(|pct| {
            let selected = !is_fit && current_val.trim() == pct.to_string();
            div()
                .id(("zoom-preset", pct as usize))
                .px_3()
                .py_1p5()
                .rounded_sm()
                .border_1()
                .border_color(if selected { theme.primary } else { theme.border })
                .bg(if selected { theme.accent } else { theme.background })
                .cursor_pointer()
                .text_sm()
                .text_color(if selected { theme.primary } else { theme.foreground })
                .child(format!("{pct}%"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.zoom_dialog_input.update(cx, |inp, cx| inp.set_value(pct.to_string(), window, cx));
                    cx.notify();
                }))
        });

        div()
            .id("zoom-dialog-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000066))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                v_flex()
                    .id("zoom-dialog")
                    .w(px(380.))
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .shadow_xl()
                    .p_5()
                    .gap_4()
                    // 1. ヘッダー
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(div().font_weight(FontWeight::BOLD).text_base().child(crate::shell::i18n::text("zoom.dialog.title")))
                            .child(
                                div()
                                    .id("zoom-dialog-close")
                                    .cursor_pointer()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .hover(|s| s.text_color(theme.foreground))
                                    .child("✕")
                                    .on_click(cx.listener(|this, _, _, cx| this.close_zoom_dialog(cx))),
                            ),
                    )
                    // 2. プリセット
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.muted_foreground)
                                    .child(crate::shell::i18n::text("zoom.dialog.presets")),
                            )
                            .child(
                                h_flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .children(preset_buttons)
                                    .child(
                                        div()
                                            .id("zoom-preset-fit")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(if is_fit { theme.primary } else { theme.border })
                                            .bg(if is_fit { theme.accent } else { theme.background })
                                            .cursor_pointer()
                                            .text_sm()
                                            .text_color(if is_fit { theme.primary } else { theme.foreground })
                                            .child(crate::shell::i18n::text("zoom.dialog.fit"))
                                            .on_click(cx.listener(|this, _, window, cx| this.fit_zoom_dialog(window, cx))),
                                    ),
                            ),
                    )
                    // 3. 倍率入力
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(div().text_sm().child(crate::shell::i18n::text("zoom.dialog.percent")))
                            .child(Input::new(&input).w(px(80.)))
                            .child(div().text_sm().child("%")),
                    )
                    // 4. ボタン群
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("zoom-dialog-cancel")
                                    .label(crate::shell::i18n::text("_shell.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| this.close_zoom_dialog(cx))),
                            )
                            .child(
                                Button::new("zoom-dialog-ok")
                                    .primary()
                                    .label(crate::shell::i18n::text("OK"))
                                    .on_click(cx.listener(|this, _, window, cx| this.apply_zoom_dialog(window, cx))),
                            ),
                    ),
            )
    }

    /// Open a new window duplicating current presentation or open path.
    pub(crate) fn new_window(&mut self, cx: &mut App) {
        if self.is_workbook() {
            let wb_id = if self.workbook_id == 0 {
                let id = NEXT_WORKBOOK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.workbook_id = id;
                id
            } else {
                self.workbook_id
            };
            let pres = self.presentation().clone();
            let doc = self.document.clone();
            let name = self.file_name.clone().unwrap_or_else(|| self.workbook_display_name().into());
            let path = self.file_path();
            let source = self.source.clone();
            let current = self.current_slide;
            let current_zoom = self.zoom;
            let existing_count = Self::count_matching_windows(wb_id, cx) + 1;
            if self.window_number.is_none() {
                self.window_number = Some(1);
            }
            let new_num = existing_count + 1;
            crate::open_window_with(cx, move |editor, window, cx| {
                editor.load(pres, name.clone(), window, cx);
                editor.document = doc;
                editor.set_file_path(path);
                editor.source = source;
                editor.workbook_id = wb_id;
                editor.window_number = Some(new_num);
                editor.current_slide = current;
                editor.zoom = current_zoom;
                let title = format!("{name}:{new_num}");
                window.set_window_title(&format!("{title} - ibisOffice"));
            });
        } else if self.is_document() {
            let doc_id = if self.document_id == 0 {
                let id = NEXT_DOCUMENT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.document_id = id;
                id
            } else {
                self.document_id
            };
            let pres = self.presentation().clone();
            let doc = self.document.clone();
            let name = self.file_name.clone().unwrap_or_else(|| crate::shell::i18n::text("_shell.document-1-docx").into());
            let path = self.file_path();
            let source = self.source.clone();
            let current = self.current_slide;
            let current_zoom = self.zoom;
            let existing_count = Self::count_matching_document_windows(doc_id, cx) + 1;
            if self.window_number.is_none() {
                self.window_number = Some(1);
            }
            let new_num = existing_count + 1;
            crate::open_window_with(cx, move |editor, window, cx| {
                editor.load(pres, name.clone(), window, cx);
                editor.document = doc;
                editor.set_file_path(path);
                editor.source = source;
                editor.document_id = doc_id;
                editor.window_number = Some(new_num);
                editor.current_slide = current;
                editor.zoom = current_zoom;
                editor.relayout_if_changed();
                let title = format!("{name}:{new_num}");
                window.set_window_title(&format!("{title} - ibisOffice"));
            });
        } else if let Some(path) = self.file_path() {
            let slide = self.presentation().slide_index(self.current_slide).map(|i| i + 1);
            crate::open_window_with(cx, move |editor, window, cx| {
                #[cfg(not(target_family = "wasm"))]
                editor.open_path(path, slide, window, cx);
                #[cfg(target_family = "wasm")]
                let _ = (editor, path, slide, window, cx);
            });
        } else {
            let pres = self.presentation().clone();
            let name = self.file_name.clone().unwrap_or_else(|| crate::shell::i18n::text("file.new.presentation").into());
            let current = self.current_slide;
            crate::open_window_with(cx, move |editor, window, cx| {
                editor.load(pres, name, window, cx);
                editor.current_slide = current;
            });
        }
    }

    /// The file's path (always `None` in the browser).
    pub(crate) fn file_path(&self) -> Option<std::path::PathBuf> {
        #[cfg(not(target_family = "wasm"))]
        {
            self.path.clone()
        }
        #[cfg(target_family = "wasm")]
        {
            None
        }
    }

    /// Remember the file's path (nothing in the browser).
    pub(crate) fn set_file_path(&mut self, path: Option<std::path::PathBuf>) {
        #[cfg(not(target_family = "wasm"))]
        {
            self.path = path;
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = path;
        }
    }

    pub(crate) fn find_other_matching_windows(
        workbook_id: u64,
        my_entity_id: gpui::EntityId,
        cx: &mut App,
    ) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        if workbook_id == 0 {
            return Vec::new();
        }
        let mut matching = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some(entity)) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.entity_id() != my_entity_id
                            && entity.read(app).is_workbook()
                            && entity.read(app).workbook_id == workbook_id
                        {
                            return Some(entity);
                        }
                    }
                    None
                }) {
                    matching.push((w, entity));
                }
            }
        }
        matching
    }

    pub(crate) fn find_other_matching_document_windows(
        document_id: u64,
        my_entity_id: gpui::EntityId,
        cx: &mut App,
    ) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        if document_id == 0 {
            return Vec::new();
        }
        let mut matching = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some(entity)) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.entity_id() != my_entity_id
                            && entity.read(app).is_document()
                            && entity.read(app).document_id == document_id
                        {
                            return Some(entity);
                        }
                    }
                    None
                }) {
                    matching.push((w, entity));
                }
            }
        }
        matching
    }

    pub(crate) fn find_matching_windows(workbook_id: u64, cx: &mut App) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        if workbook_id == 0 {
            return Vec::new();
        }
        let mut matching = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some(entity)) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.read(app).is_workbook() && entity.read(app).workbook_id == workbook_id {
                            return Some(entity);
                        }
                    }
                    None
                }) {
                    matching.push((w, entity));
                }
            }
        }
        matching
    }

    pub(crate) fn find_matching_document_windows(document_id: u64, cx: &mut App) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        if document_id == 0 {
            return Vec::new();
        }
        let mut matching = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some(entity)) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.read(app).is_document() && entity.read(app).document_id == document_id {
                            return Some(entity);
                        }
                    }
                    None
                }) {
                    matching.push((w, entity));
                }
            }
        }
        matching
    }

    pub(crate) fn count_matching_windows(workbook_id: u64, cx: &mut App) -> usize {
        Self::find_matching_windows(workbook_id, cx).len()
    }

    pub(crate) fn count_matching_document_windows(document_id: u64, cx: &mut App) -> usize {
        Self::find_matching_document_windows(document_id, cx).len()
    }

    pub(crate) fn renumber_workbook_windows(workbook_id: u64, cx: &mut App) {
        if workbook_id == 0 {
            return;
        }
        let matching = Self::find_matching_windows(workbook_id, cx);
        let count = matching.len();
        for (i, (w, editor_entity)) in matching.into_iter().enumerate() {
            let num = if count > 1 { Some(i + 1) } else { None };
            let _ = w.update(cx, |_, window, app| {
                editor_entity.update(app, |editor, cx| {
                    editor.window_number = num;
                    let title = editor.display_title_formatted();
                    window.set_window_title(&format!("{title} - ibisOffice"));
                    cx.notify();
                    window.refresh();
                });
            });
        }
    }

    pub(crate) fn renumber_document_windows(document_id: u64, cx: &mut App) {
        if document_id == 0 {
            return;
        }
        let matching = Self::find_matching_document_windows(document_id, cx);
        let count = matching.len();
        for (i, (w, editor_entity)) in matching.into_iter().enumerate() {
            let num = if count > 1 { Some(i + 1) } else { None };
            let _ = w.update(cx, |_, window, app| {
                editor_entity.update(app, |editor, cx| {
                    editor.window_number = num;
                    let title = editor.display_title_formatted();
                    window.set_window_title(&format!("{title} - ibisOffice"));
                    cx.notify();
                    window.refresh();
                });
            });
        }
    }

    pub(crate) fn display_title_formatted(&self) -> String {
        let base = self.file_name.as_ref().map(|s| s.to_string()).unwrap_or_else(|| {
            if self.is_workbook() {
                self.workbook_display_name()
            } else if self.is_document() {
                crate::shell::i18n::text("_shell.document-1-docx").to_string()
            } else {
                crate::shell::i18n::text("_shell.presentation").to_string()
            }
        });
        if self.is_workbook() || self.is_document() {
            if let Some(num) = self.window_number {
                return format!("{base}:{num}");
            }
        }
        base
    }

    pub(crate) fn sync_workbook_peers(&self, cx: &mut Context<Self>) {
        if !self.is_workbook() || self.workbook_id == 0 {
            return;
        }
        let my_id = self.workbook_id;
        let my_entity_id = cx.entity_id();
        let doc = self.document.clone();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(root_ref) = root.read(cx) {
                    if let Ok(peer_entity) = root_ref.view().clone().downcast::<SlideEditor>() {
                        if peer_entity.entity_id() == my_entity_id {
                            continue;
                        }
                        let _ = w.update(cx, |_, window, app| {
                            peer_entity.update(app, |peer, cx| {
                                if peer.is_workbook() && peer.workbook_id == my_id {
                                    peer.document = doc.clone();
                                    peer.fix_selection();
                                    peer.sync_sheet_zoom();
                                    cx.notify();
                                    window.refresh();
                                }
                            });
                        });
                    }
                }
            }
        }
    }

    pub(crate) fn sync_document_peers(&self, cx: &mut Context<Self>) {
        if !self.is_document() || self.document_id == 0 {
            return;
        }
        let my_id = self.document_id;
        let my_entity_id = cx.entity_id();
        let doc = self.document.clone();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(root_ref) = root.read(cx) {
                    if let Ok(peer_entity) = root_ref.view().clone().downcast::<SlideEditor>() {
                        if peer_entity.entity_id() == my_entity_id {
                            continue;
                        }
                        let _ = w.update(cx, |_, window, app| {
                            peer_entity.update(app, |peer, cx| {
                                if peer.is_document() && peer.document_id == my_id {
                                    peer.document = doc.clone();
                                    peer.sync_document_view_mode();
                                    peer.relayout_if_changed();
                                    cx.notify();
                                    window.refresh();
                                }
                            });
                        });
                    }
                }
            }
        }
    }

    pub(crate) fn find_other_workbook_windows(
        my_entity_id: gpui::EntityId,
        workbook_id: u64,
        cx: &mut App,
    ) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        let mut same_wb = Vec::new();
        let mut diff_wb = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some((entity, is_same))) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.entity_id() != my_entity_id && entity.read(app).is_workbook() {
                            let is_same = workbook_id != 0 && entity.read(app).workbook_id == workbook_id;
                            return Some((entity, is_same));
                        }
                    }
                    None
                }) {
                    if is_same {
                        same_wb.push((w, entity));
                    } else {
                        diff_wb.push((w, entity));
                    }
                }
            }
        }
        if !same_wb.is_empty() {
            same_wb
        } else {
            diff_wb
        }
    }

    pub(crate) fn toggle_view_side_by_side(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.side_by_side {
            self.side_by_side = false;
            self.sync_scrolling = false;
            if let Some((_, peer_entity)) = self.side_by_side_peer.take() {
                let _ = peer_entity.update(cx, |peer, cx| {
                    peer.side_by_side = false;
                    peer.sync_scrolling = false;
                    peer.side_by_side_peer = None;
                    cx.notify();
                });
            }
            cx.notify();
            return;
        }

        let my_id = cx.entity_id();
        let other = Self::find_other_workbook_windows(my_id, self.workbook_id, cx);
        let Some((peer_window_handle, peer_entity)) = other.first().cloned() else {
            return;
        };

        let my_handle = window.window_handle();
        let my_entity = cx.entity();
        self.side_by_side = true;
        self.sync_scrolling = true;
        self.side_by_side_peer = Some((peer_window_handle, peer_entity.clone()));
        self.last_synced_scroll.set(self.workspace_scroll.offset());

        let _ = peer_entity.update(cx, |peer, cx| {
            peer.side_by_side = true;
            peer.sync_scrolling = true;
            peer.side_by_side_peer = Some((my_handle, my_entity));
            peer.last_synced_scroll.set(peer.workspace_scroll.offset());
            cx.notify();
        });

        Self::arrange_side_by_side_windows(window, peer_window_handle, cx);
        cx.notify();
    }

    pub(crate) fn arrange_side_by_side_windows(
        window: &mut Window,
        peer_handle: AnyWindowHandle,
        cx: &mut App,
    ) {
        let (screen_w, screen_h) = cx
            .primary_display()
            .map(|d| {
                let b = d.visible_bounds();
                (f32::from(b.size.width), f32::from(b.size.height))
            })
            .unwrap_or((1280.0, 800.0));
        let tile_w = screen_w;
        let tile_h = (screen_h / 2.0).max(200.0);
        window.resize(size(px(tile_w), px(tile_h)));
        window.bounds_changed(cx);
        let _ = peer_handle.update(cx, |_, w, cx| {
            w.resize(size(px(tile_w), px(tile_h)));
            w.bounds_changed(cx);
        });
    }

    pub(crate) fn reset_side_by_side_positions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.side_by_side {
            return;
        }
        let peer_opt = self.side_by_side_peer.clone();
        if let Some((peer_handle, _)) = peer_opt {
            if !cx.windows().contains(&peer_handle) {
                self.side_by_side = false;
                self.sync_scrolling = false;
                self.side_by_side_peer = None;
                return;
            }
            Self::arrange_side_by_side_windows(window, peer_handle, cx);
            cx.notify();
        }
    }

    pub(crate) fn toggle_sync_scrolling(&mut self, cx: &mut Context<Self>) {
        if !self.side_by_side {
            return;
        }
        self.sync_scrolling = !self.sync_scrolling;
        let new_val = self.sync_scrolling;
        self.last_synced_scroll.set(self.workspace_scroll.offset());
        let peer_opt = self.side_by_side_peer.clone();
        if let Some((peer_handle, peer_entity)) = peer_opt {
            if !cx.windows().contains(&peer_handle) {
                self.side_by_side = false;
                self.sync_scrolling = false;
                self.side_by_side_peer = None;
                return;
            }
            let _ = peer_entity.update(cx, |peer, cx| {
                peer.sync_scrolling = new_val;
                peer.last_synced_scroll.set(peer.workspace_scroll.offset());
                cx.notify();
            });
        }
        cx.notify();
    }

    pub(crate) fn sync_scroll_to_peer(&mut self, cx: &mut Context<Self>) {
        if !self.sync_scrolling || self.is_syncing_scroll.get() {
            return;
        }
        let peer_opt = self.side_by_side_peer.clone();
        let Some((peer_handle, peer_entity)) = peer_opt else {
            self.side_by_side = false;
            self.sync_scrolling = false;
            return;
        };
        if !cx.windows().contains(&peer_handle) {
            self.side_by_side = false;
            self.sync_scrolling = false;
            self.side_by_side_peer = None;
            return;
        }
        let cur_offset = self.workspace_scroll.offset();
        let last_offset = self.last_synced_scroll.get();
        let delta = point(cur_offset.x - last_offset.x, cur_offset.y - last_offset.y);
        if delta.x == px(0.0) && delta.y == px(0.0) {
            return;
        }
        self.last_synced_scroll.set(cur_offset);
        let _ = peer_entity.update(cx, |peer, cx| {
            if peer.is_syncing_scroll.get() {
                return;
            }
            peer.is_syncing_scroll.set(true);
            let peer_cur = peer.workspace_scroll.offset();
            let peer_new = point(peer_cur.x + delta.x, peer_cur.y + delta.y);
            peer.workspace_scroll.set_offset(peer_new);
            peer.clamp_workspace_scroll();
            peer.last_synced_scroll.set(peer.workspace_scroll.offset());
            peer.is_syncing_scroll.set(false);
            cx.notify();
        });
    }

    pub(crate) fn scroll_workspace_by(&mut self, delta: Point<Pixels>, cx: &mut Context<Self>) {
        let cur = self.workspace_scroll.offset();
        self.workspace_scroll.set_offset(point(cur.x + delta.x, cur.y + delta.y));
        self.clamp_workspace_scroll();
        if self.is_workbook() && self.sync_scrolling {
            self.sync_scroll_to_peer(cx);
        }
        cx.notify();
    }

    pub(crate) fn close_save_confirm_dialog(&mut self, cx: &mut Context<Self>) {
        self.save_confirm_dialog_open = false;
        cx.notify();
    }

    pub(crate) fn confirm_save_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_confirm_dialog_open = false;
        self.save(&crate::Save, window, cx);
        self.force_close_file(window, cx);
    }

    pub(crate) fn confirm_discard_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_workbook() {
            self.preserve_or_cleanup_autorecover_on_discard();
        }
        self.force_close_file(window, cx);
    }

    pub(crate) fn render_save_confirm_dialog(&self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let name = self.file_name.as_ref().map(|s| s.to_string()).unwrap_or_else(|| {
            if self.is_workbook() {
                self.workbook_display_name()
            } else if self.is_document() {
                crate::shell::i18n::text("_shell.document-1-docx").to_string()
            } else {
                crate::shell::i18n::text("_shell.presentation").to_string()
            }
        });
        let message = crate::shell::i18n::text("fix.editor.do-you-want-save-changes");

        div()
            .id("save-confirm-dialog-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000066))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                v_flex()
                    .id("save-confirm-dialog")
                    .w(px(380.))
                    .p_6()
                    .rounded_lg()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_xl()
                    .gap_4()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child(name),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(message),
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("save-confirm-cancel")
                                    .label(crate::shell::i18n::text("_shell.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| this.close_save_confirm_dialog(cx))),
                            )
                            .child(
                                Button::new("save-confirm-discard")
                                    .label(crate::shell::i18n::text("fix.editor.don-t-save"))
                                    .on_click(cx.listener(|this, _, window, cx| this.confirm_discard_and_close(window, cx))),
                            )
                            .child(
                                Button::new("save-confirm-save")
                                    .primary()
                                    .label(crate::shell::i18n::text("_shell.save"))
                                    .on_click(cx.listener(|this, _, window, cx| this.confirm_save_and_close(window, cx))),
                            ),
                    ),
            )
    }

    /// Arrange all open windows across the screen evenly.
    pub(crate) fn arrange_all_windows(window: Option<&mut Window>, cx: &mut App) {
        let current = window.map(|w| {
            let h = w.window_handle();
            (w, h)
        });
        Self::arrange_windows_impl(
            crate::xlsx::arrange_windows::ArrangeKind::Tiled,
            false,
            current,
            None,
            String::new(),
            cx,
        );
    }

    /// Cascade all open windows.
    pub(crate) fn cascade_windows(window: Option<&mut Window>, cx: &mut App) {
        let current = window.map(|w| {
            let h = w.window_handle();
            (w, h)
        });
        Self::arrange_windows_impl(
            crate::xlsx::arrange_windows::ArrangeKind::Cascade,
            false,
            current,
            None,
            String::new(),
            cx,
        );
    }

    /// Word: ウィンドウの分割状態 (DOCX-VIEW-019).
    pub(crate) fn is_document_split(&self) -> bool {
        self.doc_split.is_some()
    }

    /// Word: ウィンドウの分割を切り替える (表示 > ウィンドウ > 分割 / 分割の解除, ⌥⌘S, DOCX-VIEW-019).
    pub(crate) fn toggle_document_split(&mut self, cx: &mut Context<Self>) {
        if self.doc_split.is_some() {
            self.doc_split = None;
            self.doc_split_dragging = false;
            self.doc_split_active = false;
        } else {
            self.doc_split = Some(0.5);
            self.doc_split_scroll.set_offset(self.workspace_scroll.offset());
            self.doc_split_active = false;
        }
        cx.notify();
    }

    /// Word: ウィンドウの分割比率を設定する (DOCX-VIEW-019).
    pub(crate) fn set_document_split(&mut self, split: Option<f32>, cx: &mut Context<Self>) {
        self.doc_split = split;
        if split.is_some() {
            self.doc_split_scroll.set_offset(self.workspace_scroll.offset());
        } else {
            self.doc_split_dragging = false;
            self.doc_split_active = false;
        }
        cx.notify();
    }

    /// Word: ページの左右移動モードかどうか (DOCX-VIEW-008).
    pub(crate) fn is_page_movement_side_to_side(&self) -> bool {
        self.page_movement_side_to_side
    }

    /// Word: ページの移動方向（縦／左右）を設定する (DOCX-VIEW-008).
    pub(crate) fn set_page_movement_side_to_side(&mut self, side_to_side: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.page_movement_side_to_side == side_to_side {
            return;
        }
        let visible = self.visible_page();
        self.page_movement_side_to_side = side_to_side;
        self.workspace_scroll.set_offset(Point::default());
        self.doc_split_scroll.set_offset(Point::default());
        self.scroll_to_page(visible, cx);
        cx.notify();
        window.refresh();
    }

    /// Word: ページの移動方向（縦／左右）を切り替える (DOCX-VIEW-008).
    #[allow(dead_code)]
    pub(crate) fn toggle_page_movement_side_to_side(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = !self.page_movement_side_to_side;
        self.set_page_movement_side_to_side(next, window, cx);
    }

    /// Word: 文書のビューモードを取得する (DOCX-VIEW-001..005).
    pub(crate) fn document_view_mode(&self) -> ibis_doc_model::DocumentViewMode {
        if let Some(flow) = self.presentation().flow.as_ref() {
            flow.view_mode
        } else if self.doc_read_mode {
            ibis_doc_model::DocumentViewMode::Reading
        } else {
            ibis_doc_model::DocumentViewMode::Print
        }
    }

    /// Word: 文書のビューモードを設定する (DOCX-VIEW-001..005).
    pub(crate) fn set_document_view_mode(&mut self, mode: ibis_doc_model::DocumentViewMode, window: &mut Window, cx: &mut Context<Self>) {
        self.doc_read_mode = mode == ibis_doc_model::DocumentViewMode::Reading;
        if mode == ibis_doc_model::DocumentViewMode::Outline {
            self.outline_open = true;
        }
        if self.is_document() {
            self.run_flow(vec![ibis_doc_model::FlowCommand::SetViewMode(mode)], cx);
        }
        cx.notify();
        window.refresh();
    }

    /// Word: 閲覧モードかどうか (DOCX-VIEW-005).
    pub(crate) fn is_read_mode(&self) -> bool {
        self.doc_read_mode
    }

    /// Word: 閲覧モードを設定する (DOCX-VIEW-005).
    pub(crate) fn set_read_mode(&mut self, read: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if read { ibis_doc_model::DocumentViewMode::Reading } else { ibis_doc_model::DocumentViewMode::Print };
        self.set_document_view_mode(mode, window, cx);
        if !read && self.flow().is_some_and(|d| d.interaction_mode == ibis_doc_model::DocumentInteractionMode::Viewing) {
            self.run_flow(vec![ibis_doc_model::FlowCommand::SetInteractionMode(ibis_doc_model::DocumentInteractionMode::Editing)], cx);
        }
    }

    /// Word: 閲覧モードを切り替える (DOCX-VIEW-005).
    #[allow(dead_code)]
    pub(crate) fn toggle_read_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = !self.doc_read_mode;
        self.set_read_mode(next, window, cx);
    }

    /// Word: 閲覧・レビュー・編集モードを取得する (DOCX-VIEW-021).
    pub(crate) fn interaction_mode(&self) -> ibis_doc_model::DocumentInteractionMode {
        if let Some(doc) = self.flow() {
            if self.is_read_mode() || doc.interaction_mode == ibis_doc_model::DocumentInteractionMode::Viewing || self.is_protection_read_only() {
                ibis_doc_model::DocumentInteractionMode::Viewing
            } else if doc.interaction_mode == ibis_doc_model::DocumentInteractionMode::Reviewing || doc.track_changes {
                ibis_doc_model::DocumentInteractionMode::Reviewing
            } else {
                ibis_doc_model::DocumentInteractionMode::Editing
            }
        } else {
            ibis_doc_model::DocumentInteractionMode::Editing
        }
    }

    /// Word: 編集モードかどうか (DOCX-VIEW-021).
    pub(crate) fn is_editing_mode(&self) -> bool {
        self.interaction_mode() == ibis_doc_model::DocumentInteractionMode::Editing
    }

    /// Word: レビューモードかどうか (DOCX-VIEW-021).
    pub(crate) fn is_reviewing_mode(&self) -> bool {
        self.interaction_mode() == ibis_doc_model::DocumentInteractionMode::Reviewing
    }

    /// Word: 閲覧モードかどうか (DOCX-VIEW-021).
    pub(crate) fn is_viewing_mode(&self) -> bool {
        self.interaction_mode() == ibis_doc_model::DocumentInteractionMode::Viewing
    }

    /// Word: 閲覧・レビュー・編集モードを設定する (DOCX-VIEW-021).
    pub(crate) fn set_interaction_mode(&mut self, mode: ibis_doc_model::DocumentInteractionMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.interaction_mode() == mode {
            return;
        }
        if mode == ibis_doc_model::DocumentInteractionMode::Editing && self.is_track_changes_locked() {
            self.status = Some(crate::shell::i18n::text("docx.interaction.locked-warning").into());
            cx.notify();
            return;
        }
        if mode != ibis_doc_model::DocumentInteractionMode::Viewing && self.is_read_mode() {
            self.doc_read_mode = false;
        }
        if self.is_document() {
            self.run_flow(vec![ibis_doc_model::FlowCommand::SetInteractionMode(mode)], cx);
        }
        cx.notify();
        window.refresh();
    }

    /// Word: 印刷レイアウト表示かどうか (DOCX-VIEW-001).
    pub(crate) fn is_print_layout_mode(&self) -> bool {
        self.document_view_mode() == ibis_doc_model::DocumentViewMode::Print
    }

    /// Word: Web レイアウト表示かどうか (DOCX-VIEW-002).
    pub(crate) fn is_web_layout_mode(&self) -> bool {
        self.document_view_mode() == ibis_doc_model::DocumentViewMode::Web
    }

    /// Word: 下書き表示かどうか (DOCX-VIEW-003).
    pub(crate) fn is_draft_mode(&self) -> bool {
        self.document_view_mode() == ibis_doc_model::DocumentViewMode::Draft
    }

    /// Word: アウトライン表示かどうか (DOCX-VIEW-004).
    pub(crate) fn is_outline_mode(&self) -> bool {
        self.document_view_mode() == ibis_doc_model::DocumentViewMode::Outline
    }

    /// Word: フォーカス表示モードかどうか (DOCX-VIEW-006).
    pub(crate) fn is_focus_mode(&self) -> bool {
        self.doc_focus_mode
    }

    /// Word: フォーカス表示モードを設定する (DOCX-VIEW-006).
    pub(crate) fn set_focus_mode(&mut self, focus: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if self.doc_focus_mode == focus {
            return;
        }
        self.doc_focus_mode = focus;
        cx.notify();
    }

    /// Word: フォーカス表示モードを切り替える (DOCX-VIEW-006).
    pub(crate) fn toggle_focus_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = !self.doc_focus_mode;
        self.set_focus_mode(next, window, cx);
    }

    /// Keep the status bar's slider on the zoom shown, which changes with
    /// the window size while the slide is fitted.
    fn sync_zoom_slider(&self, window: &mut Window, cx: &mut Context<Self>) {
        let percent = (self.current_zoom() * 100.0).clamp(MIN_ZOOM * 100.0, MAX_ZOOM * 100.0);
        // Setting the value notifies; only a real change may, or render would repeat.
        if (self.zoom_slider.read(cx).value().end() - percent).abs() > 0.01 {
            self.zoom_slider.update(cx, |slider, cx| slider.set_value(percent, window, cx));
        }
    }

    /// Whether the slide is taller than the workspace, so it scrolls vertically.
    fn slide_overflows_vertically(&self) -> bool {
        match (self.workspace_content(), self.workspace_bounds) {
            (Some(content), Some(ws)) => content.size.height > ws.size.height + px(0.5),
            _ => false,
        }
    }

    /// Keep the workspace's scrolling within the current content.
    pub(crate) fn clamp_workspace_scroll(&self) {
        let (Some(content), Some(ws)) = (self.workspace_content(), self.workspace_bounds) else { return };
        let offset = self.workspace_scroll.offset();
        let clamp = |v: Pixels, view: Pixels, content: Pixels| {
            px(f32::from(v).clamp(f32::from(view - content).min(0.0), 0.0))
        };
        self.workspace_scroll.set_offset(point(
            clamp(offset.x, ws.size.width, content.size.width),
            clamp(offset.y, ws.size.height, content.size.height),
        ));
    }

    /// The frame to draw for `shape`, including an in-progress drag.
    pub(crate) fn display_frame(&self, shape: &Shape) -> EmuRect {
        match (self.drag, self.slide_layout()) {
            (Some(drag @ (Drag::Move { shape: id, .. } | Drag::Resize { shape: id, .. })), Some(layout))
                if id == shape.id =>
            {
                self.display_frame_for(shape, drag, layout)
            }
            _ => shape.frame,
        }
    }

    /// The rubber band of a text box being drawn.
    pub(crate) fn creation_rect(&self) -> Option<EmuRect> {
        let Some(Drag::Create { start, current }) = self.drag else { return None };
        let layout = self.slide_layout()?;
        Some(EmuRect::from_corners(layout.to_emu(start), layout.to_emu(current)))
    }

    // ----- Document edits ------------------------------------------------

    pub(crate) fn execute(&mut self, commands: Vec<Command>, cx: &mut Context<Self>) {
        let is_admin = commands.iter().all(|c| matches!(c, Command::SetMarkAsFinal { .. } | Command::SetReadOnlyRecommended { .. } | Command::SetModifyVerifier { .. }));
        if !is_admin && self.pages_are_slides() {
            if self.is_pptx_mark_as_final() {
                log::warn!("edit rejected: presentation is marked as final");
                return;
            }
            if self.is_pptx_read_only_blocked() {
                log::warn!("edit rejected: presentation is read-only (recommended, or locked by a password)");
                return;
            }
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("edit rejected: {err}");
        }
        self.fix_selection();
        self.sync_workbook_peers(cx);
        self.sync_document_peers(cx);
        cx.notify();
    }

    /// Drop references to shapes or slides that an undo or redo removed.
    fn fix_selection(&mut self) {
        if self.presentation().slide(self.current_slide).is_none() {
            self.current_slide = self.presentation().slides[0].id;
        }
        if let Some(id) = self.selection {
            if self.slide().shape(id).is_none() {
                self.selection = None;
            }
        }
    }

    pub(crate) fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_some() {
            return;
        }
        self.commit_edit(window, cx);
        self.document.undo();
        self.sync_sheet_zoom();
        self.sync_document_view_mode();
        self.fix_selection();
        self.sync_workbook_peers(cx);
        self.sync_document_peers(cx);
        cx.notify();
    }

    pub(crate) fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_some() {
            return;
        }
        self.commit_edit(window, cx);
        self.document.redo();
        self.sync_sheet_zoom();
        self.sync_document_view_mode();
        self.fix_selection();
        self.sync_workbook_peers(cx);
        self.sync_document_peers(cx);
        cx.notify();
    }

    fn sync_document_view_mode(&mut self) {
        if self.is_document() {
            if let Some(view_mode) = self.flow().map(|f| f.view_mode) {
                self.doc_read_mode = view_mode == ibis_doc_model::DocumentViewMode::Reading;
                if view_mode == ibis_doc_model::DocumentViewMode::Outline && !self.outline_open {
                    self.outline_open = true;
                }
            }
        }
    }

    fn sync_sheet_zoom(&mut self) {
        if self.sheet_state().is_some() {
            if let Some((_, sheet)) = self.current_sheet() {
                let sheet_zoom_val = sheet.zoom.map(|z| z as f32 / 100.0).unwrap_or(1.0);
                self.zoom = Some(sheet_zoom_val);
            }
        }
    }

    pub(crate) fn delete_selection(
        &mut self,
        _: &DeleteSelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editing.is_some() || self.slideshow.is_some() {
            return;
        }
        if let Some(shape) = self.selection.take() {
            // A worksheet's grid or a document's page is the page itself,
            // not a shape to remove.
            if self.slide().shape(shape).is_some_and(|s| matches!(s.kind, ShapeKind::Sheet(_) | ShapeKind::Page { .. })) {
                return;
            }
            self.execute(vec![Command::RemoveShape { slide: self.current_slide, shape }], cx);
        }
    }

    pub(crate) fn cancel_tool(&mut self, _: &CancelTool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(show) = self.slideshow.as_mut() {
            if let Some(rehearsal) = show.rehearsal.as_mut() {
                if rehearsal.dialog_open {
                    return self.discard_rehearsal_timings(window, cx);
                } else {
                    let elapsed_ms = (rehearsal.slide_accumulated + rehearsal.slide_started.elapsed()).as_millis().max(1000) as u32;
                    rehearsal.recorded_timings.insert(self.current_slide, elapsed_ms);
                    rehearsal.dialog_open = true;
                    let total = rehearsal.total_accumulated + rehearsal.total_started.elapsed();
                    rehearsal.final_total = Some(total);
                    cx.notify();
                    return;
                }
            }
            if show.grid_open {
                show.grid_open = false;
                cx.notify();
                return;
            }
            if show.zoom > 1.001 || show.pan != (0.0, 0.0) {
                self.reset_slideshow_zoom(cx);
                return;
            }
            if show.has_inks() {
                if show.ink_dialog_open {
                    return self.end_slideshow_with_ink_choice(false, window, cx);
                } else {
                    show.ink_dialog_open = true;
                    cx.notify();
                    return;
                }
            }
            return self.end_slideshow(window, cx);
        }
        if self.zoom_dialog_open {
            self.close_zoom_dialog(cx);
            return;
        }
        if self.pptx().is_some_and(|p| p.slide_zoom_dialog.is_some()) {
            self.close_slide_zoom_dialog(cx);
            return;
        }
        if self.pptx().is_some_and(|p| p.section_zoom_dialog.is_some()) {
            self.close_section_zoom_dialog(cx);
            return;
        }
        if self.pptx().is_some_and(|p| p.summary_zoom_dialog.is_some()) {
            self.close_summary_zoom_dialog(cx);
            return;
        }
        if self.cancel_cell_jump(window, cx) {
            return;
        }
        if self.is_grid_view() {
            self.close_grid_view(cx);
            return;
        }
        if self.is_notes_page_view() {
            self.switch_to_normal_view(window, cx);
            return;
        }
        if self.editing.is_some() {
            self.commit_edit(window, cx);
        } else if self.tool != Tool::Select {
            self.tool = Tool::Select;
        } else {
            self.selection = None;
        }
        cx.notify();
    }

    pub(crate) fn set_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        // A document's pages are laid out from its text; no text box is put on them.
        let tool = if self.allows_shapes() { tool } else { Tool::Select };
        self.tool = tool;
        if tool == Tool::TextBox {
            self.selection = None;
        }
        cx.notify();
    }

    pub(crate) fn add_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.pages_are_slides() {
            return;
        }
        if self.is_pptx_mark_as_final() || self.is_pptx_read_only_blocked() {
            log::warn!("add_slide rejected: presentation is read-only");
            return;
        }
        self.commit_edit(window, cx);
        let index = self.presentation().slide_index(self.current_slide).map_or(0, |i| i + 1);
        let id = self.document.ids().new_slide_id();
        let layout = self.slide().layout;
        let slide = self.new_slide(index, id, layout);
        self.execute(vec![Command::InsertSlide { index, slide }], cx);
        self.current_slide = id;
        self.selection = None;
    }

    /// A new slide `id` to put at `index`, following `layout` (else the
    /// current slide's): with the layout's placeholders empty, as the file
    /// reads them, when there is a file; else without them.
    pub(crate) fn new_slide(&mut self, index: usize, id: ibis_doc_model::SlideId, layout: Option<ibis_doc_model::LayoutId>) -> ibis_doc_model::Slide {
        if let (Some(source), Some(layout)) = (self.source.clone(), layout) {
            let current = self.presentation().clone();
            match source.new_slide(&current, index, id, layout, || self.document.ids().new_shape_id()) {
                Ok(slide) => return slide,
                Err(err) => log::warn!("Cannot make the placeholder of the new slide: {err}"),
            }
        }
        match layout.and_then(|l| self.presentation().layout(l)) {
            Some(layout) => ibis_doc_model::Slide::from_layout(id, layout),
            None => self.presentation().new_slide_like(id, self.current_slide),
        }
    }

    pub(crate) fn delete_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.pages_are_slides() {
            return;
        }
        self.commit_edit(window, cx);
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        // Several slides selected in the list (not all of them): all taken away, as one edit.
        let chosen = self.selected_slide_ids();
        if chosen.len() > 1 && chosen.len() < self.presentation().slides.len() {
            let first = chosen.iter().filter_map(|id| self.presentation().slide_index(*id)).min().unwrap_or(index);
            self.execute(vec![Command::Group(chosen.into_iter().map(|slide| Command::RemoveSlide { slide }).collect())], cx);
            let slides = &self.presentation().slides;
            self.current_slide = slides[first.min(slides.len() - 1)].id;
            self.selection = None;
            if let Some(s) = self.pptx_mut() {
                s.slide_selection.clear();
            }
            return;
        }
        self.execute(vec![Command::RemoveSlide { slide: self.current_slide }], cx);
        let slides = &self.presentation().slides;
        self.current_slide = slides[index.min(slides.len() - 1)].id;
        self.selection = None;
    }

    fn step_slide(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            return;
        }
        // PageDown / PageUp in a document scroll to the next or previous page,
        // in a worksheet by a screen (Control+PageDown changes sheets).
        if with_format!(self, step_page(self, forward, window, cx)) {
            return;
        }
        let slides = &self.presentation().slides;
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let next = if forward { (index + 1).min(slides.len() - 1) } else { index.saturating_sub(1) };
        // Past the first or last slide nothing changes, and nothing is drawn
        // again (keys held down there would keep the window busy).
        if next == index {
            return;
        }
        let id = slides[next].id;
        self.select_slide(id, window, cx);
    }

    /// Show the next or previous worksheet.
    pub(crate) fn step_sheet(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let slides = &self.presentation().slides;
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let next = if forward { (index + 1).min(slides.len() - 1) } else { index.saturating_sub(1) };
        // Past the first or last slide nothing changes, and nothing is drawn
        // again (keys held down there would keep the window busy).
        if next == index {
            return;
        }
        let id = slides[next].id;
        self.select_slide(id, window, cx);
    }

    /// Show slide `number` (1-based), clamped to the presentation.
    pub(crate) fn show_slide_number(&mut self, number: usize, window: &mut Window, cx: &mut Context<Self>) {
        let slides = &self.presentation().slides;
        let id = slides[number.clamp(1, slides.len()) - 1].id;
        self.select_slide(id, window, cx);
    }

    pub(crate) fn next_slide(&mut self, _: &NextSlide, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_some() {
            return self.step_slideshow(true, window, cx);
        }
        self.step_slide(true, window, cx);
    }

    pub(crate) fn previous_slide(&mut self, _: &PreviousSlide, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_some() {
            return self.step_slideshow(false, window, cx);
        }
        self.step_slide(false, window, cx);
    }

    /// The last slide (`last`) or the first, in the editor or the slide show.
    fn end_slide(&mut self, last: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_some() {
            return self.slideshow_to_end(last, cx);
        }
        if self.editing.is_some() || !self.pages_are_slides() {
            return cx.propagate();
        }
        let count = self.presentation().slides.len();
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let target = if last { count - 1 } else { 0 };
        if target != index {
            self.show_slide_number(target + 1, window, cx);
        }
    }

    pub(crate) fn select_slide(&mut self, slide: SlideId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.presentation().slide_index(slide) {
            // Its pictures before those still waiting.
            self.resources.preload.prioritize(&self.presentation().slides[index]);
        }
        if let Some(s) = self.pptx_mut() {
            s.slide_selection.clear();
        }
        let list_focused = self.list_focus.is_focused(window);
        self.commit_edit(window, cx);
        if list_focused {
            // Committing an edit focuses the workspace; the list keeps the keys.
            self.list_focus.focus(window, cx);
        }
        if with_format!(self, show_page(self, slide, window, cx)) {
            return cx.notify();
        }
        if self.current_slide != slide {
            // Slides may differ in size: the scrolling is kept only as far
            // as it goes.
            let offset = self.workspace_scroll.offset();
            self.current_slide = slide;
            self.workspace_scroll.set_offset(offset);
            self.clamp_workspace_scroll();
        }
        self.selection = None;
        if let Some(index) = self.presentation().slide_index(slide) {
            let row = self.row_of_slide(index);
            self.list_scroll.scroll_to_item(row);
        }
        cx.notify();
    }

    /// The mouse wheel over the workspace turns pages: a notch of a wheel
    /// one page, a trackpad every `WHEEL_PAGE` pixels. When the zoomed slide
    /// is taller than the workspace, the wheel scrolls it instead (the
    /// workspace's own scrolling) and turns no pages.
    ///
    /// Browsers report a notch in pixels, like a trackpad, with no gesture
    /// phase, and some send only a few pixels for it. There the first pixel
    /// scroll after a pause turns a page at once; the rest of a quick scroll
    /// (a spun wheel, a trackpad swipe and its momentum) counts pixels.
    pub(crate) fn on_workspace_scroll(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        // A worksheet scrolls like Excel's; its pages change with the tabs.
        // A document's pages scroll by, as in Word.
        if !self.pages_are_slides() || self.slide_overflows_vertically() {
            self.wheel = 0.0;
            return;
        }
        let steps = self.wheel_pages(event);
        if steps != 0 {
            for _ in 0..steps.unsigned_abs() {
                self.step_slide(steps > 0, window, cx);
            }
            cx.stop_propagation();
        }
    }

    /// The pages the wheel turns with `event`, forward if positive: scrolling
    /// down (content moving up) shows the next page. See `on_workspace_scroll`.
    pub(crate) fn wheel_pages(&mut self, event: &ScrollWheelEvent) -> i32 {
        match event.delta {
            ScrollDelta::Lines(delta) => {
                self.wheel = 0.0;
                self.wheel_line_pages(-delta.y)
            }
            ScrollDelta::Pixels(delta) => {
                let y = f32::from(delta.y);
                #[cfg(target_family = "wasm")]
                {
                    let now = js_sys::Date::now();
                    let paused = now - self.wheel_at > WHEEL_PAUSE_MS;
                    self.wheel_at = now;
                    // Mostly sideways scrolling does not start a page turn.
                    if paused && y.abs() > f32::from(delta.x).abs() {
                        self.wheel = 0.0;
                        return if y < 0.0 { 1 } else { -1 };
                    }
                }
                // A change of direction starts over.
                if y * self.wheel < 0.0 {
                    self.wheel = 0.0;
                }
                self.wheel += y;
                let pages = (self.wheel / WHEEL_PAGE).trunc();
                self.wheel -= pages * WHEEL_PAGE;
                -(pages as i32)
            }
        }
    }

    /// The pages `lines` of a wheel turn (down if positive): a line one page,
    /// with nothing held over.
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    fn wheel_line_pages(&mut self, lines: f32) -> i32 {
        let pages = lines.round() as i32;
        if pages == 0 && lines != 0.0 { if lines > 0.0 { 1 } else { -1 } } else { pages }
    }

    /// The pages `lines` of a wheel turn (down if positive): a notch one page.
    /// Windows reports a notch as the lines the system scrolls for it (3 by
    /// default), and a high-resolution wheel parts of a notch, which add up.
    /// GPUI's Linux platform does the same, with 3 lines to a notch.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn wheel_line_pages(&mut self, lines: f32) -> i32 {
        let notches = lines / wheel_scroll_lines();
        // A change of direction starts over.
        if notches * self.wheel_notches < 0.0 {
            self.wheel_notches = 0.0;
        }
        self.wheel_notches += notches;
        let pages = self.wheel_notches.trunc();
        self.wheel_notches -= pages;
        pages as i32
    }

    /// Change the text style of the selected text box.
    pub(crate) fn update_style(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut TextStyle),
    ) {
        self.commit_edit(window, cx);
        let Some(shape) = self.selected_shape() else { return };
        let Some(body) = shape.text_body() else { return };
        let mut style = body.style.clone();
        update(&mut style);
        if style != body.style {
            let command = Command::SetTextStyle { slide: self.current_slide, shape: shape.id, style };
            self.execute(vec![command], cx);
        }
    }

    pub(crate) fn step_font_size(&mut self, grow: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.update_style(window, cx, |s| s.font_size = step_font_size(s.font_size, grow));
    }

    // ----- In-place text editing ------------------------------------------

    pub(crate) fn begin_edit(
        &mut self,
        shape: ShapeId,
        is_new: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editing.as_ref().is_some_and(|e| e.shape == shape) {
            return;
        }
        self.commit_edit(window, cx);
        let text = self
            .slide()
            .shape(shape)
            .and_then(|s| s.text_body())
            .map(|b| b.text())
            .unwrap_or_default();
        let state = cx.new(|cx| TextareaState::new(window, cx).default_value(text));
        let subscription = cx.subscribe_in(&state, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::Blur = event {
                this.commit_edit(window, cx);
            }
        });
        state.update(cx, |state, cx| state.focus(window, cx));
        self.selection = Some(shape);
        self.editing = Some(TextEdit { shape, state, is_new, _subscription: subscription });
        cx.notify();
    }

    /// Finish in-place editing and write the text back to the document.
    pub(crate) fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.editing.take() else { return };
        let text = edit.state.read(cx).value().to_string();
        let slide = self.current_slide;
        let old = self.slide().shape(edit.shape).and_then(|s| s.text_body()).map(|b| b.text());

        if edit.is_new && text.trim().is_empty() {
            // PowerPoint drops a new text box that was left empty.
            self.document.retract_last();
            self.selection = None;
        } else if old.as_deref() != Some(text.as_str()) {
            // A new box takes the text as it is; an existing one gets only
            // what changed, keeping its runs and their formatting (and so
            // what saving can write back into the file).
            let command = match (&old, edit.is_new) {
                (Some(old), false) => {
                    let (at, remove, insert) = text_change(old, &text);
                    Command::EditText { slide, shape: edit.shape, at, remove, insert }
                }
                _ => Command::SetText { slide, shape: edit.shape, text },
            };
            let result = if edit.is_new {
                self.document.amend_last(vec![command])
            } else {
                self.document.execute(vec![command])
            };
            if let Err(err) = result {
                log::warn!("edit rejected: {err}");
            }
        }
        self.fix_selection();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    // ----- Pointer interaction --------------------------------------------

    /// Press on the workspace or empty slide area.
    pub(crate) fn on_workspace_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_edit(window, cx);
        self.focus_handle.focus(window, cx);
        match self.tool {
            Tool::TextBox if self.allows_shapes() => {
                self.drag = Some(Drag::Create { start: event.position, current: event.position });
            }
            _ => self.selection = None,
        }
        cx.notify();
    }

    /// Press on a shape body.
    pub(crate) fn on_shape_down(
        &mut self,
        shape: ShapeId,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tool == Tool::TextBox {
            // Drawing a text box may start on top of another shape.
            return self.on_workspace_down(event, window, cx);
        }
        cx.stop_propagation();
        if self.editing.as_ref().is_some_and(|e| e.shape == shape) {
            return; // the text field handles its own clicks
        }
        if self.slide().shape(shape).is_some_and(|s| matches!(s.kind, ShapeKind::Slicer(_))) {
            self.commit_edit(window, cx);
            self.focus_handle.focus(window, cx);
            if self.slicer_press(shape, event, cx) {
                self.set_selection(vec![shape]);
                return cx.notify();
            }
        }
        if event.click_count >= 2 {
            return self.begin_edit(shape, false, window, cx);
        }
        self.commit_edit(window, cx);
        self.focus_handle.focus(window, cx);
        if event.modifiers.shift || event.modifiers.platform || event.modifiers.control {
            let mut ids = self.selected_ids();
            if ids.contains(&shape) {
                ids.retain(|&id| id != shape);
            } else {
                ids.push(shape);
            }
            self.set_selection(ids);
        } else {
            self.set_selection(vec![shape]);
        }
        if let Some(origin) = self.slide().shape(shape).map(|s| s.frame) {
            self.drag = Some(Drag::Move {
                shape,
                start: event.position,
                current: event.position,
                origin,
            });
        }
        cx.notify();
    }

    /// Press on a selection handle.
    pub(crate) fn on_handle_down(
        &mut self,
        shape: ShapeId,
        handle: ResizeHandle,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.commit_edit(window, cx);
        if let Some(origin) = self.slide().shape(shape).map(|s| s.frame) {
            self.drag = Some(Drag::Resize {
                shape,
                handle,
                start: event.position,
                current: event.position,
                origin,
            });
        }
        cx.notify();
    }

    pub(crate) fn on_pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.cell_edit().is_some_and(|e| e.dragging.is_some()) {
            if event.pressed_button == Some(MouseButton::Left) {
                self.drag_in_cell_text(event.position, window, cx);
            } else if let Some(edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.as_mut()) {
                edit.dragging = None;
            }
            return;
        }
        if self.sheet_state().is_some_and(|s| s.cell_drag) {
            if event.pressed_button == Some(MouseButton::Left) {
                self.extend_cells_to(event.position);
                cx.notify();
            } else if let Some(state) = self.sheet_state_mut() {
                state.cell_drag = false;
            }
            return;
        }
        let Some(drag) = self.drag.as_mut() else { return };
        match drag {
            Drag::Move { current, .. } | Drag::Resize { current, .. } | Drag::Create { current, .. } => {
                *current = event.position;
            }
        }
        cx.notify();
    }

    pub(crate) fn on_pointer_up(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.as_mut())
            && edit.dragging.take().is_some()
        {
            return cx.notify();
        }
        if let Some(state) = self.sheet_state_mut()
            && state.cell_drag
        {
            state.cell_drag = false;
            return cx.notify();
        }
        let Some(drag) = self.drag.take() else { return };
        let Some(layout) = self.slide_layout() else { return };
        let slide = self.current_slide;
        match drag {
            Drag::Move { shape, start, current, .. } | Drag::Resize { shape, start, current, .. } => {
                if !moved(start, current) {
                    return cx.notify();
                }
                let Some(current_shape) = self.slide().shape(shape) else { return };
                let frame = self.display_frame_for(current_shape, drag, layout);
                if frame != current_shape.frame {
                    self.execute(vec![Command::SetFrame { slide, shape, frame }], cx);
                }
            }
            Drag::Create { start, current } => {
                let frame = if moved(start, current) {
                    let r = EmuRect::from_corners(layout.to_emu(start), layout.to_emu(current));
                    EmuRect { width: r.width.max(MIN_SHAPE_SIDE), height: r.height.max(MIN_SHAPE_SIDE), ..r }
                } else {
                    self.presentation().default_text_box_frame(layout.to_emu(start))
                };
                let id = self.document.ids().new_shape_id();
                let index = self.slide().shapes.len();
                self.execute(
                    vec![Command::InsertShape { slide, index, shape: Shape::text_box(id, frame, "") }],
                    cx,
                );
                self.tool = Tool::Select;
                self.begin_edit(id, true, window, cx);
            }
        }
    }

    fn display_frame_for(&self, shape: &Shape, drag: Drag, layout: SlideLayout) -> EmuRect {
        match drag {
            Drag::Move { start, current, origin, .. } => origin.translated(
                layout.to_emu_len(current.x - start.x),
                layout.to_emu_len(current.y - start.y),
            ),
            Drag::Resize { handle, start, current, origin, .. } => origin.resized(
                handle,
                layout.to_emu_len(current.x - start.x),
                layout.to_emu_len(current.y - start.y),
                MIN_SHAPE_SIDE,
            ),
            Drag::Create { .. } => shape.frame,
        }
    }
}

/// The smallest change from `old` to `new`: the character where they
/// start to differ, how many characters of `old` are replaced from there,
/// and by what.
pub(crate) fn text_change(old: &str, new: &str) -> (usize, usize, String) {
    let (a, b): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let rest = a.len().min(b.len()) - prefix;
    let suffix = a.iter().rev().zip(b.iter().rev()).take(rest).take_while(|(x, y)| x == y).count();
    (prefix, a.len() - prefix - suffix, b[prefix..b.len() - suffix].iter().collect())
}

/// The zooms of the status bar's list, in percent, largest first: 100%,
/// and from it up by `ZOOM_LIST_UP` and down by `ZOOM_LIST_DOWN` as far as
/// the zoom range goes, each rounded to a whole percent.
pub(crate) fn zoom_levels() -> Vec<u32> {
    let mut levels = Vec::new();
    let mut zoom = ZOOM_LIST_UP;
    while zoom <= MAX_ZOOM + 1e-4 {
        levels.push((zoom * 100.0).round() as u32);
        zoom *= ZOOM_LIST_UP;
    }
    levels.reverse();
    let mut zoom = 1.0;
    while zoom >= MIN_ZOOM - 1e-4 {
        levels.push((zoom * 100.0).round() as u32);
        zoom *= ZOOM_LIST_DOWN;
    }
    levels
}

/// A zoom typed in percent, such as `150`, `150%` or ` 72.5 `.
pub(crate) fn parse_percent(text: &str) -> Option<f32> {
    let text = text.trim();
    let text = text.strip_suffix('%').or_else(|| text.strip_suffix('％')).unwrap_or(text).trim();
    text.parse::<f32>().ok().filter(|p| p.is_finite() && *p > 0.0)
}

/// The scrolling on one axis that keeps the content point at `anchor` (from
/// the start of a view `view` long) there when the zoom changes by `ratio`:
/// the page starts at `before_origin` in the content scrolled by `old`, and
/// at `after_origin` in the new content, `content` long. Kept within the
/// content, so a point near its edge moves.
fn zoom_axis_offset(anchor: f32, view: f32, old: f32, before_origin: f32, after_origin: f32, ratio: f32, content: f32) -> f32 {
    let at = after_origin + (anchor - old - before_origin) * ratio;
    (anchor - at).clamp((view - content).min(0.0), 0.0)
}

/// Whether the wheel zooms with `modifiers` held: Command and Control on a
/// Mac (`mac`), Control elsewhere, as in PowerPoint for Windows.
fn wheel_zooms(modifiers: &Modifiers, mac: bool) -> bool {
    if mac { modifiers.platform && modifiers.control } else { modifiers.control }
}

/// Whether the wheel's zoom modifier is a Mac's. In the browser, the
/// browser's platform says (see `secondary_modifier` in `lib.rs`).
fn zoom_modifier_is_mac() -> bool {
    #[cfg(target_family = "wasm")]
    return crate::web::is_mac();
    #[cfg(not(target_family = "wasm"))]
    cfg!(target_os = "macos")
}

/// Pixels per EMU at `zoom`, 1.0 being 100%.
pub(crate) fn zoom_scale(zoom: f32) -> f32 {
    zoom * PX_PER_INCH_AT_100 / ibis_doc_model::EMU_PER_INCH as f32
}

/// The worksheet of `slide` and the shape that holds it.
pub(crate) fn sheet_of(slide: &Slide) -> Option<(&Shape, &std::sync::Arc<Sheet>)> {
    slide.shapes.iter().find_map(|shape| match &shape.kind {
        ShapeKind::Sheet(sheet) => Some((shape, sheet)),
        _ => None,
    })
}

fn moved(a: Point<Pixels>, b: Point<Pixels>) -> bool {
    let dx = f32::from(b.x - a.x);
    let dy = f32::from(b.y - a.y);
    dx.abs() >= DRAG_THRESHOLD || dy.abs() >= DRAG_THRESHOLD
}

impl Focusable for SlideEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SlideEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_zoom_slider(window, cx);
        if self.zoom_menu_open && !self.has_document {
            self.zoom_menu_open = false;
        }
        with_format!(self, before_render(self, window, cx));
        let theme = cx.theme();
        v_flex()
            .id("slide-editor")
            .key_context(CONTEXT)
            // Documents dropped from the Finder on the window are opened (pictures dropped on a slide are put on it).
            .on_drop(cx.listener(|_, paths: &ExternalPaths, _, cx| {
                #[cfg(not(target_family = "wasm"))]
                for path in paths.paths() {
                    if path.extension().and_then(|e| e.to_str()).and_then(ibis_pptx::Kind::from_extension).is_some() {
                        crate::open_path_in_app(path.clone(), cx);
                    }
                }
                #[cfg(target_family = "wasm")]
                let _ = (paths, cx);
            }))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::delete_selection))
            .on_action(cx.listener(Self::cancel_tool))
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(|this, _: &crate::NewDocument, window, cx| this.new_file(ibis_pptx::Kind::Document, window, cx)))
            .on_action(cx.listener(|this, _: &crate::NewFromTemplate, window, cx| this.new_from_template(window, cx)))
            // Only with a file open, so ファイル > 閉じる is greyed on the start view.
            .when(self.has_document, |d| d.on_action(cx.listener(Self::close_file)))
            .when(self.has_document, |d| d.on_action(cx.listener(Self::save)).on_action(cx.listener(Self::save_as)).on_action(cx.listener(|this, _: &crate::PrintFile, window, cx| this.print(window, cx))))
            .when(self.has_document, |d| {
                d.on_action(cx.listener(|this, _: &crate::ToggleMenus, _, cx| {
                    this.chrome_compact = !this.chrome_compact;
                    cx.notify();
                }))
            })
            .on_action(cx.listener(Self::next_slide))
            .on_action(cx.listener(Self::previous_slide))
            .on_action(cx.listener(|this, _: &crate::FirstSlide, window, cx| this.end_slide(false, window, cx)))
            .on_action(cx.listener(|this, _: &crate::LastSlide, window, cx| this.end_slide(true, window, cx)))
            .on_action(cx.listener(|this, _: &crate::StartSlideshow, window, cx| this.start_slideshow(window, cx)))
            .on_action(cx.listener(|this, _: &crate::StartPresenterView, window, cx| this.start_presenter_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::InsertHyperlink, window, cx| {
                if this.pptx().is_some() && this.slideshow.is_none() {
                    this.slide_open_link_panel(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::InsertComment, window, cx| {
                if let Some(add) = crate::shell::format::with_format!(this, add_comment(this)) {
                    add(this, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::StartSlideshowFromBeginning, window, cx| {
                if this.slideshow.is_none() && this.pages_are_slides() && this.has_document && !this.loading {
                    this.show_slide_number(1, window, cx);
                    this.start_slideshow(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::BlackScreen, _, cx| this.blank_slideshow(0x000000, cx)))
            .on_action(cx.listener(|this, _: &crate::WhiteScreen, _, cx| this.blank_slideshow(0xffffff, cx)))
            .on_action(cx.listener(|this, _: &crate::LaserPointer, _, cx| this.set_slideshow_pointer_mode(crate::pptx::slideshow::PointerMode::Laser, cx)))
            .on_action(cx.listener(|this, _: &crate::PenPointer, _, cx| this.set_slideshow_pointer_mode(crate::pptx::slideshow::PointerMode::Pen, cx)))
            .on_action(cx.listener(|this, _: &crate::HighlighterPointer, _, cx| this.set_slideshow_pointer_mode(crate::pptx::slideshow::PointerMode::Highlighter, cx)))
            .on_action(cx.listener(|this, _: &crate::ArrowPointer, _, cx| this.set_slideshow_pointer_mode(crate::pptx::slideshow::PointerMode::Arrow, cx)))
            .on_action(cx.listener(|this, _: &crate::EraserPointer, _, cx| this.set_slideshow_pointer_mode(crate::pptx::slideshow::PointerMode::Eraser, cx)))
            .on_action(cx.listener(|this, _: &crate::ClearSlideInks, _, cx| this.clear_current_slide_inks(cx)))
            .on_action(cx.listener(|this, _: &crate::ToggleSlideshowGrid, _, cx| this.toggle_slideshow_grid(cx)))
            .on_action(cx.listener(|this, _: &crate::ZoomInSlideshow, _, cx| this.zoom_slideshow(1.25, cx)))
            .on_action(cx.listener(|this, _: &crate::ZoomOutSlideshow, _, cx| this.zoom_slideshow(0.8, cx)))
            .on_action(cx.listener(|this, _: &crate::ResetSlideshowZoom, _, cx| this.reset_slideshow_zoom(cx)))
            .on_action(cx.listener(|this, _: &crate::RehearseTimings, window, cx| this.start_rehearsal(window, cx)))
            .on_action(cx.listener(|this, _: &crate::ToggleSlideGridView, window, cx| this.toggle_grid_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::ToggleReadingView, window, cx| {
                if this.is_reading_view() {
                    this.switch_to_normal_view(window, cx);
                } else {
                    this.start_reading_view(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::SwitchToNormalView, window, cx| this.switch_to_normal_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::ToggleOutlineView, window, cx| this.toggle_outline_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::SwitchToOutlineView, window, cx| this.switch_to_outline_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::ToggleNotesPageView, window, cx| this.toggle_notes_page_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::SwitchToNotesPageView, window, cx| this.switch_to_notes_page_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::OpenZoomDialog, window, cx| this.open_zoom_dialog(window, cx)))
            .on_action(cx.listener(|this, _: &crate::NewEditorWindow, _, cx| this.new_window(cx)))
            .on_action(cx.listener(|_, _: &crate::ArrangeAllWindows, window, cx| SlideEditor::arrange_all_windows(Some(window), cx)))
            .on_action(cx.listener(|_, _: &crate::CascadeWindows, window, cx| SlideEditor::cascade_windows(Some(window), cx)))
            .on_action(cx.listener(|this, _: &crate::CopyAnimation, _, cx| this.copy_shape_animations(cx)))
            .on_action(cx.listener(|this, _: &crate::PasteAnimation, _, cx| this.paste_shape_animations(cx)))
            // Drags are tracked on the whole window so they continue past the slide.
            .on_mouse_move(cx.listener(Self::on_pointer_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pointer_up))
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_action(cx.listener(|this, _: &crate::SelectCellUp, window, cx| this.move_cell(0, -1, window, cx)))
            .on_action(cx.listener(|this, _: &crate::SelectCellDown, window, cx| this.move_cell(0, 1, window, cx)))
            .on_action(cx.listener(|this, _: &crate::SelectCellLeft, window, cx| this.move_cell(-1, 0, window, cx)))
            .on_action(cx.listener(|this, _: &crate::SelectCellRight, window, cx| this.move_cell(1, 0, window, cx)))
            .on_action(cx.listener(|this, _: &crate::NextSheet, window, cx| this.step_sheet(true, window, cx)))
            .on_action(cx.listener(|this, _: &crate::PreviousSheet, window, cx| this.step_sheet(false, window, cx)))
            .on_action(cx.listener(|this, _: &crate::ExtendCellsUp, _, cx| this.extend_cells(0, -1, cx)))
            .on_action(cx.listener(|this, _: &crate::ExtendCellsDown, _, cx| this.extend_cells(0, 1, cx)))
            .on_action(cx.listener(|this, _: &crate::ExtendCellsLeft, _, cx| this.extend_cells(-1, 0, cx)))
            .on_action(cx.listener(|this, _: &crate::ExtendCellsRight, _, cx| this.extend_cells(1, 0, cx)))
            .on_action(cx.listener(|this, _: &crate::ScrollLineUp, _, cx| this.scroll_lines(-1.0, cx)))
            .on_action(cx.listener(|this, _: &crate::ScrollLineDown, _, cx| this.scroll_lines(1.0, cx)))
            .on_action(cx.listener(|this, _: &crate::ScrollToStart, _, cx| this.scroll_to_end(false, cx)))
            .on_action(cx.listener(|this, _: &crate::ScrollToEnd, _, cx| this.scroll_to_end(true, cx)))
            .map(|d| Self::cell_jump_actions(d, cx))
            // The slide show takes the whole window, in place of the rest.
            .when_some(self.render_slideshow(window, cx), |d, show| d.child(show))
            .when(!self.show_in_main(), |d| self.render_window(d, window, cx))
            .when(self.zoom_dialog_open, |d| d.child(self.render_zoom_dialog(window, cx)))
            .when(self.save_confirm_dialog_open, |d| d.child(self.render_save_confirm_dialog(window, cx)))
    }
}

impl SlideEditor {
    /// The editor window's parts, into `d`: the title bar, and the start view
    /// or the chrome, the document and the status bar.
    fn render_window(&mut self, d: Stateful<Div>, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        // The mock's layout (shell/chrome.rs): the title row, the menu bar
        // and the toolbar (the first two hidden by the toolbar's ^), the
        // document with the sidebar on its right, and the status bar.
        if !self.has_document {
            window.set_window_title(&crate::shell::start_view::start_title());
            return d.child(self.render_title_row(cx)).child(self.render_start_view(cx));
        }
        let compact = self.chrome_compact;
        let focus = self.is_focus_mode();
        let body = with_format!(self, render_body(self, window, cx));
        let sidebar = if focus { None } else { self.render_sidebar(window, cx) };
        d.when(!compact && !focus, |d| d.child(self.render_title_row(cx)).child(self.render_menu_bar(window, cx)))
            .when(!focus, |d| d.child(self.render_toolbar(window, cx)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    // Text input goes to the format (shell/input.rs).
                    .child(div().relative().flex().flex_col().flex_1().min_w_0().min_h_0().child(body).child(crate::shell::input::input_region(cx)))
                    .children(sidebar),
            )
            .when(!focus, |d| d.child(self.render_status_bar(cx)))
    }
}

/// An element filling its (positioned) parent that calls `f` with the
/// parent's bounds in window coordinates at every prepaint. (gpui-kit's
/// `on_prepaint` adds an absolute child without an inset, which lands after
/// the parent's other children rather than at its origin.)
pub(crate) fn bounds_observer(f: impl FnOnce(Bounds<Pixels>, &mut Window, &mut App) + 'static) -> impl IntoElement {
    canvas(move |bounds, window, cx| f(bounds, window, cx), |_, _, _, _| {}).absolute().top_0().left_0().size_full()
}

/// An element filling the (positioned) workspace that hands the wheel over
/// it to `SlideEditor::on_wheel_zoom` before anything in the workspace
/// scrolls, and stops the wheel there when it zooms.
pub(crate) fn wheel_zoom_listener(cx: &Context<SlideEditor>) -> impl IntoElement {
    let editor = cx.entity().downgrade();
    let f = move |event: &ScrollWheelEvent, window: &mut Window, cx: &mut App| {
        editor.update(cx, |this, cx| this.on_wheel_zoom(event, window, cx)).unwrap_or(false)
    };
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && hitbox.is_hovered(window) && f(event, window, cx) {
                    cx.stop_propagation();
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::{MAX_ZOOM, MIN_ZOOM, parse_percent, wheel_zooms, zoom_axis_offset, zoom_levels};
    use gpui_kit::Modifiers;

    #[test]
    fn text_change_is_the_smallest_edit() {
        assert_eq!(super::text_change("abcdef", "abXdef"), (2, 1, "X".into()));
        assert_eq!(super::text_change("abc", "abc!"), (3, 0, "!".into()));
        assert_eq!(super::text_change("aaa", "aa"), (2, 1, String::new()));
        assert_eq!(super::text_change("こんにちは", "こんばんは"), (2, 2, "ばん".into()));
        assert_eq!(super::text_change("", "x"), (0, 0, "x".into()));
    }

    #[test]
    fn zoom_levels_step_from_100() {
        let levels = zoom_levels();
        assert_eq!(levels, [358, 299, 249, 207, 173, 144, 120, 100, 80, 64, 51, 41, 33, 26, 21, 17, 13, 11]);
        assert!(levels.iter().all(|&p| p as f32 <= MAX_ZOOM * 100.0 && p as f32 >= MIN_ZOOM * 100.0));
    }

    #[test]
    fn percent_is_read_with_or_without_the_sign() {
        assert_eq!(parse_percent("150"), Some(150.0));
        assert_eq!(parse_percent(" 72.5 % "), Some(72.5));
        assert_eq!(parse_percent("80％"), Some(80.0));
        assert_eq!(parse_percent("abc"), None);
        assert_eq!(parse_percent("0"), None);
        assert_eq!(parse_percent(""), None);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        // The page at 40 in the content, scrolled by -300: the pointer at 250
        // in the view is 510 into the page, and 1020 once the zoom doubles,
        // which the new scrolling brings under the pointer.
        let offset = zoom_axis_offset(250.0, 1000.0, -300.0, 40.0, 40.0, 2.0, 5000.0);
        assert_eq!(250.0 - offset - 40.0, 1020.0);
        // The scrolling stays within the content: at its start, and at its end.
        assert_eq!(zoom_axis_offset(500.0, 1000.0, 0.0, 40.0, 40.0, 0.5, 3000.0), 0.0);
        assert_eq!(zoom_axis_offset(990.0, 1000.0, -2000.0, 40.0, 40.0, 1.0, 2500.0), -1500.0);
    }

    #[test]
    fn the_zoom_modifier_follows_the_platform() {
        let ctrl = Modifiers { control: true, ..Default::default() };
        let cmd_ctrl = Modifiers { control: true, platform: true, ..Default::default() };
        let cmd = Modifiers { platform: true, ..Default::default() };
        assert!(wheel_zooms(&cmd_ctrl, true));
        assert!(!wheel_zooms(&ctrl, true));
        assert!(!wheel_zooms(&cmd, true));
        assert!(wheel_zooms(&ctrl, false));
        assert!(!wheel_zooms(&cmd, false));
        assert!(!wheel_zooms(&Modifiers::default(), false));
    }
}
