// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Documents (Word): laying the text out into pages, the pages and the
//! ruler, and the document's editing — the document担当's directory.
//!
//! See `shell/format.rs` for how the shell hands over to a format.

pub(crate) mod doc_ui;
pub(crate) mod editing;
pub(crate) mod text_format;
pub(crate) mod block_tree;
pub(crate) mod clipboard;
pub(crate) mod kinsoku_panel;
pub(crate) mod find;
pub(crate) mod wildcard;
pub(crate) mod comments;
pub(crate) mod link_panel;
pub(crate) mod index_fields;
pub(crate) mod merge_panel;
pub(crate) mod outline_ops;
pub(crate) mod ruby_panel;
pub(crate) mod text_box_panel;
pub(crate) mod symbol_panel;
pub(crate) mod style_panel;
pub(crate) mod paragraph_panel;
pub(crate) mod tab_panel;
pub(crate) mod color_panel;
pub(crate) mod font_panel;
pub(crate) mod paragraph_decorations;
pub(crate) mod picture_panel;
pub(crate) mod table_size_panel;
pub(crate) mod table_formula_panel;
pub(crate) mod help_panels;
pub(crate) mod list_formats;
mod style_new;
pub(crate) mod cell_borders;
pub(crate) mod cell_structure;
pub(crate) mod table_convert;
pub(crate) mod art_border;
pub(crate) mod flow_layout;
pub(crate) mod input;
pub(crate) mod page_view;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
pub(crate) mod scenarios;
pub(crate) mod toolbar;
pub(crate) mod accessibility;
pub(crate) mod goto;

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use ibis_doc_model::{FlowDocument, FlowPos, InsertFormat, Presentation, SlideId};

use crate::shell::chrome::{Menus, ToolItem};
use crate::shell::editor::{SlideEditor, WorkspaceContent, zoom_scale};
use crate::shell::format::{FormatEditor, FormatState};

pub(crate) use editing::key_bindings;

/// The formatting captured by the format painter brush.
#[derive(Clone, Debug)]
pub(crate) struct FormatBrush {
    pub(crate) run: ibis_doc_model::RunFormat,
    pub(crate) para: Option<ibis_doc_model::ParaFormat>,
    pub(crate) persistent: bool,
}

/// The tabs in Word's Navigation Pane (見出し・ページ・結果).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum NavigationTab {
    #[default]
    Headings,
    Pages,
    Results,
}

