// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The window's chrome, as the UI mock has it (`tasks/doc/design/toolbar.html`):
//! the title row (36 px), the menu bar (28 px), the one-row toolbar (44 px)
//! and the sidebar on the right; the status bar is in `status_bar.rs`.
//!
//! What the menus, the toolbar and the sidebar hold is registered, not
//! written here. The shell builds each menu with the items every format
//! shares (and placeholders for the rest), then hands the menus to the
//! format (`FormatEditor::menus`), which adds items or connects them. The
//! toolbar's right half and the sidebar are the format's
//! (`FormatEditor::toolbar_right`, `FormatEditor::sidebar`); until a format
//! gives its own, `chrome_defaults.rs` has one per format made of what the
//! editor can already do.
//!
//! Registration, from a format's `FormatEditor` implementation:
//!
//! ```ignore
//! fn menus(editor: &SlideEditor, menus: &mut Menus) {
//!     // Connect a placeholder the shell put in (see `chrome_defaults.rs` for the ids)…
//!     menus.connect("xlsx.insert-row-above", |editor, window, cx| editor.insert_rows(true, window, cx));
//!     // …or add an item.
//!     menus.items_mut(MenuId::Data).push(MenuItem::tr("xlsx.sort").icon("sort").on(…).into());
//! }
//! fn toolbar_right(editor: &SlideEditor) -> Option<Vec<ToolItem>> {
//!     Some(vec![ToolItem::Separator, ToolButton::new("bold", "format_bold", "_shell.bold-b").on(bold).click(…).into()])
//! }
//! ```
//!
//! An item with no handler is shown disabled, so a placeholder is visible
//! before it works.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::shell::format::with_format;
use crate::shell::icons::Material;

// ----- Colours and sizes of the mock ---------------------------------------

pub(crate) const APP_BG: u32 = 0xf8fafd;
/// The background of the primary button (the main action of a panel or dialog: 適用, OK, 挿入, 保存…), one colour for the
/// whole app. The text on it is white. A button is made primary with `.primary()`; this sets what that looks like.
pub(crate) const PRIMARY_BUTTON_BG: u32 = 0x2d599f;
/// The primary button's background under the pointer and while pressed.
const PRIMARY_BUTTON_HOVER_BG: u32 = 0x3a6bb8;
const PRIMARY_BUTTON_ACTIVE_BG: u32 = 0x234a85;
/// The icon in a menu row is drawn this many times larger than the menu's own icon size (12 px).
pub(crate) const MENU_ICON_SCALE: f32 = 1.4;
const TOOLBAR_BG: u32 = 0xedf2f7;
const ICON: u32 = 0x444746;
pub(crate) const TEXT: u32 = 0x1f1f1f;
const TEXT_SECONDARY: u32 = 0x5f6368;
const DIVIDER: u32 = 0xc7c7c7;
const BORDER: u32 = 0xdadce0;
const SIDEBAR_WIDTH: f32 = 300.0;

fn disabled_color() -> Hsla {
    hsla(0.0, 0.0, 0.12, 0.38)
}

// ----- What is registered --------------------------------------------------

/// What a menu item or toolbar button does.
pub(crate) type Handler = Rc<dyn Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>)>;

/// The menus of the menu bar, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MenuId {
    File,
    Edit,
    View,
    Insert,
    Format,
    Data,
    Slide,
    Arrange,
    Tools,
    Help,
}

impl MenuId {
    /// Its key in the tables of the UI's words.
    pub(crate) fn key(self) -> &'static str {
        match self {
            MenuId::File => "menu.file",
            MenuId::Edit => "menu.edit",
            MenuId::View => "menu.view",
            MenuId::Insert => "menu.insert",
            MenuId::Format => "menu.format",
            MenuId::Data => "menu.data",
            MenuId::Slide => "menu.slide",
            MenuId::Arrange => "menu.arrange",
            MenuId::Tools => "menu.tools",
            MenuId::Help => "menu.help",
        }
    }

    /// Its name in the UI's language.
    pub(crate) fn label(self) -> SharedString {
        crate::shell::i18n::text(self.key())
    }
}

/// A line of a menu.
#[derive(Clone)]
pub(crate) enum MenuEntry {
    Item(MenuItem),
    Separator,
    Submenu(Submenu),
    /// The squares of a colour palette (`color_palette.rs`).
    Palette(crate::shell::color_palette::Palette),
}

/// A menu item. It is enabled when it has a handler and is not disabled.
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct MenuItem {
    /// For finding it (`Menus::connect`), such as `"xlsx.insert-row-above"`.
    pub id: &'static str,
    pub icon: Option<&'static str>,
    pub label: SharedString,
    /// The shortcut shown, such as `"⌘S"`.
    pub shortcut: Option<&'static str>,
    pub checked: bool,
    pub disabled: bool,
    pub handler: Option<Handler>,
}

impl MenuItem {
    pub(crate) fn new(id: &'static str, label: impl Into<SharedString>) -> Self {
        Self { id, icon: None, label: label.into(), shortcut: None, checked: false, disabled: false, handler: None }
    }

    /// An item labelled in the UI's language by its id (`i18n/*.tsv`).
    pub(crate) fn tr(id: &'static str) -> Self {
        Self::new(id, crate::shell::i18n::text(id))
    }

    pub(crate) fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }

    pub(crate) fn shortcut(mut self, shortcut: &'static str) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    pub(crate) fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn on(mut self, f: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> Self {
        self.handler = Some(Rc::new(f));
        self
    }

    pub(crate) fn enabled(&self) -> bool {
        self.handler.is_some() && !self.disabled
    }
}