/// What only an open document has.
#[derive(Default)]
pub(crate) struct State {
    /// The characters of the text, for the status bar.
    pub(crate) characters: usize,
    /// A page to scroll to once the workspace is laid out.
    pub(crate) pending_page: Option<usize>,
    /// The caret, and the other end of the selection when there is one.
    pub(crate) caret: Option<FlowPos>,
    pub(crate) anchor: Option<FlowPos>,
    /// The other ranges of a selection made of several (⌘-click or ⌘-drag adds one;
    /// the caret and anchor are the last), each in document order (start, end).
    pub(crate) extra_selections: Vec<(FlowPos, FlowPos)>,
    /// Where ↑ / ↓ aim sideways (points), kept over a run of them.
    pub(crate) goal_x: Option<f32>,
    /// Formatting chosen with no selection, for the text typed next.
    pub(crate) pending: Option<InsertFormat>,
    /// Supported resolved character/paragraph properties captured by the brush.
    pub(crate) format_brush: Option<FormatBrush>,
    /// Text the input method is composing, shown at the caret.
    pub(crate) marked: Option<String>,
    /// The document the pages were last laid out from.
    pub(crate) laid_out: Option<Arc<FlowDocument>>,
    /// A drag selecting text is in progress.
    pub(crate) dragging: bool,
    /// 検索と置換's panel, once opened (and while it is the one shown).
    pub(crate) find: Option<find::FindState>,
    /// The link panel, while it is the one shown.
    pub(crate) link: Option<link_panel::LinkState>,
    /// スタイルの変更's panel, while it is the one shown.
    pub(crate) style_edit: Option<style_panel::StyleEditState>,
    pub(crate) cell_border_edge: Option<usize>,
    pub(crate) paragraph_border_edge: Option<usize>,
    pub(crate) font_edit: Option<font_panel::FontEditState>,
    pub(crate) color_edit: Option<color_panel::ColorEditState>,
    pub(crate) paragraph_edit: Option<paragraph_panel::ParagraphEditState>,
    pub(crate) tab_edit: Option<tab_panel::TabEditState>,
    pub(crate) picture_edit: Option<picture_panel::PictureEditState>,
    pub(crate) table_size: Option<table_size_panel::TableSizeState>,
    pub(crate) table_formula: Option<table_formula_panel::TableFormulaState>,
    /// The comments' panel, while it is the one shown.
    pub(crate) comments: Option<comments::CommentState>,
    /// ルビ's panel, once opened.
    pub(crate) ruby: Option<ruby_panel::RubyState>,
    /// テキスト ボックスの文字's panel, while it is the one shown.
    pub(crate) text_box: Option<text_box_panel::TextBoxState>,
    /// 差し込み印刷's panel and its data (宛先一覧).
    pub(crate) merge: Option<merge_panel::MergeState>,
    /// A floating text box being dragged.
    pub(crate) shape_drag: Option<text_box_panel::ShapeDrag>,
    /// 記号と特殊文字's panel, while it is the one shown.
    pub(crate) symbol: Option<symbol_panel::SymbolState>,
    /// The fix担当's panels (ページ設定).
    pub(crate) helper: help_panels::HelpPanels,
    /// Selection Pane (オブジェクトの選択と表示).
    pub(crate) selection_pane: bool,
    /// Accessibility Checker (アクセシビリティ チェック).
    pub(crate) accessibility_panel: bool,
    /// Word's Navigation Pane active tab (見出し / ページ / 結果).
    pub(crate) navigation_tab: NavigationTab,
    /// Word's Navigation Pane search query.
    pub(crate) nav_query: String,
    /// 読み取り専用の推奨の情報バーで「編集を有効にする」を押したか。
    pub(crate) read_only_dismissed: bool,
    /// 上書き入力モード (Word's Overtype mode).
    pub(crate) overtype: bool,
}

/// A document's editor: its pages are laid out from the text and shown
/// stacked in one scrolling column under a ruler, like Word's print layout.
pub(crate) struct Document;

impl FormatEditor for Document {
    const PAGE_NOUN: &'static str = "docx.mod.page";
    const FIT_TOOLTIP: &'static str = "docx.mod.zoom-page-width";
    /// The pages are laid out from the text; no text box is put on them.
    const SHAPES: bool = false;