impl From<MenuItem> for MenuEntry {
    fn from(item: MenuItem) -> Self {
        MenuEntry::Item(item)
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct Submenu {
    pub id: &'static str,
    pub icon: Option<&'static str>,
    pub label: SharedString,
    pub items: Vec<MenuEntry>,
}

impl Submenu {
    pub(crate) fn new(id: &'static str, icon: &'static str, label: impl Into<SharedString>, items: Vec<MenuEntry>) -> Self {
        Self { id, icon: Some(icon), label: label.into(), items }
    }

    /// A submenu labelled in the UI's language by its id.
    pub(crate) fn tr(id: &'static str, icon: &'static str, items: Vec<MenuEntry>) -> Self {
        Self::new(id, icon, crate::shell::i18n::text(id), items)
    }
}

impl From<Submenu> for MenuEntry {
    fn from(submenu: Submenu) -> Self {
        MenuEntry::Submenu(submenu)
    }
}

/// The menu bar's menus, as built for the open document.
#[derive(Clone, Default)]
#[allow(dead_code)] // The registration API: used by the formats' code.
pub(crate) struct Menus {
    pub menus: Vec<(MenuId, Vec<MenuEntry>)>,
}

#[allow(dead_code)] // The registration API: used by the formats' code.
impl Menus {
    /// The items of menu `id` (an empty menu is added if it is not there).
    pub(crate) fn items_mut(&mut self, id: MenuId) -> &mut Vec<MenuEntry> {
        if let Some(i) = self.menus.iter().position(|(m, _)| *m == id) {
            return &mut self.menus[i].1;
        }
        self.menus.push((id, Vec::new()));
        &mut self.menus.last_mut().expect("just pushed").1
    }

    /// A copy of the entry (an item or a submenu) with id `id`, in any menu or submenu.
    pub(crate) fn find_entry(&self, id: &str) -> Option<MenuEntry> {
        fn find(entries: &[MenuEntry], id: &str) -> Option<MenuEntry> {
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) if item.id == id => return Some(entry.clone()),
                    MenuEntry::Palette(p) if p.find(id).is_some() => return p.find(id).cloned().map(MenuEntry::Item),
                    MenuEntry::Submenu(sub) => {
                        if sub.id == id {
                            return Some(entry.clone());
                        }
                        if let Some(found) = find(&sub.items, id) {
                            return Some(found);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        self.menus.iter().find_map(|(_, entries)| find(entries, id))
    }

    /// The item with id `id`, in any menu or submenu.
    pub(crate) fn find_mut(&mut self, id: &str) -> Option<&mut MenuItem> {
        fn find<'a>(entries: &'a mut [MenuEntry], id: &str) -> Option<&'a mut MenuItem> {
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) if item.id == id => return Some(item),
                    MenuEntry::Palette(p) => {
                        if let Some(found) = p.find_mut(id) {
                            return Some(found);
                        }
                    }
                    MenuEntry::Submenu(sub) => {
                        if let Some(found) = find(&mut sub.items, id) {
                            return Some(found);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        self.menus.iter_mut().find_map(|(_, entries)| find(entries, id))
    }

    /// Give the item with id `id` its handler (it becomes enabled).
    /// Whether the item was found.
    pub(crate) fn connect(&mut self, id: &str, f: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> bool {
        match self.find_mut(id) {
            Some(item) => {
                item.handler = Some(Rc::new(f));
                true
            }
            None => false,
        }
    }
}

/// A button of the toolbar.
#[derive(Clone)]
pub(crate) struct ToolButton {
    pub id: &'static str,
    pub icon: Option<&'static str>,
    /// A text button (`背景`, `¥`) instead of, or after, the icon.
    pub text: Option<SharedString>,
    pub tooltip: SharedString,
    /// Pressed (a toggle that is on, or the tool in use).
    pub on: bool,
    pub disabled: bool,
    /// A colour bar under the icon (text colour, fill colour).
    pub color: Option<Hsla>,
    /// A menu opened by the button (a ▼ is drawn after the icon).
    pub menu: Option<Vec<MenuEntry>>,
    pub handler: Option<Handler>,
    /// A split button (as Office's): what the left half does at once (the last used choice), the ▼ on the
    /// right having the menu.
    pub quick: Option<Handler>,
}

impl ToolButton {
    pub(crate) fn new(id: &'static str, icon: &'static str, tooltip: impl Into<SharedString>) -> Self {
        Self { id, icon: Some(icon), text: None, tooltip: tooltip.into(), on: false, disabled: false, color: None, menu: None, handler: None, quick: None }
    }

    /// A text button.
    pub(crate) fn text(id: &'static str, text: impl Into<SharedString>, tooltip: impl Into<SharedString>) -> Self {
        Self { id, icon: None, text: Some(text.into()), tooltip: tooltip.into(), on: false, disabled: false, color: None, menu: None, handler: None, quick: None }
    }

    pub(crate) fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// A button of a palette (`color_palette.rs`): its left half sets the colour last picked from it in this window
    /// (else `default`) and shows it as the bar under the icon; the ▼ opens the palette. `id` names the button's
    /// last colour.
    pub(crate) fn palette(mut self, editor: &SlideEditor, id: &'static str, default: u32, mut items: Vec<MenuEntry>) -> Self {
        let last = editor.last_colors.get(id).copied().unwrap_or(ibis_doc_model::Rgb(default));
        let mut pick = None;
        for entry in &mut items {
            if let MenuEntry::Palette(p) = entry {
                p.button = Some(id);
                pick = p.pick.clone();
                // Picking a square (here or by the scenarios) is the button's last colour from then on.
                for swatch in p.theme.iter_mut().flatten().chain(p.standard.iter_mut()) {
                    let (rgb, inner) = (swatch.rgb, swatch.item.handler.clone());
                    swatch.item.handler = inner.map(|inner| -> Handler {
                        Rc::new(move |e, w, cx| {
                            e.last_colors.insert(id, rgb);
                            inner(e, w, cx)
                        })
                    });
                }
            }
        }
        self.color = Some(rgb(last.0).into());
        self.quick = pick.map(|pick| -> Handler { Rc::new(move |e, w, cx| pick(e, Some(last), w, cx)) });
        self.menu = Some(items);
        self
    }

    pub(crate) fn menu(mut self, items: Vec<MenuEntry>) -> Self {
        self.menu = Some(items);
        self
    }

    pub(crate) fn click(mut self, f: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> Self {
        self.handler = Some(Rc::new(f));
        self
    }

    pub(crate) fn enabled(&self) -> bool {
        !self.disabled && (self.handler.is_some() || self.menu.as_ref().is_some_and(|m| !m.is_empty()))
    }

    /// The width it takes in the toolbar, in pixels (for folding into ︙).
    fn width(&self) -> f32 {
        let text = self.text.as_ref().map_or(0.0, |t| text_width(t, 14.0) + 12.0);
        let icon = if self.icon.is_some() { 26.0 } else { 0.0 };
        let caret = if self.menu.is_some() { 14.0 } else { 0.0 };
        (icon + text + caret).max(28.0)
    }
}

impl From<ToolButton> for ToolItem {
    fn from(button: ToolButton) -> Self {
        ToolItem::Button(button)
    }
}

/// A part of the toolbar.
#[derive(Clone)]
pub(crate) enum ToolItem {
    Button(ToolButton),
    Separator,
    /// A drop-down box with its value shown (フォント, スタイル).
    Select { id: &'static str, value: SharedString, width: f32, tooltip: SharedString, menu: Vec<MenuEntry>, disabled: bool },
    /// − [size] + : the font size.
    FontSize { value: Option<SharedString>, smaller: Option<Handler>, larger: Option<Handler> },
}

impl ToolItem {
    fn width(&self) -> f32 {
        match self {
            ToolItem::Button(b) => b.width(),
            ToolItem::Separator => 9.0,
            ToolItem::Select { width, .. } => *width,
            ToolItem::FontSize { .. } => 96.0,
        }
    }
}

/// A tab of the sidebar's top (a format's panels that stand side by side):
/// its label, the title of the panel it shows (to tell which is selected)
/// and what pressing it does.
pub(crate) struct SidebarTab {
    pub label: &'static str,
    pub panel_title: &'static str,
    pub open: Handler,
}

/// The sidebar's content.
pub(crate) struct SidebarPanel {
    pub title: SharedString,
    pub body: AnyElement,
}

/// A rough width of `text` at `size` px: full-width characters are square.
fn text_width(text: &str, size: f32) -> f32 {
    text.chars().map(|c| if c.is_ascii() { size * 0.58 } else { size }).sum()
}

// ----- Drawing -------------------------------------------------------------

/// Make the theme's primary button the app's one colour ([`PRIMARY_BUTTON_BG`]).
pub(crate) fn apply_primary_button_colors(theme: &mut gpui_kit::component::Theme) {
    let (normal, hover, active, text): (Hsla, Hsla, Hsla, Hsla) = (rgb(PRIMARY_BUTTON_BG).into(), rgb(PRIMARY_BUTTON_HOVER_BG).into(), rgb(PRIMARY_BUTTON_ACTIVE_BG).into(), rgb(0xffffff).into());
    // The colours, and the tokens the buttons read (made from the colours when the theme was chosen).
    theme.primary = normal;
    theme.primary_hover = hover;
    theme.primary_active = active;
    theme.primary_foreground = text;
    theme.button_primary = normal;
    theme.button_primary_hover = hover;
    theme.button_primary_active = active;
    theme.button_primary_foreground = text;
    theme.tokens.primary = normal.into();
    theme.tokens.primary_hover = hover.into();
    theme.tokens.primary_active = active.into();
    theme.tokens.primary_foreground = text.into();
    theme.tokens.button_primary = normal.into();
    theme.tokens.button_primary_hover = hover.into();
    theme.tokens.button_primary_active = active.into();
    theme.tokens.button_primary_foreground = text.into();
}

/// The icon of a menu row: the menu draws it at its own small size, so it is scaled up about its centre.
fn menu_icon(name: &'static str) -> Icon {
    Icon::new(Material(name)).transform(gpui::Transformation::scale(gpui::size(MENU_ICON_SCALE, MENU_ICON_SCALE)))
}

/// A handler as a click listener: it reaches the editor through a weak
/// handle, so it works in popups too.
fn listener(editor: &WeakEntity<SlideEditor>, handler: &Handler) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let (editor, handler) = (editor.clone(), handler.clone());
    move |_, window, cx| {
        editor.update(cx, |this, cx| handler(this, window, cx)).ok();
    }
}

pub(crate) fn menu_item_row(item: &MenuItem, editor: &WeakEntity<SlideEditor>) -> PopupMenuItem {
                // Icon (or ✓) in the menu's icon column, the label, the shortcut.
                // The code's words in the UI's language; a document's names (sheets,
                // layouts, styles, fonts, files) as they are.
                const NAMES: [&str; 8] = ["file.recent.item", "xlsx.unhide-sheet", "xlsx.validation-item", "pptx.new-slide.layout", "pptx.layout.apply", "docx.style.update", "xlsx.font", "xlsx.function"];
                let label = if NAMES.contains(&item.id) { item.label.clone() } else if crate::shell::i18n::is_font_name(&item.label) { crate::shell::i18n::font_label(&item.label) } else { crate::shell::i18n::text(&item.label) };
                let (shortcut, enabled) = (item.shortcut, item.enabled());
                let has_icon = item.icon.is_some_and(|i| !i.is_empty()) && !item.checked;
                let mut row = PopupMenuItem::element(move |_, cx| {
                    let fg: Hsla = if enabled { rgb(TEXT).into() } else { disabled_color() };
                    h_flex()
                        .w_full()
                        .gap(px(16.))
                        .items_center()
                        .child(div().flex_1().text_color(fg).when(has_icon, |d| d.pl(px(3.))).child(label.clone()))
                        .when_some(shortcut, |d, key| d.child(div().text_color(if enabled { cx.theme().muted_foreground } else { disabled_color() }).child(key)))
                })
                .checked(item.checked)
                .disabled(!enabled);
                if let Some(name) = item.icon.filter(|i| !i.is_empty() && !item.checked) {
                    row = row.icon(menu_icon(name));
                }
                if let Some(handler) = &item.handler {
                    row = row.on_click(listener(editor, handler));
                }
    row
}

/// `entries` without the submenus that have nothing to show (no items, or
/// only separators), and without the separators that would be left at the
/// start, at the end or twice in a row. Applied to every submenu too.
pub(crate) fn prune_entries(entries: Vec<MenuEntry>) -> Vec<MenuEntry> {
    let mut out: Vec<MenuEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        match entry {
            MenuEntry::Submenu(mut sub) => {
                sub.items = prune_entries(std::mem::take(&mut sub.items));
                if !sub.items.is_empty() {
                    out.push(MenuEntry::Submenu(sub));
                }
            }
            MenuEntry::Separator => {
                if !matches!(out.last(), None | Some(MenuEntry::Separator)) {
                    out.push(MenuEntry::Separator);
                }
            }
            other => out.push(other),
        }
    }
    while matches!(out.last(), Some(MenuEntry::Separator)) {
        out.pop();
    }
    out
}

/// `entries` into `menu`.
pub(crate) fn fill_menu(mut menu: PopupMenu, entries: &[MenuEntry], editor: &WeakEntity<SlideEditor>, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let entries = prune_entries(entries.to_vec());
    for entry in &entries {
        menu = match entry {
            MenuEntry::Separator => menu.separator(),
            MenuEntry::Submenu(sub) => {
                let (items, editor) = (sub.items.clone(), editor.clone());
                let icon = sub.icon.map(menu_icon);
                menu.submenu_with_icon(icon, crate::shell::i18n::text(&sub.label), window, cx, move |m, window, cx| fill_menu(m, &items, &editor, window, cx))
            }
            MenuEntry::Palette(palette) => crate::shell::color_palette::fill_palette(menu, palette, editor, cx),
            MenuEntry::Item(item) => menu.item(menu_item_row(item, editor)),
        };
    }
    menu
}

/// The ibisOffice logo (the mark and the name, logo/ibisOffice_logo.svg) as
/// the start view's title row shows it: its height in pixels, and its width
/// over its height. The SVG's own size is this size (99.25×22): GPUI draws
/// an SVG picture at twice its own size and scales that to the element, and
/// scaled down from its 600×133 it came out jagged.
const LOGO_HEIGHT: f32 = 22.0;
const LOGO_ASPECT: f32 = 600.0 / 133.0;

fn app_logo() -> std::sync::Arc<Image> {
    static LOGO: std::sync::OnceLock<std::sync::Arc<Image>> = std::sync::OnceLock::new();
    LOGO.get_or_init(|| std::sync::Arc::new(Image::from_bytes(ImageFormat::Svg, include_bytes!("../../logo/ibisOffice_logo.svg").to_vec()))).clone()
}

fn icon(name: &'static str, size: f32, color: Hsla) -> Icon {
    Icon::new(Material(name)).size(px(size)).text_color(color)
}

impl SlideEditor {
    /// The menus for the open document: the shell's, then the format's additions.
    pub(crate) fn menus(&self) -> Menus {
        let mut menus = self.menus_unpruned();
        for (_, entries) in &mut menus.menus {
            *entries = prune_entries(std::mem::take(entries));
        }
        menus
    }

    /// The menus as the shell and the format built them, before the empty
    /// submenus are left out (for the scenario that lists them).
    pub(crate) fn menus_unpruned(&self) -> Menus {
        let mut menus = crate::shell::chrome_defaults::menus(self);
        with_format!(self, menus(self, &mut menus));
        crate::shell::menu_bridge::fill(&mut menus, || {
            let mut tools = self.toolbar_items();
            if self.kind() == ibis_pptx::Kind::Presentation {
                tools.extend(crate::pptx::chrome::menu_buttons(self));
            }
            tools
        });
        menus
    }

    /// The toolbar's parts, in order: the common left part, the format's
    /// left additions, and the right half for what is selected.
    pub(crate) fn toolbar_items(&self) -> Vec<ToolItem> {
        let mut items = crate::shell::chrome_defaults::toolbar_left(self);
        items.extend(with_format!(self, toolbar_left(self)));
        let right = with_format!(self, toolbar_right(self)).unwrap_or_else(|| crate::shell::chrome_defaults::toolbar_right(self));
        if !right.is_empty() {
            items.push(ToolItem::Separator);
            items.extend(right);
        }
        items
    }

    /// The title row: the app's icon, undo and redo, the slide show (a
    /// presentation), the file name in the middle, comments, the outline
    /// (a document) and the sidebar.
    pub(crate) fn render_title_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = cx.entity().downgrade();
        let row = h_flex().h(px(36.)).flex_shrink_0().pl(px(14.)).pr(px(12.)).gap(px(8.)).items_center().bg(rgb(APP_BG)).text_color(rgb(TEXT));
        if !self.has_document {
            // The start view: the app's logo (開く and 新規作成 are in the view).
            return row
                .child(img(app_logo()).h(px(LOGO_HEIGHT)).w(px(LOGO_HEIGHT * LOGO_ASPECT)))
                .child(div().flex_1())
                .child(title_button("help", "help", "_shell.help", None, &editor))
                .into_any_element();
        }
        let (app_icon, app_color) = match self.kind() {
            ibis_pptx::Kind::Document => ("description", 0x2f6fd6),
            ibis_pptx::Kind::Workbook => ("table_chart", 0x1e8e3e),
            ibis_pptx::Kind::Presentation => ("slideshow", 0xe8a100),
        };
        let fill_icon = |name: &'static str, size: f32, color: Hsla| Icon::new(Material::fill(name)).size(px(size)).text_color(color);
        let can_undo = self.document.can_undo() && !self.loading;
        let can_redo = self.document.can_redo() && !self.loading;
        let name = if self.is_workbook() {
            self.display_title_formatted().into()
        } else {
            self.file_name.clone().unwrap_or_default()
        };
        let sidebar_open = self.sidebar_open;
        let add_comment = crate::shell::format::with_format!(self, add_comment(self));
        let show_comments = crate::shell::format::with_format!(self, show_comments(self));
        let buttons = |d: Div| {
            // The formats connect them (`FormatEditor::add_comment`,
            // `show_comments`).
            d.child(title_button("add-comment", "add_comment", "_shell.new-comment-m", add_comment.clone(), &editor))
                .child(match &show_comments {
                    Some((true, h)) => title_button_icon("show-comments", fill_icon("comment", 20.0, rgb(ICON).into()), "_shell.hide-comments", Some(h.clone()), &editor),
                    Some((false, h)) => title_button("show-comments", "comment", "_shell.show-comments", Some(h.clone()), &editor),
                    None => title_button("show-comments", "comment", "_shell.show-comments", None, &editor),
                })
                .when(self.kind() == ibis_pptx::Kind::Document, |d| {
                    let open = self.outline_open;
                    d.child(
                        title_button_icon("outline", if open { fill_icon("dock_to_right", 20.0, rgb(ICON).into()) } else { icon("dock_to_right", 20.0, rgb(ICON).into()) }, if open { "_shell.hide-outline" } else { "_shell.show-outline" }, Some(Rc::new(|this: &mut SlideEditor, _, cx| {
                            this.outline_open = !this.outline_open;
                            cx.notify();
                        })), &editor),
                    )
                })
                .child(title_button_icon(
                    "sidebar",
                    if sidebar_open { fill_icon("dock_to_left", 20.0, rgb(ICON).into()) } else { icon("dock_to_left", 20.0, rgb(ICON).into()) },
                    if sidebar_open { "_shell.hide-sidebar" } else { "view.sidebar" },
                    Some(Rc::new(|this: &mut SlideEditor, window, cx| {
                        this.toggle_sidebar(window, cx);
                    })),
                    &editor,
                ))
        };
        let slideshow = self.pages_are_slides().then(|| self.render_slideshow_button(&editor, cx));
        row.child(fill_icon(app_icon, 24.0, rgb(app_color).into()))
            .child(
                h_flex()
                    .child(title_button("undo", "undo", "_shell.undo-z", can_undo.then(|| -> Handler { Rc::new(|this: &mut SlideEditor, window, cx| this.undo(&crate::Undo, window, cx)) }), &editor))
                    .child(title_button("redo", "redo", "_shell.redo-z", can_redo.then(|| -> Handler { Rc::new(|this: &mut SlideEditor, window, cx| this.redo(&crate::Redo, window, cx)) }), &editor)),
            )
            .children(slideshow)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .justify_center()
                    .child(div().text_size(px(15.)).px(px(6.)).truncate().child(name)),
            )
            .child(buttons(h_flex()))
            .into_any_element()
    }

    /// スライドショー, with ▼ for how to start.
    fn render_slideshow_button(&self, editor: &WeakEntity<SlideEditor>, _cx: &mut Context<Self>) -> AnyElement {
        let disabled = self.loading;
        let start: Handler = Rc::new(|this: &mut SlideEditor, window, cx| this.start_slideshow(window, cx));
        let from_start: Handler = Rc::new(|this: &mut SlideEditor, window, cx| {
            this.show_slide_number(1, window, cx);
            this.start_slideshow(window, cx);
        });
        let mut items: Vec<MenuEntry> = vec![
            MenuItem::tr("slideshow.from-start").icon("play_arrow").shortcut("⇧⌘↩").on(move |e, w, c| from_start(e, w, c)).into(),
            MenuItem::tr("slideshow.from-current").icon("slideshow").shortcut("⌘↩").on({
                let start = start.clone();
                move |e, w, c| start(e, w, c)
            })
            .into(),
            MenuItem::tr("slideshow.presenter").icon("co_present").shortcut("⌥⌘↩").on(|e, w, c| e.start_presenter_view(w, c)).into(),
        ];
        let custom_shows = self.presentation().custom_shows.clone();
        if !custom_shows.is_empty() {
            let mut sub_items: Vec<MenuEntry> = Vec::new();
            for cs in custom_shows {
                let id = cs.id;
                sub_items.push(MenuItem::new("slideshow.custom-show-item", cs.name).icon("filter_none").on(move |e, w, c| e.start_custom_slideshow(id, w, c)).into());
            }
            items.push(Submenu::tr("slideshow.custom-shows", "filter_none", sub_items).into());
        }
        items.push(MenuEntry::Separator);
        items.push(MenuItem::tr("slideshow.rehearse-timings").icon("schedule").on(|e, w, c| e.start_rehearsal(w, c)).into());
        items.push(MenuItem::tr("slideshow.settings").icon("settings").on(|e, w, c| e.open_show_panel(w, c)).into());
        let theme_border = rgb(0x747775);
        let caret_editor = editor.clone();
        h_flex()
            .h(px(30.))
            .rounded(px(15.))
            .border_1()
            .border_color(theme_border)
            .bg(white())
            .when(disabled, |d| d.opacity(0.5))
            .child(
                div()
                    .id("slideshow")
                    .h_full()
                    .pl(px(10.))
                    .pr(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded_l(px(15.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.04)))
                    .child(icon("slideshow", 18.0, rgb(ICON).into()))
                    .child(crate::shell::i18n::text("_shell.slide-show"))
                    .when(!disabled, |d| d.on_click(listener(editor, &start)))
                    .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text("_shell.play-from-current-slide")).build(window, cx)),
            )
            .child(div().w(px(1.)).h(px(18.)).bg(hsla(0.0, 0.0, 0.0, 0.15)))
            .child(
                Button::new("slideshow-caret")
                    .ghost()
                    .xsmall()
                    .icon(Icon::new(Material("arrow_drop_down")))
                    .tooltip(crate::shell::i18n::text("_shell.choose-how-to-start"))
                    .disabled(disabled)
                    .rounded_r(px(15.))
                    .dropdown_menu(move |menu, window, cx| fill_menu(menu.min_w(px(300.)), &items, &caret_editor, window, cx)),
            )
            .into_any_element()
    }

    /// The toolbar: the common left part, the format's right part, the
    /// buttons that do not fit folded into ︙, and ^ to hide the menus.
    pub(crate) fn render_toolbar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = cx.entity().downgrade();
        let items = self.toolbar_items();
        // What fits: the bar is the window less 8 px each side, 6 px padding
        // each side, and ︙ and ^ on the right.
        let available = f32::from(window.viewport_size().width) - 16.0 - 12.0 - 2.0 * 29.0;
        let mut used = 0.0;
        let mut shown = Vec::new();
        let mut folded = Vec::new();
        for item in items {
            let w = item.width() + 1.0;
            if folded.is_empty() && used + w <= available {
                used += w;
                shown.push(item);
            } else {
                folded.push(item);
            }
        }
        while matches!(shown.last(), Some(ToolItem::Separator)) {
            shown.pop();
        }
        while matches!(folded.first(), Some(ToolItem::Separator)) {
            folded.remove(0);
        }
        let compact = self.chrome_compact;
        let more = (!folded.is_empty()).then(|| {
            let folded = Rc::new(folded);
            let editor = editor.clone();
            Popover::new("toolbar-more")
                .anchor(Anchor::TopRight)
                .trigger(Button::new("toolbar-more-button").ghost().xsmall().icon(Icon::new(Material("more_vert"))).tooltip(crate::shell::i18n::text("_shell.more")).h(px(28.)).w(px(28.)).rounded(px(4.)))
                .content(move |_, _, _| {
                    // The rest of the toolbar in one row, as the mock's ︙.
                    h_flex()
                        .h(px(36.))
                        .px(px(6.))
                        .gap(px(1.))
                        .items_center()
                        .children(folded.iter().enumerate().map(|(i, item)| tool_item(item, i + 1000, &editor)))
                })
        });
        div().px(px(8.)).pt(px(2.)).pb(px(6.)).flex_shrink_0().bg(rgb(APP_BG)).child(
            h_flex()
                .h(px(36.))
                .px(px(6.))
                .gap(px(1.))
                .items_center()
                .rounded(px(18.))
                .bg(rgb(TOOLBAR_BG))
                .children(shown.iter().enumerate().map(|(i, item)| tool_item(item, i, &editor)))
                .child(div().flex_1())
                .children(more)
                .child(tool_item(
                    &ToolButton::new("toggle-menus", if compact { "keyboard_arrow_down" } else { "keyboard_arrow_up" }, if compact { "_shell.show-menus-f" } else { "_shell.hide-menus-f" })
                        .click(|this, _, cx| {
                            this.chrome_compact = !this.chrome_compact;
                            cx.notify();
                        })
                        .into(),
                    9999,
                    &editor,
                )),
        )
    }