    fn new_state(presentation: &Presentation, _: &mut Window, _: &mut Context<SlideEditor>) -> FormatState {
        let characters = presentation.flow.as_deref().map_or(0, doc_ui::count_characters);
        // The caret starts at the beginning, as in Word.
        let caret = presentation.flow.as_deref().and_then(|d| {
            d.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(FlowPos::new(p.id, 0)) } else { None })
        });
        FormatState::Document(State { characters, caret, laid_out: presentation.flow.clone(), ..State::default() })
    }

    /// At 100%, as in Word.
    fn initial_zoom(_: &SlideEditor) -> Option<f32> {
        Some(1.0)
    }

    fn workspace_content(editor: &SlideEditor, workspace: Size<Pixels>) -> WorkspaceContent {
        doc_ui::flow_content(workspace, &editor.page_sizes(), editor.zoom.map(zoom_scale), editor.page_gap(), editor.page_movement_side_to_side)
    }

    /// The page in view keeps its place as the zoom changes (and when the
    /// pages are fitted to the width).
    fn zoom_scroll_y(editor: &SlideEditor, before: &WorkspaceContent, after: &WorkspaceContent, old: Pixels, view: Pixels, anchor: Pixels) -> Option<Pixels> {
        Some(editor.flow_zoom_offset(before, after, old, view, anchor))
    }

    /// PageDown / PageUp scroll to the next or previous page.
    fn step_page(editor: &mut SlideEditor, forward: bool, _: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.step_page(forward, cx);
        true
    }

    /// The pages are in one column: scroll to the page.
    fn show_page(editor: &mut SlideEditor, slide: SlideId, _: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        if let Some(index) = editor.presentation().slide_index(slide) {
            editor.current_slide = slide;
            editor.selection = None;
            editor.scroll_to_page(index, cx);
        }
        true
    }

    /// The page in view is the current one (for the status bar).
    fn before_render(editor: &mut SlideEditor, _: &mut Window, _: &mut Context<SlideEditor>) {
        editor.relayout_if_changed();
        let index = editor.visible_page();
        if let Some(slide) = editor.presentation().slides.get(index) {
            editor.current_slide = slide.id;
        }
    }

    /// Word's print layout: the ruler over the pages, and no page list.
    fn render_body(editor: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>) -> AnyElement {
        let is_focus = editor.is_focus_mode();
        let mut pages = div().flex().flex_col().flex_1().min_w_0().min_h_0();
        if let Some(bar) = editor.render_docx_infobar(cx) {
            pages = pages.child(bar);
        }
        let show_v_ruler = !is_focus && editor.ruler_open && editor.vertical_ruler_open;
        let theme = cx.theme().clone();
        if !is_focus && editor.ruler_open {
            if show_v_ruler {
                let corner = SlideEditor::render_ruler_corner(theme.border, editor.ruler_unit)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.cycle_ruler_unit(cx)));
                let h_ruler = editor.render_ruler(theme.border, theme.font_family.clone());
                let ruler_row = div()
                    .flex()
                    .flex_row()
                    .flex_shrink_0()
                    .w_full()
                    .child(corner)
                    .child(div().flex_1().min_w_0().child(h_ruler));
                pages = pages.child(ruler_row);
            } else {
                pages = pages.child(editor.render_ruler(theme.border, theme.font_family.clone()));
            }
        }
        let mut content_row = div().flex().flex_1().min_w_0().min_h_0();
        if show_v_ruler {
            content_row = content_row.flex_row().child(editor.render_vertical_ruler(theme.border, theme.font_family.clone()));
        } else {
            content_row = content_row.flex_col();
        }
        content_row = content_row.child(editor.render_document_workspace(cx));
        let pages = pages.child(content_row);
        // Word's Navigation Pane (ナビゲーション ウィンドウ):
        // left of the pages: Headings (見出し), Pages (ページ), Results (検索結果).
        let outline = (!is_focus && editor.outline_open).then(|| {
            let tab = editor.doc_state().map_or(NavigationTab::Headings, |s| s.navigation_tab);
            let nav_query = editor.doc_state().map(|s| s.nav_query.clone()).unwrap_or_default();

            let tab_btn = |id_str: &'static str, label: &'static str, active: bool, target: NavigationTab| {
                let mut b = div()
                    .id(id_str)
                    .px(px(8.))
                    .py(px(4.))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .rounded_md();
                if active {
                    b = b.bg(rgb(0xe0e0e0)).font_weight(FontWeight::SEMIBOLD);
                } else {
                    b = b.hover(|h| h.bg(rgb(0xf0f0f0)));
                }
                b.child(crate::shell::i18n::text(label))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_navigation_tab(target, cx);
                    }))
            };

            let tabs = div()
                .flex()
                .flex_row()
                .gap_1()
                .px(px(12.))
                .pb(px(8.))
                .child(tab_btn("nav-tab-headings", "docx.navigation.headings", tab == NavigationTab::Headings, NavigationTab::Headings))
                .child(tab_btn("nav-tab-pages", "docx.navigation.pages", tab == NavigationTab::Pages, NavigationTab::Pages))
                .child(tab_btn("nav-tab-results", "docx.navigation.results", tab == NavigationTab::Results, NavigationTab::Results));

            let content: AnyElement = match tab {
                NavigationTab::Headings => {
                    let items = editor.outline();
                    if items.is_empty() {
                        div().px(px(16.)).py(px(8.)).text_size(px(12.)).text_color(rgb(0x888888))
                            .child(crate::shell::i18n::text("docx.navigation.no-headings"))
                            .into_any_element()
                    } else {
                        let elements: Vec<AnyElement> = items
                            .into_iter()
                            .enumerate()
                            .map(|(i, (level, text, id))| {
                                // ▾ open / ▸ closed, for a heading with something under it.
                                let has_content = editor.heading_has_content(id);
                                let collapsed = editor.heading_collapsed(id);
                                let marker = div()
                                    .id(("outline-fold", i))
                                    .w(px(14.))
                                    .text_size(px(11.));
                                let marker = if has_content {
                                    marker.cursor_pointer().child(if collapsed { "▸" } else { "▾" }).on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        let now = this.heading_collapsed(id);
                                        this.set_heading_collapsed(id, !now, cx);
                                    }))
                                } else {
                                    marker
                                };
                                div()
                                    .id(("outline", i))
                                    .flex()
                                    .flex_row()
                                    .pl(px(6.0 + 14.0 * (level.saturating_sub(1)) as f32))
                                    .pr(px(8.))
                                    .py(px(4.))
                                    .text_size(px(13.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0xe8eaed)))
                                    .child(marker)
                                    .child(SharedString::from(text))
                                    .on_click(cx.listener(move |this, _, _, cx| this.jump_to(id, cx)))
                                    .into_any_element()
                            })
                            .collect();
                        // Promote, demote and move the heading under the caret.
                        let op = |id: &'static str, glyph: &'static str, f: fn(&mut SlideEditor, &mut Context<SlideEditor>)| {
                            div().id(id).px(px(8.)).py(px(2.)).rounded_md().cursor_pointer().hover(|s| s.bg(rgb(0xe8eaed))).child(glyph)
                                .on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
                        };
                        let ops = div().flex().flex_row().gap_1().px(px(12.)).pb(px(4.))
                            .child(op("outline-promote", "←", |e, cx| { e.change_heading_level(-1, cx); }))
                            .child(op("outline-demote", "→", |e, cx| { e.change_heading_level(1, cx); }))
                            .child(op("outline-up", "↑", |e, cx| { e.move_heading(true, cx); }))
                            .child(op("outline-down", "↓", |e, cx| { e.move_heading(false, cx); }));
                        div().child(ops).children(elements).into_any_element()
                    }
                }
                NavigationTab::Pages => {
                    let total_pages = editor.page_sizes().len().max(1);
                    let page_cards: Vec<AnyElement> = (0..total_pages)
                        .map(|page_idx| {
                            let label = crate::shell::i18n::format("_docx.page-page", &[("page", &(page_idx + 1))]);
                            div()
                                .id(("nav-page", page_idx))
                                .mx(px(12.))
                                .my(px(4.))
                                .p(px(8.))
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(0xdddddd))
                                .bg(rgb(0xffffff))
                                .hover(|s| s.bg(rgb(0xf5f5f5)).border_color(rgb(0x1a73e8)))
                                .cursor_pointer()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(label)
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.scroll_to_page(page_idx, cx);
                                }))
                                .into_any_element()
                        })
                        .collect();
                    div().children(page_cards).into_any_element()
                }
                NavigationTab::Results => {
                    let flow = editor.flow();
                    let matches = flow.map(|f| find::find_all(f, &nav_query)).unwrap_or_default();
                    let count_text = if nav_query.is_empty() {
                        crate::shell::i18n::text("docx.navigation.search-prompt").to_string()
                    } else {
                        crate::shell::i18n::format("_docx.count-results", &[("count", &matches.len().to_string())])
                    };

                    let result_cards: Vec<AnyElement> = matches
                        .into_iter()
                        .enumerate()
                        .take(50)
                        .map(|(i, (from, to))| {
                            let snippet = flow.and_then(|f| {
                                for b in &f.body {
                                    if let ibis_doc_model::Block::Paragraph(p) = b {
                                        if p.id == from.para {
                                            let s: String = p.runs.iter().map(|r| match &r.kind {
                                                ibis_doc_model::FlowRunKind::Text(t) => t.as_str(),
                                                ibis_doc_model::FlowRunKind::RichText { text }
                                                | ibis_doc_model::FlowRunKind::PlainText { text } => text.as_str(),
                                                _ => "",
                                            }).collect();
                                            return Some(s);
                                        }
                                    }
                                }
                                None
                            }).unwrap_or_default();

                            let display_text = if snippet.len() > 60 {
                                format!("{}...", &snippet[..snippet.floor_char_boundary(60)])
                            } else {
                                snippet
                            };

                            div()
                                .id(("nav-match", i))
                                .mx(px(12.))
                                .my(px(3.))
                                .p(px(6.))
                                .rounded_sm()
                                .border_1()
                                .border_color(rgb(0xeeeeee))
                                .hover(|s| s.bg(rgb(0xe8f0fe)).border_color(rgb(0x1a73e8)))
                                .cursor_pointer()
                                .child(div().text_size(px(12.)).child(display_text))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.jump_to_range(from, to, cx);
                                }))
                                .into_any_element()
                        })
                        .collect();

                    div()
                        .child(
                            div()
                                .px(px(12.))
                                .py(px(4.))
                                .text_size(px(12.))
                                .text_color(rgb(0x666666))
                                .child(count_text)
                        )
                        .children(result_cards)
                        .into_any_element()
                }
            };

            div()
                .id("docx-outline")
                .w(px(240.))
                .h_full()
                .flex_shrink_0()
                .overflow_y_scroll()
                .py(px(8.))
                .border_r_1()
                .border_color(rgb(0xe0e0e0))
                .bg(rgb(0xfafafa))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .items_center()
                        .px(px(12.))
                        .pb(px(6.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::BOLD)
                                .child(crate::shell::i18n::text("docx.navigation.title"))
                        )
                        .child(
                            div()
                                .id("docx-nav-close")
                                .text_size(px(12.))
                                .cursor_pointer()
                                .text_color(rgb(0x888888))
                                .hover(|s| s.text_color(rgb(0x000000)))
                                .child("✕")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.outline_open = false;
                                    cx.notify();
                                }))
                        )
                )
                .child(tabs)
                .child(content)
        });
        let exit_focus_btn = if is_focus {
            Some(
                div()
                    .id("docx-focus-mode-exit")
                    .absolute()
                    .top(px(16.))
                    .right(px(24.))
                    .px(px(14.))
                    .py(px(6.))
                    .rounded_md()
                    .bg(hsla(0.0, 0.0, 0.1, 0.75))
                    .hover(|h| h.bg(hsla(0.0, 0.0, 0.1, 0.9)))
                    .text_size(px(12.))
                    .text_color(rgb(0xffffff))
                    .cursor_pointer()
                    .child(crate::shell::i18n::text("docx.focus.exit"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_focus_mode(false, window, cx);
                    })),
            )
        } else {
            None
        };
        div().flex().flex_row().flex_1().min_h_0().relative().children(outline).child(pages).children(exit_focus_btn).into_any_element()
    }

    /// Typing and the input method go to the document while it has a caret.
    fn wants_text_input(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.doc_accepts_text_input(window, cx)
    }

    fn text_for_range(editor: &mut SlideEditor, range: Range<usize>, adjusted: &mut Option<Range<usize>>, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<String> {
        editor.doc_text_for_range(range, adjusted, window, cx)
    }

    fn selected_text_range(editor: &mut SlideEditor, ignore_disabled_input: bool, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<UTF16Selection> {
        editor.doc_selected_text_range(ignore_disabled_input, window, cx)
    }

    fn marked_text_range(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<Range<usize>> {
        editor.doc_marked_text_range(window, cx)
    }

    fn unmark_text(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.doc_unmark_text(window, cx)
    }

    fn replace_text_in_range(editor: &mut SlideEditor, range: Option<Range<usize>>, text: &str, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.doc_replace_text_in_range(range, text, window, cx)
    }

    fn replace_and_mark_text_in_range(editor: &mut SlideEditor, range: Option<Range<usize>>, text: &str, selected: Option<Range<usize>>, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.doc_replace_and_mark_text_in_range(range, text, selected, window, cx)
    }

    fn bounds_for_range(editor: &mut SlideEditor, range: Range<usize>, element: Bounds<Pixels>, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<Bounds<Pixels>> {
        editor.doc_bounds_for_range(range, element, window, cx)
    }

    fn character_index_for_point(editor: &mut SlideEditor, point: Point<Pixels>, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<usize> {
        editor.doc_character_index_for_point(point, window, cx)
    }

    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn scenarios() -> Vec<(&'static str, fn())> {
        scenarios::all()
    }

    /// The sidebar: 検索と置換's panel when it is open.
    fn sidebar(editor: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>) -> Option<crate::shell::chrome::SidebarPanel> {
        editor.font_panel(cx).or_else(|| editor.color_panel(cx)).or_else(|| editor.table_size_panel(cx)).or_else(|| editor.table_formula_panel(cx)).or_else(|| editor.picture_panel(cx)).or_else(|| editor.paragraph_panel(cx)).or_else(|| editor.tab_panel(cx)).or_else(|| editor.comments_panel(cx)).or_else(|| editor.style_panel(cx)).or_else(|| editor.link_panel(cx)).or_else(|| editor.find_panel(cx)).or_else(|| editor.ruby_panel(cx)).or_else(|| editor.text_box_panel(cx)).or_else(|| editor.merge_panel(cx)).or_else(|| editor.doc_page_setup_panel(cx)).or_else(|| editor.notes_panel(cx)).or_else(|| editor.bookmark_panel(cx)).or_else(|| editor.watermark_panel(cx)).or_else(|| editor.new_style_panel(cx)).or_else(|| editor.symbol_panel(cx)).or_else(|| editor.word_count_panel(cx)).or_else(|| editor.field_panel(cx)).or_else(|| editor.caption_panel(cx)).or_else(|| editor.doc_properties_panel(cx)).or_else(|| editor.cross_reference_panel(cx)).or_else(|| editor.selection_pane_panel(cx)).or_else(|| editor.accessibility_panel(cx)).or_else(|| editor.update_toc_panel(cx)).or_else(|| editor.user_info_panel(cx)).or_else(|| editor.kinsoku_panel(cx)).or_else(|| editor.go_to_panel(cx))
    }

    /// The title bar's コメントを追加: a comment on the selection.
    fn add_comment(_: &SlideEditor) -> Option<crate::shell::chrome::Handler> {
        Some(std::rc::Rc::new(|this: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>| this.open_comments(window, cx)))
    }

    /// The title bar's コメントを表示: the comments' panel shown or hidden.
    fn show_comments(editor: &SlideEditor) -> Option<(bool, crate::shell::chrome::Handler)> {
        let shown = editor.sidebar_open && editor.doc_state().is_some_and(|s| s.comments.is_some());
        Some((
            shown,
            std::rc::Rc::new(move |this: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>| {
                if shown {
                    if let Some(state) = this.doc_state_mut() {
                        state.comments = None;
                    }
                    cx.notify();
                } else {
                    this.open_comments(window, cx);
                }
            }),
        ))
    }

    fn menus(editor: &SlideEditor, menus: &mut Menus) {
        toolbar::menus(editor, menus);
    }

    fn format_paint(editor: &SlideEditor) -> Option<(bool, crate::shell::chrome::Handler)> {
        let on = editor.doc_state()?.format_brush.is_some();
        Some((on, std::rc::Rc::new(|this, _, cx| this.toggle_doc_format_brush(cx))))
    }

    fn toolbar_right(editor: &SlideEditor) -> Option<Vec<ToolItem>> {
        Some(toolbar::toolbar_right(editor))
    }

    fn status_items(editor: &SlideEditor) -> Vec<SharedString> {
        let characters = editor.doc_state().map_or(0, |s| s.characters);
        let mode = if editor.is_overtype() {
            crate::shell::i18n::text("docx.status.overtype")
        } else {
            crate::shell::i18n::text("docx.status.insert")
        };
        let mut items = vec![
            crate::shell::i18n::format("docx.mod.n-characters", &[("n", &crate::shell::ribbon::thousands(characters))]).into(),
            mode,
        ];
        match editor.interaction_mode() {
            ibis_doc_model::DocumentInteractionMode::Reviewing => {
                items.push(crate::shell::i18n::text("docx.status.interaction-reviewing"));
            }
            ibis_doc_model::DocumentInteractionMode::Viewing => {
                items.push(crate::shell::i18n::text("docx.status.interaction-viewing"));
            }
            ibis_doc_model::DocumentInteractionMode::Editing => {}
        }
        match editor.document_view_mode() {
            ibis_doc_model::DocumentViewMode::Reading => items.push(crate::shell::i18n::text("docx.status.read-mode")),
            ibis_doc_model::DocumentViewMode::Web => items.push(crate::shell::i18n::text("docx.status.web-layout")),
            ibis_doc_model::DocumentViewMode::Outline => items.push(crate::shell::i18n::text("docx.status.outline")),
            ibis_doc_model::DocumentViewMode::Draft => items.push(crate::shell::i18n::text("docx.status.draft")),
            ibis_doc_model::DocumentViewMode::Print => {}
        }
        items
    }
}

impl SlideEditor {
    /// Lay the pages out again now (after a change of how they are shown).
    pub(crate) fn relayout_now(&mut self, cx: &mut Context<SlideEditor>) {
        if let Some(state) = self.doc_state_mut() {
            state.laid_out = None;
        }
        self.relayout_if_changed();
        cx.notify();
    }

    /// After an edit (or undo), lay the pages out again and keep the caret
    /// in the document.
    pub(crate) fn relayout_if_changed(&mut self) {
        let Some(flow) = self.presentation().flow.clone() else { return };
        if self.doc_state().is_none_or(|s| s.laid_out.as_ref().is_some_and(|l| Arc::ptr_eq(l, &flow))) {
            return;
        }
        let resources = &self.resources;
        let started = crate::debug_timing::start();
        flow_layout::expand_pages(self.document.ids(), resources, None);
        crate::debug_timing::report("layout", started);
        if self.presentation().slide(self.current_slide).is_none() {
            self.current_slide = self.presentation().slides[0].id;
        }
        let characters = doc_ui::count_characters(&flow);
        let valid = |pos: Option<FlowPos>| {
            let pos = pos?;
            let p = ibis_doc_model::flow_command::paragraph(&flow, pos.para)?;
            Some(FlowPos::new(pos.para, pos.offset.min(p.len())))
        };
        let first = flow.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(FlowPos::new(p.id, 0)) } else { None });
        if let Some(state) = self.doc_state_mut() {
            state.laid_out = Some(flow.clone());
            state.characters = characters;
            state.caret = valid(state.caret).or(first);
            state.anchor = valid(state.anchor);
        }
        self.normalize_visible_selection();
    }

    /// Word's Navigation Pane: switch active tab (Headings / Pages / Results).
    pub(crate) fn set_navigation_tab(&mut self, tab: NavigationTab, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.navigation_tab = tab;
        }
        cx.notify();
    }

    /// Word's Navigation Pane: get active tab.
    #[allow(dead_code)]
    pub(crate) fn navigation_tab(&self) -> NavigationTab {
        self.doc_state().map_or(NavigationTab::Headings, |s| s.navigation_tab)
    }

    /// Word's Navigation Pane: set search query.
    #[allow(dead_code)]
    pub(crate) fn set_nav_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.nav_query = query.to_string();
        }
        cx.notify();
    }

    /// Word's Navigation Pane: get search query.
    #[allow(dead_code)]
    pub(crate) fn nav_query(&self) -> String {
        self.doc_state().map(|s| s.nav_query.clone()).unwrap_or_default()
    }

    /// Jump to a range (selecting from..to) and scroll to it.
    pub(crate) fn jump_to_range(&mut self, from: FlowPos, to: FlowPos, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.anchor = Some(from);
            state.caret = Some(to);
        }
        self.scroll_to_caret(cx);
        cx.notify();
    }
}