    /// The sidebar, right of the document: the panel, under the tabs the
    /// format has for it.
    pub(crate) fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = cx.entity().downgrade();
        if !self.sidebar_open {
            return None;
        }
        let tabs_of = with_format!(self, sidebar_tabs());
        let panel = with_format!(self, sidebar(self, window, cx)).or_else(|| crate::shell::chrome_defaults::sidebar(self, window, cx));
        let panel = match panel {
            Some(panel) => panel,
            // Nothing open: the tabs alone, for a format that has them.
            None if !tabs_of.is_empty() => SidebarPanel { title: "".into(), body: div().into_any_element() },
            None => return None,
        };
        // The tabs show with their panels, or with none open; another
        // panel (the name manager, ...) has its own title only.
        let selected = tabs_of.iter().position(|t| crate::shell::i18n::text(t.panel_title) == crate::shell::i18n::text(&panel.title));
        let tabs = (selected.is_some() || panel.title.is_empty()).then(|| {
            h_flex().h(px(40.)).px(px(6.)).gap(px(1.)).flex_shrink_0().border_b_1().border_color(rgb(BORDER)).children(tabs_of.iter().enumerate().map(|(i, tab)| {
                let on = selected == Some(i);
                let handler = tab.open.clone();
                div()
                    .id(("sidebar-tab", i))
                    .h_full()
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .whitespace_nowrap()
                    .text_color(if on { rgb(0x0b57d0) } else { rgb(TEXT_SECONDARY) })
                    .when(on, |d| d.font_weight(FontWeight::MEDIUM).border_b_2().border_color(rgb(0x0b57d0)))
                    .hover(|s| s.bg(hsla(0.0, 0.0, 0.27, 0.06)))
                    .child(crate::shell::i18n::text(tab.label))
                    .on_click(listener(&editor, &handler))
            }))
        });
        let close: Handler = Rc::new(|this: &mut SlideEditor, window, cx| {
            this.sidebar_open = false;
            cx.notify();
            window.refresh();
        });
        Some(
            v_flex()
                .w(px(SIDEBAR_WIDTH))
                .h_full()
                .flex_shrink_0()
                .bg(white())
                .border_l_1()
                .border_color(rgb(BORDER))
                .rounded_tl(px(16.))
                .child(
                    h_flex()
                        .h(px(52.))
                        .pl(px(18.))
                        .pr(px(10.))
                        .items_center()
                        .justify_between()
                        .child(div().text_size(px(16.)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)).child(crate::shell::i18n::text(&panel.title)))
                        .child(title_button("sidebar-close", "close", "_shell.close", Some(close), &editor)),
                )
                .children(tabs)
                .child(div().id("sidebar-body").flex_1().min_h_0().overflow_y_scroll().child(panel.body))
                .into_any_element(),
        )
    }
}

/// A round icon button of the title row (32 px).
pub(crate) fn title_button(id: &'static str, name: &'static str, tooltip: &'static str, handler: Option<Handler>, editor: &WeakEntity<SlideEditor>) -> AnyElement {
    let color: Hsla = if handler.is_some() { rgb(ICON).into() } else { disabled_color() };
    title_button_icon(id, icon(name, 20.0, color), tooltip, handler, editor)
}

fn title_button_icon(id: &'static str, icon: Icon, tooltip: &'static str, handler: Option<Handler>, editor: &WeakEntity<SlideEditor>) -> AnyElement {
    let enabled = handler.is_some();
    div()
        .id(id)
        .size(px(32.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(enabled, |d| d.cursor_pointer().hover(|s| s.bg(hsla(0.0, 0.0, 0.27, 0.08))))
        .child(icon)
        .when_some(handler, |d, h| d.on_click(listener(editor, &h)))
        .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text(&tooltip)).build(window, cx))
        .into_any_element()
}

/// One part of the toolbar.
pub(crate) fn tool_item(item: &ToolItem, index: usize, editor: &WeakEntity<SlideEditor>) -> AnyElement {
    match item {
        ToolItem::Separator => div().w(px(1.)).h(px(20.)).mx(px(4.)).bg(rgb(DIVIDER)).flex_none().into_any_element(),
        ToolItem::Button(b) => tool_button(b, index, editor),
        ToolItem::Select { id, value, width, tooltip, menu, disabled } => {
            let enabled = !disabled && !menu.is_empty();
            let color: Hsla = if enabled { rgb(TEXT).into() } else { disabled_color() };
            let (menu, editor) = (Rc::new(menu.clone()), editor.clone());
            Button::new(SharedString::from(format!("{id}-{index}")))
                .ghost()
                .xsmall()
                .h(px(28.))
                .w(px(*width))
                .rounded(px(4.))
                .tooltip(crate::shell::i18n::text(tooltip))
                .disabled(!enabled)
                .child(
                    h_flex()
                        .w_full()
                        .pl(px(6.))
                        .justify_between()
                        .text_size(px(14.))
                        .text_color(color)
                        .child(div().truncate().child(if *id == "font" { crate::shell::i18n::font_label(value) } else { value.clone() }))
                        .child(icon("arrow_drop_down", 18.0, if enabled { rgb(ICON).into() } else { disabled_color() })),
                )
                .dropdown_menu(move |m, window, cx| fill_menu(m.min_w(px(220.)), &menu, &editor, window, cx))
                .into_any_element()
        }
        ToolItem::FontSize { value, smaller, larger } => {
            let enabled = value.is_some();
            let color: Hsla = if enabled { rgb(TEXT).into() } else { disabled_color() };
            let step = |id: &'static str, name: &'static str, tip: &'static str, handler: &Option<Handler>| {
                tool_button(&ToolButton { handler: handler.clone(), ..ToolButton::new(id, name, tip) }, index, editor)
            };
            h_flex()
                .flex_none()
                .items_center()
                .child(step("font-smaller", "remove", "_shell.decrease-font-size", smaller))
                .child(
                    div()
                        .w(px(36.))
                        .h(px(26.))
                        .mx(px(2.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_1()
                        .border_color(if enabled { rgb(0x747775).into() } else { disabled_color() })
                        .rounded(px(4.))
                        .text_size(px(14.))
                        .text_color(color)
                        .child(value.clone().unwrap_or_default()),
                )
                .child(step("font-larger", "add", "_shell.increase-font-size", larger))
                .into_any_element()
        }
    }
}

fn tool_button(b: &ToolButton, index: usize, editor: &WeakEntity<SlideEditor>) -> AnyElement {
    let enabled = b.enabled();
    let color: Hsla = if !enabled {
        disabled_color()
    } else if b.on {
        rgb(0x041e49).into()
    } else {
        rgb(ICON).into()
    };
    let content = h_flex()
        .items_center()
        .justify_center()
        .when_some(b.icon, |d, name| {
            match b.color {
                // The icon over a colour bar.
                Some(bar) => d.child(v_flex().items_center().pt(px(2.)).child(icon(name, 18.0, color)).child(div().w(px(18.)).h(px(3.)).mt(px(-1.)).rounded(px(1.)).bg(bar))),
                None => d.child(icon(name, 20.0, color)),
            }
        })
        .when_some(b.text.clone(), |d, text| d.child(div().px(px(3.)).text_size(px(14.)).text_color(if enabled { rgb(TEXT).into() } else { disabled_color() }).child(crate::shell::i18n::text(&text))))
        .when(b.menu.is_some(), |d| d.child(div().ml(px(-2.)).child(icon("arrow_drop_down", 16.0, color))));
    // A split button: the icon (what was last used is done at once) and the ▼ (the menu) are two buttons side by side.
    if let (Some(menu), Some(quick), true) = (&b.menu, &b.quick, enabled) {
        let left = Button::new(SharedString::from(format!("{}-{index}", b.id)))
            .ghost()
            .xsmall()
            .h(px(28.))
            .min_w(px(28.))
            .px(px(3.))
            .rounded_l(px(4.))
            .tooltip(crate::shell::i18n::text(&b.tooltip))
            .child(h_flex().items_center().justify_center().when_some(b.icon, |d, name| match b.color {
                Some(bar) => d.child(v_flex().items_center().pt(px(2.)).child(icon(name, 18.0, color)).child(div().w(px(18.)).h(px(3.)).mt(px(-1.)).rounded(px(1.)).bg(bar))),
                None => d.child(icon(name, 20.0, color)),
            }))
            .on_click(listener(editor, quick));
        let (menu, editor) = (Rc::new(menu.clone()), editor.clone());
        let right = Button::new(SharedString::from(format!("{}-{index}-menu", b.id)))
            .ghost()
            .xsmall()
            .h(px(28.))
            .w(px(16.))
            .px(px(0.))
            .rounded_r(px(4.))
            .tooltip(crate::shell::i18n::text(&b.tooltip))
            .child(icon("arrow_drop_down", 16.0, color))
            .dropdown_menu(move |m, window, cx| fill_menu(m.min_w(px(220.)), &menu, &editor, window, cx));
        return h_flex().items_center().child(left).child(right).into_any_element();
    }
    let button = Button::new(SharedString::from(format!("{}-{index}", b.id)))
        .ghost()
        .xsmall()
        .h(px(28.))
        .min_w(px(28.))
        .px(px(3.))
        .rounded(px(4.))
        .tooltip(crate::shell::i18n::text(&b.tooltip))
        .selected(b.on)
        .disabled(!enabled)
        .child(content);
    match (&b.menu, &b.handler) {
        (Some(menu), _) if enabled => {
            let (menu, editor) = (Rc::new(menu.clone()), editor.clone());
            button.dropdown_menu(move |m, window, cx| fill_menu(m.min_w(px(220.)), &menu, &editor, window, cx)).into_any_element()
        }
        (_, Some(handler)) if enabled => button.on_click(listener(editor, handler)).into_any_element(),
        _ => button.into_any_element(),
    }
}

/// The colour of the chrome's text, for panels.
pub(crate) fn text_secondary() -> Hsla {
    rgb(TEXT_SECONDARY).into()
}

/// A pressed-state background, for panels.
#[allow(dead_code)]
pub(crate) fn pressed() -> Hsla {
    rgb(0xd3e3fd).into()
}


#[cfg(test)]
mod tests {
    use super::{MenuEntry, MenuId, MenuItem, Menus, SlideEditor, Submenu, ToolButton, ToolItem};
    use gpui_kit::{Context, Window};

    fn sample() -> Menus {
        Menus {
            menus: vec![(
                MenuId::Insert,
                vec![
                    MenuItem::new("a", "A").into(),
                    MenuEntry::Separator,
                    Submenu::new("sub", "add", "Sub", vec![MenuItem::new("b", "B").into()]).into(),
                ],
            )],
        }
    }

    #[test]
    fn placeholders_are_disabled_until_connected() {
        let mut menus = sample();
        assert!(!menus.find_mut("b").unwrap().enabled());
        assert!(menus.connect("b", |_: &mut SlideEditor, _: &mut Window, _: &mut Context<SlideEditor>| {}));
        assert!(menus.find_mut("b").unwrap().enabled());
        assert!(!menus.connect("missing", |_: &mut SlideEditor, _: &mut Window, _: &mut Context<SlideEditor>| {}));
    }

    #[test]
    fn empty_submenus_and_stray_separators_are_pruned() {
        let empty = |id| Submenu::new(id, "add", "E", vec![MenuEntry::Separator]);
        let entries = vec![
            MenuEntry::Separator,
            MenuItem::new("a", "A").into(),
            MenuEntry::Separator,
            empty("e1").into(),
            MenuEntry::Separator,
            Submenu::new("outer", "add", "O", vec![empty("e2").into(), MenuItem::new("b", "B").into(), empty("e3").into()]).into(),
            MenuEntry::Separator,
            empty("e4").into(),
        ];
        let out = super::prune_entries(entries);
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], MenuEntry::Item(_)) && matches!(out[1], MenuEntry::Separator));
        let MenuEntry::Submenu(outer) = &out[2] else { panic!("outer") };
        assert_eq!(outer.items.len(), 1);
    }

    #[test]
    fn a_format_can_add_a_menu() {
        let mut menus = sample();
        menus.items_mut(MenuId::Data).push(MenuItem::new("c", "C").into());
        assert_eq!(menus.menus.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [MenuId::Insert, MenuId::Data]);
    }

    #[test]
    fn toolbar_widths_follow_the_mock() {
        assert_eq!(ToolItem::Separator.width(), 9.0);
        assert_eq!(ToolItem::from(ToolButton::new("b", "format_bold", "太字")).width(), 28.0);
        assert!(ToolItem::from(ToolButton::new("b", "shapes", "図形").menu(Vec::new())).width() > 28.0);
    }

    #[test]
    fn every_icon_used_by_the_chrome_is_bundled() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/shell/");
        let source = ["chrome.rs", "chrome_defaults.rs"].iter().map(|f| std::fs::read_to_string(format!("{dir}{f}")).unwrap()).collect::<String>();
        let mut missing = Vec::new();
        for pattern in ["icon(\"", "Material(\"", "Material::fill(\"", "ToolButton::new(\""] {
            let mut at = 0;
            while let Some(i) = source[at..].find(pattern) {
                let start = at + i;
                at = start + pattern.len();
                // `title_button_icon("id", …)` names an element, not an icon.
                if source[..start].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let part = &source[at..];
                let rest = if pattern.starts_with("ToolButton") { part.split_once(", \"").map(|(_, r)| r).unwrap_or("") } else { part };
                let name: String = rest.chars().take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_').collect();
                if !name.is_empty() && !crate::shell::icons::has(&name) {
                    missing.push(name);
                }
            }
        }
        assert!(missing.is_empty(), "{missing:?}");
    }
}
