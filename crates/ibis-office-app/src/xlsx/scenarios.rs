// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The workbook's work scenarios (`tasks/doc/scenarios-xlsx.md`), driven
//! through the window (`shell/scenario.rs`).

// The public repository has no editing TSVs. Linux reads them at run time,
// so their absence does not prevent the rest of the scenarios from building.
macro_rules! scenario_cases {
    ($path:literal) => {{
        #[cfg(not(target_os = "linux"))]
        { include_str!($path) }
        #[cfg(target_os = "linux")]
        {
            static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
            TEXT.get_or_init(|| {
                let path = crate::shell::scenario::repo_root().join($path.trim_start_matches("../../../../"));
                crate::shell::scenario::require_fixture(&path);
                std::fs::read_to_string(path).expect($path)
            }).as_str()
        }
    }};
}

use ibis_doc_model::{CellRange, CellValue, sheet_calc::sheet_of_slide, sheet_command::Axis};

use gpui_kit::{Bounds, point, px, size};

use crate::shell::scenario::Scenario;

/// Steps of a worksheet.
pub(crate) trait SheetSteps {
    /// Click the cell at a reference such as `B3` (on the current sheet).
    fn click_cell(&mut self, reference: &str);
    /// Double-click the cell (to edit its content).
    fn double_click_cell(&mut self, reference: &str);
    /// The value of a cell of the current sheet.
    fn value(&mut self, reference: &str) -> CellValue;
    /// The text a cell of the current sheet shows.
    fn shown(&mut self, reference: &str) -> String;
    /// The selected cell as a reference.
    fn selected(&mut self) -> String;
    /// The middle of a column's header (`"C"`) or a row's (`"5"`).
    fn header(&mut self, name: &str) -> (f32, f32);
    /// The line after a column's or row's header (where it is sized).
    fn header_line(&mut self, name: &str) -> (f32, f32);
    /// Right-click a header and choose item `id` of the menu that opens.
    fn header_menu(&mut self, name: &str, id: &str);
    /// A column's width or a row's height, in points.
    fn size_pt(&mut self, name: &str) -> f32;
    /// Drag the fill handle of the selection (ending at cell `from`) to the middle of cell `to`.
    fn drag_fill(&mut self, from: &str, to: &str);
    /// Type `text` into the selected cell and press `key` (`"enter"`, `"tab"`).
    fn enter(&mut self, text: &str, key: &str);
    /// The formula of a cell of the current sheet (without `=`).
    fn formula(&mut self, reference: &str) -> Option<String>;
}

/// A header's name: a column's letters or a row's number, as (axis, index).
fn header_of(name: &str) -> (Axis, u32) {
    match name.parse::<u32>() {
        Ok(row) => (Axis::Rows, row - 1),
        Err(_) => (Axis::Columns, ibis_calc::formula::parser::column_index(name).unwrap()),
    }
}

fn parse(reference: &str) -> (u32, u32) {
    let split = reference.find(|c: char| c.is_ascii_digit()).unwrap();
    let column = ibis_calc::formula::parser::column_index(&reference[..split]).unwrap();
    let row: u32 = reference[split..].parse().unwrap();
    (column, row - 1)
}

impl SheetSteps for Scenario {
    fn click_cell(&mut self, reference: &str) {
        let (column, row) = parse(reference);
        // Off screen (or under the headers): scrolled into view first, as a
        // user would (with the name box), then clicked.
        let visible = |s: &mut Scenario| {
            s.with(move |e, _| {
                let (p, ws) = (e.cell_center(column, row)?, e.workspace_bounds?);
                let inner = Bounds::new(ws.origin + point(px(super::sheet_ui::ROW_HEADER_WIDTH), px(super::sheet_ui::COLUMN_HEADER_HEIGHT)), ws.size - size(px(super::sheet_ui::ROW_HEADER_WIDTH + super::sheet_ui::SCROLLBAR_ROOM), px(super::sheet_ui::COLUMN_HEADER_HEIGHT + super::sheet_ui::SCROLLBAR_ROOM)));
                inner.contains(&p).then_some(p)
            })
        };
        let mut p = match visible(self) {
            Some(p) => p,
            None => {
                let reference = reference.to_string();
                let r2 = reference.clone();
                self.update(move |e, window, cx| e.go_to_reference(&r2, window, cx));
                let info = self.with(move |e, _| (e.cell_center(column, row), e.workspace_bounds));
                visible(self).unwrap_or_else(|| panic!("the cell on screen: {reference} {info:?}"))
            }
        };
        // A cell at the foot of the workspace is in the zone where a pressed
        // pointer scrolls it (and selects further): moved up first, if the
        // sheet goes on.
        let foot = self.with(|e, _| e.workspace_bounds.map(|ws| f32::from(ws.bottom())));
        if foot.is_some_and(|foot| foot - f32::from(p.y) < 40.0) {
            self.wheel((600.0, 300.0), -60.0);
            if let Some(moved) = visible(self) {
                p = moved;
            }
        }
        self.click((f32::from(p.x), f32::from(p.y)));
    }

    fn double_click_cell(&mut self, reference: &str) {
        let (column, row) = parse(reference);
        let p = self.with(|e, _| e.cell_center(column, row)).expect("the cell on screen");
        self.double_click((f32::from(p.x), f32::from(p.y)));
    }

    fn value(&mut self, reference: &str) -> CellValue {
        let (column, row) = parse(reference);
        self.with(|e, _| sheet_of_slide(e.slide()).and_then(|s| s.cell(row, column)).map(|c| c.value.clone()).unwrap_or_default())
    }

    fn shown(&mut self, reference: &str) -> String {
        let (column, row) = parse(reference);
        self.with(|e, _| sheet_of_slide(e.slide()).and_then(|s| s.cell(row, column)).map(|c| c.text.clone()).unwrap_or_default())
    }

    fn selected(&mut self) -> String {
        self.with(|e, _| e.current_cell().map(|(c, r)| ibis_doc_model::cell_name(c, r)).unwrap_or_default())
    }

    fn header(&mut self, name: &str) -> (f32, f32) {
        let (axis, index) = header_of(name);
        self.with(move |e, _| {
            let g = e.sheet_state().unwrap().header_geometry.borrow().clone();
            let (ox, oy) = g.origin;
            match axis {
                Axis::Columns => g.columns.iter().find(|c| c.0 == index).map(|c| ((c.1 + c.2) / 2.0, oy + 10.0)),
                Axis::Rows => g.rows.iter().find(|r| r.0 == index).map(|r| (ox + 20.0, (r.1 + r.2) / 2.0)),
            }
            .expect("the header on screen")
        })
    }

    fn header_line(&mut self, name: &str) -> (f32, f32) {
        let (axis, index) = header_of(name);
        self.with(move |e, _| {
            let g = e.sheet_state().unwrap().header_geometry.borrow().clone();
            let (ox, oy) = g.origin;
            match axis {
                Axis::Columns => g.columns.iter().find(|c| c.0 == index).map(|c| (c.2 - 1.0, oy + 10.0)),
                Axis::Rows => g.rows.iter().find(|r| r.0 == index).map(|r| (ox + 20.0, r.2 - 1.0)),
            }
            .expect("the header on screen")
        })
    }

    fn header_menu(&mut self, name: &str, id: &str) {
        let (axis, _) = header_of(name);
        let p = self.header(name);
        self.right_click(p);
        assert!(self.with(|e, _| e.sheet_state().unwrap().header_popup.is_some()), "the menu opens");
        // Its item is chosen through its handler, so close it first.
        self.key("escape");
        assert!(self.with(|e, _| e.sheet_state().unwrap().header_popup.is_none()), "Esc closes it");
        let entries = self.with(move |e, _| e.header_menu(axis));
        let handler = crate::shell::scenario::find_item(&entries, id).and_then(|i| i.handler.clone()).unwrap_or_else(|| panic!("no header menu item {id}"));
        self.update(|e, window, cx| handler(e, window, cx));
    }

    fn drag_fill(&mut self, from: &str, to: &str) {
        let (c0, r0) = parse(from);
        let (c1, r1) = parse(to);
        let a = self.with(|e, _| e.cell_corner(c0, r0)).expect("the cell on screen");
        let b = self.with(|e, _| e.cell_center(c1, r1)).expect("the cell on screen");
        // Just inside the corner, on the handle.
        self.drag((f32::from(a.x) - 1.0, f32::from(a.y) - 1.0), (f32::from(b.x), f32::from(b.y)));
    }

    fn enter(&mut self, text: &str, key: &str) {
        self.type_text(text);
        self.key(key);
    }

    fn formula(&mut self, reference: &str) -> Option<String> {
        let (column, row) = parse(reference);
        self.with(move |e, _| sheet_of_slide(e.slide()).and_then(|s| s.cell(row, column)).and_then(|c| c.formula.clone()))
    }

    fn size_pt(&mut self, name: &str) -> f32 {
        let (axis, index) = header_of(name);
        self.with(move |e, _| {
            let sheet = sheet_of_slide(e.slide()).unwrap();
            let v = match axis {
                Axis::Columns => sheet.columns[index as usize],
                Axis::Rows => sheet.rows[index as usize],
            };
            v as f32 / ibis_doc_model::EMU_PER_POINT as f32
        })
    }
}

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::perf_sheet_switch", perf_sheet_switch),
        ("xlsx::perf_sheet_zoom", perf_sheet_zoom),
        ("xlsx::perf_sheet_shots", perf_sheet_shots),
        ("xlsx::xsc55_arraytotext_empty", xsc55_arraytotext_empty),
        ("xlsx::xsc55_filter_empty", xsc55_filter_empty),
        ("xlsx::xsc55_sortby_shape_orders", xsc55_sortby_shape_orders),
        ("xlsx::xsc55_xlookup_empty", xsc55_xlookup_empty),
        ("xlsx::xsc55_xlookup_shape", xsc55_xlookup_shape),
        ("xlsx::xsc55_regex_lookup_unsupported", xsc55_regex_lookup_unsupported),
        ("xlsx::xsc55_expand_options", xsc55_expand_options),
        ("xlsx::xsc55_wrap_count", xsc55_wrap_count),
        ("xlsx::xsc24_subtotal_filter", xsc24_subtotal_filter),
        ("xlsx::xsc39_aggregate_enums", xsc39_aggregate_enums),
        ("xlsx::xsc39_subtotal_hidden", xsc39_subtotal_hidden),("xlsx::type_numbers_and_a_formula", type_numbers_and_a_formula), ("xlsx::edit_with_f2_and_cancel", edit_with_f2_and_cancel), ("xlsx::format_insert_rows_and_sheets", format_insert_rows_and_sheets), ("xlsx::copy_cut_and_paste", copy_cut_and_paste), ("xlsx::auto_sum", auto_sum), ("xlsx::rename_a_sheet", rename_a_sheet), ("xlsx::hide_rows_and_columns", hide_rows_and_columns), ("xlsx::headers", headers), ("xlsx::xsc01_sales_table", xsc01_sales_table),
        ("xlsx::xsc02_typed_values", xsc02_typed_values),
        ("xlsx::xsc03_insert_row_grows_sum", xsc03_insert_row_grows_sum),
        ("xlsx::xsc04_delete_row_ref_error", xsc04_delete_row_ref_error),
        ("xlsx::xsc05_insert_delete_columns", xsc05_insert_delete_columns),
        ("xlsx::xsc06_sum_across_sheets", xsc06_sum_across_sheets),
        ("xlsx::xsc07_relative_absolute", xsc07_relative_absolute),
        ("xlsx::xsc10_errors", xsc10_errors),
        ("xlsx::xsc11_circular", xsc11_circular),
        ("xlsx::xsc18_merge", xsc18_merge),
        ("xlsx::xsc33_sheets", xsc33_sheets),
        ("xlsx::xsc37_one_number", xsc37_one_number),
        ("xlsx::xsc38_shared_formula", xsc38_shared_formula),
        ("xlsx::xsc41_save_unchanged", xsc41_save_unchanged),
        ("xlsx::xsc42_keyboard_only", xsc42_keyboard_only),
        ("xlsx::xsc43_clear_range", xsc43_clear_range),
        ("xlsx::xsc45_japanese", xsc45_japanese),
        ("xlsx::xsc49_cut_follows", xsc49_cut_follows),
        ("xlsx::xsc55_spill", xsc55_spill),
        ("xlsx::xsc64_hide_sheet_and_sum", xsc64_hide_sheet_and_sum),
        ("xlsx::xsc65_paste_special", xsc65_paste_special),
        ("xlsx::xsc28_freeze", xsc28_freeze),
        ("xlsx::fix5_frozen_wheel_past_top", fix5_frozen_wheel_past_top),
        ("xlsx::fix2_border_width_steady_with_zoom", fix2_border_width_steady_with_zoom),
        ("xlsx::table013_custom_list_sort", table013_custom_list_sort),
        ("xlsx::formula016_calculation_options", formula016_calculation_options),
        ("xlsx::formula017_iterative_calculation", formula017_iterative_calculation),
        ("xlsx::formula019_precision_as_displayed", formula019_precision_as_displayed),
        ("xlsx::formula022_trace_arrows", formula022_trace_arrows),
        ("xlsx::formula023_evaluate_and_watch", formula023_evaluate_and_watch),
        ("xlsx::formula008_named_formulas", formula008_named_formulas),
        ("xlsx::table014_case_sensitive_sort", table014_case_sensitive_sort),
        ("xlsx::table015_advanced_filter", table015_advanced_filter),
        ("xlsx::table016_remove_duplicates", table016_remove_duplicates),
        ("xlsx::table017_text_to_columns", table017_text_to_columns),
        ("xlsx::table018_consolidate", table018_consolidate),
        ("xlsx::cell012_paste_skip_blanks", cell012_paste_skip_blanks),
        ("xlsx::cell018_goto_special", cell018_goto_special),
        ("xlsx::fn_text_delimiter_functions", fn_text_delimiter_functions),
        ("xlsx::fn_char_and_code_functions", fn_char_and_code_functions),
        ("xlsx::fn_acot_acoth_type_functions", fn_acot_acoth_type_functions),
        ("xlsx::fn_n_and_t_functions", fn_n_and_t_functions),
        ("xlsx::fn_factdouble_function", fn_factdouble_function),
        ("xlsx::sheet016_split_window", sheet016_split_window),
        ("xlsx::format014_cell_styles", format014_cell_styles),
        ("xlsx::format015_custom_cell_style", format015_custom_cell_style),
        ("xlsx::cf003_date_occurring", cf003_date_occurring),
        ("xlsx::cell016_flash_fill", cell016_flash_fill),
        ("xlsx::valid001_rules_and_messages", valid001_rules_and_messages),
        ("xlsx::valid010_copy_and_paste_validation", valid010_copy_and_paste_validation),
        ("xlsx::valid012_paste_and_recalc_bypass", valid012_paste_and_recalc_bypass),
        ("xlsx::sec009_notes", sec009_notes),
        ("xlsx::sec002_protect_sheet", sec002_protect_sheet),
        ("xlsx::sec004_allow_edit_ranges", sec004_allow_edit_ranges),
        ("xlsx::sec005_protect_workbook", sec005_protect_workbook),
        ("xlsx::sec002_unprotect_excel_file", sec002_unprotect_excel_file),
        ("xlsx::cell015_series", cell015_series),
        ("xlsx::whatif014_trend_linear_and_growth", whatif014_trend_linear_and_growth),
        ("xlsx::xsc20_format_painter", xsc20_format_painter),
        ("xlsx::xsc51_names", xsc51_names),
        ("xlsx::xsc52_validation_list", xsc52_validation_list),
        ("xlsx::xsc59_shift_cells", xsc59_shift_cells),
        ("xlsx::xsc61_csv", xsc61_csv),
        ("xlsx::xsc48_autofill", xsc48_autofill),
        ("xlsx::xsc29_find_replace", xsc29_find_replace),
        ("xlsx::xsc31_hyperlinks", xsc31_hyperlinks),
        ("xlsx::opt005_autolink", opt005_autolink),
        ("xlsx::view002_page_layout", view002_page_layout),
        ("xlsx::cell005_mouse_select", cell005_mouse_select),
        ("xlsx::cell005_caret_format", cell005_caret_format),
        ("xlsx::cell018_date_time_keys", cell018_date_time_keys),
        ("xlsx::cell019_fill_down_right", cell019_fill_down_right),
        ("xlsx::cell020_format_and_filter_keys", cell020_format_and_filter_keys),
        ("xlsx::format021_cell_format_pane", format021_cell_format_pane),
        ("xlsx::clip022_system_clipboard", clip022_system_clipboard),
        ("xlsx::xsc58_outline", xsc58_outline),
        ("xlsx::xsc57_copy_move_sheet", xsc57_copy_move_sheet),
        ("xlsx::xsc54_f4", xsc54_f4),
        ("xlsx::xsc56_undo_twenty", xsc56_undo_twenty),
        ("xlsx::xsc53_widths_and_wrapping", xsc53_widths_and_wrapping),
        ("xlsx::xsc50_insert_row_grows_ranges", xsc50_insert_row_grows_ranges),
        ("xlsx::xsc35_page_setup", xsc35_page_setup),
        ("xlsx::xsc08_functions", xsc08_functions),
        ("xlsx::xsc09_dates", xsc09_dates),
        ("xlsx::xsc39_unsupported_function", xsc39_unsupported_function),
        ("xlsx::xsc40_formats_file", xsc40_formats_file),
        ("xlsx::xsc13_table_look", xsc13_table_look),
        ("xlsx::xsc14_number_formats", xsc14_number_formats),
        ("xlsx::xsc15_date_formats", xsc15_date_formats),
        ("xlsx::xsc16_conditional_sections", xsc16_conditional_sections),
        ("xlsx::xsc17_wrap_and_indent", xsc17_wrap_and_indent),
        ("xlsx::xsc36_grid_lines", xsc36_grid_lines),
        ("xlsx::xsc34_tabs", xsc34_tabs),
        ("xlsx::split_color_buttons", split_color_buttons),
        ("xlsx::xsc44_point_references", xsc44_point_references),
        ("xlsx::xsc32_large_table", xsc32_large_table),
        ("xlsx::xsc23_sort", xsc23_sort),
        ("xlsx::xsc24_filter", xsc24_filter),
        ("xlsx::xsc24_filter_panel", xsc24_filter_panel),
        ("xlsx::xsc05_threaded_comments", xsc05_threaded_comments),
        ("xlsx::xsc26_make_table", xsc26_make_table),
        ("xlsx::xsc26_table_filter", xsc26_table_filter),
        ("xlsx::xsc26_table_to_range", xsc26_table_to_range),
        ("xlsx::xsc25_conditional_quick", xsc25_conditional_quick),
        ("xlsx::xsc13_direction_effects_patterns", xsc13_direction_effects_patterns),
        ("xlsx::format013_fill_patterns_and_gradients", format013_fill_patterns_and_gradients),
        ("xlsx::format016_theme_palette_and_colors", format016_theme_palette_and_colors),
        ("xlsx::cell007_multi_range_select", cell007_multi_range_select),
        ("xlsx::cell003_long_numbers_precision", cell003_long_numbers_precision),
        ("xlsx::cell004_line_break", cell004_line_break),
        ("xlsx::pivot018_refresh", pivot018_refresh),
        ("xlsx::pivot001_create", pivot001_create),
        ("xlsx::pivot002_layout", pivot002_layout),
        ("xlsx::pivot003_value_field_settings", pivot003_value_field_settings),
        ("xlsx::pivot009_group_and_ungroup", pivot009_group_and_ungroup),
        ("xlsx::pivot010_options", pivot010_options),
        ("xlsx::pivot010_options_and_field_settings", pivot010_options_and_field_settings),
        ("xlsx::pivot011_calculated_field", pivot011_calculated_field),
        ("xlsx::pivot012_calculated_item", pivot012_calculated_item),
        ("xlsx::pivot010_manual_grouping", pivot010_manual_grouping),
        ("xlsx::pivot013_sort_by_value", pivot013_sort_by_value),
        ("xlsx::pivot001_table_source", pivot001_table_source),
        ("xlsx::table006_structure_edits", table006_structure_edits),
        ("xlsx::pivot019_show_details", pivot019_show_details),
        ("xlsx::pivot013_buttons", pivot013_buttons),
        ("xlsx::slicer001_buttons", slicer001_buttons),
        ("xlsx::sec006_open_password", sec006_open_password),
        ("xlsx::chart001_insert_over_cells", chart001_insert_over_cells),
        ("xlsx::options003_cell_menu", options003_cell_menu),
        ("xlsx::file010_legacy_xls", file010_legacy_xls),
        ("xlsx::file010_legacy_features", file010_legacy_features),
        ("xlsx::image002_cell_pictures", image002_cell_pictures),
        ("xlsx::options006_error_check", options006_error_check),
        ("xlsx::options003_tab_menu", options003_tab_menu),
        ("xlsx::options003_shape_menu", options003_shape_menu),
        ("xlsx::formula006_external_links", formula006_external_links),
        ("xlsx::cell005_formula_bar_edit", cell005_formula_bar_edit),
        ("xlsx::pq001_import_text", pq001_import_text),
        ("xlsx::pq001_import_text_dates", pq001_import_text_dates),
        ("xlsx::cell005_rich_text_wrapped", cell005_rich_text_wrapped),
        ("xlsx::table_design", table_design),
        ("xlsx::formula024_autocomplete", formula024_autocomplete),
        ("xlsx::xsc27_chart_format", xsc27_chart_format),
        ("xlsx::chart003_axis_titles", chart003_axis_titles),
        ("xlsx::chart005_data_labels", chart005_data_labels),
        ("xlsx::chart006_data_table", chart006_data_table),
        ("xlsx::chart016_error_bars", chart016_error_bars),
        ("xlsx::chart017_trendline", chart017_trendline),
        ("xlsx::chart010_axis_orientation", chart010_axis_orientation),
        ("xlsx::chart012_gridlines", chart012_gridlines),
        ("xlsx::chart007_axis_units", chart007_axis_units),
        ("xlsx::chart013_series_gap_and_overlap", chart013_series_gap_and_overlap),
        ("xlsx::chart008_axis_log_scale", chart008_axis_log_scale),
        ("xlsx::chart018_disp_blanks_and_plot_vis_only", chart018_disp_blanks_and_plot_vis_only),
        ("xlsx::chart019_style_and_palette", chart019_style_and_palette),
        ("xlsx::chart014_series_marker_and_stroke", chart014_series_marker_and_stroke),
        ("xlsx::chart009_date_axis", chart009_date_axis),
        ("xlsx::chart011_secondary_and_series_axis", chart011_secondary_and_series_axis),
        ("xlsx::chart015_series_fill_and_outline", chart015_series_fill_and_outline),
        ("xlsx::formula010_array_constants", formula010_array_constants),
        ("xlsx::formula005_3d_references", formula005_3d_references),
        ("xlsx::formula002_operator_precedence", formula002_operator_precedence),
        ("xlsx::formula013_implicit_intersection", formula013_implicit_intersection),
        ("xlsx::xsc27_chart_data", xsc27_chart_data),
        ("xlsx::cell005_rich_text", cell005_rich_text),
        ("xlsx::pivot005_design", pivot005_design),
        ("xlsx::pivot014_filters", pivot014_filters),
        ("xlsx::whatif001_goal_seek", whatif001_goal_seek),
        ("xlsx::whatif002_data_table", whatif002_data_table),
        ("xlsx::whatif004_scenarios", whatif004_scenarios),
        ("xlsx::whatif006_merge_scenarios", whatif006_merge_scenarios),
        ("xlsx::spark001_create_edit_clear", spark001_create_edit_clear),
        ("xlsx::spark002_style_color_display_hidden", spark002_style_color_display_hidden),
        ("xlsx::spark003_date_axis_and_cross_sheet", spark003_date_axis_and_cross_sheet),
        ("xlsx::subtotal001_apply_and_remove", subtotal001_apply_and_remove),
        ("xlsx::cell005_rich_text_edit", cell005_rich_text_edit),
        ("xlsx::xsc28_print_titles_and_footer", xsc28_print_titles_and_footer),
        ("xlsx::xsc_english_panels", xsc_english_panels),
        ("xlsx::spelling_check", spelling_check),
        ("xlsx::xsc25_conditional", xsc25_conditional),
        ("xlsx::xsc25_conditional_cell_is", xsc25_conditional_cell_is),
        ("xlsx::xsc25_conditional_opaque_edit", xsc25_conditional_opaque_edit),
        ("xlsx::xsc25_conditional_insert", xsc25_conditional_insert),
        ("xlsx::xsc25_conditional_manage", xsc25_conditional_manage),
        ("xlsx::cf018_duplicate_and_delete_rule", cf018_duplicate_and_delete_rule),
        ("xlsx::cf011_threshold_types", cf011_threshold_types),
        ("xlsx::cf012_databar_negative_and_axis", cf012_databar_negative_and_axis),
        ("xlsx::cf010_icon_sets_shapes_and_custom", cf010_icon_sets_shapes_and_custom),
        ("xlsx::table011_icon_filter_and_sort", table011_icon_filter_and_sort),
        ("xlsx::view015_window_hide_unhide_and_protection", view015_window_hide_unhide_and_protection),
        ("xlsx::chedit020_move_chart_to_existing_and_new_sheet", chedit020_move_chart_to_existing_and_new_sheet),
        ("xlsx::chedit021_chart_templates", chedit021_chart_templates),
        ("xlsx::chedit001_series_add_delete_and_reorder", chedit001_series_add_delete_and_reorder),
        ("xlsx::sheet017_arrange_windows", sheet017_arrange_windows),
        ("xlsx::view011_new_window_for_same_workbook", view011_new_window_for_same_workbook),
        ("xlsx::view013_view_side_by_side_and_sync_scrolling", view013_view_side_by_side_and_sync_scrolling),
        ("xlsx::file013_compatibility_check", file013_compatibility_check),
        ("xlsx::file014_autorecover_settings", file014_autorecover_settings),
        ("xlsx::file015_and_file016_recover_and_save_workbook", file015_and_file016_recover_and_save_workbook),
        ("xlsx::file019_backup_and_save_location", file019_backup_and_save_location),
        ("xlsx::file020_open_and_repair_damaged_workbook", file020_open_and_repair_damaged_workbook),
        ("xlsx::chtype022_radar_chart_styles_and_rendering", chtype022_radar_chart_styles_and_rendering),
        ("xlsx::chtype011_of_pie_chart_styles_and_rendering", chtype011_of_pie_chart_styles_and_rendering),
        ("xlsx::chtype018_stock_chart_hlc_and_ohlc", chtype018_stock_chart_hlc_and_ohlc),
        ("xlsx::chtype020_stock_chart_volume_vhlc_and_vohlc", chtype020_stock_chart_volume_vhlc_and_vohlc),
        ("xlsx::chtype021_surface_and_contour_charts", chtype021_surface_and_contour_charts),
        ("xlsx::view016_custom_views_register_show_delete_and_table_exclusion", view016_custom_views_register_show_delete_and_table_exclusion),
        ("xlsx::object015_wordart_insert_styles_and_warp", object015_wordart_insert_styles_and_warp),
        ("xlsx::pivot020_pivot_chart_create_and_sync", pivot020_pivot_chart_create_and_sync),
        ("xlsx::cell006_phonetic_edit_and_formula_sync", cell006_phonetic_edit_and_formula_sync),
        ("xlsx::fn_regex_test_extract_replace", fn_regex_test_extract_replace),
        ("xlsx::fn_bahttext", fn_bahttext),
        ("xlsx::fn_cumipmt_cumprinc", fn_cumipmt_cumprinc),
        ("xlsx::fn_ppmt", fn_ppmt),
        ("xlsx::xsc05_cell_comments", xsc05_cell_comments),
        ("xlsx::xsc13_border_styles", xsc13_border_styles),
        ("xlsx::xsc13_alignment_options", xsc13_alignment_options),
        ("xlsx::xsc13_alignment_all_values", xsc13_alignment_all_values),
        ("xlsx::xsc13_underline_kinds", xsc13_underline_kinds),
        ("xlsx::xsc55_implicit_operators", xsc55_implicit_operators),
        ("xlsx::xsc39_external_cached_values", xsc39_external_cached_values),
        ("xlsx::xsc55_flatten_options", xsc55_flatten_options),
        ("xlsx::xsc39_weekend_options", xsc39_weekend_options),
        ("xlsx::xsc39_datedif_units", xsc39_datedif_units),
        ("xlsx::xsc39_information_options", xsc39_information_options),
        ("xlsx::xsc55_sort_options", xsc55_sort_options),
        ("xlsx::xsc55_sort_errors", xsc55_sort_errors),
        ("xlsx::xsc55_textsplit_options", xsc55_textsplit_options),
        ("xlsx::xsc55_textsplit_shape", xsc55_textsplit_shape),
        ("xlsx::xsc55_text_casefold", xsc55_text_casefold),
        ("xlsx::xsc39_subtotal_literals", xsc39_subtotal_literals),
        ("xlsx::xsc39_subtotal_count", xsc39_subtotal_count),
        ("xlsx::xsc39_aggregate_count_options", xsc39_aggregate_count_options),
        ("xlsx::xsc08_jis_name", xsc08_jis_name),
        ("xlsx::xsc39_aggregate_resize", xsc39_aggregate_resize),
        ("xlsx::xsc55_choose_lines_options", xsc55_choose_lines_options),
        ("xlsx::xsc23_sort_manage", xsc23_sort_manage),
        ("xlsx::xsc23_sort_keys", xsc23_sort_keys),
        ("xlsx::xsc23_sort_filter", xsc23_sort_filter),
        ("xlsx::xsc55_spill_structure", xsc55_spill_structure),
        ("xlsx::xsc55_spill_cross_sheet", xsc55_spill_cross_sheet),
        ("xlsx::xsc55_dynamic_reference", xsc55_dynamic_reference),
        ("xlsx::xsc55_dynamic_reopen", xsc55_dynamic_reopen),
        ("xlsx::xsc55_new_dynamic_metadata", xsc55_new_dynamic_metadata),
        ("xlsx::xsc55_composite_formula_literal", xsc55_composite_formula_literal),
        ("xlsx::xsc39_week_return_types", xsc39_week_return_types),
        ("xlsx::xsc39_day_count_options", xsc39_day_count_options),
        ("xlsx::xsc55_spill_partial_delete", xsc55_spill_partial_delete),
        ("xlsx::xsc08_match_options", xsc08_match_options),
        ("xlsx::xsc55_unique_options", xsc55_unique_options),
        ("xlsx::xsc55_makearray_dimensions", xsc55_makearray_dimensions),
        ("xlsx::xsc55_lambda_bindings", xsc55_lambda_bindings),
        ("xlsx::xsc55_map_shapes", xsc55_map_shapes),
        ("xlsx::xsc55_lambda_arity", xsc55_lambda_arity),
        ("xlsx::xsc55_text_boundary", xsc55_text_boundary),
        ("xlsx::xsc55_compatibility_len", xsc55_compatibility_len),
        ("xlsx::xsc55_compatibility_slice", xsc55_compatibility_slice),
        ("xlsx::xsc55_compatibility_search", xsc55_compatibility_search),
        ("xlsx::xsc55_compatibility_toggle", xsc55_compatibility_toggle),
        ("xlsx::xsc55_compatibility_whitespace", xsc55_compatibility_whitespace),
        ("xlsx::xsc55_isomitted", xsc55_isomitted),
        ("xlsx::xsc01_scrollbar_edge_selection", xsc01_scrollbar_edge_selection),
        ("xlsx::ui_menu_icons", ui_menu_icons),
    ]
}

/// Menu icons and the primary button (screenshots to look at).
fn ui_menu_icons() {
    let mut s = Scenario::open_sized("test/editing/xlsx-sales.xlsx", 1500.0);
    s.click((739.0, 84.0));
    s.settle();
    s.click((772.0, 435.0));
    s.settle();
    s.screenshot("border-styles");
    s.key("escape");
    s.key("escape");
    s.settle();
    s.click((780.0, 84.0));
    s.settle();
    s.screenshot("merge");
    s.key("escape");
    s.settle();
    s.click((826.0, 84.0));
    s.settle();
    s.screenshot("halign");
    s.key("escape");
    s.settle();
    s.size(1500.0, 1100.0);
    s.update(|e, window, cx| e.open_sheet_doc_properties_panel(window, cx));
    s.settle();
    s.screenshot("properties");
}

/// Typing into cells: a number replaces a cell's value (and what depends on
/// it is calculated again), Enter moves down, a formula, a Japanese entry,
/// undo, save and read back.
fn type_numbers_and_a_formula() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.screenshot("opened");
    assert_eq!(s.value("E4"), CellValue::Number(3_750_000.0));
    s.click_cell("B4");
    assert_eq!(s.selected(), "B4");
    s.type_text("1300000");
    s.screenshot("typing");
    s.key("enter");
    assert_eq!(s.selected(), "B5", "Enter moves down");
    assert_eq!(s.value("B4"), CellValue::Number(1_300_000.0));
    assert_eq!(s.value("E4"), CellValue::Number(3_800_000.0), "the row's total follows");
    assert_eq!(s.shown("B4"), "1,300,000", "the cell's format stays");
    // A formula typed into an empty cell, and Tab.
    s.click_cell("G4");
    s.type_text("=E4/1000");
    s.key("tab");
    assert_eq!(s.selected(), "H4");
    assert_eq!(s.value("G4"), CellValue::Number(3800.0));
    // Japanese through the input method, and an automatic format.
    s.click_cell("G5");
    s.ime("うりあげ", "売上");
    s.key("enter");
    assert_eq!(s.value("G5"), CellValue::Text("売上".into()));
    s.click_cell("G6");
    s.type_text("50%");
    s.key("enter");
    assert_eq!(s.value("G6"), CellValue::Number(0.5));
    assert_eq!(s.shown("G6"), "50%");
    s.screenshot("entered");
    // Undo the last entry (the value and its format together).
    s.key("cmd-z");
    assert!(s.value("G6").is_empty());
    s.key("cmd-shift-z");
    assert_eq!(s.shown("G6"), "50%");
    s.save_as("xlsx-sales-typed.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.cell(3, 1).unwrap().value, CellValue::Number(1_300_000.0));
    assert_eq!(sheet.cell(3, 6).unwrap().formula.as_deref(), Some("E4/1000"));
    assert_eq!(sheet.cell(5, 6).unwrap().text, "50%");
}

/// F2 edits a cell's content, Esc gives it up, Backspace takes a
/// character, Delete clears cells.
fn edit_with_f2_and_cancel() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B4");
    s.key("f2");
    assert!(s.with(|e, _| e.cell_edit().is_some_and(|c| c.text == "1250000")));
    s.key("backspace");
    s.key("escape");
    assert_eq!(s.value("B4"), CellValue::Number(1_250_000.0), "Esc gives the entry up");
    s.key("f2");
    s.key("backspace");
    s.key("backspace");
    s.type_text("99");
    s.key("enter");
    assert_eq!(s.value("B4"), CellValue::Number(1_250_099.0));
    // Delete clears the selected cell (B5 now).
    s.key("delete");
    assert!(s.value("B5").is_empty());
    assert_eq!(s.value("E5"), CellValue::Number(2_115_000.0), "the total follows");
    s.key("cmd-z");
    assert_eq!(s.value("B5"), CellValue::Number(980_000.0));
}

/// The toolbar formats the selected range; the menus insert a row and a
/// sheet; saved and read back.
fn format_insert_rows_and_sheets() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B4");
    // Shift+arrows stretch the range to C5.
    s.key("shift-right");
    s.key("shift-down");
    assert_eq!(s.toolbar_state("bold").map(|t| t.on), Some(false));
    s.toolbar("bold");
    assert_eq!(s.toolbar_state("bold").map(|t| t.on), Some(true));
    s.key("cmd-i");
    s.toolbar_menu("fill", "xlsx.fill");
    s.toolbar("decimal-more");
    let style = |s: &mut Scenario, r: u32, c: u32| s.with(move |e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.styles[sheet.cell(r, c).unwrap().style as usize].clone()
    });
    let st = style(&mut s, 4, 2);
    assert!(st.bold && st.italic && st.fill.is_some());
    assert_eq!(s.shown("B4"), "1,250,000.0");
    s.screenshot("formatted");
    // Rows above the selection (two rows selected: two rows) from the menu.
    s.menu("xlsx.insert-row-above");
    assert_eq!(s.value("B6"), CellValue::Number(1_250_000.0));
    assert!(s.value("B4").is_empty());
    // A new sheet.
    s.menu("xlsx.insert-sheet");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 4);
    s.save_as("xlsx-sales-formatted.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides.len(), 4);
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    let c = sheet.cell(5, 1).unwrap();
    assert!(sheet.styles[c.style as usize].bold);
    assert_eq!(c.text, "1,250,000.0");
}

/// Copy and paste a formula (its references move), paste values only, cut
/// and paste (the cells go, the formula stays as it was), clear formats.
fn copy_cut_and_paste() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let formula = |s: &mut Scenario, r: &str| {
        let (column, row) = parse(r);
        s.with(move |e, _| sheet_of_slide(e.slide()).and_then(|sh| sh.cell(row, column)).and_then(|c| c.formula.clone()))
    };
    s.click_cell("E4");
    s.key("cmd-c");
    s.click_cell("E13");
    s.key("cmd-v");
    assert_eq!(formula(&mut s, "E13").as_deref(), Some("SUM(B13:D13)"), "the references move");
    assert_eq!(s.value("E13"), CellValue::Number(0.0));
    assert_eq!(s.shown("E13"), "0", "the format comes too");
    // Values only.
    s.click_cell("E9");
    s.key("cmd-c");
    s.click_cell("G4");
    s.key("cmd-shift-v");
    assert_eq!(formula(&mut s, "G4"), None);
    assert_eq!(s.value("G4"), CellValue::Number(20_515_000.0));
    // Cut B4:C4 to B13: the cells go, E4 and E13 follow.
    s.click_cell("B4");
    s.key("shift-right");
    s.key("cmd-x");
    s.click_cell("B13");
    s.key("cmd-v");
    assert!(s.value("B4").is_empty() && s.value("C4").is_empty());
    assert_eq!(s.value("B13"), CellValue::Number(1_250_000.0));
    assert_eq!(s.value("E4"), CellValue::Number(1_180_000.0));
    assert_eq!(s.value("E13"), CellValue::Number(2_570_000.0));
    s.screenshot("pasted");
    // Clear formats (⌘\\): the number shows plain.
    s.key("cmd-\\");
    assert_eq!(s.shown("B13"), "1250000");
    // One step each: undo the clearing and the cut.
    s.key("cmd-z");
    s.key("cmd-z");
    assert_eq!(s.value("B4"), CellValue::Number(1_250_000.0));
    // Text from another app is read as typed.
    s.update(|_, _, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("10\t=G5*2\n50%\t".into())));
    s.click_cell("G5");
    s.key("cmd-v");
    assert_eq!(s.value("G5"), CellValue::Number(10.0));
    assert_eq!(s.value("H5"), CellValue::Number(20.0));
    assert_eq!(s.value("G6"), CellValue::Number(0.5));
    s.save_as("xlsx-sales-pasted.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.cell(12, 4).unwrap().formula.as_deref(), Some("SUM(B13:D13)"));
    assert_eq!(sheet.cell(4, 7).unwrap().value, CellValue::Number(20.0));
}

/// Σ: on one cell, the numbers above go into SUM and Enter confirms; on a
/// range, the totals go into its empty last row. A formula takes the
/// number format of the cell it refers to.
fn auto_sum() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("G4");
    for n in ["10", "20", "30"] {
        s.type_text(n);
        s.key("enter");
    }
    assert_eq!(s.selected(), "G7");
    s.toolbar_menu("functions", "xlsx.function");
    assert!(s.with(|e, _| e.cell_edit().is_some_and(|c| c.text == "=SUM(G4:G6)")), "the entry is left open");
    s.screenshot("sum");
    s.key("enter");
    assert_eq!(s.value("G7"), CellValue::Number(60.0));
    // A range with an empty last row: a total under each column with numbers.
    s.click_cell("H4");
    s.type_text("5");
    s.key("enter");
    s.type_text("7");
    s.key("enter");
    s.click_cell("H4");
    s.key("shift-down");
    s.key("shift-down");
    s.key("shift-right");
    s.toolbar_menu("functions", "xlsx.function");
    assert_eq!(s.value("H6"), CellValue::Number(12.0));
    assert!(s.value("I6").is_empty(), "no numbers, no total");
    // A formula shows as the cell it refers to.
    s.click_cell("F4");
    s.type_text("=B4*2");
    s.key("enter");
    assert_eq!(s.shown("F4"), "2,500,000");
    s.save_as("xlsx-sales-autosum.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.cell(6, 6).unwrap().formula.as_deref(), Some("SUM(G4:G6)"));
    assert_eq!(sheet.cell(5, 7).unwrap().formula.as_deref(), Some("SUM(H4:H5)"));
    assert_eq!(sheet.cell(3, 5).unwrap().text, "2,500,000");
    s.key("cmd-z");
    assert!(s.value("F4").is_empty());
}

/// Renaming a sheet in its tab's name field: a name Excel does not take
/// is refused; formulas on other sheets follow the new name.
fn rename_a_sheet() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let names = |s: &mut Scenario| s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().map(|x| x.name.clone()).collect::<Vec<_>>());
    let before = names(&mut s);
    s.menu("xlsx.rename-sheet");
    assert!(s.with(|e, _| e.sheet_state().unwrap().jump.renaming.is_some()));
    s.type_text("売上/月");
    s.key("enter");
    assert_eq!(names(&mut s), before, "a / is not taken");
    assert!(s.with(|e, _| e.sheet_state().unwrap().jump.renaming.is_some()), "the field stays open");
    s.key("cmd-a");
    s.type_text("売上2026");
    s.screenshot("renaming");
    s.key("enter");
    assert_eq!(names(&mut s)[0], "売上2026");
    assert!(s.with(|e, _| e.sheet_state().unwrap().jump.renaming.is_none()));
    s.save_as("xlsx-sales-renamed.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.workbook.as_ref().unwrap().sheets[0].name, "売上2026");
    // The keys go to the cells again.
    s.click_cell("G4");
    s.type_text("1");
    s.key("enter");
    assert_eq!(s.value("G4"), CellValue::Number(1.0));
    s.key("cmd-z");
    s.key("cmd-z");
    assert_eq!(names(&mut s), before);
    // A double-click on the second tab (商品) opens its name field; Enter
    // with the name unchanged just closes it.
    s.double_click((227.0, 680.0));
    assert_eq!(s.with(|e, _| e.sheet_state().unwrap().jump.renaming), s.with(|e, _| Some(e.presentation().slides[1].id)));
    s.key("enter");
    assert!(s.with(|e, _| e.sheet_state().unwrap().jump.renaming.is_none()));
    // Adding a sheet via menu or tab button.
    s.menu("xlsx.insert-sheet");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 4);
}

/// ⌃9 hides rows and ⌃0 columns; the arrows pass over them; ⌃⇧9 shows
/// them again at their height; saved as hidden.
fn hide_rows_and_columns() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let rows = |s: &mut Scenario| s.with(|e, _| sheet_of_slide(e.slide()).unwrap().rows.clone());
    let heights = rows(&mut s);
    s.click_cell("B5");
    s.key("shift-down");
    s.key("ctrl-9");
    let hidden = rows(&mut s);
    assert_eq!((hidden[4], hidden[5]), (0, 0));
    s.screenshot("rows-hidden");
    s.click_cell("B4");
    s.key("down");
    assert_eq!(s.selected(), "B7", "the arrow passes the hidden rows");
    // B7 to B4: the range spans the hidden rows.
    s.key("shift-up");
    s.key("ctrl-shift-9");
    assert_eq!(rows(&mut s), heights, "shown at their heights again");
    // A column, saved hidden.
    s.click_cell("C4");
    s.key("ctrl-0");
    assert_eq!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().columns[2]), 0);
    assert_eq!(s.value("E4"), CellValue::Number(3_750_000.0), "hidden cells still count");
    s.save_as("xlsx-sales-hidden.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.columns[2], 0);
    assert!(sheet.hidden_columns.get(&2).is_some_and(|&w| w > 0));
    assert_eq!(sheet.rows, heights);
    s.key("cmd-z");
    assert!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().columns[2]) > 0);
}

/// The headers: a click selects a column, Shift+click stretches to more;
/// dragging a line sizes (to nothing hides); a double-click on it fits the
/// column; the right button's menu inserts, hides and shows.
fn headers() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let range = |s: &mut Scenario| s.with(|e, _| e.selected_range().map(|r| (r.first_column, r.first_row, r.last_column, r.last_row)));
    // C is under the merged A1:E1: its column takes A to E, as in Excel.
    let p = s.header("C");
    s.click(p);
    assert_eq!(range(&mut s).map(|r| (r.0, r.1, r.2)), Some((0, 0, 4)), "columns A to E");
    let p = s.header("G");
    s.click(p);
    assert_eq!(range(&mut s).map(|r| (r.0, r.1, r.2)), Some((6, 0, 6)), "the whole column G");
    let p = s.header("H");
    s.shift_click(p);
    assert_eq!(range(&mut s).map(|r| (r.0, r.2)), Some((6, 7)), "G to H");
    let p = s.header("3");
    s.click(p);
    assert_eq!(range(&mut s).map(|r| (r.1, r.3, r.0)), Some((2, 2, 0)), "the whole row 3");
    // Drag C's right line 40 px right.
    let before = s.size_pt("C");
    let scale = s.with(|e, _| e.sheet_state().unwrap().header_geometry.borrow().scale) * ibis_doc_model::EMU_PER_POINT as f32;
    let (x, y) = s.header_line("C");
    s.drag((x, y), (x + 40.0, y));
    let after = s.size_pt("C");
    assert!((after - before - 40.0 / scale).abs() < 1.5, "{before} → {after}");
    s.screenshot("sized");
    // Row 5's line dragged up to its top: hidden.
    let (x, y) = s.header_line("5");
    let top = s.with(|e, _| e.sheet_state().unwrap().header_geometry.borrow().rows.iter().find(|r| r.0 == 4).unwrap().1);
    s.drag((x, y), (x, top));
    assert_eq!(s.size_pt("5"), 0.0);
    s.key("cmd-z");
    assert!(s.size_pt("5") > 0.0);
    // A double-click on B's line fits it to 1,250,000 and the like.
    let (x, y) = s.header_line("B");
    s.double_click((x, y));
    let fitted = s.size_pt("B");
    assert!(fitted > 20.0 && fitted < 80.0, "fitted to {fitted} pt");
    // The menu: hide D, show it again, insert a row above row 4.
    let p = s.header("D");
    s.right_click(p);
    s.screenshot("header-menu");
    s.key("escape");
    s.header_menu("D", "xlsx.header-hide-columns");
    assert_eq!(s.size_pt("D"), 0.0);
    // Select C to E by dragging along the headers, and show D.
    let (from, to) = (s.header("C"), s.header("E"));
    s.drag(from, to);
    s.header_menu("C", "xlsx.header-unhide-columns");
    assert!(s.size_pt("D") > 0.0);
    s.header_menu("4", "xlsx.header-insert-above");
    assert_eq!(s.value("B5"), CellValue::Number(1_250_000.0));
    assert!(s.value("B4").is_empty());
    s.save_as("xlsx-sales-headers.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    let pt = |v: i64| v as f32 / ibis_doc_model::EMU_PER_POINT as f32;
    assert!((pt(sheet.columns[2]) - after).abs() < 0.5, "{} vs {after}", pt(sheet.columns[2]));
    assert!((pt(sheet.columns[1]) - fitted).abs() < 0.5);
}

/// XSC-01: a sales table made on a new sheet: headings, three months of
/// numbers, `=B2+C2` filled down with the fill handle, AutoSum under the
/// columns, bold headings; saved and read back.
fn xsc01_sales_table() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("xlsx.insert-sheet");
    assert_eq!(s.selected(), "A1");
    for (i, h) in ["月", "店舗A", "店舗B", "合計"].iter().enumerate() {
        s.enter(h, if i < 3 { "tab" } else { "enter" });
    }
    // Enter after Tabs goes back to the first column, as in Excel.
    assert_eq!(s.selected(), "A2");
    for (m, a, b) in [("4月", "120", "80"), ("5月", "135", "95"), ("6月", "150", "70")] {
        s.enter(m, "tab");
        s.enter(a, "tab");
        s.enter(b, "enter");
    }
    s.click_cell("D2");
    s.enter("=B2+C2", "enter");
    s.click_cell("D2");
    s.drag_fill("D2", "D4");
    assert_eq!(s.formula("D3").as_deref(), Some("B3+C3"));
    assert_eq!(s.value("D4"), CellValue::Number(220.0));
    // AutoSum under B to D: the numbers above go in, Enter confirms.
    s.click_cell("B5");
    s.key("shift-right");
    s.key("shift-right");
    s.toolbar_menu("functions", "xlsx.function");
    assert_eq!(s.formula("B5").as_deref(), Some("SUM(B2:B4)"));
    assert_eq!(s.value("D5"), CellValue::Number(650.0));
    // Bold headings.
    s.click_cell("A1");
    s.key("shift-right");
    s.key("shift-right");
    s.key("shift-right");
    s.key("cmd-b");
    s.screenshot("table");
    s.save_as("xsc01.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[1]).unwrap().clone();
    assert_eq!(sheet.cell(3, 3).unwrap().formula.as_deref(), Some("B4+C4"));
    assert_eq!(sheet.cell(4, 1).unwrap().value, CellValue::Number(405.0));
    assert!(sheet.styles[sheet.cell(0, 2).unwrap().style as usize].bold);
}

/// A new sheet to work on (Excel's 新規 is a new sheet here: a new book cannot be saved yet).
fn new_sheet() -> Scenario {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("xlsx.insert-sheet");
    s
}

/// The number format of a cell of the current sheet.
fn number_format(s: &mut Scenario, reference: &str) -> String {
    let (column, row) = parse(reference);
    s.with(move |e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.cell(row, column).map_or("General".into(), |c| sheet.styles[c.style as usize].number_format.clone())
    })
}

/// XSC-02: what is typed is read as Excel reads it, with the format it gives.
fn xsc02_typed_values() {
    let mut s = new_sheet();
    let typed = ["1,234", "50%", "2026/9/30", "12:30", "¥1,000", "TRUE", "'0123", "1e3", "3/4", "-5"];
    for t in typed {
        s.enter(t, "enter");
    }
    let n = |v: f64| CellValue::Number(v);
    assert_eq!(s.value("A1"), n(1234.0));
    assert_eq!(s.shown("A1"), "1,234");
    assert_eq!(s.value("A2"), n(0.5));
    assert_eq!(s.shown("A2"), "50%");
    assert_eq!(s.value("A3"), n(46295.0));
    assert_eq!(s.shown("A3"), "2026/9/30");
    assert_eq!(s.value("A4"), n(12.5 / 24.0));
    assert_eq!(s.shown("A4"), "12:30");
    assert_eq!(s.value("A5"), n(1000.0));
    assert_eq!(s.shown("A5"), "¥1,000");
    assert_eq!(s.value("A6"), CellValue::Bool(true));
    assert_eq!(s.shown("A6"), "TRUE");
    assert_eq!(s.value("A7"), CellValue::Text("0123".into()));
    assert_eq!(s.value("A8"), n(1000.0));
    assert_eq!(s.shown("A8"), "1.00E+03");
    // 3/4 is a date this year (3月4日).
    assert_eq!(s.shown("A9"), "3月4日");
    assert_eq!(s.value("A10"), n(-5.0));
    s.save_as("xsc02.xlsx");
    s.reopen();
}

/// XSC-03: a row inserted inside the summed range widens the SUM; the
/// formulas below move with it.
fn xsc03_insert_row_grows_sum() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    assert_eq!(s.formula("B9").as_deref(), Some("SUM(B4:B8)"));
    s.click_cell("B6");
    s.menu("xlsx.insert-row-above");
    assert_eq!(s.formula("B10").as_deref(), Some("SUM(B4:B9)"), "the SUM widens");
    assert_eq!(s.formula("E7").as_deref(), Some("SUM(B7:D7)"), "the row's total moves down");
    s.click_cell("A6");
    s.enter("新町店", "tab");
    s.enter("500000", "tab");
    s.enter("600000", "tab");
    s.enter("700000", "enter");
    assert_eq!(s.value("B10"), CellValue::Number(7_120_000.0));
    // E6 had no formula: the total above is filled down into it.
    s.click_cell("E5");
    s.drag_fill("E5", "E6");
    assert_eq!(s.formula("E6").as_deref(), Some("SUM(B6:D6)"));
    assert_eq!(s.value("E6"), CellValue::Number(1_800_000.0));
    assert_eq!(s.value("E10"), CellValue::Number(22_315_000.0), "the grand total follows");
    s.save_as("xsc03.xlsx");
    s.reopen();
}

/// XSC-04: deleting a row a formula refers to gives #REF!; undo brings it back.
fn xsc04_delete_row_ref_error() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("G4");
    s.enter("=B5*2", "enter");
    assert_eq!(s.value("G4"), CellValue::Number(1_960_000.0));
    s.click_cell("A5");
    s.menu("xlsx.delete-row");
    assert_eq!(s.formula("G4").as_deref(), Some("#REF!*2"));
    assert_eq!(s.shown("G4"), "#REF!");
    assert_eq!(s.formula("B8").as_deref(), Some("SUM(B4:B7)"), "the SUM shrinks");
    s.save_as("xsc04.xlsx");
    s.key("cmd-z");
    assert_eq!(s.formula("G4").as_deref(), Some("B5*2"));
    assert_eq!(s.value("G4"), CellValue::Number(1_960_000.0));
}

/// XSC-05: a column inserted before B moves the formulas and widths; deleting it puts them back.
fn xsc05_insert_delete_columns() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let width_c = s.size_pt("C");
    s.click_cell("B4");
    s.menu("xlsx.insert-column-left");
    assert_eq!(s.formula("F4").as_deref(), Some("SUM(C4:E4)"));
    assert_eq!(s.value("C4"), CellValue::Number(1_250_000.0));
    assert!((s.size_pt("D") - width_c).abs() < 0.01, "the widths move with the columns");
    assert_eq!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().merges.clone()).first().map(|m| (m.first_column, m.last_column)), Some((0, 5)), "the merge widens");
    s.save_as("xsc05-inserted.xlsx");
    s.click_cell("B4");
    s.menu("xlsx.delete-column");
    assert_eq!(s.formula("E4").as_deref(), Some("SUM(B4:D4)"));
    s.save_as("xsc05.xlsx");
    s.reopen();
}

/// XSC-06: a sheet named 集計 sums another sheet; changing a number there
/// recalculates; renaming the sheet renames it in the formula.
fn xsc06_sum_across_sheets() {
    let mut s = new_sheet();
    s.menu("xlsx.rename-sheet");
    s.key("cmd-a");
    s.enter("集計", "enter");
    s.click_cell("A1");
    s.enter("=SUM(月次売上!E4:E8)", "enter");
    assert_eq!(s.value("A1"), CellValue::Number(20_515_000.0));
    // Back to the first sheet: change a number.
    s.key("ctrl-pageup");
    s.click_cell("B4");
    s.enter("1350000", "enter");
    s.key("ctrl-pagedown");
    assert_eq!(s.value("A1"), CellValue::Number(20_615_000.0));
    s.key("ctrl-pageup");
    s.menu("xlsx.rename-sheet");
    s.key("cmd-a");
    s.enter("売上", "enter");
    s.key("ctrl-pagedown");
    assert_eq!(s.formula("A1").as_deref(), Some("SUM(売上!E4:E8)"));
    s.save_as("xsc06.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.workbook.as_ref().unwrap().sheets[1].name, "集計");
}

/// XSC-07: `=B2*$E$1` copied down and right: only the $ parts stay.
fn xsc07_relative_absolute() {
    let mut s = new_sheet();
    s.click_cell("E1");
    s.enter("0.1", "enter");
    s.click_cell("B2");
    for v in ["100", "200", "300"] {
        s.enter(v, "enter");
    }
    s.click_cell("C2");
    s.enter("=B2*$E$1", "enter");
    s.click_cell("C2");
    s.key("cmd-c");
    s.click_cell("C3");
    s.key("shift-down");
    s.key("cmd-v");
    assert_eq!(s.formula("C4").as_deref(), Some("B4*$E$1"));
    assert_eq!(s.value("C4"), CellValue::Number(30.0));
    s.click_cell("D2");
    s.key("cmd-v");
    assert_eq!(s.formula("D2").as_deref(), Some("C2*$E$1"));
    assert_eq!(s.value("D2"), CellValue::Number(1.0));
    s.save_as("xsc07.xlsx");
    s.reopen();
}

/// XSC-10: errors as Excel shows them, wrapped in IFERROR, then IFNA.
fn xsc10_errors() {
    let mut s = new_sheet();
    s.enter("=1/0", "enter");
    s.enter("=VLOOKUP(\"無い\",月次売上!A4:E8,2,FALSE)", "enter");
    assert_eq!(s.shown("A1"), "#DIV/0!");
    assert_eq!(s.shown("A2"), "#N/A");
    s.click_cell("A1");
    s.enter("=IFERROR(1/0,\"-\")", "enter");
    s.enter("=IFNA(VLOOKUP(\"無い\",月次売上!A4:E8,2,FALSE),0)", "enter");
    assert_eq!(s.value("A1"), CellValue::Text("-".into()));
    assert_eq!(s.value("A2"), CellValue::Number(0.0));
    s.click_cell("A3");
    s.enter("=IFNA(1/0,0)", "enter");
    assert_eq!(s.shown("A3"), "#DIV/0!", "IFNA lets other errors through");
    s.save_as("xsc10.xlsx");
    s.reopen();
}

/// XSC-11: a circular reference: no hang, 0 as Excel shows; clearing one side recalculates.
fn xsc11_circular() {
    let mut s = new_sheet();
    s.enter("=B1+1", "tab");
    s.enter("=A1", "enter");
    // The cell entered closing the circle shows 0, as Excel's does (no hang).
    assert_eq!(s.value("B1"), CellValue::Number(0.0));
    s.click_cell("B1");
    s.key("delete");
    assert_eq!(s.value("A1"), CellValue::Number(1.0));
    s.save_as("xsc11.xlsx");
    s.reopen();
}

/// XSC-18: merge and centre a title, unmerge, merge across rows.
fn xsc18_merge() {
    let mut s = new_sheet();
    s.enter("売上報告", "enter");
    s.click_cell("A1");
    for _ in 0..3 {
        s.key("shift-right");
    }
    s.toolbar_menu("merge", "xlsx.merge-center");
    let merges = |s: &mut Scenario| s.with(|e, _| sheet_of_slide(e.slide()).unwrap().merges.iter().map(|m| (m.first_row, m.first_column, m.last_row, m.last_column)).collect::<Vec<_>>());
    assert_eq!(merges(&mut s), [(0, 0, 0, 3)]);
    s.toolbar_menu("merge", "xlsx.unmerge");
    assert!(merges(&mut s).is_empty());
    assert_eq!(s.value("A1"), CellValue::Text("売上報告".into()), "the value stays in the top-left cell");
    // Across: A3:C4 becomes A3:C3 and A4:C4.
    s.click_cell("A3");
    s.key("shift-down");
    s.key("shift-right");
    s.key("shift-right");
    s.toolbar_menu("merge", "xlsx.merge-across");
    assert_eq!(merges(&mut s), [(2, 0, 2, 2), (3, 0, 3, 2)]);
    s.key("cmd-z");
    assert!(merges(&mut s).is_empty(), "one undo");
    s.key("cmd-shift-z");
    s.save_as("xsc18.xlsx");
    let saved = s.reopen();
    assert_eq!(sheet_of_slide(&saved.slides[1]).unwrap().merges.len(), 2);
}

/// XSC-33: two sheets added, one renamed, one deleted (its references become #REF!), undo.
fn xsc33_sheets() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("xlsx.insert-sheet");
    s.enter("=月次売上!B4", "enter");
    s.menu("xlsx.insert-sheet");
    s.menu("xlsx.rename-sheet");
    s.key("cmd-a");
    s.enter("予備", "enter");
    let names = |s: &mut Scenario| s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().map(|x| x.name.clone()).collect::<Vec<_>>());
    assert_eq!(names(&mut s), ["月次売上", "Sheet4", "予備", "商品", "メモ"]);
    // The sheet a formula refers to goes: #REF!.
    s.key("ctrl-pageup");
    s.key("ctrl-pageup");
    s.menu("xlsx.delete-sheet");
    s.key("ctrl-pagedown");
    s.key("ctrl-pagedown");
    let _ = names(&mut s);
    s.save_as("xsc33.xlsx");
    s.key("cmd-z");
    assert_eq!(names(&mut s)[0], "月次売上");
    s.key("cmd-shift-z");
    let formula = s.with(|e, _| {
        let wb = e.presentation().workbook.clone().unwrap();
        let slide = wb.sheets.iter().find(|x| x.name == "Sheet4").and_then(|x| x.slide).unwrap();
        sheet_of_slide(e.presentation().slide(slide).unwrap()).unwrap().cell(0, 0).unwrap().formula.clone()
    });
    assert_eq!(formula.as_deref(), Some("#REF!"));
}

/// XSC-37: one number of an Excel file changed and saved: only the sheet's part changes.
fn xsc37_one_number() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B4");
    s.enter("1260000", "enter");
    let path = s.save_as("xsc37.xlsx");
    let diff = std::process::Command::new(crate::shell::scenario::repo_root().join("target/release/ibis-ooxml-diff"))
        .arg(crate::shell::scenario::repo_root().join("test/editing/xlsx-sales.xlsx"))
        .arg(&path)
        .arg("--json")
        .output();
    if let Ok(out) = diff {
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("xl/worksheets/sheet1.xml"), "{text}");
        for part in ["xl/charts/", "xl/drawings/", "xl/styles.xml", "xl/sharedStrings.xml", "xl/workbook.xml"] {
            assert!(!text.contains(&format!("\"part\": \"{part}")) && !text.contains(&format!("\"part\":\"{part}")), "{part} changed: {text}");
        }
    }
    let saved = s.reopen();
    assert_eq!(sheet_of_slide(&saved.slides[0]).unwrap().cell(3, 4).unwrap().value, CellValue::Number(3_760_000.0));
}

/// XSC-38: one cell of a shared formula changed; the others keep their values; undo.
fn xsc38_shared_formula() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("E6");
    s.enter("=B6*2", "enter");
    assert_eq!(s.value("E6"), CellValue::Number(3_060_000.0));
    assert_eq!(s.formula("E5").as_deref(), Some("SUM(B5:D5)"));
    assert_eq!(s.formula("E7").as_deref(), Some("SUM(B7:D7)"));
    s.save_as("xsc38.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.cell(6, 4).unwrap().formula.as_deref(), Some("SUM(B7:D7)"));
    s.key("cmd-z");
    assert_eq!(s.formula("E6").as_deref(), Some("SUM(B6:D6)"));
}

/// XSC-41: saved without a change: the same bytes (⌘S on the copy, and 名前を付けて保存).
fn xsc41_save_unchanged() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let original = std::fs::read(crate::shell::scenario::repo_root().join("test/editing/xlsx-sales.xlsx")).unwrap();
    s.key("cmd-s");
    let path = s.with(|e, _| e.path.clone()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original, "⌘S");
    let copy = s.save_as("xsc41.xlsx");
    assert_eq!(std::fs::read(copy).unwrap(), original, "名前を付けて保存");
}

/// XSC-42: entering by keyboard only: Enter down, Tab right, F2 to edit, Esc to give up, ⌘Z.
fn xsc42_keyboard_only() {
    let mut s = new_sheet();
    s.enter("品名", "tab");
    s.enter("数量", "enter");
    s.enter("りんご", "tab");
    s.enter("3", "enter");
    assert_eq!(s.selected(), "A3");
    s.key("up");
    s.key("right");
    assert_eq!(s.selected(), "B2");
    s.key("f2");
    s.type_text("0");
    assert!(s.with(|e, _| e.cell_edit().is_some_and(|c| c.text == "30")), "the formula bar has the entry");
    s.key("escape");
    assert_eq!(s.value("B2"), CellValue::Number(3.0), "Esc gives it up");
    s.type_text("5");
    s.key("enter");
    assert_eq!(s.value("B2"), CellValue::Number(5.0));
    s.key("cmd-z");
    assert_eq!(s.value("B2"), CellValue::Number(3.0));
    s.save_as("xsc42.xlsx");
    s.reopen();
}

/// XSC-43: a range dragged out, cleared (values only), formats cleared, all cleared; undo.
fn xsc43_clear_range() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let (a, b) = (s.with(|e, _| e.cell_center(1, 3)).unwrap(), s.with(|e, _| e.cell_center(2, 4)).unwrap());
    s.drag((f32::from(a.x), f32::from(a.y)), (f32::from(b.x), f32::from(b.y)));
    assert_eq!(s.with(|e, _| e.selected_range().map(|r| (r.first_column, r.first_row, r.last_column, r.last_row))), Some((1, 3, 2, 4)));
    s.key("delete");
    assert!(s.value("B4").is_empty() && s.value("C5").is_empty());
    assert_eq!(number_format(&mut s, "B4"), "#,##0", "the formats stay");
    s.key("cmd-z");
    assert_eq!(s.value("B4"), CellValue::Number(1_250_000.0));
    s.key("cmd-\\");
    assert_eq!(number_format(&mut s, "B4"), "General");
    assert_eq!(s.shown("B4"), "1250000", "the values stay");
    s.key("cmd-z");
    s.menu("xlsx.clear-all");
    assert!(s.value("B4").is_empty());
    assert_eq!(number_format(&mut s, "B4"), "General");
    s.key("cmd-z");
    assert_eq!(s.shown("B4"), "1,250,000");
    s.save_as("xsc43.xlsx");
    s.reopen();
}

/// XSC-45: Japanese through the input method: composing shows, Esc mid-way, then committed.
fn xsc45_japanese() {
    let mut s = new_sheet();
    s.update(|e, window, cx| {
        e.cell_replace(None, "とうきょう", Some(Some(0..5)), cx);
        let _ = window;
    });
    assert!(s.with(|e, _| e.cell_edit().is_some_and(|c| c.marked.is_some() && c.text == "とうきょう")), "composing");
    s.screenshot("composing");
    s.key("escape");
    assert!(s.value("A1").is_empty(), "Esc gives the entry up");
    s.ime("とうきょう", "東京");
    s.key("enter");
    assert_eq!(s.value("A1"), CellValue::Text("東京".into()));
    s.save_as("xsc45.xlsx");
    let saved = s.reopen();
    assert_eq!(sheet_of_slide(&saved.slides[1]).unwrap().cell(0, 0).unwrap().value, CellValue::Text("東京".into()));
}

/// XSC-49: cutting and pasting cells moves them: formulas referring to them
/// follow (a copy does not); undo puts everything back.
fn xsc49_cut_follows() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("G4");
    s.enter("=B4*2", "tab");
    s.enter("=SUM(B4:C5)", "enter");
    // Cut B4:C5 and paste at G10.
    s.click_cell("B4");
    s.key("shift-right");
    s.key("shift-down");
    s.key("cmd-x");
    s.click_cell("G10");
    s.key("cmd-v");
    assert_eq!(s.formula("G4").as_deref(), Some("G10*2"), "the reference follows the cell");
    assert_eq!(s.formula("H4").as_deref(), Some("SUM(G10:H11)"), "a range wholly inside follows");
    assert_eq!(s.formula("B9").as_deref(), Some("SUM(B4:B8)"), "a range partly inside stays");
    assert_eq!(s.value("G4"), CellValue::Number(2_500_000.0));
    assert_eq!(s.value("B9"), CellValue::Number(4_390_000.0));
    assert_eq!(s.value("G10"), CellValue::Number(1_250_000.0));
    s.save_as("xsc49.xlsx");
    // A copy does not change the references.
    s.key("cmd-z");
    assert_eq!(s.formula("G4").as_deref(), Some("B4*2"));
    s.click_cell("B4");
    s.key("cmd-c");
    s.click_cell("G10");
    s.key("cmd-v");
    assert_eq!(s.formula("G4").as_deref(), Some("B4*2"));
    assert_eq!(s.value("B4"), CellValue::Number(1_250_000.0));
}

/// XSC-55: a dynamic array spills; `A1#` refers to it; a value in its way
/// gives #SPILL!; cleared, it spills again; saved and read back.
fn xsc55_spill() {
    let mut s = new_sheet();
    s.enter("=SEQUENCE(5)", "enter");
    assert_eq!(s.value("A5"), CellValue::Number(5.0));
    s.click_cell("C1");
    s.enter("=SUM(A1#)", "enter");
    assert_eq!(s.value("C1"), CellValue::Number(15.0));
    s.click_cell("A3");
    s.enter("x", "enter");
    assert_eq!(s.shown("A1"), "#SPILL!");
    assert!(s.value("A5").is_empty());
    s.screenshot("blocked");
    s.click_cell("A3");
    s.key("delete");
    assert_eq!(s.value("A3"), CellValue::Number(3.0));
    assert_eq!(s.value("C1"), CellValue::Number(15.0));
    s.click_cell("A1");
    s.enter("=SEQUENCE(2,2)", "enter");
    assert_eq!(s.value("B2"), CellValue::Number(4.0));
    assert!(s.value("A5").is_empty(), "the rest is taken back");
    assert_eq!(s.value("C1"), CellValue::Number(10.0), "A1# is the new area");
    s.save_as("xsc55.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[1]).unwrap().clone();
    assert!(sheet.spills.contains_key(&(0, 0)));
    assert_eq!(sheet.cell(1, 1).unwrap().value, CellValue::Number(4.0));
    s.key("cmd-z");
    assert_eq!(s.value("A5"), CellValue::Number(5.0));
    s.key("cmd-z");
    assert_eq!(s.shown("A1"), "#SPILL!");
}

/// XSC-64: a sheet hidden and shown again (saved hidden); the status bar's
/// average, count and sum of a range (hidden rows left out).
fn xsc64_hide_sheet_and_sum() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B4");
    s.key("shift-down");
    s.key("shift-down");
    let summary = |s: &mut Scenario| s.status_texts().into_iter().find(|t| t.starts_with("平均"));
    assert_eq!(summary(&mut s).as_deref(), Some("平均: 1,253,333　データの個数: 3　合計: 3,760,000"));
    s.click_cell("A5");
    s.key("ctrl-9");
    s.click_cell("B4");
    s.key("shift-down");
    s.key("shift-down");
    // B4:B7 now (Shift+↓ passes the hidden row), its three cells shown.
    assert_eq!(summary(&mut s).as_deref(), Some("平均: 1,180,000　データの個数: 3　合計: 3,540,000"), "the hidden row is left out");
    s.key("cmd-z");
    // メモ (the third sheet) hidden: two tabs.
    s.key("ctrl-pagedown");
    s.key("ctrl-pagedown");
    s.menu("xlsx.hide-sheet");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 2);
    s.save_as("xsc64.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides.len(), 2);
    assert_eq!(saved.workbook.as_ref().unwrap().sheets[2].state, ibis_doc_model::SheetState::Hidden);
    s.menu("xlsx.unhide-sheet");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 3);
    assert_eq!(s.with(|e, _| e.slide().name.clone()).as_deref(), Some("メモ"));
}

/// XSC-65 (and XSC-20): 形式を選択して貼り付け: values, formats, formulas,
/// transposed, operations, links.
fn xsc65_paste_special() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // E4:E5 (formulas, #,##0) as values to G4.
    s.click_cell("E4");
    s.key("shift-down");
    s.key("cmd-c");
    s.click_cell("G4");
    s.menu("xlsx.paste-values");
    assert_eq!(s.formula("G4"), None);
    assert_eq!(s.value("G4"), CellValue::Number(3_750_000.0));
    assert_eq!(number_format(&mut s, "G4"), "General", "values only: no format");
    // Their format only onto G4:G5: the values stay, the format comes.
    s.menu("xlsx.paste-formats");
    assert_eq!(s.shown("G4"), "3,750,000");
    // Formulas only, to H4: references move, no format.
    s.click_cell("H4");
    s.menu("xlsx.paste-formulas");
    assert_eq!(s.formula("H4").as_deref(), Some("SUM(E4:G4)"));
    // A3:E3 (the headings) transposed to G10: a column.
    s.click_cell("A3");
    for _ in 0..4 {
        s.key("shift-right");
    }
    s.key("cmd-c");
    s.click_cell("G10");
    s.menu("xlsx.paste-transpose");
    assert_eq!(s.value("G11"), CellValue::Text("4月".into()));
    assert_eq!(s.value("G14"), CellValue::Text("合計".into()));
    // Add 1000 to B4:B5.
    s.click_cell("I1");
    s.enter("1000", "enter");
    s.click_cell("I1");
    s.key("cmd-c");
    s.click_cell("B4");
    s.key("shift-down");
    s.click_cell("B4");
    s.menu("xlsx.paste-add");
    assert_eq!(s.value("B4"), CellValue::Number(1_251_000.0));
    assert_eq!(s.value("E4"), CellValue::Number(3_751_000.0), "the totals follow");
    // A link to B4.
    s.click_cell("B4");
    s.key("cmd-c");
    s.click_cell("I3");
    s.menu("xlsx.paste-link");
    assert_eq!(s.formula("I3").as_deref(), Some("B4"));
    s.save_as("xsc65.xlsx");
    s.reopen();
}

/// XSC-28: panes frozen at B4 (the headings and the shop names stay), scrolled, saved, unfrozen.
fn xsc28_freeze() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B4");
    s.menu("xlsx.freeze");
    let frozen = |s: &mut Scenario| s.with(|e, _| sheet_of_slide(e.slide()).unwrap().frozen);
    assert_eq!(frozen(&mut s), Some((1, 3)));
    s.key("pagedown");
    s.screenshot("frozen-scrolled");
    // Row 3 (the headings) is still where it was.
    s.key("pageup");
    s.save_as("xsc28.xlsx");
    let saved = s.reopen();
    assert_eq!(sheet_of_slide(&saved.slides[0]).unwrap().frozen, Some((1, 3)));
    s.menu("xlsx.freeze");
    assert_eq!(frozen(&mut s), None);
    s.key("cmd-z");
    assert_eq!(frozen(&mut s), Some((1, 3)));
}

/// XSC-20: 書式のコピー/貼り付け: the headings' format onto another
/// range (its values stay); clicking the button again gives it up.
fn xsc20_format_painter() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B3");
    s.key("shift-right");
    s.toolbar("format-paint");
    assert_eq!(s.toolbar_state("format-paint").map(|t| t.on), Some(true));
    // Again: given up (the next click is a plain click).
    s.toolbar("format-paint");
    assert_eq!(s.toolbar_state("format-paint").map(|t| t.on), Some(false));
    s.click_cell("G4");
    s.enter("見出し", "tab");
    s.enter("2", "enter");
    s.click_cell("B3");
    s.key("shift-right");
    s.toolbar("format-paint");
    let (a, b) = (s.with(|e, _| e.cell_center(6, 3)).unwrap(), s.with(|e, _| e.cell_center(7, 3)).unwrap());
    s.drag((f32::from(a.x), f32::from(a.y)), (f32::from(b.x), f32::from(b.y)));
    assert_eq!(s.toolbar_state("format-paint").map(|t| t.on), Some(false), "used once");
    let style = |s: &mut Scenario, r: &str| {
        let (column, row) = parse(r);
        s.with(move |e, _| {
            let sheet = sheet_of_slide(e.slide()).unwrap();
            sheet.styles[sheet.cell(row, column).unwrap().style as usize].clone()
        })
    };
    let (src, dst) = (style(&mut s, "B3"), style(&mut s, "G4"));
    assert_eq!(dst.fill, src.fill);
    assert_eq!(dst.bold, src.bold);
    assert_eq!(s.value("G4"), CellValue::Text("見出し".into()), "the value stays");
    assert_eq!(s.value("H4"), CellValue::Number(2.0));
    s.key("cmd-z");
    assert_ne!(style(&mut s, "G4").fill, src.fill);
    s.save_as("xsc20.xlsx");
    s.reopen();
}

/// XSC-51: a name typed in the name box for a range, used in a formula,
/// changed in 名前の管理, deleted (the formula gives #NAME?); saved.
fn xsc51_names() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B4");
    for _ in 0..4 {
        s.key("shift-down");
    }
    // The name box (left of the formula bar).
    s.click((54.0, 123.0));
    s.key("cmd-a");
    s.type_text("四月");
    s.key("enter");
    let names = |s: &mut Scenario| s.with(|e, _| e.presentation().workbook.as_ref().unwrap().names.iter().map(|n| (n.name.clone(), n.formula.clone())).collect::<Vec<_>>());
    assert!(names(&mut s).contains(&("四月".to_string(), "月次売上!$B$4:$B$8".to_string())), "{:?}", names(&mut s));
    s.click_cell("G4");
    s.enter("=SUM(四月)", "enter");
    assert_eq!(s.value("G4"), CellValue::Number(6_620_000.0));
    // The name box goes to the name.
    s.click((54.0, 123.0));
    s.key("cmd-a");
    s.type_text("四月");
    s.key("enter");
    assert_eq!(s.with(|e, _| e.selected_range().map(|r| (r.first_row, r.last_row))), Some((3, 7)));
    // 名前の管理: B4:B5 instead.
    s.menu("xlsx.named-ranges");
    s.screenshot("names");
    s.click_cell("B4");
    s.key("shift-down");
    s.update(|e, _, cx| e.name_selection("四月", cx));
    assert_eq!(s.value("G4"), CellValue::Number(2_230_000.0));
    s.save_as("xsc51.xlsx");
    let saved = s.reopen();
    assert!(saved.workbook.as_ref().unwrap().names.iter().any(|n| n.name == "四月" && n.formula == "月次売上!$B$4:$B$5"));
    s.update(|e, _, cx| e.delete_name("四月", None, cx));
    assert_eq!(s.shown("G4"), "#NAME?");
    s.key("cmd-z");
    assert_eq!(s.value("G4"), CellValue::Number(2_230_000.0));
}

/// XSC-52: a list rule (with a message) on G4:G6: the drop-down and the
/// message by the selected cell, a listed value goes in, another is
/// refused; saved and read back.
fn xsc52_validation_list() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("G4");
    s.key("shift-down");
    s.key("shift-down");
    s.menu("xlsx.validation");
    s.update(|e, _, cx| e.set_list_validation("りんご,みかん,ぶどう", "果物を選んでください", cx));
    s.click_cell("G5");
    s.screenshot("dropdown-and-message");
    // The drop-down button, just right of the cell's bottom.
    let corner = s.with(|e, _| e.cell_corner(6, 4)).unwrap();
    s.click((f32::from(corner.x) + 8.0, f32::from(corner.y) - 6.0));
    assert!(s.with(|e, _| e.sheet_state().unwrap().list_popup.is_some()), "the list opens");
    s.screenshot("list");
    s.key("escape");
    s.type_text("みかん");
    s.key("enter");
    assert_eq!(s.value("G5"), CellValue::Text("みかん".into()));
    s.click_cell("G6");
    s.type_text("メロン");
    s.key("enter");
    assert!(s.value("G6").is_empty(), "refused");
    assert!(s.status_texts().iter().any(|t| t.contains("入力規則")), "{:?}", s.status_texts());
    assert!(s.with(|e, _| e.cell_edit().is_some()), "still being typed");
    s.key("escape");
    s.save_as("xsc52.xlsx");
    let saved = s.reopen();
    let v = &sheet_of_slide(&saved.slides[0]).unwrap().validations;
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].formula1.as_deref(), Some("\"りんご,みかん,ぶどう\""));
    assert_eq!(v[0].prompt.as_ref().map(|p| p.1.as_str()), Some("果物を選んでください"));
}

/// XSC-59: cells inserted with the cells below moving down (formulas
/// follow), deleted with those right moving left; a merged range moves
/// whole; undo.
fn xsc59_shift_cells() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("G4");
    s.enter("=B6", "enter");
    // B6:C6 inserted, shifting down: B6 (東町店 4月) goes to B7.
    s.click_cell("B6");
    s.key("shift-right");
    s.menu("xlsx.insert-cells");
    assert!(s.value("B6").is_empty());
    assert_eq!(s.value("B7"), CellValue::Number(1_530_000.0));
    assert_eq!(s.formula("G4").as_deref(), Some("B7"), "the reference follows");
    assert_eq!(s.value("D6"), CellValue::Number(1_620_000.0), "D does not move");
    assert_eq!(s.formula("B10").as_deref(), Some("SUM(B4:B9)"), "the column's total widens");
    s.save_as("xsc59.xlsx");
    s.key("cmd-z");
    assert_eq!(s.value("B6"), CellValue::Number(1_530_000.0));
    // C5 deleted, shifting left: D5 into C5.
    s.click_cell("C5");
    s.menu("xlsx.delete-cells-left");
    assert_eq!(s.value("C5"), CellValue::Number(1_105_000.0));
    // The title's merged range (A1:E1) moves whole with its row.
    s.click_cell("A1");
    s.menu("xlsx.insert-cells-right");
    assert_eq!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().merges.first().map(|m| (m.first_column, m.last_column))), Some((5, 9)));
    s.reopen();
}

/// XSC-61: a UTF-8 CSV (with a BOM) opened: values read as Excel reads
/// them (numbers, a date, text with a comma, a leading zero dropped);
/// edited; saved as CSV and as a workbook.
fn xsc61_csv() {
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("xsc61.csv");
    std::fs::write(&source, "\u{feff}品名,数量,入荷日,コード\r\nりんご,3,2026/9/30,0123\r\n\"み,かん\",12.5,2026/10/1,0456\r\n").unwrap();
    let mut s = Scenario::open("target/scenario-input/xsc61.csv");
    assert_eq!(s.value("A1"), CellValue::Text("品名".into()));
    assert_eq!(s.value("B2"), CellValue::Number(3.0));
    assert_eq!(s.shown("C2"), "2026/9/30");
    assert_eq!(s.value("A3"), CellValue::Text("み,かん".into()));
    assert_eq!(s.value("D2"), CellValue::Number(123.0), "a leading zero goes, as in Excel");
    s.click_cell("B4");
    s.enter("=SUM(B2:B3)", "enter");
    assert_eq!(s.value("B4"), CellValue::Number(15.5));
    // ⌘S on the .csv writes CSV.
    s.key("cmd-s");
    let path = s.with(|e, _| e.path.clone()).unwrap();
    let text = String::from_utf8(std::fs::read(&path).unwrap()).unwrap();
    assert!(text.starts_with("\u{feff}品名,数量,入荷日,コード\r\n"), "{text}");
    assert!(text.contains("\"み,かん\",12.5,2026/10/1,456\r\n"), "{text}");
    assert!(text.contains(",15.5,"), "{text}");
    // As a workbook too.
    s.save_as("xsc61.xlsx");
    let saved = s.reopen();
    assert_eq!(sheet_of_slide(&saved.slides[0]).unwrap().cell(3, 1).unwrap().formula.as_deref(), Some("SUM(B2:B3)"));
}

/// XSC-48: オートフィル: numbers, 曜日, 四半期, dates by days, weekdays and
/// months (オートフィル オプション), and a copy instead of a series.
fn xsc48_autofill() {
    let mut s = new_sheet();
    s.enter("1", "enter");
    s.enter("2", "enter");
    s.click_cell("A1");
    s.key("shift-down");
    s.drag_fill("A2", "A6");
    assert_eq!(s.value("A6"), CellValue::Number(6.0));
    // オートフィル オプション: セルのコピー.
    assert!(s.with(|e, _| e.fill_options_range().is_some()), "the options show");
    s.update(|e, _, cx| e.refill(super::fill::FillMode::Copy, cx));
    assert_eq!(s.value("A3"), CellValue::Number(1.0));
    assert_eq!(s.value("A6"), CellValue::Number(2.0));
    s.click_cell("B1");
    s.enter("月", "enter");
    s.click_cell("B1");
    s.drag_fill("B1", "B4");
    assert_eq!(s.value("B4"), CellValue::Text("木".into()));
    s.click_cell("C1");
    s.enter("第1四半期", "enter");
    s.click_cell("C1");
    s.drag_fill("C1", "C5");
    assert_eq!(s.value("C5"), CellValue::Text("第1四半期".into()), "四半期 wraps round");
    s.click_cell("D1");
    s.enter("2026/9/30", "enter");
    s.click_cell("D1");
    s.drag_fill("D1", "D4");
    assert_eq!(s.shown("D4"), "2026/10/3", "by days");
    let corner = s.with(|e, _| e.cell_corner(3, 3)).unwrap();
    s.click((f32::from(corner.x) + 14.0, f32::from(corner.y) + 12.0));
    assert!(s.with(|e, _| e.sheet_state().unwrap().list_popup.is_some()), "オートフィル オプション opens");
    s.screenshot("fill-options");
    s.key("escape");
    s.update(|e, _, cx| e.refill(super::fill::FillMode::Weekdays, cx));
    assert_eq!(s.shown("D4"), "2026/10/5", "weekdays pass the weekend");
    s.update(|e, _, cx| e.refill(super::fill::FillMode::Months, cx));
    assert_eq!(s.shown("D2"), "2026/10/30");
    assert_eq!(s.shown("D4"), "2026/12/30");
    s.key("cmd-z");
    assert!(s.value("D4").is_empty(), "one step");
    s.save_as("xsc48.xlsx");
    s.reopen();
}

/// XSC-29: 検索と置換: the next cell found, all replaced (formulas left
/// alone unless asked), undo.
fn xsc29_find_replace() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("edit.find-replace");
    s.type_text("町店");
    s.screenshot("find");
    s.update(|e, _, cx| {
        e.find_next_cell("町店", false, cx);
    });
    assert_eq!(s.selected(), "A4");
    s.update(|e, _, cx| {
        e.find_next_cell("町店", false, cx);
    });
    assert_eq!(s.selected(), "A5");
    let n = s.update(|e, _, cx| e.replace_all_cells("店舗", "支店", false, cx));
    assert_eq!(n, 1);
    assert_eq!(s.value("A3"), CellValue::Text("支店".into()));
    // SUM in formulas: left alone, then replaced when formulas are searched too.
    assert_eq!(s.update(|e, _, cx| e.replace_all_cells("SUM", "MAX", false, cx)), 0);
    let n = s.update(|e, _, cx| e.replace_all_cells("SUM", "MAX", true, cx));
    assert!(n >= 9, "{n}");
    assert_eq!(s.formula("E4").as_deref(), Some("MAX(B4:D4)"));
    assert_eq!(s.value("E4"), CellValue::Number(1_320_000.0));
    // Back to the sheet (the keys went to the search field), then undo.
    s.click_cell("G1");
    s.key("cmd-z");
    assert_eq!(s.formula("E4").as_deref(), Some("SUM(B4:D4)"));
    s.save_as("xsc29.xlsx");
    s.reopen();
}

/// XSC-31: a web link and a link to another sheet (⌘K), the text changed
/// (the link stays), ⌘-click follows the sheet link, a link taken off; saved.
fn xsc31_hyperlinks() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("G4");
    s.key("cmd-k");
    assert!(s.with(|e, _| e.sheet_state().unwrap().link.is_some()), "the panel opens");
    s.update(|e, _, cx| e.insert_cell_link("https://example.com/sales", cx));
    assert_eq!(s.value("G4"), CellValue::Text("https://example.com/sales".into()));
    s.click_cell("H4");
    s.update(|e, _, cx| e.insert_cell_link("#商品!A1", cx));
    s.screenshot("links");
    let style = |s: &mut Scenario, r: &str| {
        let (column, row) = parse(r);
        s.with(move |e, _| {
            let sheet = sheet_of_slide(e.slide()).unwrap();
            sheet.styles[sheet.cell(row, column).unwrap().style as usize].clone()
        })
    };
    let st = style(&mut s, "G4");
    assert!(st.underline.enabled() && st.color == ibis_doc_model::Rgb(0x0563C1), "Excel's hyperlink style");
    // The text typed over: the link stays.
    s.click_cell("G4");
    s.enter("売上の資料", "enter");
    assert_eq!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().hyperlinks.len()), 2);
    // ⌘-click the sheet link: to 商品.
    let p = s.with(|e, _| e.cell_center(7, 3)).unwrap();
    s.dispatch_click_with((f32::from(p.x), f32::from(p.y)), gpui_kit::Modifiers { platform: true, ..Default::default() });
    assert_eq!(s.with(|e, _| e.slide().name.clone()).as_deref(), Some("商品"));
    s.key("ctrl-pageup");
    s.save_as("xsc31.xlsx");
    let saved = s.reopen();
    let links = sheet_of_slide(&saved.slides[0]).unwrap().hyperlinks.clone();
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].target.as_deref(), Some("https://example.com/sales"));
    assert_eq!(links[1].location.as_deref(), Some("商品!A1"));
    s.click_cell("G4");
    s.update(|e, _, cx| e.remove_cell_link(cx));
    assert_eq!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().hyperlinks.len()), 1);
}

/// XLSX-OPTIONS-005, オートコレクト: a URL and an email address typed into
/// cells become links (the link's colour and underline), a plain text does
/// not; one undo takes the entry and the link back together; saved.
fn opt005_autolink() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("G4");
    s.enter("www.example.com", "enter");
    s.click_cell("G5");
    s.enter("user@example.com", "enter");
    s.click_cell("G6");
    s.enter("売上の資料", "enter");
    let links = s.with(|e, _| sheet_of_slide(e.slide()).unwrap().hyperlinks.clone());
    assert_eq!(links.len(), 2, "{links:?}");
    assert_eq!(links[0].target.as_deref(), Some("http://www.example.com"));
    assert_eq!(links[1].target.as_deref(), Some("mailto:user@example.com"));
    assert_eq!(s.value("G4"), CellValue::Text("www.example.com".into()), "the text stays as typed");
    let (column, row) = parse("G4");
    let st = s.with(move |e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.styles[sheet.cell(row, column).unwrap().style as usize].clone()
    });
    assert!(st.underline.enabled() && st.color == ibis_doc_model::Rgb(0x0563C1), "Excel's hyperlink style");
    s.key("cmd-z");
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().hyperlinks.len()), 1, "the mail entry undone with its link");
    s.save_as("opt005.xlsx");
    let saved = s.reopen();
    let links = sheet_of_slide(&saved.slides[0]).unwrap().hyperlinks.clone();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("http://www.example.com"));
}

/// XLSX-CELL-005, the mouse on the text being typed: a press puts the caret
/// there (and F2's Edit mode), a drag selects, a double-click selects the
/// word, Shift extends.
fn cell005_mouse_select() {
    let mut s = new_sheet();
    s.click_cell("A1");
    s.enter("abcdef ghi 日本語", "enter");
    s.click_cell("A1");
    s.key("f2");
    s.screenshot("editing");
    let r = s.with(|e, _| e.cell_edit().and_then(|c| c.rect.get())).expect("the text is painted");
    let (left, right, y) = (f32::from(r.origin.x) + 4.0, f32::from(r.origin.x + r.size.width), f32::from(r.origin.y + r.size.height / 2.0));
    let state = |s: &mut Scenario| s.with(|e, _| e.cell_edit().map(|c| (c.caret, c.anchor, c.selection())).unwrap());
    // A press near the start: the caret at the start.
    s.click((left + 0.5, y));
    assert_eq!(state(&mut s).0, 0);
    // A drag from there to past the end selects it all.
    s.drag((left + 0.5, y), (right + 40.0, y));
    let (caret, anchor, selection) = state(&mut s);
    assert_eq!((caret, anchor, selection), (14, Some(0), Some(0..14)));
    s.screenshot("selected");
    // A double-click on the first word selects it.
    s.double_click((left + 2.0, y));
    assert_eq!(state(&mut s).2, Some(0..6), "the word abcdef");
    // Typing replaces the selection.
    s.key("x");
    let text = s.with(|e, _| e.cell_edit().map(|c| c.text.clone())).unwrap();
    assert_eq!(text, "x ghi 日本語");
    s.key("escape");
    // The formula bar the same: a press begins the edit, the next puts the caret, a drag selects.
    s.click_cell("A1");
    let bar = s.with(|e, _| e.sheet_state().and_then(|st| st.bar_bounds.get())).expect("the bar's text is painted");
    let (bar_left, bar_y) = (f32::from(bar.origin.x) + 8.0, f32::from(bar.origin.y + bar.size.height / 2.0));
    // The first press begins the edit with the caret where it was pressed (not at the end).
    s.click((bar_left + 0.5, bar_y));
    assert_eq!(state(&mut s).0, 0, "the caret where the bar was pressed");
    s.click((bar_left + 400.0, bar_y));
    assert_eq!(state(&mut s).0, 14, "past the text: the end");
    s.click((bar_left + 0.5, bar_y));
    assert_eq!(state(&mut s).0, 0);
    s.drag((bar_left + 0.5, bar_y), (bar_left + 400.0, bar_y));
    assert_eq!(state(&mut s).2, Some(0..14), "the whole text selected by dragging in the bar");
    s.key("escape");
}

/// XLSX-VIEW-002, 表示 > ページ レイアウト (the simple form: the pages' areas,
/// numbers and the paper's edge over the cells): on and off by the menu, off
/// when the page break preview comes on; saved as view="pageLayout"; undone.
fn view002_page_layout() {
    let flags = |s: &mut Scenario| s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        (sheet.page_break_preview, sheet.page_layout)
    });
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    assert_eq!(flags(&mut s), (false, false));
    assert_eq!(s.menu_state("xlsx.page-layout"), Some((true, false)));
    s.menu("xlsx.page-break-preview");
    s.screenshot("break-preview");
    // A manual page break above row 10: the page lines and the numbers show it.
    s.click_cell("A10");
    s.update(|e, _, cx| e.toggle_row_break(cx));
    s.screenshot("break-preview-manual");
    // Printed at 300 %, the sheet is several pages across and down.
    s.update(|e, _, cx| e.set_print(|p| p.scale = 300, None, cx));
    s.screenshot("break-preview-pages");
    s.menu("xlsx.page-layout");
    s.screenshot("page-layout-pages");
    s.update(|e, _, cx| e.set_print(|p| p.scale = 100, None, cx));
    s.menu("xlsx.page-break-preview");
    s.menu("xlsx.page-layout");
    assert_eq!(flags(&mut s), (false, true), "the page layout view replaces the page break preview");
    assert_eq!(s.menu_state("xlsx.page-layout"), Some((true, true)));
    s.screenshot("page-layout");
    s.save_as("view002.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap();
    assert!(sheet.page_layout && !sheet.page_break_preview, "view=pageLayout is kept");
    s.key("cmd-z");
    assert_eq!(flags(&mut s), (true, false), "undone to the page break preview");
    s.key("cmd-shift-z");
    s.menu("xlsx.page-layout");
    assert_eq!(flags(&mut s), (false, false));
}

/// XLSX-CELL-005, a font change with the caret alone (⌘B, a colour, the size):
/// it goes to the text typed next; the same toggle again takes it back; moving
/// the caret drops it; kept as parts of the cell's text.
fn cell005_caret_format() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("A6");
    s.key("f2");
    s.key("cmd-b");
    s.key("X");
    s.key("cmd-b");
    s.key("Y");
    s.key("cmd-i");
    s.key("left");
    s.key("Z");
    s.screenshot("typing");
    let text = s.with(|e, _| e.cell_edit().map(|c| c.text.clone())).unwrap();
    assert_eq!(text, "東町店XZY", "the left arrow moved before Y: {text}");
    let runs = s.with(|e, _| e.cell_edit().and_then(|c| c.runs.clone())).expect("runs while editing");
    let shape: Vec<(String, Option<bool>, Option<bool>)> = runs.iter().map(|r| (r.text.clone(), r.bold, r.italic)).collect();
    assert_eq!(shape[0].0, "東町店");
    assert!(shape.iter().any(|(t, b, _)| t.contains('X') && *b == Some(true)), "X is bold: {shape:?}");
    assert!(shape.iter().any(|(t, b, _)| t.contains('Y') && *b == Some(false)), "Y is not: {shape:?}");
    assert!(shape.iter().all(|(t, _, i)| !t.contains('Z') || *i != Some(true)), "Z is not italic: {shape:?}");
    s.key("enter");
    let kept = s.with(|e, _| e.current_sheet().and_then(|(_, sheet)| sheet.cell(5, 0).and_then(|c| c.runs.clone()))).expect("runs kept");
    assert!(kept.len() >= 2);
}

/// ⌃; (the date), ⌃⇧; (the time), ⌃Space and ⇧Space (the whole column and
/// row), the keys an office worker uses all day; NOW() has the time of day.
fn cell018_date_time_keys() {
    let mut s = new_sheet();
    ibis_calc::eval::convert::set_today(ibis_calc::eval::convert::serial_from_date(2026, 10, 7, false) - 25569);
    s.click_cell("B20");
    s.key("ctrl-;");
    assert_eq!(s.value("B20"), CellValue::Number(ibis_calc::eval::convert::serial_from_date(2026, 10, 7, false) as f64), "today, a date value");
    let format = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.styles[sheet.cell(19, 1).unwrap().style as usize].number_format.clone()
    });
    assert_eq!(format, "yyyy/m/d");
    ibis_calc::eval::convert::set_today(i64::MIN);
    s.click_cell("B21");
    s.key("ctrl-shift-;");
    let CellValue::Number(t) = s.value("B21") else { panic!("a time: {:?}", s.value("B21")) };
    assert!((0.0..1.0).contains(&t), "the time of day: {t}");
    // Typing: the date goes in at the caret.
    s.click_cell("B22");
    s.key("f2");
    s.key("a");
    s.key("ctrl-;");
    let typed = s.with(|e, _| e.cell_edit().map(|c| c.text.clone())).unwrap();
    assert!(typed.starts_with('a') && typed.len() > 8, "{typed}");
    s.key("escape");
    s.click_cell("C5");
    s.key("ctrl-space");
    assert_eq!(s.with(|e, _| e.selected_lines(ibis_doc_model::sheet_command::Axis::Columns)), Some((2, 2)));
    s.click_cell("C5");
    s.key("shift-space");
    assert_eq!(s.with(|e, _| e.selected_lines(ibis_doc_model::sheet_command::Axis::Rows)), Some((4, 4)));
    // NOW() is today and the time of day.
    s.click_cell("D1");
    s.enter("=NOW()", "enter");
    let CellValue::Number(now) = s.value("D1") else { panic!("{:?}", s.value("D1")) };
    assert!(now.fract() > 0.0 && now > 46000.0, "NOW has the time: {now}");
}

/// ⌘D and ⌘R (フィル > 下方向へコピー・右方向へコピー): the first row (column)
/// of the selection over the rest, with formulas moved; with one row selected
/// the one above is copied into it; the selection stays; one undo each.
fn cell019_fill_down_right() {
    let mut s = new_sheet();
    s.click_cell("A1");
    s.enter("10", "enter");
    s.click_cell("B1");
    s.enter("=A1*2", "enter");
    s.click_cell("A1");
    s.key("shift-right");
    for _ in 0..3 {
        s.key("shift-down");
    }
    s.key("cmd-d");
    assert_eq!(s.value("A4"), CellValue::Number(10.0));
    assert_eq!(s.formula("B4").as_deref(), Some("A4*2"), "the formula moved with its row");
    assert_eq!(s.value("B4"), CellValue::Number(20.0));
    assert_eq!(s.with(|e, _| e.selected_range()).map(|r| (r.first_row, r.last_row, r.first_column, r.last_column)), Some((0, 3, 0, 1)), "the selection stays");
    // One row selected: the row above goes into it.
    s.click_cell("A6");
    s.enter("=SUM(A1:A4)", "enter");
    s.click_cell("A7");
    s.key("cmd-d");
    assert_eq!(s.formula("A7").as_deref(), Some("SUM(A2:A5)"), "copied from above, moved down");
    // Right: the first column over the rest.
    s.click_cell("A9");
    s.enter("5", "enter");
    s.click_cell("A9");
    for _ in 0..3 {
        s.key("shift-right");
    }
    s.key("cmd-r");
    assert_eq!(s.value("D9"), CellValue::Number(5.0));
    s.key("cmd-z");
    assert_eq!(s.value("D9"), CellValue::Empty, "one undo takes the whole fill back");
}

/// ⌘1 (the cell format settings) and ⌃⇧L (the filter on and off).
fn cell020_format_and_filter_keys() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("A3");
    let before = s.with(|e, _| sheet_of_slide(e.slide()).unwrap().auto_filter.is_some());
    s.key("ctrl-shift-l");
    let after = s.with(|e, _| sheet_of_slide(e.slide()).unwrap().auto_filter.is_some());
    assert_ne!(before, after, "the filter toggled");
    s.key("ctrl-shift-l");
    assert_eq!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().auto_filter.is_some()), before);
    s.key("cmd-1");
    assert!(s.with(|e, _| e.sheet_state().is_some_and(|st| st.custom_format.is_some())), "the format settings open");
}

/// ⌘1, セルの書式設定: the pane with its six sections (表示形式・配置・フォント・罫線・塗りつぶし・保護), each a
/// row with a triangle that opens and closes it (kept when the pane is opened again); the choices are
/// drop-downs, the font size has − and +, the colours open the palette, and the border lines are drawn on
/// a sample that is pressed to draw or take off a line.
fn format021_cell_format_pane() {
    use super::custom_format::FormatTab;
    use crate::shell::chrome::MenuEntry;
    use crate::shell::i18n::{self, Language};
    let t = |id: &str| i18n::text(id).to_string();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B4");
    s.key("cmd-1");
    let open = |s: &mut Scenario| s.with(|e, _| e.sheet_state().unwrap().format_open);
    assert_eq!(open(&mut s), [true; 6], "all the sections are open at first");
    s.screenshot("sections");
    fn bounds(s: &mut Scenario, id: &str) -> Bounds<gpui_kit::Pixels> {
        let id = id.to_string();
        s.with(|e, _| *e.sheet_state().and_then(|st| st.custom_format.as_ref()).expect("the pane").ui.borrow().get(&id).unwrap_or_else(|| panic!("not found in the pane: {id}")))
    }
    fn press(s: &mut Scenario, id: &str) {
        let b = bounds(s, id);
        s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
    }
    /// The item of drop-down `id` named `label`, found among the items and the palette's squares.
    fn choose(s: &mut Scenario, id: &str, label: &str) {
        let (id, label) = (id.to_string(), label.to_string());
        let handler = s.with(|e, _| {
            let menus = e.sheet_state().and_then(|st| st.custom_format.as_ref()).expect("the pane").menus.borrow();
            let entries = menus.get(&id).unwrap_or_else(|| panic!("no drop-down {id}"));
            entries.iter().find_map(|entry| match entry {
                MenuEntry::Item(i) if i.label.as_ref() == label => i.handler.clone(),
                MenuEntry::Palette(p) => p.find(&label).and_then(|i| i.handler.clone()),
                _ => None,
            })
        });
        let handler = handler.unwrap_or_else(|| panic!("{id} has no item {label}"));
        s.update(|e, window, cx| handler(e, window, cx));
    }
    let style = |s: &mut Scenario| s.with(|e, _| e.current_style());
    // Only the section `n` open (the others closed), so that its parts are in view to be pressed.
    fn only(s: &mut Scenario, n: usize) {
        s.update(move |e, _, cx| {
            let mut open = [false; 6];
            open[n] = true;
            e.wb_mut().format_open = open;
            cx.notify();
        });
    }
    // A triangle row closes its section and opens it again; the state is kept when the pane is opened again.
    press(&mut s, "cell-format-section-Number");
    assert!(!open(&mut s)[0] && open(&mut s)[1], "the number section is closed");
    s.screenshot("number-closed");
    s.key("cmd-1");
    assert!(!open(&mut s)[0], "kept when the pane is opened again");
    press(&mut s, "cell-format-section-Number");
    assert!(open(&mut s)[0]);
    // 表示形式: the category is a drop-down.
    press(&mut s, "cell-format-number");
    s.screenshot("number-dropdown");
    s.key("escape");
    choose(&mut s, "cell-format-number", &t("_xlsx.percentage"));
    assert_eq!(style(&mut s).number_format, "0%");
    s.key("cmd-z");
    // 配置: 横位置, 縦位置 and 文字の方向 are drop-downs.
    choose(&mut s, "cell-format-halign", &t("_xlsx.right-indent"));
    assert_eq!(style(&mut s).align, ibis_doc_model::CellAlign::Right);
    choose(&mut s, "cell-format-valign", &t("_shell.center-2"));
    assert_eq!(style(&mut s).vertical, ibis_doc_model::CellVerticalAlign::Center);
    choose(&mut s, "cell-format-rotation", "45°");
    assert_eq!(style(&mut s).rotation, 45);
    s.screenshot("alignment");
    only(&mut s, 2);
    // フォント: the size with − and +, the toolbar's buttons, the colour's palette.
    let size = style(&mut s).font_size;
    press(&mut s, "cell-format-size-more");
    assert!(style(&mut s).font_size > size, "+ makes it bigger");
    press(&mut s, "cell-format-size-less");
    assert_eq!(style(&mut s).font_size, size, "− takes it back");
    let typed = s.with(|e, cx| e.sheet_state().unwrap().custom_format.as_ref().unwrap().size.read(cx).value().to_string());
    assert_eq!(typed, format!("{}", size as f32 / 100.0), "the field follows the buttons");
    let bold = style(&mut s).bold;
    press(&mut s, "cell-format-bold");
    assert_ne!(style(&mut s).bold, bold, "bold toggled");
    press(&mut s, "cell-format-strike");
    assert!(style(&mut s).strike);
    press(&mut s, "cell-format-color");
    s.screenshot("color-dropdown");
    s.key("escape");
    choose(&mut s, "cell-format-color", &t("_shell.red"));
    assert_eq!(style(&mut s).color.0, 0xff0000);
    s.screenshot("font");
    only(&mut s, 3);
    s.click_cell("H20");
    // 罫線: pressing a line of the sample draws it in the chosen style and colour, and again takes it off.
    choose(&mut s, "cell-format-line", &t("_shell.thick"));
    choose(&mut s, "cell-format-border-color", &t("_shell.red"));
    assert!(style(&mut s).top.is_none());
    press(&mut s, "cell-format-sample-top");
    let top = style(&mut s).top.expect("the top line is drawn");
    assert_eq!((top.line, top.color.0), (ibis_doc_model::BorderLine::Thick, 0xff0000));
    s.screenshot("border-top");
    press(&mut s, "cell-format-sample-top");
    assert!(style(&mut s).top.is_none(), "taken off");
    press(&mut s, "cell-format-border-outside");
    assert!(style(&mut s).left.is_some() && style(&mut s).bottom.is_some(), "the outside preset");
    press(&mut s, "cell-format-border-diagonal-down");
    assert!(style(&mut s).diagonal_down.is_some());
    s.screenshot("border-outside");
    press(&mut s, "cell-format-border-none");
    assert!(style(&mut s).left.is_none() && style(&mut s).diagonal_down.is_none(), "none takes them all off");
    // Several cells: the inside lines.
    s.click_cell("H20");
    s.key("shift-right");
    s.key("shift-down");
    choose(&mut s, "cell-format-line", &t("_shell.dashed"));
    press(&mut s, "cell-format-border-inside");
    assert!(style(&mut s).right.is_some() && style(&mut s).bottom.is_some(), "the lines between the cells");
    s.screenshot("border-inside");
    press(&mut s, "cell-format-border-none");
    only(&mut s, 4);
    // 塗りつぶし.
    choose(&mut s, "cell-format-fill", &t("_shell.yellow"));
    assert_eq!(style(&mut s).fill.map(|f| f.0), Some(0xffff00));
    s.screenshot("fill");
    choose(&mut s, "cell-format-fill", &t("xlsx.fill-none"));
    assert!(style(&mut s).fill.is_none());
    only(&mut s, 5);
    // 保護.
    let was = style(&mut s).unlocked;
    press(&mut s, "cell-format-locked");
    assert_ne!(style(&mut s).unlocked, was, "the lock toggled");
    s.screenshot("protection");
    s.update(move |e, window, cx| e.open_cell_format(FormatTab::Number, window, cx));
    assert!(open(&mut s)[0], "opening a section by its name shows it");
    // English: the whole pane, with no Japanese left (a picture to look at).
    i18n::show(Language::English);
    s.update(|_, _, _| {});
    s.screenshot("english");
    i18n::show(Language::Japanese);
}

/// The system clipboard: the text typed into a cell is copied, cut and pasted
/// through it; a range of cells goes to it as tab-separated text; a table
/// (tab-separated) from another app is pasted into cells, a date keeping its
/// format.
fn clip022_system_clipboard() {
    let mut s = new_sheet();
    let clipboard = |s: &mut Scenario| s.update(|_, _, cx| cx.read_from_clipboard().and_then(|c| c.text())).unwrap_or_default();
    // Typing: select two letters, copy, paste at the end; cut, paste.
    s.click_cell("A1");
    s.enter("abcdef", "enter");
    s.click_cell("A1");
    s.key("f2");
    s.key("shift-left");
    s.key("shift-left");
    s.key("cmd-c");
    assert_eq!(clipboard(&mut s), "ef", "the selected text is on the system clipboard");
    s.key("right");
    s.key("cmd-v");
    assert_eq!(s.with(|e, _| e.cell_edit().map(|c| c.text.clone())).unwrap(), "abcdefef", "pasted at the caret");
    s.key("shift-left");
    s.key("shift-left");
    s.key("cmd-x");
    assert_eq!(s.with(|e, _| e.cell_edit().map(|c| c.text.clone())).unwrap(), "abcdef");
    assert_eq!(clipboard(&mut s), "ef");
    s.key("enter");
    assert_eq!(s.value("A1"), CellValue::Text("abcdef".into()), "the cell kept what was typed, not a cell copy");
    // A range goes out as tab-separated text.
    s.click_cell("B1");
    s.enter("1", "tab");
    s.enter("2", "enter");
    s.click_cell("A1");
    s.key("shift-right");
    s.key("shift-right");
    s.key("cmd-c");
    assert_eq!(clipboard(&mut s), "abcdef\t1\t2\n");
    // A table from another app comes in as cells, a date with its format.
    s.update(|_, _, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("x\t2026/10/11\ny\t50%\n".to_string())));
    s.click_cell("D5");
    s.key("cmd-v");
    assert_eq!(s.value("D5"), CellValue::Text("x".into()));
    assert!(matches!(s.value("E5"), CellValue::Number(n) if n > 46000.0));
    assert_eq!(s.value("E6"), CellValue::Number(0.5));
    let format = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.styles[sheet.cell(4, 4).unwrap().style as usize].number_format.clone()
    });
    assert_eq!(format, "yyyy/m/d", "the pasted date is shown as a date");
}

/// XSC-58: rows grouped, collapsed (hidden), expanded, ungrouped; saved
/// with the outline level.
fn xsc58_outline() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("A4");
    for _ in 0..4 {
        s.key("shift-down");
    }
    s.menu("xlsx.group");
    let levels = |s: &mut Scenario| s.with(|e, _| sheet_of_slide(e.slide()).unwrap().outline_rows.clone());
    assert_eq!(levels(&mut s).len(), 5);
    s.click_cell("A5");
    s.menu("xlsx.collapse");
    assert_eq!(s.size_pt("6"), 0.0, "the group is hidden");
    assert_eq!(s.value("B9"), CellValue::Number(6_620_000.0), "the total stays");
    s.save_as("xsc58.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.outline_rows.get(&3), Some(&1));
    assert_eq!(sheet.rows[5], 0);
    // Expanded: the first row of the group is selected again.
    s.update(|e, _, cx| e.collapse_group(false, cx));
    assert!(s.size_pt("6") > 0.0);
    s.click_cell("A4");
    for _ in 0..4 {
        s.key("shift-down");
    }
    s.menu("xlsx.ungroup");
    assert!(levels(&mut s).is_empty());
}

/// XSC-57: a sheet copied (its cells, formats, widths and merged range; its
/// own formulas stay on the copy), moved right and left; saved.
fn xsc57_copy_move_sheet() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("xlsx.duplicate-sheet");
    let names = |s: &mut Scenario| s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().map(|x| x.name.clone()).collect::<Vec<_>>());
    assert_eq!(names(&mut s), ["月次売上", "月次売上 (2)", "商品", "メモ"]);
    assert_eq!(s.with(|e, _| e.slide().name.clone()).as_deref(), Some("月次売上 (2)"));
    assert_eq!(s.shown("B4"), "1,250,000");
    assert_eq!(s.formula("E4").as_deref(), Some("SUM(B4:D4)"));
    // Changing the copy changes only the copy.
    s.click_cell("B4");
    s.enter("1", "enter");
    assert_eq!(s.value("E4"), CellValue::Number(2_500_001.0));
    s.menu("xlsx.move-sheet-right");
    assert_eq!(names(&mut s), ["月次売上", "商品", "月次売上 (2)", "メモ"]);
    assert_eq!(s.with(|e, _| e.presentation().slides.iter().map(|x| x.name.clone().unwrap_or_default()).collect::<Vec<_>>()), ["月次売上", "商品", "月次売上 (2)", "メモ"], "the tabs follow");
    s.save_as("xsc57.xlsx");
    let saved = s.reopen();
    let wb = saved.workbook.as_ref().unwrap();
    assert_eq!(wb.sheets.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["月次売上", "商品", "月次売上 (2)", "メモ"]);
    let copy = sheet_of_slide(&saved.slides[2]).unwrap().clone();
    assert_eq!(copy.cell(3, 4).unwrap().value, CellValue::Number(2_500_001.0));
    assert_eq!(copy.cell(3, 1).unwrap().text, "1");
    assert_eq!(copy.merges.len(), 1);
    assert_eq!(copy.columns[0], sheet_of_slide(&saved.slides[0]).unwrap().columns[0], "the widths come along");
    s.menu("xlsx.move-sheet-left");
    assert_eq!(names(&mut s)[1], "月次売上 (2)");
}

/// XSC-54: F4 cycles the reference being typed ($B$4 → B$4 → $B4 → B4);
/// filled down, only the parts without $ move.
fn xsc54_f4() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("G4");
    s.type_text("=B4");
    let text = |s: &mut Scenario| s.with(|e, _| e.cell_edit().map(|c| c.text.clone())).unwrap_or_default();
    s.key("f4");
    assert_eq!(text(&mut s), "=$B$4");
    s.key("f4");
    assert_eq!(text(&mut s), "=B$4");
    s.key("f4");
    assert_eq!(text(&mut s), "=$B4");
    s.key("f4");
    assert_eq!(text(&mut s), "=B4");
    s.key("f4");
    s.key("f4");
    s.type_text("*C4");
    s.key("enter");
    assert_eq!(s.formula("G4").as_deref(), Some("B$4*C4"));
    s.click_cell("G4");
    s.drag_fill("G4", "G6");
    assert_eq!(s.formula("G6").as_deref(), Some("B$4*C6"), "B$4 keeps its row");
    s.save_as("xsc54.xlsx");
    s.reopen();
}

/// XSC-56: twenty edits of different kinds, all undone (the book is as it
/// was opened), half redone (as after the tenth).
fn xsc56_undo_twenty() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // The book's content (not the counter of ids handed out, which undo leaves).
    let book = |s: &mut Scenario| s.with(|e, _| (e.presentation().slides.clone(), e.presentation().workbook.clone()));
    let mut states = vec![book(&mut s)];
    let steps: Vec<Box<dyn Fn(&mut Scenario)>> = vec![
        Box::new(|s| { s.click_cell("B4"); s.enter("1300000", "enter"); }),
        Box::new(|s| { s.click_cell("G4"); s.enter("=E4/1000", "enter"); }),
        Box::new(|s| { s.click_cell("A3"); s.key("cmd-b"); }),
        Box::new(|s| { s.click_cell("A4"); s.toolbar_menu("fill", "xlsx.fill"); }),
        Box::new(|s| { s.click_cell("A6"); s.menu("xlsx.insert-row-above"); }),
        Box::new(|s| { s.click_cell("G8"); s.menu("xlsx.delete-column"); }),
        Box::new(|s| { s.click_cell("G2"); s.key("shift-right"); s.toolbar_menu("merge", "xlsx.merge"); }),
        Box::new(|s| { s.click_cell("B5"); s.key("cmd-c"); s.click_cell("H5"); s.key("cmd-v"); }),
        Box::new(|s| { s.click_cell("A10"); s.key("ctrl-9"); }),
        Box::new(|s| { s.click_cell("C4"); s.key("cmd-\\"); }),
        Box::new(|s| { s.click_cell("B4"); s.toolbar("decimal-more"); }),
        Box::new(|s| { s.click_cell("H5"); s.key("delete"); }),
        Box::new(|s| { s.click_cell("B5"); s.menu("xlsx.freeze"); }),
        Box::new(|s| { s.click_cell("E9"); s.drag_fill("E9", "E10"); }),
        Box::new(|s| { s.click_cell("C6"); s.menu("xlsx.insert-cells"); }),
        Box::new(|s| { s.click_cell("B4"); s.toolbar("italic"); }),
        Box::new(|s| { s.click_cell("D10"); s.enter("メモ", "enter"); }),
        Box::new(|s| { s.click_cell("A4"); s.key("shift-down"); s.menu("xlsx.group"); }),
        Box::new(|s| { s.menu("xlsx.insert-sheet"); }),
        Box::new(|s| { s.menu("xlsx.rename-sheet"); s.key("cmd-a"); s.enter("追加", "enter"); }),
    ];
    for step in &steps {
        step(&mut s);
        states.push(book(&mut s));
    }
    for (i, w) in states.windows(2).enumerate() {
        assert!(w[0] != w[1], "step {} changed the book", i + 1);
    }
    s.click_cell("A1");
    for k in (0..20).rev() {
        s.key("cmd-z");
        let now = book(&mut s);
        if now != states[k] {
            let a = &states[k];
            let diff = if a.0.len() != now.0.len() { "slides".to_string() } else if a.1 != now.1 { "workbook".to_string() } else {
                let i = a.0.iter().zip(&now.0).position(|(x, y)| x != y).unwrap_or(0);
                let (x, y) = (sheet_of_slide(&a.0[i]), sheet_of_slide(&now.0[i]));
                match (x, y) {
                    (Some(x), Some(y)) => format!("slide {i}: cells {} styles {} merges {} rows {} cols {} frozen {} spills {}", x.cells == y.cells, x.styles == y.styles, x.merges == y.merges, x.rows == y.rows, x.columns == y.columns, x.frozen == y.frozen, x.spills == y.spills),
                    _ => format!("slide {i} other"),
                }
            };
            panic!("undo of step {} did not give back the book: {diff}", k + 1);
        }
    }
    for _ in 0..10 {
        s.key("cmd-shift-z");
    }
    assert!(book(&mut s) == states[10], "half redone: as after the tenth");
    s.save_as("xsc56.xlsx");
    s.reopen();
}

/// XSC-53: a number column narrowed (it shows ###) and fitted again; long
/// text runs over empty cells and is cut by a filled one; wrapping it makes
/// the row taller (in the same step), fitting the row too.
fn xsc53_widths_and_wrapping() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let (x, y) = s.header_line("B");
    s.drag((x, y), (x - 60.0, y));
    s.screenshot("narrow-hashes");
    let (x, y) = s.header_line("B");
    s.double_click((x, y));
    assert!(s.size_pt("B") > 60.0, "fitted again: {}", s.size_pt("B"));
    s.click_cell("G4");
    s.enter("折り返して表示する長い文字列の例です", "enter");
    s.screenshot("runs-over");
    s.click_cell("H4");
    s.enter("x", "enter");
    s.screenshot("cut");
    let before = s.size_pt("4");
    s.click_cell("G4");
    s.toolbar("wrap");
    let wrapped = s.size_pt("4");
    assert!(wrapped > before * 1.5, "the row grows: {before} → {wrapped}");
    s.key("cmd-z");
    assert_eq!(s.size_pt("4"), before, "one step");
    s.key("cmd-shift-z");
    s.save_as("xsc53.xlsx");
    let saved = s.reopen();
    assert!(sheet_of_slide(&saved.slides[0]).unwrap().rows[3] as f32 / ibis_doc_model::EMU_PER_POINT as f32 > before * 1.5);
}

/// XSC-50: a row inserted inside a range: a name, a validation rule, a
/// merged range, the chart's data and the SUM grow; one inserted below the
/// last row leaves them (as Excel does).
fn xsc50_insert_row_grows_ranges() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B4");
    for _ in 0..4 {
        s.key("shift-down");
    }
    s.click((54.0, 123.0));
    s.key("cmd-a");
    s.type_text("四月");
    s.key("enter");
    s.click_cell("G4");
    for _ in 0..4 {
        s.key("shift-down");
    }
    s.update(|e, _, cx| e.set_list_validation("A,B", "", cx));
    s.toolbar_menu("merge", "xlsx.merge");
    // Inside: row 6.
    s.click_cell("A6");
    s.menu("xlsx.insert-row-above");
    let sheet = s.with(|e, _| sheet_of_slide(e.slide()).unwrap().clone());
    let names = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().names.clone());
    assert!(names.iter().any(|n| n.name == "四月" && n.formula == "月次売上!$B$4:$B$9"), "{names:?}");
    assert_eq!(sheet.validations[0].ranges[0].last_row, 8);
    assert!(sheet.merges.iter().any(|m| (m.first_row, m.last_row, m.first_column) == (3, 8, 6)));
    assert_eq!(s.formula("B10").as_deref(), Some("SUM(B4:B9)"));
    // Below the last row (row 10, above the totals): the ranges stay.
    s.click_cell("A10");
    s.menu("xlsx.insert-row-above");
    assert_eq!(s.formula("B11").as_deref(), Some("SUM(B4:B9)"));
    let names = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().names.clone());
    assert!(names.iter().any(|n| n.formula == "月次売上!$B$4:$B$9"));
    // The chart's ranges in the saved file grow too.
    let path = s.save_as("xsc50.xlsx");
    let chart = std::process::Command::new("unzip").arg("-p").arg(&path).arg("xl/charts/chart1.xml").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    assert!(chart.contains("月次売上!$B$4:$B$9"), "the chart's data grows");
    s.reopen();
}

/// XSC-35: ページ設定: landscape, narrow margins, one page, a print area
/// and a page break; saved and read back; undo.
fn xsc35_page_setup() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("file.page-setup");
    s.screenshot("page-setup");
    s.update(|e, _, cx| e.set_print(|p| p.landscape = true, None, cx));
    s.update(|e, _, cx| e.set_print(|p| p.margins = [228_600, 228_600, 685_800, 685_800], None, cx));
    s.update(|e, _, cx| e.set_print(|p| p.fit_to = Some((1, 1)), None, cx));
    s.click_cell("A1");
    s.key("shift-down");
    for _ in 0..11 {
        s.key("shift-down");
    }
    s.update(|e, _, cx| {
        let r = e.selected_range();
        e.set_print(|_| {}, Some(r), cx)
    });
    s.click_cell("A6");
    s.update(|e, _, cx| e.toggle_row_break(cx));
    let print = s.with(|e, _| e.current_print().unwrap());
    assert!(print.landscape && print.fit_to == Some((1, 1)) && print.row_breaks == vec![5]);
    s.save_as("xsc35.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert!(sheet.print.landscape);
    assert_eq!(sheet.print.fit_to, Some((1, 1)));
    assert_eq!(sheet.print.margins[0], 228_600);
    assert_eq!(sheet.print.row_breaks, vec![5]);
    assert_eq!(sheet.print_area.map(|r| (r.first_row, r.last_row, r.last_column)), Some((0, 12, 4)));
    s.click_cell("A1");
    s.key("cmd-z");
    assert!(s.with(|e, _| e.current_print().unwrap().row_breaks.is_empty()));
}

/// XSC-08: a summary with VLOOKUP, IF, COUNTIF, SUMIFS, AVERAGEIF, ROUND
/// and TEXT over a table; the data changed, everything follows.
fn xsc08_functions() {
    let mut s = new_sheet();
    for row in [["品名", "店舗", "数量", "単価"], ["りんご", "北", "10", "120"], ["みかん", "南", "25", "80"], ["りんご", "南", "7", "120"], ["ぶどう", "北", "3", "450"]] {
        for (i, v) in row.iter().enumerate() {
            s.enter(v, if i < 3 { "tab" } else { "enter" });
        }
    }
    let formulas = [
        ("F1", "=VLOOKUP(\"みかん\",A2:D5,4,FALSE)", CellValue::Number(80.0)),
        ("F2", "=IF(C2>5,\"多い\",\"少ない\")", CellValue::Text("多い".into())),
        ("F3", "=COUNTIF(A2:A5,\"りんご\")", CellValue::Number(2.0)),
        ("F4", "=SUMIFS(C2:C5,A2:A5,\"りんご\",B2:B5,\"南\")", CellValue::Number(7.0)),
        ("F5", "=AVERAGEIF(B2:B5,\"北\",C2:C5)", CellValue::Number(6.5)),
        ("F6", "=ROUND(C2*D2/7,1)", CellValue::Number(171.4)),
        ("F7", "=TEXT(SUMPRODUCT(C2:C5,D2:D5),\"#,##0円\")", CellValue::Text("5,390円".into())),
    ];
    for (at, f, _) in &formulas {
        s.click_cell(at);
        s.enter(f, "enter");
    }
    for (at, _, v) in &formulas {
        assert_eq!(&s.value(at), v, "{at}");
    }
    // The data changed: they follow.
    s.click_cell("C4");
    s.enter("17", "enter");
    assert_eq!(s.value("F4"), CellValue::Number(17.0));
    assert_eq!(s.value("F7"), CellValue::Text("6,590円".into()));
    s.save_as("xsc08.xlsx");
    s.reopen();
}

/// XSC-09: dates: DATEDIF, NETWORKDAYS, EDATE, EOMONTH, WEEKDAY, shown in
/// yyyy"年"m"月"d"日", aaaa and the Japanese era.
fn xsc09_dates() {
    let mut s = new_sheet();
    s.enter("2026/4/1", "enter");
    s.enter("2026/9/30", "enter");
    let cells = [
        ("B1", "=DATEDIF(A1,A2,\"M\")", "5"),
        ("B2", "=NETWORKDAYS(A1,A2)", "131"),
        ("B3", "=EDATE(A1,3)", "46204"),
        ("B4", "=EOMONTH(A1,0)", "46142"),
        ("B5", "=WEEKDAY(A2)", "4"),
    ];
    for (at, f, _) in &cells {
        s.click_cell(at);
        s.enter(f, "enter");
    }
    for (at, _, shown) in &cells {
        assert_eq!(s.shown(at), *shown, "{at}");
    }
    let code = |s: &mut Scenario, at: &str, code: &str| {
        s.click_cell(at);
        let code = code.to_string();
        s.update(move |e, _, cx| e.format_cells(ibis_doc_model::StyleChange::NumberFormat(code), cx));
    };
    code(&mut s, "B3", "yyyy\"年\"m\"月\"d\"日\"");
    assert_eq!(s.shown("B3"), "2026年7月1日");
    code(&mut s, "A2", "aaaa");
    assert_eq!(s.shown("A2"), "水曜日");
    code(&mut s, "B4", "[$-411]ggge\"年\"m\"月\"d\"日\"");
    assert_eq!(s.shown("B4"), "令和8年4月30日");
    s.save_as("xsc09.xlsx");
    s.reopen();
}

/// XSC-39: a formula with a function the engine does not have: an edit it
/// does not depend on leaves its value (and no full calculation); an edit
/// it depends on marks the book for Excel to calculate on opening.
fn xsc39_unsupported_function() {
    let mut s = new_sheet();
    s.enter("5", "enter");
    s.click_cell("B1");
    // WEBSERVICE is not calculated by the engine: Excel's value is kept.
    s.enter("=IF(A1>0,WEBSERVICE(\"https://example.com\"),0)", "enter");
    s.click_cell("C1");
    s.enter("7", "enter");
    let workbook = |p: &std::path::Path| std::process::Command::new("unzip").arg("-p").arg(p).arg("xl/workbook.xml").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    s.click_cell("A1");
    s.enter("9", "enter");
    let path = s.save_as("xsc39.xlsx");
    assert!(workbook(&path).contains("fullCalcOnLoad=\"1\""), "Excel is to calculate it again");
}

/// XSC-40: a workbook of many number formats: bold and borders added to
/// its formats column; the formats and what the cells show stay.
fn xsc40_formats_file() {
    let mut s = Scenario::open("test/editing/xlsx-formats.xlsx");
    let shown_before: Vec<String> = (2..=19).map(|r| s.shown(&format!("C{r}"))).collect();
    s.click_cell("C2");
    for _ in 0..17 {
        s.key("shift-down");
    }
    s.key("cmd-b");
    s.toolbar_menu("borders", "xlsx.border-all");
    let shown_after: Vec<String> = (2..=19).map(|r| s.shown(&format!("C{r}"))).collect();
    assert_eq!(shown_before, shown_after, "what they show stays");
    s.save_as("xsc40.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    for r in 1..19u32 {
        let c = sheet.cell(r, 2).unwrap();
        assert!(sheet.styles[c.style as usize].bold);
        assert_eq!(c.text, shown_before[(r - 1) as usize], "C{}", r + 1);
    }
}

/// The format of a cell of the current sheet.
fn style_at(s: &mut Scenario, r: &str) -> ibis_doc_model::CellStyle {
    let (column, row) = parse(r);
    s.with(move |e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.cell(row, column).map(|c| sheet.styles[c.style as usize].clone()).unwrap_or_default()
    })
}

/// A number format code put on the selected cells through 表示形式 > ユーザー定義.
fn custom_format(s: &mut Scenario, code: &str) {
    s.toolbar_menu("number-format", "xlsx.number-format-custom");
    assert!(s.with(|e, _| e.sheet_state().unwrap().custom_format.is_some()));
    let code = code.to_string();
    s.update(move |e, window, cx| {
        if let Some(p) = e.sheet_state().and_then(|st| st.custom_format.as_ref()) {
            let input = p.code.clone();
            input.update(cx, |i, cx| i.set_value(code.clone(), window, cx));
        }
        e.format_cells(ibis_doc_model::StyleChange::NumberFormat(code), cx);
    });
}

/// XSC-13: a table made to look right: bold, filled and centred headings,
/// a grid, a double line under the body, another font and size.
fn xsc13_table_look() {
    let mut s = new_sheet();
    for row in [["月", "店舗A", "店舗B"], ["4月", "120", "80"], ["5月", "135", "95"], ["合計", "255", "175"]] {
        for (i, v) in row.iter().enumerate() {
            s.enter(v, if i < 2 { "tab" } else { "enter" });
        }
    }
    s.click_cell("A1");
    s.key("shift-right");
    s.key("shift-right");
    s.key("cmd-b");
    s.toolbar_menu("fill", "xlsx.fill");
    s.toolbar_menu("halign", "xlsx.align-center");
    s.click_cell("A1");
    for _ in 0..3 {
        s.key("shift-down");
    }
    s.key("shift-right");
    s.key("shift-right");
    s.toolbar_menu("borders", "xlsx.border-all");
    s.click_cell("A3");
    s.key("shift-right");
    s.key("shift-right");
    s.toolbar_menu("borders", "xlsx.border-double-bottom");
    s.click_cell("A1");
    for _ in 0..3 {
        s.key("shift-down");
    }
    s.key("shift-right");
    s.key("shift-right");
    s.toolbar_menu("font", "xlsx.font");
    s.toolbar_state("bold");
    s.screenshot("table");
    let h = style_at(&mut s, "B1");
    assert!(h.bold && h.fill.is_some() && h.align == ibis_doc_model::CellAlign::Center);
    assert_eq!(style_at(&mut s, "B3").bottom.map(|b| b.line), Some(ibis_doc_model::BorderLine::Double));
    assert!(style_at(&mut s, "C4").left.is_some());
    s.save_as("xsc13.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[1]).unwrap().clone();
    assert!(sheet.styles[sheet.cell(0, 1).unwrap().style as usize].bold);
}

/// XSC-14: a number column: comma style, currency, percent, a decimal more
/// and less, and a user code `#,##0,"千円"`.
fn xsc14_number_formats() {
    let mut s = new_sheet();
    s.enter("1234567", "enter");
    s.click_cell("A1");
    s.toolbar_menu("number-format", "xlsx.number-format");
    assert_eq!(s.shown("A1"), "1234567");
    s.toolbar("currency");
    assert_eq!(s.shown("A1"), "¥1,234,567");
    s.toolbar("percent");
    assert_eq!(s.shown("A1"), "123456700%");
    s.toolbar("decimal-more");
    assert_eq!(s.shown("A1"), "123456700.0%");
    s.toolbar("decimal-less");
    s.toolbar("decimal-less");
    assert_eq!(s.shown("A1"), "123456700%");
    custom_format(&mut s, "#,##0,\"千円\"");
    assert_eq!(s.shown("A1"), "1,235千円");
    s.save_as("xsc14.xlsx");
    let saved = s.reopen();
    assert_eq!(sheet_of_slide(&saved.slides[1]).unwrap().cell(0, 0).unwrap().text, "1,235千円");
}

/// XSC-15: date and time formats on one value.
fn xsc15_date_formats() {
    let mut s = new_sheet();
    s.enter("2026/9/30 14:05", "enter");
    s.click_cell("A1");
    for (code, shown) in [
        ("yyyy/m/d", "2026/9/30"),
        ("m\"月\"d\"日\"(aaa)", "9月30日(水)"),
        ("h:mm AM/PM", "2:05 PM"),
        ("[h]:mm", "1111094:05"),
        ("[$-411]ggge\"年\"m\"月\"d\"日\"", "令和8年9月30日"),
    ] {
        custom_format(&mut s, code);
        assert_eq!(s.shown("A1"), shown, "{code}");
    }
    s.save_as("xsc15.xlsx");
    s.reopen();
}

/// XSC-16: sections with conditions and colours.
fn xsc16_conditional_sections() {
    let mut s = new_sheet();
    for v in ["-5", "3", "0"] {
        s.enter(v, "enter");
    }
    s.click_cell("A1");
    s.key("shift-down");
    s.key("shift-down");
    custom_format(&mut s, "[Red][<0]0;[Blue]0");
    assert_eq!(s.shown("A1"), "5");
    assert_eq!(s.shown("A2"), "3");
    custom_format(&mut s, "0;\"▲\"0");
    assert_eq!(s.shown("A1"), "▲5");
    assert_eq!(s.shown("A3"), "0");
    s.screenshot("sections");
    s.save_as("xsc16.xlsx");
    s.reopen();
}

/// XSC-17: long text wrapped (the row grows), indented more and less.
fn xsc17_wrap_and_indent() {
    let mut s = new_sheet();
    s.enter("折り返して表示する長い文字列の例です。二行以上になります。", "enter");
    s.click_cell("A1");
    let before = s.size_pt("1");
    s.toolbar("wrap");
    assert!(s.size_pt("1") > before, "the row grows");
    s.toolbar("indent-more");
    s.toolbar("indent-more");
    assert_eq!(style_at(&mut s, "A1").indent, 2);
    s.toolbar("indent-less");
    assert_eq!(style_at(&mut s, "A1").indent, 1);
    s.screenshot("wrapped");
    s.save_as("xsc17.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[1]).unwrap().clone();
    let st = &sheet.styles[sheet.cell(0, 0).unwrap().style as usize];
    assert!(st.wrap && st.indent == 1);
}

/// XSC-36 (目盛線): the grid lines hidden from the 表示 menu (checked off),
/// saved, shown again.
fn xsc36_grid_lines() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    assert_eq!(s.menu_state("xlsx.gridlines"), Some((true, true)));
    s.menu("xlsx.gridlines");
    assert_eq!(s.menu_state("xlsx.gridlines"), Some((true, false)));
    s.screenshot("no-grid");
    s.save_as("xsc36.xlsx");
    let saved = s.reopen();
    assert!(!sheet_of_slide(&saved.slides[0]).unwrap().grid_lines);
    s.menu("xlsx.gridlines");
    assert!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().grid_lines));
}

/// XSC-34: the first tab dragged onto the third (the sheets reorder), a
/// tab colour; saved.
fn xsc34_tabs() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.toolbar_state("bold");
    // The tabs along the bottom: 月次売上, 商品, メモ.
    s.drag((130.0, 680.0), (260.0, 680.0));
    let names = |s: &mut Scenario| s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().map(|x| x.name.clone()).collect::<Vec<_>>());
    assert_eq!(names(&mut s), ["商品", "メモ", "月次売上"], "dragged to the third place");
    s.menu("xlsx.tab-color");
    s.screenshot("tabs");
    let color = s.with(|e, _| sheet_of_slide(e.slide()).unwrap().tab_color);
    assert!(color.is_some());
    s.save_as("xsc34.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.workbook.as_ref().unwrap().sheets[2].name, "月次売上");
    assert!(sheet_of_slide(&saved.slides[2]).unwrap().tab_color.is_some());
}

/// XSC-44: `=SUM(` typed, then a range dragged out on the sheet: its
/// reference goes into the formula; `)` and Enter finish it.
fn xsc44_point_references() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("G4");
    s.type_text("=SUM(");
    let (a, b) = (s.with(|e, _| e.cell_center(1, 3)).unwrap(), s.with(|e, _| e.cell_center(3, 3)).unwrap());
    s.drag((f32::from(a.x), f32::from(a.y)), (f32::from(b.x), f32::from(b.y)));
    assert_eq!(s.with(|e, _| e.cell_edit().map(|c| c.text.clone())).as_deref(), Some("=SUM(B4:D4"));
    s.screenshot("pointing");
    s.type_text(")*");
    // A single click after an operator: one cell.
    s.click_cell("E4");
    assert_eq!(s.with(|e, _| e.cell_edit().map(|c| c.text.clone())).as_deref(), Some("=SUM(B4:D4)*E4"));
    s.key("enter");
    assert_eq!(s.formula("G4").as_deref(), Some("SUM(B4:D4)*E4"));
    // Not after an operator: a click leaves the entry (as in Excel).
    s.click_cell("H4");
    s.type_text("12");
    s.click_cell("H5");
    assert_eq!(s.value("H4"), CellValue::Number(12.0));
    assert_eq!(s.selected(), "H5");
}

/// XSC-32: 10,000 rows (made up) summed with SUMIFS and COUNTIFS; one cell
/// changed: the totals follow within half a second.
fn xsc32_large_table() {
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let mut csv = String::from("店舗,品目,数量\n");
    let shops = ["北", "南", "東", "西"];
    let items = ["りんご", "みかん", "ぶどう"];
    for i in 0..10_000 {
        csv.push_str(&format!("{},{},{}\n", shops[i % 4], items[i % 3], i % 10 + 1));
    }
    std::fs::write(dir.join("xsc32.csv"), csv).unwrap();
    let mut s = Scenario::open("target/scenario-input/xsc32.csv");
    s.click_cell("E1");
    s.enter("=SUMIFS(C2:C10001,A2:A10001,\"北\",B2:B10001,\"りんご\")", "enter");
    s.enter("=COUNTIFS(A2:A10001,\"南\")", "enter");
    let before = s.value("E1");
    assert_eq!(s.value("E2"), CellValue::Number(2500.0));
    // Row 2 (北, りんご, 1) changed to 101.
    s.click_cell("C2");
    let started = crate::clock::Instant::now();
    s.enter("101", "enter");
    let took = started.elapsed();
    let CellValue::Number(b) = before else { panic!() };
    assert_eq!(s.value("E1"), CellValue::Number(b + 100.0));
    assert!(took.as_millis() < 500, "the edit took {} ms", took.as_millis());
}

/// In English: the filter panel and its オートフィルター オプション, テーブルの
/// 作成, グラフの書式, ページ設定 and 並べ替え drawn from the tables (pictures
/// to look at for Japanese left over).
fn xsc_english_panels() {
    use crate::shell::i18n::{self, Language};
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    i18n::show(Language::English);
    s.click_cell("A3");
    s.menu("xlsx.filter");
    let corner = s.with(|e, _| e.cell_corner(0, 2)).unwrap();
    s.click((f32::from(corner.x) - 6.0, f32::from(corner.y) - 6.0));
    s.update(|e, _, cx| e.filter_panel_section(crate::xlsx::filter::Section::Conditions, cx));
    s.screenshot("english-filter-panel");
    s.update(|e, window, cx| e.filter_panel_condition([Some(10), None], true, window, cx));
    s.screenshot("english-filter-dialog");
    s.update(|e, _, cx| e.close_filter_panel(cx));
    s.menu("xlsx.filter");
    s.click_cell("A4");
    s.key("cmd-t");
    s.screenshot("english-table-dialog");
    s.update(|e, window, cx| e.close_table_dialog(window, cx));
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    s.screenshot("english-chart-format");
    s.click_cell("A1");
    s.menu("file.page-setup");
    s.screenshot("english-page-setup");
    s.menu("xlsx.sort-manage");
    s.screenshot("english-sort");
    i18n::show(Language::Japanese);
    s.update(|_, _, _| {});
}

/// XSC-28, ページ設定: row 3 (the headings) as the title rows printed on
/// every page, 1/? ページ in the footer; saved and read back.
fn xsc28_print_titles_and_footer() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("file.page-setup");
    s.click_cell("A3");
    s.update(|e, _, cx| {
        e.set_print_title_rows(Some((2, 2)), cx);
        e.set_print(|p| p.header_footer.odd_footer = Some("&C&P / &N ページ".into()), None, cx);
    });
    assert_eq!(s.with(|e, _| e.print_title_rows()), Some((2, 2)));
    s.screenshot("page-setup");
    s.save_as("xsc28-print.xlsx");
    let saved = s.reopen();
    let wb = saved.workbook.as_ref().unwrap();
    let name = wb.names.iter().find(|n| n.name == "_xlnm.Print_Titles" && n.sheet == Some(0)).expect("the name");
    assert_eq!(name.formula, "月次売上!$3:$3");
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap();
    assert_eq!(sheet.print.header_footer.odd_footer.as_deref(), Some("&C&P / &N ページ"));
    s.update(|e, _, cx| e.set_print_title_rows(None, cx));
    assert_eq!(s.with(|e, _| e.print_title_rows()), None);
}

/// XSC-27, グラフの書式: the chart selected, its title retyped, the legend
/// put under it, the value axis to 3,000,000 by 500,000 without gridlines,
/// the chart moved; saved and read back as set, and undone step by step.
fn xsc27_chart_format() {
    use ibis_doc_model::chart::LegendPosition;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));
    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let axis = crate::xlsx::chart_edit::value_axis(&chart(&mut s)).unwrap();
    s.update(|e, window, cx| {
        let p = e.wb().chart_edit.as_ref().unwrap();
        let (title, max, unit) = (p.title.clone(), p.max.clone(), p.unit.clone());
        title.update(cx, |i, cx| i.set_value("店舗別の売上（4〜6月）", window, cx));
        max.update(cx, |i, cx| i.set_value("3000000", window, cx));
        unit.update(cx, |i, cx| i.set_value("500000", window, cx));
        e.apply_sheet_chart_panel(cx);
        e.set_sheet_chart_legend(Some(LegendPosition::Bottom), cx);
        e.set_sheet_chart_gridlines(false, cx);
    });
    s.screenshot("chart-format");
    let c = chart(&mut s);
    assert_eq!(c.title.as_ref().map(|t| t.text.as_str()), Some("店舗別の売上（4〜6月）"));
    assert_eq!(c.legend.as_ref().and_then(|l| l.source_position), Some(LegendPosition::Bottom));
    assert_eq!((c.axes[axis].max.map(|n| n.0), c.axes[axis].major_unit.map(|n| n.0), c.axes[axis].gridlines.is_some()), (Some(3_000_000.0), Some(500_000.0), false));
    // Moved down a row's height and more.
    let (slide, id, frame) = s.with(|e, _| { let sh = &e.slide().shapes[index]; (e.current_slide, sh.id, sh.frame) });
    let moved = ibis_doc_model::EmuRect::new(frame.x + 200_000, frame.y + 300_000, frame.width, frame.height);
    s.update(|e, _, cx| { e.document.execute(vec![ibis_doc_model::Command::SetFrame { slide, shape: id, frame: moved }]).unwrap(); cx.notify(); });
    s.save_as("xsc27-chart.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.title.as_ref().map(|t| t.text.as_str()), Some("店舗別の売上（4〜6月）"));
    assert_eq!(c.legend.as_ref().and_then(|l| l.source_position), Some(LegendPosition::Bottom));
    let a = &c.axes[axis];
    assert_eq!((a.max.map(|n| n.0), a.major_unit.map(|n| n.0), a.gridlines.is_some()), (Some(3_000_000.0), Some(500_000.0), false));
    assert!((sh.frame.x - moved.x).abs() <= 1 && (sh.frame.y - moved.y).abs() <= 1, "{:?} {:?}", sh.frame, moved);
    // Undone: the move, the gridlines, the legend, the title and scale.
    for _ in 0..4 {
        s.key("cmd-z");
    }
    s.click(at);
    assert_eq!(chart(&mut s), chart_of_file());
}

/// 横（項目）軸・縦（値）軸のタイトルを表示・編集し、保存・再読込・Undo/Redo を検証.
fn chart003_axis_titles() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let cat_axis = crate::xlsx::chart_edit::category_axis(&chart(&mut s)).unwrap();
    let val_axis = crate::xlsx::chart_edit::value_axis(&chart(&mut s)).unwrap();

    // 1. 横軸・縦軸のタイトルを表示し、テキストを入力して適用
    s.update(|e, window, cx| {
        e.set_sheet_chart_cat_axis_title_shown(true, cx);
        e.set_sheet_chart_val_axis_title_shown(true, cx);
        let p = e.wb().chart_edit.as_ref().unwrap();
        p.cat_title.update(cx, |i, cx| i.set_value("売上月", window, cx));
        p.val_title.update(cx, |i, cx| i.set_value("売上高（円）", window, cx));
        e.apply_sheet_chart_panel(cx);
    });

    let c = chart(&mut s);
    assert_eq!(c.axes[cat_axis].title.as_ref().map(|t| t.text.as_str()), Some("売上月"));
    assert_eq!(c.axes[val_axis].title.as_ref().map(|t| t.text.as_str()), Some("売上高（円）"));

    // 2. 保存して開き直す（ラウンドトリップ）
    s.save_as("chart-axis-titles-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.axes[cat_axis].title.as_ref().map(|t| t.text.as_str()), Some("売上月"));
    assert_eq!(c.axes[val_axis].title.as_ref().map(|t| t.text.as_str()), Some("売上高（円）"));

    // 3. 元に戻す（Undo）
    s.key("cmd-z"); // apply_sheet_chart_panel
    s.key("cmd-z"); // set_sheet_chart_val_axis_title_shown
    s.key("cmd-z"); // set_sheet_chart_cat_axis_title_shown
    let c = chart(&mut s);
    assert!(c.axes[cat_axis].title.is_none());
    assert!(c.axes[val_axis].title.is_none());

    // 4. やり直し（Redo: Cmd+Shift+Z）
    s.key("cmd-shift-z");
    s.key("cmd-shift-z");
    s.key("cmd-shift-z");
    let c = chart(&mut s);
    assert_eq!(c.axes[cat_axis].title.as_ref().map(|t| t.text.as_str()), Some("売上月"));
    assert_eq!(c.axes[val_axis].title.as_ref().map(|t| t.text.as_str()), Some("売上高（円）"));
}

/// データラベルの表示・位置設定・OOXML保存・再読込・Undo/Redo を検証.
fn chart005_data_labels() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();

    // 初期状態: ラベルなし
    let c = chart(&mut s);
    for series in c.groups.iter().flat_map(|g| &g.series) {
        assert!(series.labels.is_none());
    }

    // 1. データラベルを「外側上 (OutsideEnd)」に設定
    s.update(|e, _, cx| {
        e.set_sheet_chart_labels_position(Some(Some(ibis_doc_model::LabelPosition::OutsideEnd)), cx);
    });

    let c = chart(&mut s);
    for series in c.groups.iter().flat_map(|g| &g.series) {
        let labels = series.labels.as_ref().expect("labels set");
        assert_eq!(labels.position, Some(ibis_doc_model::LabelPosition::OutsideEnd));
        assert!(labels.show_value);
    }

    // 2. 保存して開き直す（ラウンドトリップ検証）
    s.save_as("chart-data-labels-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    for series in c.groups.iter().flat_map(|g| &g.series) {
        let labels = series.labels.as_ref().expect("labels preserved after reopen");
        assert_eq!(labels.position, Some(ibis_doc_model::LabelPosition::OutsideEnd));
        assert!(labels.show_value);
    }

    // 3. データラベルを「中央 (Center)」に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_labels_position(Some(Some(ibis_doc_model::LabelPosition::Center)), cx);
    });
    let c = chart(&mut s);
    for series in c.groups.iter().flat_map(|g| &g.series) {
        let labels = series.labels.as_ref().expect("labels set to center");
        assert_eq!(labels.position, Some(ibis_doc_model::LabelPosition::Center));
    }

    // 4. 元に戻す（Undo）
    s.key("cmd-z"); // Center -> OutsideEnd
    let c = chart(&mut s);
    for series in c.groups.iter().flat_map(|g| &g.series) {
        let labels = series.labels.as_ref().expect("labels restored to outside end");
        assert_eq!(labels.position, Some(ibis_doc_model::LabelPosition::OutsideEnd));
    }

    s.key("cmd-z"); // OutsideEnd -> None
    let c = chart(&mut s);
    for series in c.groups.iter().flat_map(|g| &g.series) {
        assert!(series.labels.is_none());
    }

    // 5. やり直し（Redo: Cmd+Shift+Z）
    s.key("cmd-shift-z"); // None -> OutsideEnd
    let c = chart(&mut s);
    for series in c.groups.iter().flat_map(|g| &g.series) {
        let labels = series.labels.as_ref().expect("labels redone to outside end");
        assert_eq!(labels.position, Some(ibis_doc_model::LabelPosition::OutsideEnd));
    }

    s.key("cmd-shift-z"); // OutsideEnd -> Center
    let c = chart(&mut s);
    for series in c.groups.iter().flat_map(|g| &g.series) {
        let labels = series.labels.as_ref().expect("labels redone to center");
        assert_eq!(labels.position, Some(ibis_doc_model::LabelPosition::Center));
    }
}

/// グラフの項目軸・値軸の反転（逆順）設定、保存・再読込・Undo/Redo を検証.
fn chart010_axis_orientation() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let cat_axis = crate::xlsx::chart_edit::category_axis(&chart(&mut s)).unwrap();
    let val_axis = crate::xlsx::chart_edit::value_axis(&chart(&mut s)).unwrap();

    // 初期状態: 軸の反転なし
    let c = chart(&mut s);
    assert!(!c.axes[cat_axis].reversed);
    assert!(!c.axes[val_axis].reversed);

    // 1. 横軸・縦軸をともに「軸を反転する」に設定
    s.update(|e, _, cx| {
        e.set_sheet_chart_cat_axis_reversed(true, cx);
        e.set_sheet_chart_val_axis_reversed(true, cx);
    });

    let c = chart(&mut s);
    assert!(c.axes[cat_axis].reversed);
    assert!(c.axes[val_axis].reversed);

    // 2. 保存して開き直す（ラウンドトリップ検証）
    s.save_as("chart-axis-orientation-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert!(c.axes[cat_axis].reversed);
    assert!(c.axes[val_axis].reversed);

    // 3. 元に戻す（Undo）
    s.key("cmd-z"); // val_reversed -> false
    let c = chart(&mut s);
    assert!(c.axes[cat_axis].reversed);
    assert!(!c.axes[val_axis].reversed);

    s.key("cmd-z"); // cat_reversed -> false
    let c = chart(&mut s);
    assert!(!c.axes[cat_axis].reversed);
    assert!(!c.axes[val_axis].reversed);

    // 4. やり直し（Redo: Cmd+Shift+Z）
    s.key("cmd-shift-z"); // cat_reversed -> true
    let c = chart(&mut s);
    assert!(c.axes[cat_axis].reversed);
    assert!(!c.axes[val_axis].reversed);

    s.key("cmd-shift-z"); // val_reversed -> true
    let c = chart(&mut s);
    assert!(c.axes[cat_axis].reversed);
    assert!(c.axes[val_axis].reversed);
}

/// グラフの項目軸主目盛線・値軸補助目盛線の表示切替、保存・再読込・Undo/Redo を検証.
fn chart012_gridlines() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let cat_axis = crate::xlsx::chart_edit::category_axis(&chart(&mut s)).unwrap();
    let val_axis = crate::xlsx::chart_edit::value_axis(&chart(&mut s)).unwrap();

    // 初期状態: 項目軸は目盛線なし、値軸は主目盛線あり・補助目盛線なし
    let c = chart(&mut s);
    assert!(c.axes[cat_axis].gridlines.is_none());
    assert!(c.axes[val_axis].gridlines.is_some());
    assert!(c.axes[val_axis].minor_gridlines.is_none());

    // 1. 項目軸に主目盛線、値軸に補助目盛線を表示
    s.update(|e, _, cx| {
        e.set_sheet_chart_cat_gridlines(true, cx);
        e.set_sheet_chart_val_minor_gridlines(true, cx);
    });

    let c = chart(&mut s);
    assert!(c.axes[cat_axis].gridlines.is_some());
    assert!(c.axes[val_axis].gridlines.is_some());
    assert!(c.axes[val_axis].minor_gridlines.is_some());

    // 2. 保存して開き直す（ラウンドトリップ検証）
    s.save_as("chart-gridlines-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert!(c.axes[cat_axis].gridlines.is_some());
    assert!(c.axes[val_axis].gridlines.is_some());
    assert!(c.axes[val_axis].minor_gridlines.is_some());

    // 3. 元に戻す（Undo）
    s.key("cmd-z"); // val_minor_gridlines -> None
    let c = chart(&mut s);
    assert!(c.axes[cat_axis].gridlines.is_some());
    assert!(c.axes[val_axis].minor_gridlines.is_none());

    s.key("cmd-z"); // cat_gridlines -> None
    let c = chart(&mut s);
    assert!(c.axes[cat_axis].gridlines.is_none());
    assert!(c.axes[val_axis].minor_gridlines.is_none());

    // 4. やり直し（Redo: Cmd+Shift+Z）
    s.key("cmd-shift-z"); // cat_gridlines -> Some
    let c = chart(&mut s);
    assert!(c.axes[cat_axis].gridlines.is_some());
    assert!(c.axes[val_axis].minor_gridlines.is_none());

    s.key("cmd-shift-z"); // val_minor_gridlines -> Some
    let c = chart(&mut s);
    assert!(c.axes[cat_axis].gridlines.is_some());
    assert!(c.axes[val_axis].minor_gridlines.is_some());
}

/// グラフの値軸の表示単位設定、保存・再読込・Undo/Redo を検証.
fn chart007_axis_units() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let val_axis = crate::xlsx::chart_edit::value_axis(&chart(&mut s)).unwrap();

    // 初期状態: 表示単位なし
    let c = chart(&mut s);
    assert!(c.axes[val_axis].display_unit.is_none());

    // 1. 値軸の表示単位を「千」（1,000.0）に設定
    s.update(|e, _, cx| {
        e.set_sheet_chart_display_unit(Some(ibis_doc_model::Number(1000.0)), cx);
    });

    let c = chart(&mut s);
    assert_eq!(c.axes[val_axis].display_unit, Some(ibis_doc_model::Number(1000.0)));

    // 2. 保存して開き直す（ラウンドトリップ検証）
    s.save_as("chart-axis-units-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.axes[val_axis].display_unit, Some(ibis_doc_model::Number(1000.0)));

    // 3. 表示単位を「百万」（1,000,000.0）に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_display_unit(Some(ibis_doc_model::Number(1_000_000.0)), cx);
    });
    let c = chart(&mut s);
    assert_eq!(c.axes[val_axis].display_unit, Some(ibis_doc_model::Number(1_000_000.0)));

    // 4. 元に戻す（Undo）
    s.key("cmd-z"); // millions -> thousands
    let c = chart(&mut s);
    assert_eq!(c.axes[val_axis].display_unit, Some(ibis_doc_model::Number(1000.0)));

    s.key("cmd-z"); // thousands -> None
    let c = chart(&mut s);
    assert!(c.axes[val_axis].display_unit.is_none());

    // 5. やり直し（Redo: Cmd+Shift+Z）
    s.key("cmd-shift-z"); // None -> thousands
    let c = chart(&mut s);
    assert_eq!(c.axes[val_axis].display_unit, Some(ibis_doc_model::Number(1000.0)));

    s.key("cmd-shift-z"); // thousands -> millions
    let c = chart(&mut s);
    assert_eq!(c.axes[val_axis].display_unit, Some(ibis_doc_model::Number(1_000_000.0)));
}

/// グラフの系列の重なり（overlap）と要素の間隔（gapWidth）の設定、保存・再読込・Undo/Redo を検証.
fn chart013_series_gap_and_overlap() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let (h, initial_gap, initial_overlap) = match &chart(&mut s).groups[0].kind {
        ibis_doc_model::ChartType::Bar { horizontal, gap_width, overlap } => (*horizontal, *gap_width, *overlap),
        _ => panic!("first group should be bar chart"),
    };

    // 1. 書式パネルの入力欄から gap_width = 80, overlap = 50 を入力して適用
    s.update(|e, window, cx| {
        if let Some(p) = e.wb().chart_edit.as_ref() {
            p.gap_width.update(cx, |inp, cx| inp.set_value("80".to_string(), window, cx));
            p.overlap.update(cx, |inp, cx| inp.set_value("50".to_string(), window, cx));
        }
    });
    s.update(|e, _, cx| e.apply_sheet_chart_panel(cx));

    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: 80, overlap: 50 });

    // 2. 保存して開き直す（ラウンドトリップ検証）
    s.save_as("chart-gap-overlap-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: 80, overlap: 50 });

    // 3. 直接メソッドまたはパネルから gap_width = 200, overlap = -30 に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_bar_options(Some(200), Some(-30), cx);
    });
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: 200, overlap: -30 });

    // 4. 元に戻す（Undo）
    s.key("cmd-z"); // 200, -30 -> 80, 50
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: 80, overlap: 50 });

    s.key("cmd-z"); // 80, 50 -> initial
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: initial_gap, overlap: initial_overlap });

    // 5. やり直し（Redo: Cmd+Shift+Z）
    s.key("cmd-shift-z"); // initial -> 80, 50
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: 80, overlap: 50 });

    s.key("cmd-shift-z"); // 80, 50 -> 200, -30
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: 200, overlap: -30 });
}

/// グラフの値軸の対数目盛（logBase）設定、保存・再読込・Undo/Redo を検証.
fn chart008_axis_log_scale() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let val_axis = crate::xlsx::chart_edit::value_axis(&chart(&mut s)).unwrap();

    // 初期状態: 対数目盛なし (log_base is None)
    let c = chart(&mut s);
    assert!(c.axes[val_axis].log_base.is_none());

    // 1. 書式パネルからの操作（set_sheet_chart_val_axis_log_scale: 底 10.0）
    s.update(|e, _, cx| {
        e.set_sheet_chart_val_axis_log_scale(Some(ibis_doc_model::Number(10.0)), cx);
    });

    let c = chart(&mut s);
    assert_eq!(c.axes[val_axis].log_base, Some(ibis_doc_model::Number(10.0)));

    // 2. 保存して開き直す（ラウンドトリップ検証）
    s.save_as("chart-log-scale-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.axes[val_axis].log_base, Some(ibis_doc_model::Number(10.0)));

    // 3. 底を 2.0 に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_val_axis_log_scale(Some(ibis_doc_model::Number(2.0)), cx);
    });
    let c = chart(&mut s);
    assert_eq!(c.axes[val_axis].log_base, Some(ibis_doc_model::Number(2.0)));

    // 4. 元に戻す（Undo）
    s.key("cmd-z"); // 2.0 -> 10.0
    let c = chart(&mut s);
    assert_eq!(c.axes[val_axis].log_base, Some(ibis_doc_model::Number(10.0)));

    s.key("cmd-z"); // 10.0 -> None
    let c = chart(&mut s);
    assert!(c.axes[val_axis].log_base.is_none());

    // 5. やり直し（Redo: Cmd+Shift+Z）
    s.key("cmd-shift-z"); // None -> 10.0
    let c = chart(&mut s);
    assert_eq!(c.axes[val_axis].log_base, Some(ibis_doc_model::Number(10.0)));

    s.key("cmd-shift-z"); // 10.0 -> 2.0
    let c = chart(&mut s);
    assert_eq!(c.axes[val_axis].log_base, Some(ibis_doc_model::Number(2.0)));
}

/// XLSX-CHEDIT-006 データテーブル:
/// グラフのデータテーブル（dTable）の表示切替、凡例キー・水平罫線・垂直罫線・外枠設定、
/// メニュー「xlsx.chart-data-table」およびサイドバーパネルからの操作、
/// スクリーンショット、ファイル保存・開き直しラウンドトリップ、Undo/Redo 検証。
fn chart006_data_table() {
    use ibis_doc_model::ChartDataTable;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();

    // 初期状態: data_table は None
    assert_eq!(chart(&mut s).data_table, None);

    // 1. メニュー「xlsx.chart-data-table」からデータテーブルを有効化
    s.menu("xlsx.chart-data-table");
    let dt = chart(&mut s).data_table.expect("data table enabled");
    assert_eq!(dt, ChartDataTable::default());
    assert!(dt.show_horz_border);
    assert!(dt.show_vert_border);
    assert!(dt.show_outline);
    assert!(!dt.show_keys);

    // 2. 書式パネルを開いて確認
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    // 3. 凡例キーを有効化、水平罫線を無効化
    s.update(|e, _, cx| {
        e.set_sheet_chart_data_table_show_keys(true, cx);
        e.set_sheet_chart_data_table_show_horz_border(false, cx);
    });
    let dt = chart(&mut s).data_table.unwrap();
    assert!(dt.show_keys);
    assert!(!dt.show_horz_border);
    assert!(dt.show_vert_border);
    assert!(dt.show_outline);

    // スクリーンショット
    s.screenshot("chart006_data_table");

    // 4. 保存して開き直す（OOXML ラウンドトリップ検証）
    s.save_as("chart006_data_table.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    let saved_dt = c.data_table.expect("saved chart has data table");
    assert!(saved_dt.show_keys);
    assert!(!saved_dt.show_horz_border);
    assert!(saved_dt.show_vert_border);
    assert!(saved_dt.show_outline);

    // 5. Undo / Redo 検証
    s.key("cmd-z"); // Undo show_horz_border change
    let dt_undo1 = chart(&mut s).data_table.unwrap();
    assert!(dt_undo1.show_horz_border);

    s.key("cmd-z"); // Undo show_keys change
    let dt_undo2 = chart(&mut s).data_table.unwrap();
    assert!(!dt_undo2.show_keys);

    s.key("cmd-z"); // Undo data table enable (back to None)
    assert_eq!(chart(&mut s).data_table, None);

    s.key("cmd-shift-z"); // Redo data table enable
    assert!(chart(&mut s).data_table.is_some());
}

/// XLSX-CHEDIT-016: グラフの誤差範囲（errBars）の表示切替、方向（Both/Plus/Minus）、計算方法（Fixed/Percentage/StdDev/StdErr）、
/// 末端キャップ表示、スクリーンショット、ファイル保存・開き直しラウンドトリップ、Undo/Redo 検証。
fn chart016_error_bars() {
    use ibis_doc_model::chart::{ErrorBarType, ErrorBarValueType};
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();

    // 初期状態: error_bars は None
    assert!(chart(&mut s).groups.iter().flat_map(|g| &g.series).all(|ser| ser.error_bars.is_none()));

    // 1. メニュー「xlsx.chart-error-bars」から誤差範囲を有効化（既定は両方向・固定値1.0・キャップあり）
    s.menu("xlsx.chart-error-bars");
    let eb = chart(&mut s).groups[0].series[0].error_bars.clone().expect("error_bars enabled");
    assert_eq!(eb.bar_type, ErrorBarType::Both);
    assert_eq!(eb.val_type, ErrorBarValueType::FixedValue);
    assert!(!eb.no_end_cap);

    // 2. 書式パネルを開いて確認
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    // 3. 表示方向を Plus（正方向）に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_error_bar_type(ErrorBarType::Plus, cx);
    });
    let eb = chart(&mut s).groups[0].series[0].error_bars.clone().unwrap();
    assert_eq!(eb.bar_type, ErrorBarType::Plus);

    // 4. 誤差計算方法を Percentage（パーセンテージ）に変更し、キャップを非表示に
    s.update(|e, _, cx| {
        e.set_sheet_chart_error_bar_val_type(ErrorBarValueType::Percentage, cx);
        e.set_sheet_chart_error_bar_end_cap(false, cx);
    });
    let eb = chart(&mut s).groups[0].series[0].error_bars.clone().unwrap();
    assert_eq!(eb.val_type, ErrorBarValueType::Percentage);
    assert!(eb.no_end_cap);

    // スクリーンショット
    s.screenshot("chart016_error_bars");

    // 5. 保存して開き直す（OOXML ラウンドトリップ検証）
    s.save_as("chart016_error_bars.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    let saved_eb = c.groups[0].series[0].error_bars.as_ref().expect("saved chart has error_bars");
    assert_eq!(saved_eb.bar_type, ErrorBarType::Plus);
    assert_eq!(saved_eb.val_type, ErrorBarValueType::Percentage);
    assert!(saved_eb.no_end_cap);

    // 6. Undo / Redo 検証
    s.key("cmd-z"); // Undo end cap false -> true
    let eb_undo1 = chart(&mut s).groups[0].series[0].error_bars.clone().unwrap();
    assert!(!eb_undo1.no_end_cap);

    s.key("cmd-z"); // Undo val_type Percentage -> FixedValue
    let eb_undo2 = chart(&mut s).groups[0].series[0].error_bars.clone().unwrap();
    assert_eq!(eb_undo2.val_type, ErrorBarValueType::FixedValue);

    s.key("cmd-z"); // Undo bar_type Plus -> Both
    let eb_undo3 = chart(&mut s).groups[0].series[0].error_bars.clone().unwrap();
    assert_eq!(eb_undo3.bar_type, ErrorBarType::Both);

    s.key("cmd-z"); // Undo enable error_bars -> None
    assert!(chart(&mut s).groups[0].series[0].error_bars.is_none());

    s.key("cmd-shift-z"); // Redo enable error_bars
    assert!(chart(&mut s).groups[0].series[0].error_bars.is_some());
}

/// XLSX-CHEDIT-017: 近似曲線（trendline）の読み込み・保存・描画・UI・Undo/Redo 検証。
/// メニュー「表示形式 > 近似曲線」および書式パネルでの種類（線形・移動平均等）切替、
/// 数式・R-2乗値表示、スクリーンショット、ファイル保存・開き直しラウンドトリップ、Undo/Redo 検証。
fn chart017_trendline() {
    use ibis_doc_model::chart::TrendlineType;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();

    // 初期状態: trendline は None
    assert!(chart(&mut s).groups.iter().flat_map(|g| &g.series).all(|ser| ser.trendline.is_none()));

    // 1. メニュー「xlsx.chart-trendline」から近似曲線を有効化（既定は線形）
    s.menu("xlsx.chart-trendline");
    let tl = chart(&mut s).groups[0].series[0].trendline.clone().expect("trendline enabled");
    assert_eq!(tl.kind, TrendlineType::Linear);
    assert!(!tl.disp_eq);
    assert!(!tl.disp_r_sqr);

    // 2. 書式パネルを開いて確認
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    // 3. 数式表示・R-2乗値表示をオン
    s.update(|e, _, cx| {
        e.set_sheet_chart_trendline_disp_eq(true, cx);
        e.set_sheet_chart_trendline_disp_r_sqr(true, cx);
    });
    let tl = chart(&mut s).groups[0].series[0].trendline.clone().unwrap();
    assert!(tl.disp_eq);
    assert!(tl.disp_r_sqr);

    // 4. 移動平均に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_trendline_type(Some(TrendlineType::MovingAverage), cx);
    });
    let tl = chart(&mut s).groups[0].series[0].trendline.clone().unwrap();
    assert_eq!(tl.kind, TrendlineType::MovingAverage);

    // スクリーンショット
    s.screenshot("chart017_trendline");

    // 5. 保存して開き直す（OOXML ラウンドトリップ検証）
    s.save_as("chart017_trendline.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    let saved_tl = c.groups[0].series[0].trendline.as_ref().expect("saved chart has trendline");
    assert_eq!(saved_tl.kind, TrendlineType::MovingAverage);

    // 6. Undo / Redo 検証
    s.key("cmd-z"); // Undo MovingAverage change (back to Linear)
    let tl_undo1 = chart(&mut s).groups[0].series[0].trendline.clone().unwrap();
    assert_eq!(tl_undo1.kind, TrendlineType::Linear);

    s.key("cmd-z"); // Undo disp_r_sqr
    let tl_undo2 = chart(&mut s).groups[0].series[0].trendline.clone().unwrap();
    assert!(!tl_undo2.disp_r_sqr);

    s.key("cmd-z"); // Undo disp_eq
    let tl_undo3 = chart(&mut s).groups[0].series[0].trendline.clone().unwrap();
    assert!(!tl_undo3.disp_eq);

    s.key("cmd-z"); // Undo trendline enable (back to None)
    assert!(chart(&mut s).groups[0].series[0].trendline.is_none());

    s.key("cmd-shift-z"); // Redo trendline enable
    assert!(chart(&mut s).groups[0].series[0].trendline.is_some());
}

/// グラフの非表示および空白のセル（dispBlanksAs, plotVisOnly）設定、保存・再読込・Undo/Redo を検証.
fn chart018_disp_blanks_and_plot_vis_only() {
    use ibis_doc_model::DispBlanksAs;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();

    // 初期状態: plot_vis_only is true, disp_blanks_as is Some(DispBlanksAs::Gap)
    let c = chart(&mut s);
    assert_eq!(c.plot_vis_only, true);
    assert_eq!(c.disp_blanks_as, Some(DispBlanksAs::Gap));

    // 1. 書式パネルからの操作（disp_blanks_as = Zero, plot_vis_only = false）
    s.update(|e, _, cx| {
        e.set_sheet_chart_disp_blanks_as(Some(DispBlanksAs::Zero), cx);
    });
    s.update(|e, _, cx| {
        e.set_sheet_chart_plot_vis_only(false, cx);
    });
    let c = chart(&mut s);
    assert_eq!(c.disp_blanks_as, Some(DispBlanksAs::Zero));
    assert_eq!(c.plot_vis_only, false);

    // 2. 保存して開き直す（ラウンドトリップ検証）
    s.save_as("chart-blanks-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.disp_blanks_as, Some(DispBlanksAs::Zero));
    assert_eq!(c.plot_vis_only, false);

    // 3. disp_blanks_as を Span に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_disp_blanks_as(Some(DispBlanksAs::Span), cx);
    });
    let c = chart(&mut s);
    assert_eq!(c.disp_blanks_as, Some(DispBlanksAs::Span));

    // 4. 元に戻す（Undo）
    s.key("cmd-z"); // Span -> Zero
    let c = chart(&mut s);
    assert_eq!(c.disp_blanks_as, Some(DispBlanksAs::Zero));

    s.key("cmd-z"); // plot_vis_only false -> true
    let c = chart(&mut s);
    assert_eq!(c.plot_vis_only, true);

    s.key("cmd-z"); // Zero -> Gap (初期値)
    let c = chart(&mut s);
    assert_eq!(c.disp_blanks_as, Some(DispBlanksAs::Gap));

    // 5. やり直し（Redo: Cmd+Shift+Z）
    s.key("cmd-shift-z"); // Gap -> Zero
    let c = chart(&mut s);
    assert_eq!(c.disp_blanks_as, Some(DispBlanksAs::Zero));

    s.key("cmd-shift-z"); // plot_vis_only true -> false
    let c = chart(&mut s);
    assert_eq!(c.plot_vis_only, false);

    s.key("cmd-shift-z"); // Zero -> Span
    let c = chart(&mut s);
    assert_eq!(c.disp_blanks_as, Some(DispBlanksAs::Span));
}

/// XLSX-CHEDIT-019: グラフのスタイルおよび配色の変更、保存・開き直し、メニュー操作、Undo/Redo を検証.
fn chart019_style_and_palette() {
    use ibis_doc_model::{Color, Fill, Rgb};
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();

    // 初期状態: スタイル 2
    let c = chart(&mut s);
    assert_eq!(c.style, Some(2));

    // 1. スタイル 3 を適用、カラフル 1（インデックス 0）を適用
    s.update(|e, _, cx| {
        e.set_sheet_chart_style(Some(3), cx);
    });
    s.update(|e, _, cx| {
        e.set_sheet_chart_color_palette(0, cx);
    });
    let c = chart(&mut s);
    assert_eq!(c.style, Some(3));
    assert_eq!(c.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0x4472C4))));
    assert_eq!(c.groups[0].series[1].fill, Fill::Solid(Color::opaque(Rgb(0xED7D31))));

    // 2. 保存して開き直す（ラウンドトリップ検証）
    s.save_as("chart-style-palette-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.style, Some(3));
    assert_eq!(c.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0x4472C4))));
    assert_eq!(c.groups[0].series[1].fill, Fill::Solid(Color::opaque(Rgb(0xED7D31))));

    // 3. メニューから「スタイル 5」および「モノクロ 1」（palette-5）を適用
    s.menu("xlsx.chart-style-5");
    let c = chart(&mut s);
    assert_eq!(c.style, Some(5));

    s.menu("xlsx.chart-palette-5");
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0x2F5597))));
    assert_eq!(c.groups[0].series[1].fill, Fill::Solid(Color::opaque(Rgb(0x4472C4))));

    // 4. メニューから「スタイルなし」を適用
    s.menu("xlsx.chart-style-none");
    let c = chart(&mut s);
    assert_eq!(c.style, None);

    // 5. Undo 検証
    s.key("cmd-z"); // スタイルなし -> スタイル 5
    let c = chart(&mut s);
    assert_eq!(c.style, Some(5));

    s.key("cmd-z"); // モノクロ 1 -> カラフル 1
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0x4472C4))));

    s.key("cmd-z"); // スタイル 5 -> スタイル 3
    let c = chart(&mut s);
    assert_eq!(c.style, Some(3));

    // 6. Redo 検証
    s.key("cmd-shift-z"); // スタイル 3 -> スタイル 5
    let c = chart(&mut s);
    assert_eq!(c.style, Some(5));

    s.key("cmd-shift-z"); // カラフル 1 -> モノクロ 1
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0x2F5597))));

    s.key("cmd-shift-z"); // スタイル 5 -> スタイルなし
    let c = chart(&mut s);
    assert_eq!(c.style, None);
}

/// XLSX-CHEDIT-014: 系列のマーカー記号および線の太さの変更、保存・開き直し、Undo/Redo
fn chart014_series_marker_and_stroke() {
    use ibis_doc_model::chart::MarkerSymbol;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();

    // 初期状態: 系列 0 は marker なし、stroke なし（棒グラフ）
    let c = chart(&mut s);
    assert!(c.groups[0].series[0].marker.is_none());
    assert!(c.groups[0].series[0].stroke.is_none());

    // 1. 書式パネルからの操作: マーカーを「ひし形」(Diamond)、線を「太」(38100) に設定
    s.update(|e, _, cx| {
        e.set_sheet_chart_marker_symbol(MarkerSymbol::Diamond, cx);
    });
    s.update(|e, _, cx| {
        e.set_sheet_chart_line_stroke(Some(38_100), cx);
    });
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].marker.as_ref().map(|m| m.symbol), Some(MarkerSymbol::Diamond));
    assert_eq!(c.groups[0].series[0].stroke.as_ref().map(|st| st.width), Some(38_100));

    // 2. 保存して開き直す（ラウンドトリップ検証）
    s.save_as("chart-marker-stroke-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.groups[0].series[0].marker.as_ref().map(|m| m.symbol), Some(MarkerSymbol::Diamond));
    assert_eq!(c.groups[0].series[0].stroke.as_ref().map(|st| st.width), Some(38_100));

    // 3. マーカーを「円」(Circle)、線を「細」(12700) に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_marker_symbol(MarkerSymbol::Circle, cx);
    });
    s.update(|e, _, cx| {
        e.set_sheet_chart_line_stroke(Some(12_700), cx);
    });
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].marker.as_ref().map(|m| m.symbol), Some(MarkerSymbol::Circle));
    assert_eq!(c.groups[0].series[0].stroke.as_ref().map(|st| st.width), Some(12_700));

    // 4. マーカーを「なし」(None)、線を「なし」(None) に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_marker_symbol(MarkerSymbol::None, cx);
    });
    s.update(|e, _, cx| {
        e.set_sheet_chart_line_stroke(None, cx);
    });
    let c = chart(&mut s);
    assert!(c.groups[0].series[0].marker.is_none());
    assert!(c.groups[0].series[0].stroke.is_none());

    // 5. 元に戻す（Undo）
    s.key("cmd-z"); // stroke None -> 12700
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].stroke.as_ref().map(|st| st.width), Some(12_700));

    s.key("cmd-z"); // marker None -> Circle
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].marker.as_ref().map(|m| m.symbol), Some(MarkerSymbol::Circle));

    s.key("cmd-z"); // stroke 12700 -> 38100
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].stroke.as_ref().map(|st| st.width), Some(38_100));

    s.key("cmd-z"); // marker Circle -> Diamond
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].marker.as_ref().map(|m| m.symbol), Some(MarkerSymbol::Diamond));

    s.key("cmd-z"); // stroke 38100 -> None (初期値)
    let c = chart(&mut s);
    assert!(c.groups[0].series[0].stroke.is_none());

    s.key("cmd-z"); // marker Diamond -> None (初期値)
    let c = chart(&mut s);
    assert!(c.groups[0].series[0].marker.is_none());

    // 6. やり直し（Redo: Cmd+Shift+Z）
    s.key("cmd-shift-z"); // marker None -> Diamond
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].marker.as_ref().map(|m| m.symbol), Some(MarkerSymbol::Diamond));

    s.key("cmd-shift-z"); // stroke None -> 38100
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].stroke.as_ref().map(|st| st.width), Some(38_100));
}

/// XLSX-CHEDIT-009: 項目軸の軸の種類（テキスト軸と日付軸）の変更、保存・開き直し、Undo/Redo
fn chart009_date_axis() {
    use ibis_doc_model::chart::{AxisKind, AxisSourceKind};
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let cat_idx = chart(&mut s).axes.iter().position(|a| a.kind == AxisKind::Category && !a.deleted).unwrap();

    // 初期状態: date: false, source_kind: AxisSourceKind::Category
    let c = chart(&mut s);
    assert_eq!(c.axes[cat_idx].date, false);
    assert_eq!(c.axes[cat_idx].source_kind, AxisSourceKind::Category);

    // 1. 書式パネルからの操作: 日付軸（is_date: true）に設定
    s.update(|e, _, cx| {
        e.set_sheet_chart_cat_axis_date(true, cx);
    });
    let c = chart(&mut s);
    assert_eq!(c.axes[cat_idx].date, true);
    assert_eq!(c.axes[cat_idx].source_kind, AxisSourceKind::Date);

    // 2. 保存して開き直す（ラウンドトリップ検証）
    s.save_as("chart-date-axis-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.axes[cat_idx].date, true);
    assert_eq!(c.axes[cat_idx].source_kind, AxisSourceKind::Date);

    // 3. テキスト軸（is_date: false）に戻す
    s.update(|e, _, cx| {
        e.set_sheet_chart_cat_axis_date(false, cx);
    });
    let c = chart(&mut s);
    assert_eq!(c.axes[cat_idx].date, false);
    assert_eq!(c.axes[cat_idx].source_kind, AxisSourceKind::Category);

    // 4. 元に戻す（Undo）
    s.key("cmd-z"); // false -> true
    let c = chart(&mut s);
    assert_eq!(c.axes[cat_idx].date, true);

    s.key("cmd-z"); // true -> false (初期値)
    let c = chart(&mut s);
    assert_eq!(c.axes[cat_idx].date, false);

    // 5. やり直し（Redo: Cmd+Shift+Z）
    s.key("cmd-shift-z"); // false -> true
    let c = chart(&mut s);
    assert_eq!(c.axes[cat_idx].date, true);

    s.key("cmd-shift-z"); // true -> false
    let c = chart(&mut s);
    assert_eq!(c.axes[cat_idx].date, false);
}

/// XLSX-CHEDIT-011: 第2軸と系列軸（第2値軸・系列軸の設定・編集UI・保存・Undo/Redo）
fn chart011_secondary_and_series_axis() {
    use ibis_doc_model::chart::AxisKind;
    use ibis_doc_model::Number;

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/chart011-fixture.xlsx");
    if let Some(parent) = fixture.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let status = std::process::Command::new("python3").arg("-c").arg(r#"
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as source, zipfile.ZipFile(sys.argv[2], 'w') as output:
    for info in source.infolist():
        data = source.read(info.filename)
        if info.filename == 'xl/charts/chart1.xml':
            text = data.decode('utf-8')
            extra = '<c:valAx><c:axId val="1642411136"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="r"/><c:crossAx val="1658744191"/><c:crosses val="autoZero"/></c:valAx><c:serAx><c:axId val="1642411137"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="1658744191"/><c:crosses val="autoZero"/></c:serAx>'
            text = text.replace('</c:plotArea>', extra + '</c:plotArea>')
            data = text.encode('utf-8')
        output.writestr(info, data)
"#).arg(&root).arg(&fixture).status().unwrap();
    assert!(status.success(), "prepare generated chart011 fixture");

    let mut s = Scenario::open(fixture.to_str().unwrap());
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let sec_val_idx = chart(&mut s).axes.iter().position(|a| a.kind == AxisKind::Value && a.source_id == Some(1642411136)).unwrap();
    let ser_idx = chart(&mut s).axes.iter().position(|a| a.kind == AxisKind::Series && a.source_id == Some(1642411137)).unwrap();

    // 1. 書式パネルからの操作: 第2値軸の設定
    s.update(|e, _, cx| {
        e.set_sheet_chart_sec_val_axis_title_shown(true, cx);
        e.set_sheet_chart_sec_val_axis_gridlines(true, cx);
        e.set_sheet_chart_sec_val_axis_minor_gridlines(true, cx);
        e.set_sheet_chart_sec_val_axis_display_unit(Some(Number(1000.0)), cx);
        e.set_sheet_chart_sec_val_axis_reversed(true, cx);
        e.set_sheet_chart_sec_val_axis_log_scale(Some(Number(10.0)), cx);
    });

    // 2. 書式パネルからの操作: 系列軸の設定
    s.update(|e, _, cx| {
        e.set_sheet_chart_ser_axis_title_shown(true, cx);
        e.set_sheet_chart_ser_axis_gridlines(true, cx);
        e.set_sheet_chart_ser_axis_reversed(true, cx);
    });

    // 3. タイトルの入力と適用（apply_sheet_chart_panel）
    s.update(|e, window, cx| {
        if let Some(p) = e.wb().chart_edit.as_ref() {
            p.sec_val_title.update(cx, |st, cx| st.set_value("売上（千円）", window, cx));
            p.ser_title.update(cx, |st, cx| st.set_value("年系列", window, cx));
        }
        e.apply_sheet_chart_panel(cx);
    });

    // 設定内容の検証
    let c = chart(&mut s);
    let sec_ax = &c.axes[sec_val_idx];
    assert_eq!(sec_ax.title.as_ref().map(|t| t.text.as_str()), Some("売上（千円）"));
    assert!(sec_ax.gridlines.is_some());
    assert!(sec_ax.minor_gridlines.is_some());
    assert_eq!(sec_ax.display_unit, Some(Number(1000.0)));
    assert_eq!(sec_ax.reversed, true);
    assert_eq!(sec_ax.log_base, Some(Number(10.0)));

    let ser_ax = &c.axes[ser_idx];
    assert_eq!(ser_ax.title.as_ref().map(|t| t.text.as_str()), Some("年系列"));
    assert!(ser_ax.gridlines.is_some());
    assert_eq!(ser_ax.reversed, true);

    // 4. 保存して開き直す（ラウンドトリップ検証）
    s.save_as("chart-sec-ser-axis-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    let re_sec = &c.axes[sec_val_idx];
    assert_eq!(re_sec.title.as_ref().map(|t| t.text.as_str()), Some("売上（千円）"));
    assert!(re_sec.gridlines.is_some());
    assert!(re_sec.minor_gridlines.is_some());
    assert_eq!(re_sec.display_unit, Some(Number(1000.0)));
    assert_eq!(re_sec.reversed, true);
    assert_eq!(re_sec.log_base, Some(Number(10.0)));

    let re_ser = &c.axes[ser_idx];
    assert_eq!(re_ser.title.as_ref().map(|t| t.text.as_str()), Some("年系列"));
    assert!(re_ser.gridlines.is_some());
    assert_eq!(re_ser.reversed, true);

    // 5. Undo / Redo
    s.key("cmd-z"); // タイトル適用前の状態へ
    let c = chart(&mut s);
    assert_eq!(c.axes[sec_val_idx].title.as_ref().map(|t| t.text.as_str()), Some("軸ラベル"));
    assert_eq!(c.axes[ser_idx].title.as_ref().map(|t| t.text.as_str()), Some("軸ラベル"));

    s.key("cmd-shift-z"); // タイトル適用後に戻す
    let c = chart(&mut s);
    assert_eq!(c.axes[sec_val_idx].title.as_ref().map(|t| t.text.as_str()), Some("売上（千円）"));
    assert_eq!(c.axes[ser_idx].title.as_ref().map(|t| t.text.as_str()), Some("年系列"));

    let _ = std::fs::remove_file(fixture);
}

/// XLSX-CHEDIT-015: 塗り・輪郭・効果（系列の塗りつぶし色・枠線色の変更、保存・開き直し、Undo/Redo）
fn chart015_series_fill_and_outline() {
    use ibis_doc_model::{Color, Fill, Rgb};

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let initial_fill = chart(&mut s).groups[0].series[0].fill.clone();

    // 1. 書式パネルからの操作: 塗りつぶしを赤、枠線の色を黒に設定
    s.update(|e, _, cx| {
        e.set_sheet_chart_series_fill(Fill::Solid(Color::opaque(Rgb(0xC00000))), cx);
        e.set_sheet_chart_series_stroke_color(Some(Rgb(0x000000)), cx);
        e.set_sheet_chart_line_stroke(Some(38_100), cx); // 太 3pt
    });

    let c = chart(&mut s);
    let ser0 = &c.groups[0].series[0];
    assert_eq!(ser0.fill, Fill::Solid(Color::opaque(Rgb(0xC00000))));
    assert_eq!(ser0.stroke.as_ref().map(|st| st.color.rgb), Some(Rgb(0x000000)));
    assert_eq!(ser0.stroke.as_ref().map(|st| st.width), Some(38_100));

    // 2. 保存して開き直す（ラウンドトリップ検証）
    s.save_as("chart015-fill-and-outline.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    let ser0 = &c.groups[0].series[0];
    assert_eq!(ser0.fill, Fill::Solid(Color::opaque(Rgb(0xC00000))));
    assert_eq!(ser0.stroke.as_ref().map(|st| st.color.rgb), Some(Rgb(0x000000)));
    assert_eq!(ser0.stroke.as_ref().map(|st| st.width), Some(38_100));

    // 3. 塗りつぶし「なし」(Fill::None)、枠線の色「なし」(None) に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_series_fill(Fill::None, cx);
        e.set_sheet_chart_series_stroke_color(None, cx);
    });
    let c = chart(&mut s);
    let ser0 = &c.groups[0].series[0];
    assert_eq!(ser0.fill, Fill::None);
    assert!(ser0.stroke.is_none());

    // 4. Undo 検証
    s.key("cmd-z"); // 枠線なし -> 枠線あり
    let c = chart(&mut s);
    assert!(c.groups[0].series[0].stroke.is_some());

    s.key("cmd-z"); // 塗りつぶしなし -> 塗りつぶし赤
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0xC00000))));

    s.key("cmd-z"); // 枠線の太さ 3pt -> 元
    s.key("cmd-z"); // 枠線の色 黒 -> 元
    s.key("cmd-z"); // 塗りつぶし 赤 -> 初期状態
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].fill, initial_fill);

    // 5. Redo 検証
    s.key("cmd-shift-z"); // 初期状態 -> 赤
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0xC00000))));
}

/// XLSX-FORMULA-010: 配列定数（直接スピル入力、関数引数、保存・開き直し、Undo/Redo）
fn formula010_array_constants() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // 新規シートを追加してクリーンな環境でテスト
    s.menu("xlsx.insert-sheet");
    let inserted_name = s.with(|e, _| e.slide().name.clone());
    s.screenshot("formula010_sheet_inserted");

    // 1. 配列定数を直接セルに入力: A1 に ={1, 2; 3, 4}
    s.click_cell("A1");
    s.enter("={1, 2; 3, 4}", "enter");

    // スピル結果の検証: A1=1, B1=2, A2=3, B2=4
    assert_eq!(s.value("A1"), CellValue::Number(1.0));
    assert_eq!(s.value("B1"), CellValue::Number(2.0));
    assert_eq!(s.value("A2"), CellValue::Number(3.0));
    assert_eq!(s.value("B2"), CellValue::Number(4.0));
    s.with(|e, _| {
        let sh = sheet_of_slide(e.slide()).unwrap();
        assert!(sh.spills.contains_key(&(0, 0)));
        assert_eq!(sh.spills.get(&(0, 0)), Some(&ibis_doc_model::CellRange {
            first_row: 0,
            last_row: 1,
            first_column: 0,
            last_column: 1,
        }));
    });

    // 2. 配列定数を各種関数の引数として入力
    // D1 に =SUM({10, 20; 30, 40}) -> 100
    s.click_cell("D1");
    s.enter("=SUM({10, 20; 30, 40})", "enter");
    assert_eq!(s.value("D1"), CellValue::Number(100.0));

    // D2 に =INDEX({100, 200; 300, 400}, 2, 2) -> 400
    s.click_cell("D2");
    s.enter("=INDEX({100, 200; 300, 400}, 2, 2)", "enter");
    assert_eq!(s.value("D2"), CellValue::Number(400.0));

    // D3 に =AVERAGE({2, 4, 6}) -> 4
    s.click_cell("D3");
    s.enter("=AVERAGE({2, 4, 6})", "enter");
    assert_eq!(s.value("D3"), CellValue::Number(4.0));

    // D4 に =VLOOKUP(2, {1, "A"; 2, "B"; 3, "C"}, 2, FALSE) -> "B"
    s.click_cell("D4");
    s.enter("=VLOOKUP(2, {1, \"A\"; 2, \"B\"; 3, \"C\"}, 2, FALSE)", "enter");
    assert_eq!(s.value("D4"), CellValue::Text("B".into()));

    // 3. Undo / Redo 検証
    s.key("cmd-z"); // D4 入力取消
    assert_eq!(s.value("D4"), CellValue::Empty);
    s.key("cmd-shift-z"); // D4 やり直し
    assert_eq!(s.value("D4"), CellValue::Text("B".into()));

    // 4. 保存して開き直す（ラウンドトリップ検証）
    s.screenshot("formula010_before_save");
    s.save_as("formula010-array-constants.xlsx");
    let p = s.reopen();
    let slide = p.slides.iter().find(|sl| sl.name == inserted_name).unwrap();
    let sh = sheet_of_slide(slide).unwrap();
    assert_eq!(sh.cell(0, 0).unwrap().value, CellValue::Number(1.0));
    assert_eq!(sh.cell(0, 1).unwrap().value, CellValue::Number(2.0));
    assert_eq!(sh.cell(1, 0).unwrap().value, CellValue::Number(3.0));
    assert_eq!(sh.cell(1, 1).unwrap().value, CellValue::Number(4.0));
    assert_eq!(sh.spills.get(&(0, 0)), Some(&ibis_doc_model::CellRange {
        first_row: 0,
        last_row: 1,
        first_column: 0,
        last_column: 1,
    }));
    assert_eq!(sh.cell(0, 3).unwrap().value, CellValue::Number(100.0));
    assert_eq!(sh.cell(1, 3).unwrap().value, CellValue::Number(400.0));
    assert_eq!(sh.cell(2, 3).unwrap().value, CellValue::Number(4.0));
    assert_eq!(sh.cell(3, 3).unwrap().value, CellValue::Text("B".into()));
}

/// XLSX-FORMULA-005: 3D参照（複数シートに跨る参照・集計関数・カスケード再計算・保存・開き直し、Undo/Redo）
fn formula005_3d_references() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. 新規シートを3枚追加（すべて ASCII 名の新規シートとし、確実にタイピング・参照可能にする）
    s.menu("xlsx.insert-sheet");
    let name1 = s.with(|e, _| e.slide().name.clone()).unwrap();
    let slide1 = s.with(|e, _| e.slide().id);

    s.menu("xlsx.insert-sheet");
    let _name2 = s.with(|e, _| e.slide().name.clone()).unwrap();
    let slide2 = s.with(|e, _| e.slide().id);

    s.menu("xlsx.insert-sheet");
    let name3 = s.with(|e, _| e.slide().name.clone()).unwrap();
    let _slide3 = s.with(|e, _| e.slide().id);

    // 2. 各シートに値を入力
    // Sheet3 (現在アクティブ): A1=30, B1=3
    s.click_cell("A1");
    s.enter("30", "enter");
    s.click_cell("B1");
    s.enter("3", "enter");

    // Sheet2 に移動: A1=20, B1=2
    s.update(|e, window, cx| e.select_slide(slide2, window, cx));
    s.click_cell("A1");
    s.enter("20", "enter");
    s.click_cell("B1");
    s.enter("2", "enter");

    // Sheet1 に移動: A1=10, B1=1
    s.update(|e, window, cx| e.select_slide(slide1, window, cx));
    s.click_cell("A1");
    s.enter("10", "enter");
    s.click_cell("B1");
    s.enter("1", "enter");

    // 3. Sheet1 に 3D 参照数式を入力
    // C1: =SUM(Sheet1:Sheet3!A1) -> 60
    let f_sum = format!("=SUM('{name1}:{name3}'!A1)");
    s.click_cell("C1");
    s.enter(&f_sum, "enter");
    assert_eq!(s.value("C1"), CellValue::Number(60.0));

    // C2: =AVERAGE(Sheet1:Sheet3!A1) -> 20
    let f_avg = format!("=AVERAGE('{name1}:{name3}'!A1)");
    s.click_cell("C2");
    s.enter(&f_avg, "enter");
    assert_eq!(s.value("C2"), CellValue::Number(20.0));

    // C3: =MAX(Sheet1:Sheet3!A1) -> 30
    let f_max = format!("=MAX('{name1}:{name3}'!A1)");
    s.click_cell("C3");
    s.enter(&f_max, "enter");
    assert_eq!(s.value("C3"), CellValue::Number(30.0));

    // C4: =MIN(Sheet1:Sheet3!A1) -> 10
    let f_min = format!("=MIN('{name1}:{name3}'!A1)");
    s.click_cell("C4");
    s.enter(&f_min, "enter");
    assert_eq!(s.value("C4"), CellValue::Number(10.0));

    // C5: =SUM(Sheet1:Sheet3!A1:B1) -> (10+1)+(20+2)+(30+3) = 66
    let f_range = format!("=SUM('{name1}:{name3}'!A1:B1)");
    s.click_cell("C5");
    s.enter(&f_range, "enter");
    assert_eq!(s.value("C5"), CellValue::Number(66.0));

    // 4. Undo / Redo 検証
    s.key("cmd-z"); // C5 入力取消
    assert_eq!(s.value("C5"), CellValue::Empty);
    s.key("cmd-shift-z"); // C5 やり直し
    assert_eq!(s.value("C5"), CellValue::Number(66.0));

    // 5. カスケード再計算検証: Sheet2 の A1 を 20 から 50 に変更
    s.update(|e, window, cx| e.select_slide(slide2, window, cx));
    s.click_cell("A1");
    s.enter("50", "enter");

    s.update(|e, window, cx| e.select_slide(slide1, window, cx));
    assert_eq!(s.value("C1"), CellValue::Number(90.0), "C1 recalculated from 60 to 90");
    assert_eq!(s.value("C5"), CellValue::Number(96.0), "C5 recalculated from 66 to 96");

    // 6. 保存と開き直し（ラウンドトリップ検証）
    s.screenshot("formula005_before_save");
    s.save_as("formula005-3d-references.xlsx");
    let p = s.reopen();
    let re_s1 = p.slides.iter().find(|sl| sl.name.as_deref() == Some(&name1)).unwrap();
    let re_sh1 = sheet_of_slide(re_s1).unwrap();
    assert_eq!(re_sh1.cell(0, 2).unwrap().value, CellValue::Number(90.0));
    assert_eq!(re_sh1.cell(1, 2).unwrap().value, CellValue::Number(30.0)); // avg of 10, 50, 30 = 30
    assert_eq!(re_sh1.cell(2, 2).unwrap().value, CellValue::Number(50.0));
    assert_eq!(re_sh1.cell(3, 2).unwrap().value, CellValue::Number(10.0));
    assert_eq!(re_sh1.cell(4, 2).unwrap().value, CellValue::Number(96.0));
}

/// XLSX-FORMULA-002: 演算子優先順位（四則演算・括弧・べき乗・単項マイナス・パーセント・文字列結合・比較・参照連動・カスケード再計算・保存・開き直し、Undo/Redo）
fn formula002_operator_precedence() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. 新規シートを追加（ASCII名となり、確実に入力・検証可能）
    s.menu("xlsx.insert-sheet");
    let sheet_name = s.with(|e, _| e.slide().name.clone()).unwrap();

    // 2. 参照用セルを設定: A1=10, B1=2, C1=3
    s.click_cell("A1");
    s.enter("10", "enter");
    s.click_cell("B1");
    s.enter("2", "enter");
    s.click_cell("C1");
    s.enter("3", "enter");

    // 3. 各種演算子優先順位の数式を入力し、評価結果を検証
    // 3.1 四則演算と括弧
    s.click_cell("D1");
    s.enter("=1+2*3", "enter");
    assert_eq!(s.value("D1"), CellValue::Number(7.0), "1+2*3 = 7");

    s.click_cell("D2");
    s.enter("=(1+2)*3", "enter");
    assert_eq!(s.value("D2"), CellValue::Number(9.0), "(1+2)*3 = 9");

    s.click_cell("D3");
    s.enter("=10-4/2", "enter");
    assert_eq!(s.value("D3"), CellValue::Number(8.0), "10-4/2 = 8");

    // 3.2 べき乗と符号
    s.click_cell("D4");
    s.enter("=2*3^2", "enter");
    assert_eq!(s.value("D4"), CellValue::Number(18.0), "2*3^2 = 18");

    s.click_cell("D5");
    s.enter("=-2^2", "enter");
    assert_eq!(s.value("D5"), CellValue::Number(4.0), "-2^2 = 4 (Excel unary minus binds tighter than power)");

    s.click_cell("D6");
    s.enter("=-(2^2)", "enter");
    assert_eq!(s.value("D6"), CellValue::Number(-4.0), "-(2^2) = -4");

    s.click_cell("D7");
    s.enter("=2^3^2", "enter");
    assert_eq!(s.value("D7"), CellValue::Number(64.0), "2^3^2 = 64 (left-associative: (2^3)^2)");

    // 3.3 パーセント
    s.click_cell("D8");
    s.enter("=50%*200", "enter");
    assert_eq!(s.value("D8"), CellValue::Number(100.0), "50%*200 = 100");

    s.click_cell("D9");
    s.enter("=10+50%", "enter");
    assert_eq!(s.value("D9"), CellValue::Number(10.5), "10+50% = 10.5");

    // 3.4 文字列連結
    s.click_cell("D10");
    s.enter("=1+2&3+4", "enter");
    assert_eq!(s.value("D10"), CellValue::Text("37".into()), "1+2&3+4 = '37'");

    // 3.5 比較演算子
    s.click_cell("D11");
    s.enter("=1+2=3", "enter");
    assert_eq!(s.value("D11"), CellValue::Bool(true), "1+2=3 = true");

    s.click_cell("D12");
    s.enter("=1+2*3=7", "enter");
    assert_eq!(s.value("D12"), CellValue::Bool(true), "1+2*3=7 = true");

    s.click_cell("D13");
    s.enter("=1&2=\"12\"", "enter");
    assert_eq!(s.value("D13"), CellValue::Bool(true), "1&2='12' = true");

    s.click_cell("D14");
    s.enter("=1+2<2*2", "enter");
    assert_eq!(s.value("D14"), CellValue::Bool(true), "1+2<2*2 = true");

    // 3.6 セル参照連動
    s.click_cell("D15");
    s.enter("=A1+B1*C1", "enter");
    assert_eq!(s.value("D15"), CellValue::Number(16.0), "A1+B1*C1 = 16");

    s.click_cell("D16");
    s.enter("=(A1+B1)*C1", "enter");
    assert_eq!(s.value("D16"), CellValue::Number(36.0), "(A1+B1)*C1 = 36");

    s.click_cell("D17");
    s.enter("=-B1^2", "enter");
    assert_eq!(s.value("D17"), CellValue::Number(4.0), "-B1^2 = 4");

    s.click_cell("D18");
    s.enter("=A1&B1", "enter");
    assert_eq!(s.value("D18"), CellValue::Text("102".into()), "A1&B1 = '102'");

    s.click_cell("D19");
    s.enter("=A1>B1*C1", "enter");
    assert_eq!(s.value("D19"), CellValue::Bool(true), "A1>B1*C1 = true");

    // 4. Undo / Redo 検証
    s.key("cmd-z"); // D19 入力取消
    assert_eq!(s.value("D19"), CellValue::Empty);
    s.key("cmd-shift-z"); // D19 やり直し
    assert_eq!(s.value("D19"), CellValue::Bool(true));

    // 5. カスケード再計算検証: B1 を 2 から 5 に変更
    s.click_cell("B1");
    s.enter("5", "enter");
    assert_eq!(s.value("D15"), CellValue::Number(25.0), "D15 cascades to 10+5*3 = 25");
    assert_eq!(s.value("D16"), CellValue::Number(45.0), "D16 cascades to (10+5)*3 = 45");
    assert_eq!(s.value("D17"), CellValue::Number(25.0), "D17 cascades to (-5)^2 = 25");
    assert_eq!(s.value("D19"), CellValue::Bool(false), "D19 cascades to 10>15 = false");

    // 6. 保存と開き直し（ラウンドトリップ検証）
    s.screenshot("formula002_before_save");
    s.save_as("formula002-operator-precedence.xlsx");
    let p = s.reopen();
    let re_s = p.slides.iter().find(|sl| sl.name.as_deref() == Some(&sheet_name)).unwrap();
    let re_sh = sheet_of_slide(re_s).unwrap();
    assert_eq!(re_sh.cell(0, 3).unwrap().value, CellValue::Number(7.0));
    assert_eq!(re_sh.cell(3, 3).unwrap().value, CellValue::Number(18.0));
    assert_eq!(re_sh.cell(4, 3).unwrap().value, CellValue::Number(4.0));
    assert_eq!(re_sh.cell(6, 3).unwrap().value, CellValue::Number(64.0));
    assert_eq!(re_sh.cell(9, 3).unwrap().value, CellValue::Text("37".into()));
    assert_eq!(re_sh.cell(10, 3).unwrap().value, CellValue::Bool(true));
    assert_eq!(re_sh.cell(14, 3).unwrap().value, CellValue::Number(25.0));
    assert_eq!(re_sh.cell(15, 3).unwrap().value, CellValue::Number(45.0));
    assert_eq!(re_sh.cell(16, 3).unwrap().value, CellValue::Number(25.0));
    assert_eq!(re_sh.cell(18, 3).unwrap().value, CellValue::Bool(false));
}

/// XLSX-FORMULA-013: 暗黙の交差演算子（@ による単一値取得・同一行/同一列の交差・交差外 #VALUE!・配列スピル抑制・カスケード再計算・保存と _xlfn.SINGLE 変換・開き直し・Undo/Redo）
fn formula013_implicit_intersection() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. 新規シートを追加（ASCII名となり、確実に入力・検証可能）
    s.menu("xlsx.insert-sheet");
    let sheet_name = s.with(|e, _| e.slide().name.clone()).unwrap();

    // 2. データセルを設定
    // 縦方向データ A1:A5 (10, 20, 30, 40, 50)
    s.click_cell("A1");
    s.enter("10", "enter");
    s.click_cell("A2");
    s.enter("20", "enter");
    s.click_cell("A3");
    s.enter("30", "enter");
    s.click_cell("A4");
    s.enter("40", "enter");
    s.click_cell("A5");
    s.enter("50", "enter");

    // 横方向データ C1:E1 (100, 200, 300)
    s.click_cell("C1");
    s.enter("100", "enter");
    s.click_cell("D1");
    s.enter("200", "enter");
    s.click_cell("E1");
    s.enter("300", "enter");

    // 3. 暗黙の交差数式を入力し、評価結果を検証
    // 3.1 列範囲の行交差
    s.click_cell("B1");
    s.enter("=@A1:A5", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(10.0), "B1: @A1:A5 intersects at row 1 = 10");

    s.click_cell("B2");
    s.enter("=@A1:A5", "enter");
    assert_eq!(s.value("B2"), CellValue::Number(20.0), "B2: @A1:A5 intersects at row 2 = 20");

    s.click_cell("B3");
    s.enter("=@A1:A5", "enter");
    assert_eq!(s.value("B3"), CellValue::Number(30.0), "B3: @A1:A5 intersects at row 3 = 30");

    s.click_cell("B5");
    s.enter("=@A1:A5", "enter");
    assert_eq!(s.value("B5"), CellValue::Number(50.0), "B5: @A1:A5 intersects at row 5 = 50");

    s.click_cell("B10");
    s.enter("=@A1:A5", "enter");
    assert_eq!(s.value("B10"), CellValue::Error(ibis_doc_model::CellError::Value), "B10: row 10 out of range = #VALUE!");

    // 3.2 行範囲の列交差 (3行目)
    s.click_cell("C3");
    s.enter("=@C1:E1", "enter");
    assert_eq!(s.value("C3"), CellValue::Number(100.0), "C3: @C1:E1 intersects at col C = 100");

    s.click_cell("D3");
    s.enter("=@C1:E1", "enter");
    assert_eq!(s.value("D3"), CellValue::Number(200.0), "D3: @C1:E1 intersects at col D = 200");

    s.click_cell("E3");
    s.enter("=@C1:E1", "enter");
    assert_eq!(s.value("E3"), CellValue::Number(300.0), "E3: @C1:E1 intersects at col E = 300");

    s.click_cell("F3");
    s.enter("=@C1:E1", "enter");
    assert_eq!(s.value("F3"), CellValue::Error(ibis_doc_model::CellError::Value), "F3: col F out of range = #VALUE!");

    // 3.3 配列定数に対する @（スピル抑制・先頭要素取得）
    s.click_cell("G1");
    s.enter("=@{1000, 2000; 3000, 4000}", "enter");
    assert_eq!(s.value("G1"), CellValue::Number(1000.0), "G1: @array extracts top-left 1000 without spill");

    // 3.4 算術連動
    s.click_cell("H2");
    s.enter("=10+@A1:A5", "enter");
    assert_eq!(s.value("H2"), CellValue::Number(30.0), "H2: 10 + @A1:A5 = 10 + 20 = 30");

    s.click_cell("H3");
    s.enter("=@A1:A5*2", "enter");
    assert_eq!(s.value("H3"), CellValue::Number(60.0), "H3: @A1:A5 * 2 = 30 * 2 = 60");

    // 4. Undo / Redo 検証
    s.key("cmd-z"); // H3 入力取消
    assert_eq!(s.value("H3"), CellValue::Empty);
    s.key("cmd-shift-z"); // H3 やり直し
    assert_eq!(s.value("H3"), CellValue::Number(60.0));

    // 5. カスケード再計算検証: A2 を 20 から 99 に変更
    s.click_cell("A2");
    s.enter("99", "enter");
    assert_eq!(s.value("B2"), CellValue::Number(99.0), "B2 cascades to 99");
    assert_eq!(s.value("H2"), CellValue::Number(109.0), "H2 cascades to 109");

    // 6. 保存と開き直し（ラウンドトリップ検証）
    s.screenshot("formula013_before_save");
    s.save_as("formula013-implicit-intersection.xlsx");
    let p = s.reopen();
    let re_s = p.slides.iter().find(|sl| sl.name.as_deref() == Some(&sheet_name)).unwrap();
    let re_sh = sheet_of_slide(re_s).unwrap();
    assert_eq!(re_sh.cell(0, 1).unwrap().formula.as_deref(), Some("@A1:A5"));
    assert_eq!(re_sh.cell(0, 1).unwrap().value, CellValue::Number(10.0));
    assert_eq!(re_sh.cell(1, 1).unwrap().value, CellValue::Number(99.0));
    assert_eq!(re_sh.cell(2, 2).unwrap().formula.as_deref(), Some("@C1:E1"));
    assert_eq!(re_sh.cell(2, 2).unwrap().value, CellValue::Number(100.0));
    assert_eq!(re_sh.cell(0, 6).unwrap().value, CellValue::Number(1000.0));
    assert_eq!(re_sh.cell(1, 7).unwrap().value, CellValue::Number(109.0));
}

/// データの範囲・行/列の切り替え: the chart's series turned to rows (5 series
/// of 3 points), saved with their references and read back, then undone.
fn xsc27_chart_data() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    let series = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.groups.iter().map(|g| g.series.len()).sum::<usize>())).unwrap();
    assert_eq!(series(&mut s), 3);
    assert_eq!(s.with(|e, cx| e.wb().chart_edit.as_ref().unwrap().range.read(cx).value().to_string()), "月次売上!$A$3:$D$8");
    s.update(|e, window, cx| e.set_sheet_chart_data(true, window, cx));
    assert_eq!(series(&mut s), 5);
    s.save_as("xsc27-chart-data.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap();
    let ibis_doc_model::ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.groups.iter().map(|g| g.series.len()).sum::<usize>(), 5);
    // A range typed: the first two months of the first four shops, in columns.
    s.update(|e, window, cx| {
        let input = e.wb().chart_edit.as_ref().unwrap().range.clone();
        input.update(cx, |i, cx| i.set_value("月次売上!$A$3:$C$7", window, cx));
        e.set_sheet_chart_data(false, window, cx);
    });
    assert_eq!(series(&mut s), 2);
    s.key("cmd-z");
    s.key("cmd-z");
    s.click(at);
    assert_eq!(s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap(), chart_of_file());
}

/// XLSX-CELL-005: text with parts in their own fonts (bold, colour, italic,
/// size, underline, strike) is read, drawn and kept by a save.
fn cell005_rich_text() {
    let mut s = Scenario::open("test/editing/xlsx-rich-text.xlsx");
    let runs = s.with(|e, _| e.current_sheet().and_then(|(_, sheet)| sheet.cell(3, 0).and_then(|c| c.runs.clone())));
    assert_eq!(runs.map(|r| r.len()), Some(2));
    s.screenshot("rich-text");
    s.save_as("cell005-rich.xlsx");
    let saved = s.reopen();
    let sheet = saved.slides[0].shapes.iter().find_map(|x| match &x.kind { ibis_doc_model::ShapeKind::Sheet(sh) => Some(sh.clone()), _ => None }).unwrap();
    assert_eq!(sheet.cell(4, 0).and_then(|c| c.runs.as_ref().map(|r| r.len())), Some(3));
}

/// XLSX-CELL-005: a cell with parts in their own fonts, wrapped: the parts
/// follow the text onto each line (the row is made tall enough to show them).
fn cell005_rich_text_wrapped() {
    let mut s = Scenario::open("test/editing/xlsx-rich-text.xlsx");
    s.click_cell("A4");
    s.update(|e, _, cx| e.format_cells(ibis_doc_model::StyleChange::Wrap(true), cx));
    let slide = s.with(|e, _| e.current_slide);
    s.update(move |e, _, cx| e.run_sheet(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetSize { slide, axis: ibis_doc_model::sheet_command::Axis::Rows, first: 3, last: 3, size: 60 * 12_700 })], cx));
    s.screenshot("wrapped");
    assert!(s.with(|e, _| e.current_sheet().and_then(|(_, sheet)| sheet.cell(3, 0).map(|c| c.runs.is_some()))).unwrap());
}

/// XLSX-CELL-005: part of the text of a cell made bold, coloured and larger
/// while it is edited (Shift+← selects, ⌘B, the colour and size), kept
/// through the Enter that finishes the entry, a save and reading it back.
fn cell005_rich_text_edit() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("A6");
    s.key("f2");
    s.key("shift-left");
    s.key("cmd-b");
    s.update(|e, _, cx| {
        e.format_cells(ibis_doc_model::StyleChange::Color(ibis_doc_model::Rgb(0xC00000)), cx);
        e.format_cells(ibis_doc_model::StyleChange::FontSize(1600), cx);
    });
    s.screenshot("rich-edit");
    let runs = s.with(|e, _| e.cell_edit().and_then(|c| c.runs.clone())).expect("runs while editing");
    assert_eq!(runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["東町", "店"]);
    assert_eq!((runs[0].bold, runs[1].bold, runs[1].color, runs[1].size), (Some(false), Some(true), Some(ibis_doc_model::Rgb(0xC00000)), Some(1600)));
    // Typing replaces the selection; here the caret is after it: a letter takes the font of the part before it.
    s.key("enter");
    let kept = s.with(|e, _| e.current_sheet().and_then(|(_, sheet)| sheet.cell(5, 0).and_then(|c| c.runs.clone()))).expect("runs kept");
    assert_eq!(kept.len(), 2);
    s.save_as("cell005-rich-edit.xlsx");
    let saved = s.reopen();
    let sheet = saved.slides[0].shapes.iter().find_map(|x| match &x.kind { ibis_doc_model::ShapeKind::Sheet(sh) => Some(sh.clone()), _ => None }).unwrap();
    let read = sheet.cell(5, 0).and_then(|c| c.runs.clone()).expect("runs read back");
    assert_eq!((read.len(), read[1].bold, read[1].size), (2, Some(true), Some(1600)));
    s.key("cmd-z");
    assert!(s.with(|e, _| e.current_sheet().and_then(|(_, sheet)| sheet.cell(5, 0).map(|c| c.runs.is_none()))).unwrap());
}

/// XLSX-PIVOT-005〜008: the layout (表形式), the subtotals, the grand totals,
/// 項目ラベルを繰り返す and the captions of empty values, set on a pivot
/// table, drawn and saved.
fn pivot005_design() {
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");
    s.click_cell("A5");
    s.menu("xlsx.pivot-fields");
    let target = s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).and_then(|p| p.target)).expect("its fields shown");
    s.screenshot("compact");
    s.update(move |e, _, cx| e.change_pivot(target, |p| {
        p.compact = false;
        p.outline = false;
        for f in &mut p.fields { f.compact = false; f.outline = false; }
        p.fill_down_labels = true;
        p.missing_caption = Some("-".into());
        p.row_grand_totals = false;
    }, cx));
    s.screenshot("tabular");
    let dump = |s: &mut Scenario| -> Vec<String> { (0..14).map(|r| (0..6).map(|c| s.with(|e, _| e.current_sheet().and_then(|(_, sh)| sh.cell(r, c).map(|x| x.text.clone()))).unwrap_or_default()).collect::<Vec<_>>().join("|")).collect() };
    // 小計を表示しない: the rows of 集計 go.
    assert!(dump(&mut s).iter().any(|l| l.contains("集計")));
    s.update(move |e, _, cx| e.change_pivot(target, |p| { for f in &mut p.fields { f.default_subtotal = false; } }, cx));
    assert!(!dump(&mut s).iter().any(|l| l.contains("集計")), "no subtotals");
    s.save_as("pivot005.xlsx");
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    let p = &sh.pivots[0];
    assert!(!p.compact && !p.outline && p.fill_down_labels && !p.row_grand_totals);
    assert_eq!(p.missing_caption.as_deref(), Some("-"));
}

/// XLSX-WHATIF-001: データ > ゴールシーク: the 4月 sales (B4) that make the
/// shop's total (E4, =SUM(B4:D4)) 4,000,000, shown first, then applied and undone.
fn whatif001_goal_seek() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("E4");
    s.menu("xlsx.goal-seek");
    s.update(|e, window, cx| {
        let p = e.wb().panels.goal_seek.as_ref().unwrap();
        let (target, changing) = (p.target.clone(), p.changing.clone());
        target.update(cx, |i, cx| i.set_value("4000000", window, cx));
        changing.update(cx, |i, cx| i.set_value("B4", window, cx));
        e.run_goal_seek(cx);
    });
    s.screenshot("goal-seek");
    let found = s.with(|e, _| e.wb().panels.goal_seek.as_ref().and_then(|p| p.found)).expect("a solution");
    assert_eq!(found.0, (3, 1));
    assert!((found.1 - 1_500_000.0).abs() < 0.01, "{}", found.1);
    s.update(|e, _, cx| e.apply_goal_seek(cx));
    assert_eq!(s.value("E4"), CellValue::Number(4_000_000.0));
    s.save_as("whatif001.xlsx");
    s.key("cmd-z");
    assert_eq!(s.value("B4"), CellValue::Number(1_250_000.0));
    // No such value: a message, nothing changed.
    s.menu("xlsx.goal-seek");
    s.update(|e, window, cx| {
        let p = e.wb().panels.goal_seek.as_ref().unwrap();
        let (formula, target, changing) = (p.formula.clone(), p.target.clone(), p.changing.clone());
        formula.update(cx, |i, cx| i.set_value("B4", window, cx));
        target.update(cx, |i, cx| i.set_value("5", window, cx));
        changing.update(cx, |i, cx| i.set_value("C4", window, cx));
        e.run_goal_seek(cx);
    });
    assert!(s.with(|e, _| e.wb().panels.goal_seek.as_ref().map(|p| p.found.is_none() && !p.message.is_empty())).unwrap());
}

/// XLSX-WHATIF-002・003: データ > データテーブル: the 4月 and 5月 sales
/// substituted in a total's formula, a 2 × 2 table made from the selection,
/// shown with TABLE in the bar, kept up to date, saved and read back.
fn whatif002_data_table() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // The corner H3 =B4+C4 (4月 + 5月 of 北町店); values across I3:J3 (×1, ×2), down H4:H5 (+0, +100000), the inputs two free cells: L1, L2.
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let num = |v: f64| ibis_doc_model::CellInput::Value(CellValue::Number(v));
        let edits = [(0, 11, num(1.0)), (1, 11, num(0.0)), (2, 7, ibis_doc_model::CellInput::Formula("B4*L1+L2".into())), (2, 8, num(1.0)), (2, 9, num(2.0)), (3, 7, num(0.0)), (4, 7, num(100_000.0))]
            .into_iter()
            .map(|(row, column, input)| ibis_doc_model::CellEdit { row, column, input })
            .collect();
        e.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetCells { slide, edits })]).unwrap();
        cx.notify();
    });
    s.click_cell("H3");
    for _ in 0..2 {
        s.key("shift-down");
    }
    for _ in 0..2 {
        s.key("shift-right");
    }
    s.menu("xlsx.data-table");
    s.update(|e, window, cx| {
        let p = e.wb().panels.data_table.as_ref().unwrap();
        let (row, column) = (p.row_input.clone(), p.column_input.clone());
        row.update(cx, |i, cx| i.set_value("L1", window, cx));
        column.update(cx, |i, cx| i.set_value("L2", window, cx));
        e.apply_data_table(cx);
    });
    assert_eq!(s.value("I4"), CellValue::Number(1_250_000.0));
    assert_eq!(s.value("J4"), CellValue::Number(2_500_000.0));
    assert_eq!(s.value("J5"), CellValue::Number(2_600_000.0));
    s.click_cell("I4");
    s.screenshot("data-table");
    // Changed: the results follow; a result is not changed on its own.
    s.click_cell("B4");
    s.type_text("1000000");
    s.key("enter");
    assert_eq!(s.value("I4"), CellValue::Number(1_000_000.0));
    s.click_cell("I4");
    s.type_text("5");
    s.key("enter");
    assert_eq!(s.value("I4"), CellValue::Number(1_000_000.0), "refused");
    s.save_as("whatif002.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap();
    assert_eq!(sheet.data_tables.len(), 1);
    assert_eq!(sheet.data_tables[0].row_input, Some((0, 11)));
}

fn scenario_manager_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.scenario_manager.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("scenario manager button not found: {id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}

fn scenario_manager_field(s: &mut Scenario, id: &str, text: &str) {
    scenario_manager_click(s, id);
    s.key("cmd-a");
    s.type_text(text);
}

/// XLSX-WHATIF-004・005・007 データ > What-If 分析 > シナリオの登録と管理:
/// 1. Create initial sheet with formula:
///    - B1 = 100 (単価)
///    - B2 = 0.2 (成長率)
///    - B3 = =B1*(1+B2) (売上予測: 120)
/// 2. Register scenario "好景気" (B1: 150, B2: 0.25, comment: "好景気時")
/// 3. Register scenario "不景気" (B1: 80, B2: 0.15, comment: "不景気時")
/// 4. Show scenario "好景気":
///    - B1 becomes 150, B2 becomes 0.25, B3 evaluates to 187.5
/// 5. Undo / Redo:
///    - Undo reverts B1->100, B2->0.2, B3->120
///    - Redo brings back B1->150, B2->0.25, B3->187.5
/// 6. Show scenario "不景気":
///    - B1 becomes 80, B2 becomes 0.15, B3 evaluates to 92
/// 7. Edit scenario "不景気":
///    - Change B1 to 70
///    - Show: B1 becomes 70, B3 evaluates to 80.5
/// 8. Scenario Summary Report:
///    - Result cells "B3"
///    - Generates new worksheet "シナリオの要約"
///    - Contains title, table headers ("現在の値", "好景気", "不景気"), input values and evaluated results
/// 9. Save and reopen:
///    - Verifies OOXML <scenarios> persisted with 2 scenarios
/// 10. Delete scenario "好景気":
///    - Deletes scenario from list, undo restores it
fn whatif004_scenarios() {
    let mut s = new_sheet();
    s.click_cell("B1");
    s.enter("100", "enter");
    s.click_cell("B2");
    s.enter("0.2", "enter");
    s.click_cell("B3");
    s.enter("=B1*(1+B2)", "enter");
    assert_eq!(s.value("B3"), CellValue::Number(120.0));

    // Open Scenario Manager with B1:B2 selected
    s.click_cell("B1");
    s.key("shift-down");
    s.menu("xlsx.scenario-manager");

    // Add first scenario: 好景気
    scenario_manager_click(&mut s, "scenario-add");
    scenario_manager_field(&mut s, "scenario-name", "好景気");
    scenario_manager_field(&mut s, "scenario-val-B1", "150");
    scenario_manager_field(&mut s, "scenario-val-B2", "0.25");
    scenario_manager_field(&mut s, "scenario-comment", "好景気時");
    scenario_manager_click(&mut s, "scenario-save");

    // Add second scenario: 不景気
    scenario_manager_click(&mut s, "scenario-add");
    scenario_manager_field(&mut s, "scenario-name", "不景気");
    scenario_manager_field(&mut s, "scenario-val-B1", "80");
    scenario_manager_field(&mut s, "scenario-val-B2", "0.15");
    scenario_manager_field(&mut s, "scenario-comment", "不景気時");
    scenario_manager_click(&mut s, "scenario-save");

    // Show "好景気" (item 0)
    scenario_manager_click(&mut s, "scenario-item-0");
    scenario_manager_click(&mut s, "scenario-show");
    assert_eq!(s.value("B1"), CellValue::Number(150.0));
    assert_eq!(s.value("B2"), CellValue::Number(0.25));
    assert_eq!(s.value("B3"), CellValue::Number(187.5));

    // Focus sheet, then Undo: reverts back to initial values (100, 0.2, 120)
    s.click_cell("E1");
    s.key("cmd-z");
    assert_eq!(s.value("B1"), CellValue::Number(100.0));
    assert_eq!(s.value("B2"), CellValue::Number(0.2));
    assert_eq!(s.value("B3"), CellValue::Number(120.0));

    // Redo: returns to "好景気" (150, 0.25, 187.5)
    s.key("cmd-shift-z");
    assert_eq!(s.value("B1"), CellValue::Number(150.0));
    assert_eq!(s.value("B2"), CellValue::Number(0.25));
    assert_eq!(s.value("B3"), CellValue::Number(187.5));

    // Show "不景気" (item 1)
    scenario_manager_click(&mut s, "scenario-item-1");
    scenario_manager_click(&mut s, "scenario-show");
    assert_eq!(s.value("B1"), CellValue::Number(80.0));
    assert_eq!(s.value("B2"), CellValue::Number(0.15));
    assert_eq!(s.value("B3"), CellValue::Number(92.0));

    // Edit "不景気" (item 1): change B1 to 70
    scenario_manager_click(&mut s, "scenario-edit");
    scenario_manager_field(&mut s, "scenario-val-B1", "70");
    scenario_manager_click(&mut s, "scenario-save");
    scenario_manager_click(&mut s, "scenario-show");
    assert_eq!(s.value("B1"), CellValue::Number(70.0));
    assert_eq!(s.value("B2"), CellValue::Number(0.15));
    assert_eq!(s.value("B3"), CellValue::Number(80.5));

    // Generate Scenario Summary Report (結果セル: B3)
    scenario_manager_click(&mut s, "scenario-summary");
    scenario_manager_field(&mut s, "scenario-result-cells", "B3");
    scenario_manager_click(&mut s, "scenario-run-summary");

    // Verify summary sheet created with correct cells
    let sum_slide = s.with(|e, _| {
        e.presentation().slides.iter().find(|sl| sl.name.as_deref() == Some("シナリオの要約")).cloned()
    }).expect("summary sheet slide");
    let sum_sheet = sheet_of_slide(&sum_slide).expect("summary sheet");
    assert_eq!(sum_sheet.cell(1, 1).unwrap().text, "シナリオの要約");
    assert_eq!(sum_sheet.cell(3, 2).unwrap().text, "現在の値");
    assert_eq!(sum_sheet.cell(3, 3).unwrap().text, "好景気");
    assert_eq!(sum_sheet.cell(3, 4).unwrap().text, "不景気");
    // Changing cells section
    assert_eq!(sum_sheet.cell(5, 1).unwrap().text, "$B$1");
    assert_eq!(sum_sheet.cell(5, 3).unwrap().value, CellValue::Number(150.0));
    assert_eq!(sum_sheet.cell(5, 4).unwrap().value, CellValue::Number(70.0));
    // Result cells section
    assert_eq!(sum_sheet.cell(8, 1).unwrap().text, "$B$3");
    assert_eq!(sum_sheet.cell(8, 3).unwrap().value, CellValue::Number(187.5));
    assert_eq!(sum_sheet.cell(8, 4).unwrap().value, CellValue::Number(80.5));

    // Save and reopen: verify OOXML <scenarios> persistence
    let orig_name = s.with(|e, _| e.slide().name.clone().unwrap_or_default());
    s.save_as("whatif004.xlsx");
    let reopened = s.reopen();
    assert!(reopened.slides.iter().any(|sl| sl.name.as_deref() == Some("シナリオの要約")));
    let orig = reopened.slides.iter().find(|sl| sl.name.as_deref() == Some(&orig_name)).unwrap();
    let orig_sheet = sheet_of_slide(orig).unwrap();
    let scs = orig_sheet.scenarios.as_ref().expect("scenarios saved");
    assert_eq!(scs.scenarios.len(), 2);
    assert_eq!(scs.scenarios[0].name, "好景気");
    assert_eq!(scs.scenarios[1].name, "不景気");

    // Delete scenario "好景気" (item 0)
    scenario_manager_click(&mut s, "scenario-item-0");
    scenario_manager_click(&mut s, "scenario-del");
    let remaining_scenarios = s.with(|e, _| {
        e.current_sheet().and_then(|(_, sh)| sh.scenarios.as_ref().map(|sc| sc.scenarios.len())).unwrap_or(0)
    });
    assert_eq!(remaining_scenarios, 1);

    // Focus sheet, then Undo delete restores scenario
    s.click_cell("E1");
    s.key("cmd-z");
    let restored_scenarios = s.with(|e, _| {
        e.current_sheet().and_then(|(_, sh)| sh.scenarios.as_ref().map(|sc| sc.scenarios.len())).unwrap_or(0)
    });
    assert_eq!(restored_scenarios, 2);

    // Close panel
    scenario_manager_click(&mut s, "scenario-close");
    assert!(s.with(|e, _| e.wb().panels.scenario_manager.is_none()));
}

/// XLSX-SPARK-005, XLSX-SPARK-006, XLSX-SPARK-007, XLSX-SPARK-023:
/// スパークラインの作成（折れ線・縦棒・勝敗）、グループ書式変更、単一スパークライン元範囲変更、
/// グループ解除・再グループ化、消去、Undo/Redo、および保存・再読み込みの検証。
fn spark001_create_edit_clear() {
    let mut s = new_sheet();

    // 1. Enter data in A1:C3
    s.click_cell("A1");
    s.enter("10", "tab");
    s.enter("20", "tab");
    s.enter("30", "enter");

    s.click_cell("A2");
    s.enter("15", "tab");
    s.enter("-5", "tab");
    s.enter("25", "enter");

    s.click_cell("A3");
    s.enter("20", "tab");
    s.enter("10", "tab");
    s.enter("-10", "enter");

    // 2. Select D1 and open sparkline panel via menu "xlsx.sparkline-line"
    s.click_cell("D1");
    s.menu("xlsx.sparkline-line");

    // Verify sparkline panel opened
    s.with(|e, _| {
        assert!(e.sheet_state().unwrap().panels.sparkline.is_some());
    });

    // Set data range to A1:C3 and location range to D1:D3, enable markers
    s.update(|e, window, cx| {
        let p = e.wb_mut().panels.sparkline.as_mut().unwrap();
        p.data_range_input.update(cx, |i, cx| i.set_value("A1:C3".to_string(), window, cx));
        p.location_range_input.update(cx, |i, cx| i.set_value("D1:D3".to_string(), window, cx));
        p.high = true;
        p.low = true;
        p.negative_points = true;
        p.markers = true;
        e.apply_sparkline(cx);
    });

    // Verify 1 sparkline group with 3 sparklines (D1, D2, D3)
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines.len(), 1);
        let g = &sheet.sparklines[0];
        assert_eq!(g.kind, ibis_doc_model::SparklineKind::Line);
        assert_eq!(g.sparklines.len(), 3);
        assert_eq!((g.sparklines[0].row, g.sparklines[0].column), (0, 3));
        assert_eq!((g.sparklines[1].row, g.sparklines[1].column), (1, 3));
        assert_eq!((g.sparklines[2].row, g.sparklines[2].column), (2, 3));
        assert!(g.high);
        assert!(g.low);
        assert!(g.negative_points);
        assert!(g.markers);
    });

    // 3. Focus sheet, test Undo and Redo
    s.click_cell("E1");
    s.key("cmd-z");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines.len(), 0);
    });

    s.key("cmd-shift-z");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines.len(), 1);
        assert_eq!(sheet.sparklines[0].sparklines.len(), 3);
    });

    // 4. Change group kind to Column: select D2, open sparkline panel, switch to Column
    s.click_cell("D2");
    s.menu("xlsx.sparkline-column");
    s.update(|e, _, cx| {
        let p = e.wb_mut().panels.sparkline.as_mut().unwrap();
        assert_eq!(p.group_index, Some(0));
        p.kind = ibis_doc_model::SparklineKind::Column;
        e.apply_sparkline(cx);
    });
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines[0].kind, ibis_doc_model::SparklineKind::Column);
    });

    // 5. Test ungrouping D1 from D2:D3 (XLSX-SPARK-006 グループ作成と解除)
    s.click_cell("D1");
    s.update(|e, _, cx| {
        e.ungroup_selected_sparklines(cx);
    });
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines.len(), 2);
        // Group 0 has 2 sparklines (D2, D3), Group 1 has 1 sparkline (D1)
        assert_eq!(sheet.sparklines[0].sparklines.len(), 2);
        assert_eq!(sheet.sparklines[1].sparklines.len(), 1);
        assert_eq!((sheet.sparklines[1].sparklines[0].row, sheet.sparklines[1].sparklines[0].column), (0, 3));
    });

    // Undo ungrouping restores single group of 3
    s.click_cell("E1");
    s.key("cmd-z");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines.len(), 1);
        assert_eq!(sheet.sparklines[0].sparklines.len(), 3);
    });

    // 6. Test Clear Selected (XLSX-SPARK-023: 選択したスパークラインの消去)
    s.click_cell("D1");
    s.menu("xlsx.sparkline-clear-selected");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines.len(), 1);
        assert_eq!(sheet.sparklines[0].sparklines.len(), 2);
        assert_eq!(sheet.sparklines[0].sparklines[0].row, 1);
        assert_eq!(sheet.sparklines[0].sparklines[1].row, 2);
    });

    // 7. Test Clear Group (XLSX-SPARK-023: グループ全体の消去)
    s.click_cell("D2");
    s.menu("xlsx.sparkline-clear-group");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines.len(), 0);
    });

    // Undo clear group and clear selected restores all 3 sparklines
    s.click_cell("E1");
    s.key("cmd-z");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines.len(), 1);
        assert_eq!(sheet.sparklines[0].sparklines.len(), 2);
    });
    s.key("cmd-z");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines.len(), 1);
        assert_eq!(sheet.sparklines[0].sparklines.len(), 3);
    });

    // 8. Save and reopen: verify OOXML roundtrip persistence
    let sheet_name = s.with(|e, _| e.slide().name.clone().unwrap_or_default());
    s.save_as("spark001.xlsx");
    let p = s.reopen();
    let slide = p.slides.iter().find(|sl| sl.name.as_deref() == Some(&sheet_name)).unwrap();
    let sheet = sheet_of_slide(slide).unwrap();
    assert_eq!(sheet.sparklines.len(), 1);
    assert_eq!(sheet.sparklines[0].kind, ibis_doc_model::SparklineKind::Column);
    assert_eq!(sheet.sparklines[0].sparklines.len(), 3);
    assert!(sheet.sparklines[0].high);
    assert!(sheet.sparklines[0].low);
    assert!(sheet.sparklines[0].negative_points);
}

/// XLSX-SPARK-008, XLSX-SPARK-022:
/// スパークラインのスタイル・線色変更、線の太さ、非表示行列のデータ表示（displayHidden）、
/// 空白セルの表示方法（displayEmptyCellsAs）、Undo/Redo、およびファイル保存・再読み込みの検証。
fn spark002_style_color_display_hidden() {
    let mut s = new_sheet();

    // 1. Enter data in A1:E3
    for r in 0..3 {
        for c in 0..5 {
            let val = ((r + 1) * 10 + c * 5).to_string();
            let col_letter = (b'A' + c as u8) as char;
            s.click_cell(&format!("{}{}", col_letter, r + 1));
            s.enter(&val, "enter");
        }
    }

    // 2. Hide column C (index 2)
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run_sheet(
            vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::Hide {
                slide,
                axis: ibis_doc_model::sheet_command::Axis::Columns,
                first: 2,
                last: 2,
                hidden: true,
            })],
            cx,
        );
    });

    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(sheet.is_col_hidden(2));
    });

    // 3. Create sparklines at F1:F3 from A1:E3 via panel
    s.click_cell("F1");
    s.menu("xlsx.sparkline-line");

    s.update(|e, window, cx| {
        let p = e.wb_mut().panels.sparkline.as_mut().unwrap();
        p.data_range_input.update(cx, |i, cx| i.set_value("A1:E3".to_string(), window, cx));
        p.location_range_input.update(cx, |i, cx| i.set_value("F1:F3".to_string(), window, cx));
        // Select style 2 (Accent 2 / orange/red)
        e.set_sparkline_style_preset(1, cx);
        e.set_sparkline_line_weight(150, cx);
        e.apply_sparkline(cx);
    });

    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines.len(), 1);
        let g = &sheet.sparklines[0];
        assert_eq!(g.kind, ibis_doc_model::SparklineKind::Line);
        assert_eq!(g.sparklines.len(), 3);
        assert_eq!(g.series, Some(ibis_doc_model::Rgb(0xC0504D)));
        assert_eq!(g.line_weight, 150);
        assert!(!g.display_hidden); // Default: hidden cells are NOT plotted
    });

    // 4. Test display_hidden toggle via panel
    s.click_cell("F1");
    s.menu("xlsx.sparkline-line");
    s.update(|e, _, cx| {
        let p = e.wb_mut().panels.sparkline.as_mut().unwrap();
        assert_eq!(p.group_index, Some(0));
        assert!(!p.display_hidden);
        p.display_hidden = true;
        // Also change empty to Zero
        p.empty = ibis_doc_model::SparklineEmpty::Zero;
        e.apply_sparkline(cx);
    });

    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        let g = &sheet.sparklines[0];
        assert!(g.display_hidden);
        assert_eq!(g.empty, ibis_doc_model::SparklineEmpty::Zero);
    });

    // 5. Test Undo / Redo for display_hidden and style edit
    s.click_cell("G1");
    s.key("cmd-z");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(!sheet.sparklines[0].display_hidden);
    });

    s.key("cmd-shift-z");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(sheet.sparklines[0].display_hidden);
    });

    // 6. Test menu actions for direct style / color / hidden setting
    s.click_cell("F1");
    s.menu("xlsx.sparkline-style-3");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines[0].series, Some(ibis_doc_model::Rgb(0x9BBB59)));
    });

    s.menu("xlsx.sparkline-color-purple");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines[0].series, Some(ibis_doc_model::Rgb(0x7030A0)));
    });

    s.menu("xlsx.sparkline-display-hidden-off");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(!sheet.sparklines[0].display_hidden);
    });

    s.menu("xlsx.sparkline-display-hidden-on");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(sheet.sparklines[0].display_hidden);
    });

    // 7. Save and reopen to verify roundtrip fidelity
    let sheet_name = s.with(|e, _| e.slide().name.clone().unwrap_or_default());
    s.save_as("spark002.xlsx");
    let p = s.reopen();
    let slide = p.slides.iter().find(|sl| sl.name.as_deref() == Some(&sheet_name)).unwrap();
    let sheet = sheet_of_slide(slide).unwrap();
    assert_eq!(sheet.sparklines.len(), 1);
    let g = &sheet.sparklines[0];
    assert!(g.display_hidden);
    assert_eq!(g.series, Some(ibis_doc_model::Rgb(0x7030A0)));
    assert_eq!(g.line_weight, 150);
}

/// XLSX-SPARK-015, XLSX-SPARK-004:
/// スパークラインの日付軸（dateAxis）、日付範囲数式（<xm:f>）、比例プロット配置、
/// 別シート参照スパークライン（s.sheet）の描画、Undo/Redo、およびファイル保存・再読み込みの検証。
fn spark003_date_axis_and_cross_sheet() {
    let mut s = new_sheet();

    // 1. A1:C1 に日付（シリアル値: 46023 = 2026/1/1, 46024 = 2026/1/2, 46032 = 2026/1/10）
    s.click_cell("A1");
    s.enter("46023", "tab");
    s.enter("46024", "tab");
    s.enter("46032", "enter");

    // A2:C2 にデータ（10, 50, 20）
    s.click_cell("A2");
    s.enter("10", "tab");
    s.enter("50", "tab");
    s.enter("20", "enter");

    // 2. D2 を選択してスパークラインパネルを開く
    s.click_cell("D2");
    s.menu("xlsx.sparkline-line");

    s.with(|e, _| {
        assert!(e.sheet_state().unwrap().panels.sparkline.is_some());
    });

    // 3. データ範囲 A2:C2, 配置先範囲 D2, 日付軸 true, 日付範囲 A1:C1 を設定して適用
    s.update(|e, window, cx| {
        let p = e.wb_mut().panels.sparkline.as_mut().unwrap();
        p.data_range_input.update(cx, |i, cx| i.set_value("A2:C2".to_string(), window, cx));
        p.location_range_input.update(cx, |i, cx| i.set_value("D2".to_string(), window, cx));
        p.date_axis = true;
        p.date_range_input.update(cx, |i, cx| i.set_value("A1:C1".to_string(), window, cx));
        e.apply_sparkline(cx);
    });

    s.with(|e, _| {
        assert!(e.sheet_state().unwrap().panels.sparkline.is_none());
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines.len(), 1);
        let g = &sheet.sparklines[0];
        assert!(g.date_axis);
        assert_eq!(g.date_range, Some("A1:C1".to_string()));
    });

    // 4. メニューまたはアクションで日付軸をオフ（一般軸）に切替
    s.click_cell("D2");
    s.update(|e, _, cx| {
        e.set_selected_sparklines_date_axis(false, cx);
    });
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(!sheet.sparklines[0].date_axis);
    });

    // Undo で日付軸が戻ることを確認
    s.key("cmd-z");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(sheet.sparklines[0].date_axis);
        assert_eq!(sheet.sparklines[0].date_range, Some("A1:C1".to_string()));
    });

    // Redo で再度オフになることを確認
    s.key("cmd-shift-z");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(!sheet.sparklines[0].date_axis);
    });
    s.key("cmd-z"); // 日付軸オンに戻す

    // 5. XLSX-SPARK-004: 別シート参照スパークライン
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let mut group = e.current_sheet().unwrap().1.sparklines[0].clone();
        group.sparklines.push(ibis_doc_model::Sparkline {
            row: 2,
            column: 3, // D3
            sheet: Some("Sheet1".to_string()),
            data: ibis_doc_model::CellRange { first_row: 1, last_row: 1, first_column: 0, last_column: 2 },
        });
        e.run_sheet(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::EditSparklineGroup {
            slide,
            index: 0,
            group,
        })], cx);
    });
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert_eq!(sheet.sparklines[0].sparklines.len(), 2);
        assert_eq!(sheet.sparklines[0].sparklines[1].sheet, Some("Sheet1".to_string()));
    });

    // 6. 保存と再読み込みで roundtrip fidelity を検証
    let sheet_name = s.with(|e, _| e.slide().name.clone().unwrap_or_default());
    s.save_as("spark003.xlsx");
    let p = s.reopen();
    let slide = p.slides.iter().find(|sl| sl.name.as_deref() == Some(&sheet_name)).unwrap();
    let sheet = sheet_of_slide(slide).unwrap();
    assert_eq!(sheet.sparklines.len(), 1);
    let g = &sheet.sparklines[0];
    assert!(g.date_axis);
    assert_eq!(g.date_range, Some("A1:C1".to_string()));
    assert_eq!(g.sparklines.len(), 2);
    assert_eq!(g.sparklines[1].sheet, Some("Sheet1".to_string()));
}

/// XLSX-SHEET-011:
/// データ > 小計… の検証。
/// 表への小計・総計の自動挿入、SUBTOTAL 数式の生成と計算、アウトラインレベル (1〜2) の設定、
/// Undo/Redo、すべて削除 (Remove All)、およびファイル保存・再読み込みの検証。
fn subtotal001_apply_and_remove() {
    let mut s = new_sheet();

    // 1. Enter test data in A1:C5
    // Row 1 (Header): 部門, 品目, 金額
    // Row 2: 東日本, リンゴ, 100
    // Row 3: 東日本, ミカン, 200
    // Row 4: 西日本, バナナ, 300
    // Row 5: 西日本, メロン, 400
    s.click_cell("A1");
    s.enter("部門", "tab");
    s.enter("品目", "tab");
    s.enter("金額", "enter");

    s.click_cell("A2");
    s.enter("東日本", "tab");
    s.enter("リンゴ", "tab");
    s.enter("100", "enter");

    s.click_cell("A3");
    s.enter("東日本", "tab");
    s.enter("ミカン", "tab");
    s.enter("200", "enter");

    s.click_cell("A4");
    s.enter("西日本", "tab");
    s.enter("バナナ", "tab");
    s.enter("300", "enter");

    s.click_cell("A5");
    s.enter("西日本", "tab");
    s.enter("メロン", "tab");
    s.enter("400", "enter");

    // 2. Select A1 and open subtotal panel via menu "xlsx.subtotal"
    s.click_cell("A1");
    s.menu("xlsx.subtotal");

    // Verify subtotal panel opened
    s.with(|e, _| {
        assert!(e.sheet_state().unwrap().panels.subtotal.is_some());
    });

    // 3. Configure subtotal:
    // group by column 0 (部門), function code 9 (SUM), subtotal column 2 (金額)
    s.update(|e, _, _| {
        let p = e.wb_mut().panels.subtotal.as_mut().unwrap();
        p.group_by_column = 0;
        p.function_code = 9;
        p.subtotal_columns = vec![2];
        p.replace_existing = true;
        p.summary_below = true;
    });

    // Apply subtotal
    s.update(|e, _, cx| {
        e.apply_subtotal(cx);
    });

    // Verify subtotal panel closed
    s.with(|e, _| {
        assert!(e.sheet_state().unwrap().panels.subtotal.is_none());
    });

    // 4. Check cells:
    // Row 1: 部門, 品目, 金額
    // Row 2: 東日本, リンゴ, 100
    // Row 3: 東日本, ミカン, 200
    // Row 4: 東日本 集計, empty, =SUBTOTAL(9, C2:C3) -> 300
    // Row 5: 西日本, バナナ, 300
    // Row 6: 西日本, メロン, 400
    // Row 7: 西日本 集計, empty, =SUBTOTAL(9, C5:C6) -> 700
    // Row 8: 総計, empty, =SUBTOTAL(9, C2:C7) -> 1000
    assert_eq!(s.value("A4"), CellValue::Text("東日本 集計".into()));
    assert_eq!(s.value("C4"), CellValue::Number(300.0));
    assert_eq!(s.value("A7"), CellValue::Text("西日本 集計".into()));
    assert_eq!(s.value("C7"), CellValue::Number(700.0));
    assert_eq!(s.value("A8"), CellValue::Text("総計".into()));
    assert_eq!(s.value("C8"), CellValue::Number(1000.0));

    // Verify outlines
    s.with(|e, _| {
        let (_, sheet) = e.current_sheet().unwrap();
        assert_eq!(sheet.outline_rows.get(&1).copied(), Some(2));
        assert_eq!(sheet.outline_rows.get(&2).copied(), Some(2));
        assert_eq!(sheet.outline_rows.get(&3).copied(), Some(1));
        assert_eq!(sheet.outline_rows.get(&4).copied(), Some(2));
        assert_eq!(sheet.outline_rows.get(&5).copied(), Some(2));
        assert_eq!(sheet.outline_rows.get(&6).copied(), Some(1));
        assert_eq!(sheet.outline_rows.get(&7).copied(), None);
    });

    // 5. Test Undo: reverts to 5 rows without subtotals
    s.key("cmd-z");
    assert_eq!(s.value("A4"), CellValue::Text("西日本".into()));
    assert_eq!(s.value("C4"), CellValue::Number(300.0));
    s.with(|e, _| {
        let (_, sheet) = e.current_sheet().unwrap();
        assert!(sheet.outline_rows.is_empty());
    });

    // 6. Test Redo: reapplies subtotals
    s.key("cmd-shift-z");
    assert_eq!(s.value("A4"), CellValue::Text("東日本 集計".into()));
    assert_eq!(s.value("C8"), CellValue::Number(1000.0));

    // 7. Test Remove All ("すべて削除")
    s.click_cell("A1");
    s.menu("xlsx.subtotal");
    s.update(|e, _, cx| {
        e.remove_subtotal(cx);
    });
    assert_eq!(s.value("A4"), CellValue::Text("西日本".into()));
    assert_eq!(s.value("C4"), CellValue::Number(300.0));
    s.with(|e, _| {
        let (_, sheet) = e.current_sheet().unwrap();
        assert!(sheet.outline_rows.is_empty());
    });

    // Undo Remove All restores subtotals
    s.key("cmd-z");
    assert_eq!(s.value("A4"), CellValue::Text("東日本 集計".into()));
    assert_eq!(s.value("C8"), CellValue::Number(1000.0));

    // 8. Save and reload roundtrip
    s.save_as("subtotal001-sales.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(sheet.cell(3, 0).unwrap().value, CellValue::Text("東日本 集計".into()));
    assert_eq!(sheet.cell(3, 2).unwrap().value, CellValue::Number(300.0));
    assert_eq!(sheet.cell(6, 0).unwrap().value, CellValue::Text("西日本 集計".into()));
    assert_eq!(sheet.cell(6, 2).unwrap().value, CellValue::Number(700.0));
    assert_eq!(sheet.cell(7, 0).unwrap().value, CellValue::Text("総計".into()));
    assert_eq!(sheet.cell(7, 2).unwrap().value, CellValue::Number(1000.0));
    assert_eq!(sheet.outline_rows.get(&3).copied(), Some(1));
    assert_eq!(sheet.outline_rows.get(&6).copied(), Some(1));
}

/// XLSX-PIVOT-013・014: ラベル フィルター (地域 が「関」で始まる), then 値フィルター
/// トップテン (the 2 largest 商品 of each), through the ▼ panel; the totals add up
/// the items kept; saved with its filters and read back.
fn pivot014_filters() {
    use ibis_doc_model::pivot::PivotButton;
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");
    let slide = s.with(|e, _| e.current_slide);
    let dump = |s: &mut Scenario| -> Vec<String> { (0..16).map(|r| (0..7).map(|c| s.with(|e, _| e.current_sheet().and_then(|(_, sh)| sh.cell(r, c).map(|x| x.text.clone()))).unwrap_or_default()).collect::<Vec<_>>().join("|")).collect() };
    let at = s.with(|e, _| e.cell_center(0, 1)).unwrap();
    // 行ラベル ▼ > ラベル フィルター > 指定の値で始まる… 関.
    s.update(move |e, window, cx| e.open_pivot_popup((slide, 0), PivotButton::Rows, at, window, cx));
    s.update(|e, window, cx| {
        e.pivot_popup_section(crate::xlsx::pivot::PopupSection::Label, cx);
        e.pivot_popup_ask(true, "captionBeginsWith", window, cx);
        let input = e.wb().panels.pivot_popup.as_ref().and_then(|p| p.dialog.as_ref()).map(|d| d.values[0].clone()).unwrap();
        input.update(cx, |i, cx| i.set_value("関", window, cx));
        e.pivot_popup_dialog_ok(cx);
    });
    let rows = dump(&mut s);
    assert_eq!(rows[2], "関西|2240|1300|3540|2440|9520|");
    assert_eq!(rows[6], "関東|2040|4220|3120|2240|11620|");
    assert_eq!(rows[10], "総計|4280|5520|6660|4680|21140|", "the totals add up the items kept");
    s.screenshot("label-filter");
    assert_eq!(pivot_of(&mut s, 0).filters.len(), 1);
    // The second row field's 値フィルター: トップテン 上位 2 項目.
    s.update(move |e, window, cx| e.open_pivot_popup((slide, 0), PivotButton::Rows, at, window, cx));
    s.update(|e, window, cx| {
        e.pivot_popup_field(3, cx);
        e.pivot_popup_ask(false, "count", window, cx);
        let input = e.wb().panels.pivot_popup.as_ref().and_then(|p| p.dialog.as_ref()).map(|d| d.values[0].clone()).unwrap();
        input.update(cx, |i, cx| i.set_value("2", window, cx));
        e.pivot_popup_dialog_ok(cx);
    });
    let rows = dump(&mut s);
    assert_eq!(rows[2], "関西|1440|900|3540|1560|7440|");
    assert_eq!(rows[3], "りんご|1440||840|1560|3840|");
    assert_eq!(rows[4], "ぶどう||900|2700||3600|", "みかん (the smallest) goes");
    assert_eq!(rows[8], "総計|2760|4800|6660|3000|17220|");
    s.screenshot("top-filter");
    s.save_as("pivot014.xlsx");
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert_eq!(sh.pivots[0].filters.len(), 2);
    assert_eq!(sh.pivots[0].filters[0].kind, "captionBeginsWith");
    assert_eq!(sh.pivots[0].filters[1].kind, "count");
}

/// The chart of xlsx-sales.xlsx as read.
fn chart_of_file() -> ibis_doc_model::Chart {
    let p = ibis_pptx::read(&std::fs::read(crate::shell::scenario::repo_root().join("test/editing/xlsx-sales.xlsx")).unwrap()).unwrap();
    p.slides[0].shapes.iter().find_map(|x| match &x.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()), _ => None }).unwrap()
}

/// XSC-13, 表示形式: text turned up and set vertically, a superscript, a
/// pattern over a yellow fill; saved and read back.
fn xsc13_direction_effects_patterns() {
    use ibis_doc_model::VerticalPosition;
    let mut s = new_sheet();
    for (cell, text) in [("A1", "上へ"), ("B1", "縦書き"), ("C1", "m2"), ("D1", "模様")] {
        s.click_cell(cell);
        s.enter(text, "enter");
    }
    s.click_cell("A1");
    s.menu("xlsx.rotate-up");
    s.click_cell("B1");
    s.menu("xlsx.rotate-vertical");
    s.click_cell("C1");
    s.menu("xlsx.superscript");
    s.click_cell("D1");
    s.update(|e, _, cx| e.format_cells(ibis_doc_model::StyleChange::Fill(Some(ibis_doc_model::Rgb(0xFFFF00))), cx));
    s.update(|e, _, cx| e.format_cells(ibis_doc_model::StyleChange::Pattern(Some(("ltUpDiag".into(), ibis_doc_model::Rgb(0), ibis_doc_model::Rgb(0xFFFF00)))), cx));
    s.screenshot("direction-effects-patterns");
    s.save_as("xsc13-direction.xlsx");
    let saved = s.reopen();
    let sh = sheet_of_slide(&saved.slides[1]).unwrap();
    let style = |c: u32| sh.styles[sh.cell(0, c).unwrap().style as usize].clone();
    assert_eq!(style(0).rotation, 90);
    assert_eq!(style(1).rotation, 255);
    assert_eq!(style(2).vert_align, VerticalPosition::Superscript);
    assert_eq!(style(3).pattern.as_ref().map(|p| p.0.as_str()), Some("ltUpDiag"));
}

/// XLSX-CELL-007, ⌘-click (Excel's Ctrl+クリック): A1:A2, then C3 stretched
/// to D5 by dragging, then column E; the status bar sums them all, and ⌘B
/// makes them all bold; a plain click leaves one range.
fn cell007_multi_range_select() {
    let mut s = new_sheet();
    for (cell, v) in [("A1", "1"), ("A2", "2"), ("C3", "10"), ("D5", "20"), ("E1", "100"), ("B1", "1000")] {
        s.click_cell(cell);
        s.enter(v, "enter");
    }
    let r = |a: u32, b: u32, c: u32, d: u32| ibis_doc_model::CellRange { first_row: a, last_row: b, first_column: c, last_column: d };
    let cmd = gpui_kit::Modifiers { platform: true, ..Default::default() };
    s.click_cell("A1");
    let a2 = s.with(|e, _| e.cell_center(0, 1)).unwrap();
    s.shift_click((f32::from(a2.x), f32::from(a2.y)));
    let c3 = s.with(|e, _| e.cell_center(2, 2)).unwrap();
    s.dispatch_click_with((f32::from(c3.x), f32::from(c3.y)), cmd);
    assert_eq!(s.with(|e, _| e.selected_ranges()), vec![r(2, 2, 2, 2), r(0, 1, 0, 0)]);
    // Dragging on from the ⌘-click stretches the new range, the others kept.
    let d5 = s.with(|e, _| e.cell_center(3, 4)).unwrap();
    s.update(move |e, _, cx| {
        e.extend_cells_to(d5);
        cx.notify();
    });
    assert_eq!(s.with(|e, _| e.selected_ranges()), vec![r(2, 4, 2, 3), r(0, 1, 0, 0)]);
    let summary = |s: &mut Scenario| s.status_texts().into_iter().find(|t| t.starts_with("平均"));
    assert_eq!(summary(&mut s).as_deref(), Some("平均: 8.25　データの個数: 4　合計: 33"));
    // ⌘ on column E's header adds the column.
    let e_header = s.header("E");
    s.dispatch_click_with(e_header, cmd);
    let ranges = s.with(|e, _| e.selected_ranges());
    assert_eq!(ranges.len(), 3);
    assert_eq!((ranges[0].first_column, ranges[0].last_column, ranges[0].first_row), (4, 4, 0));
    assert_eq!(summary(&mut s).as_deref(), Some("平均: 26.6　データの個数: 5　合計: 133"));
    s.key("cmd-b");
    let bold = |s: &mut Scenario, row: u32, column: u32| s.with(move |e, _| e.current_sheet().and_then(|(_, sh)| sh.cell(row, column).map(|c| sh.styles[c.style as usize].bold)).unwrap_or(false));
    assert!(bold(&mut s, 1, 0) && bold(&mut s, 2, 2) && bold(&mut s, 4, 3) && bold(&mut s, 0, 4));
    assert!(!bold(&mut s, 0, 1), "B1 is not selected");
    s.screenshot("multi-range");
    // One undo takes the bold off them all.
    s.key("cmd-z");
    assert!(!bold(&mut s, 1, 0) && !bold(&mut s, 0, 4));
    s.click_cell("G7");
    assert_eq!(s.with(|e, _| e.selected_ranges().len()), 1, "a click leaves one range");
}

/// XLSX-CELL-004, ⌥Enter (Excel's Alt+Enter): a line break in the cell; the
/// cell then wraps and its row grows, in one step to undo; saved and read
/// back.
fn cell004_line_break() {
    let mut s = new_sheet();
    s.click_cell("B2");
    let before = s.size_pt("2");
    s.type_text("一行目");
    s.key("alt-enter");
    s.type_text("二行目");
    s.key("alt-enter");
    s.type_text("三行目");
    s.key("enter");
    assert_eq!(s.value("B2"), CellValue::Text("一行目\n二行目\n三行目".into()));
    let wrap = |s: &mut Scenario| s.with(|e, _| e.current_sheet().and_then(|(_, sh)| sh.cell(1, 1).map(|c| sh.styles[c.style as usize].wrap)).unwrap_or(false));
    assert!(wrap(&mut s), "the cell wraps");
    let after = s.size_pt("2");
    assert!(after > before * 2.5, "the row grows to three lines: {before} -> {after}");
    s.screenshot("line-break");
    s.key("cmd-z");
    assert_eq!(s.value("B2"), CellValue::Empty);
    assert!(!wrap(&mut s));
    assert_eq!(s.size_pt("2"), before);
    s.key("cmd-shift-z");
    // Excel for Mac's ⌃⌥Return too.
    s.click_cell("C3");
    s.type_text("左");
    s.key("ctrl-alt-enter");
    s.type_text("右");
    s.key("enter");
    assert_eq!(s.value("C3"), CellValue::Text("左\n右".into()));
    s.save_as("cell004.xlsx");
    let saved = s.reopen();
    let sh = sheet_of_slide(&saved.slides[1]).unwrap();
    let cell = sh.cell(1, 1).unwrap();
    assert_eq!(cell.value, CellValue::Text("一行目\n二行目\n三行目".into()));
    assert!(sh.styles[cell.style as usize].wrap);
}

/// XLSX-PIVOT-018, ピボットテーブルの更新: a table Excel built before its
/// source changed (P2 360 → 99999; 総計 52180) built again from it, in one
/// step to undo; saved with its cache marked to be built again on opening.
fn pivot018_refresh() {
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");
    assert_eq!(s.value("F23"), CellValue::Number(52180.0), "as Excel left it");
    s.screenshot("before");
    // From outside the table: it asks for a cell in one.
    s.click_cell("H5");
    s.menu("xlsx.pivot-refresh");
    assert_eq!(s.value("F23"), CellValue::Number(52180.0));
    s.click_cell("B4");
    s.menu("xlsx.pivot-refresh");
    assert_eq!(s.value("F23"), CellValue::Number(52180.0 - 360.0 + 99999.0));
    assert_eq!(s.value("B4"), CellValue::Number(99999.0), "北海道 りんご 第1四半期: 360 → 99999");
    assert_eq!(s.value("A3"), CellValue::Text("北海道".into()));
    // Still in its style: 北海道's row bold, 総計 too.
    let bold = |s: &mut Scenario, r: &str| {
        let (c, row) = parse(r);
        s.with(move |e, _| e.current_sheet().and_then(|(_, sh)| sh.cell(row, c).map(|x| sh.styles[x.style as usize].bold)).unwrap_or(false))
    };
    assert!(bold(&mut s, "B3") && bold(&mut s, "F23") && !bold(&mut s, "B4"));
    s.screenshot("refreshed");
    s.key("cmd-z");
    assert_eq!(s.value("F23"), CellValue::Number(52180.0));
    s.key("cmd-shift-z");
    assert_eq!(s.value("F23"), CellValue::Number(151819.0));
    let saved = s.save_as("pivot018.xlsx");
    let data = std::fs::read(&saved).unwrap();
    let zip = ibis_pptx::test_files::part_text(&data, "xl/pivotCache/pivotCacheDefinition1.xml").expect("the cache's definition");
    assert!(zip.contains("refreshOnLoad=\"1\""), "Excel builds it again on opening");
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert_eq!(sh.cell(22, 5).map(|c| c.value.clone()), Some(CellValue::Number(151819.0)));
}

/// The pivot table `index` of the current sheet.
fn pivot_of(s: &mut Scenario, index: usize) -> ibis_doc_model::PivotTable {
    s.with(move |e, _| e.current_sheet().and_then(|(_, sh)| sh.pivots.get(index).cloned())).expect("the pivot table")
}

/// XLSX-PIVOT-001・002, 挿入 > ピボットテーブル: from the data around the
/// selected cell, on a new sheet at A3; 地域 checked (to the rows), 金額
/// (to the values, 合計), 四半期 to the columns, the value made 平均 and
/// back; one step to undo each; saved for Excel to build on opening.
fn pivot001_create() {
    use ibis_doc_model::PivotAxis;
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");
    s.click_cell("K5");
    s.menu("xlsx.pivot-insert");
    let source = s.with(|e, cx| e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).and_then(|p| p.create.as_ref()).map(|c| c.source.read(cx).value().to_string()));
    assert_eq!(source.as_deref(), Some("ピボット!$J$1:$P$41"), "the data around the selected cell");
    s.update(|e, window, cx| e.create_pivot(window, cx));
    assert_eq!(s.with(|e, _| e.slide().name.clone()).as_deref(), Some("Sheet2"), "a new sheet");
    assert_eq!(s.with(|e, _| e.presentation().slides[0].name.clone()).as_deref(), Some("Sheet2"), "before the data's sheet, as Excel puts it");
    assert_eq!(pivot_of(&mut s, 0).location.first_row, 2, "at A3");
    let target = s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).and_then(|p| p.target)).expect("its fields shown");
    s.update(move |e, _, cx| e.toggle_pivot_field(target, 2, cx));
    s.update(move |e, _, cx| e.toggle_pivot_field(target, 6, cx));
    assert_eq!(s.value("A3"), CellValue::Text("行ラベル".into()));
    assert_eq!(s.value("B3"), CellValue::Text("合計 / 金額".into()));
    assert_eq!(s.value("A9"), CellValue::Text("総計".into()));
    assert_eq!(s.value("B9"), CellValue::Number(151819.0));
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.place(1, Some(PivotAxis::Column), 0), cx));
    assert_eq!(s.value("B3"), CellValue::Text("列ラベル".into()));
    assert_eq!(s.value("A4"), CellValue::Text("行ラベル".into()));
    assert_eq!(s.value("F4"), CellValue::Text("総計".into()));
    assert_eq!(s.value("F10"), CellValue::Number(151819.0));
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.set_function(0, "average"), cx));
    assert_eq!(s.value("A3"), CellValue::Text("平均 / 金額".into()));
    s.screenshot("average");
    s.key("cmd-z");
    assert_eq!(s.value("A3"), CellValue::Text("合計 / 金額".into()));
    s.screenshot("created");
    let saved = s.save_as("pivot001.xlsx");
    let data = std::fs::read(&saved).unwrap();
    let table = ibis_pptx::test_files::part_text(&data, "xl/pivotTables/pivotTable2.xml").expect("the new table's part");
    assert!(table.contains(r#"<rowFields count="1"><field x="2"/></rowFields>"#) && table.contains(r#"<colFields count="1"><field x="1"/></colFields>"#), "{table}");
    let cache = ibis_pptx::test_files::part_text(&data, "xl/pivotCache/pivotCacheDefinition2.xml").expect("its cache");
    assert!(cache.contains(r#"refreshOnLoad="1""#) && cache.contains(r#"sheet="ピボット""#));
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert_eq!(sh.pivots.len(), 1);
    assert_eq!(sh.cell(9, 5).map(|c| c.value.clone()), Some(CellValue::Number(151819.0)), "built again on opening");
}

/// XLSX-PIVOT-002, a table Excel made laid out anew: 商品 to the columns
/// is refused (it would cover the data in column J, as Excel refuses);
/// 四半期 into the filters (the table moving down for them), 商品 out;
/// row fields reordered; values area additions; Undo/Redo;
/// its definition written again; saved and opened again.
fn pivot002_layout() {
    use ibis_doc_model::PivotAxis;
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");
    s.click_cell("A5");
    s.menu("xlsx.pivot-fields");
    let target = s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).and_then(|p| p.target)).expect("its fields shown");

    // 1. 他のセル（J列）に重なる配置変更は拒否される
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.place(3, Some(PivotAxis::Column), 0), cx));
    assert_eq!(pivot_of(&mut s, 0).col_fields, vec![1], "refused");
    assert_eq!(s.value("J2"), CellValue::Number(46027.0), "the data stays");

    // 2. 四半期をフィルター（ページ）へ移動: テーブルが下に移動して空きを作る
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.place(1, Some(PivotAxis::Page), 0), cx));
    assert_eq!(s.value("A1"), CellValue::Text("四半期".into()));
    assert_eq!(s.value("B1"), CellValue::Text("(すべて)".into()));
    assert_eq!(pivot_of(&mut s, 0).location.first_row, 2, "at A3 now");

    // 3. 商品を行から除外
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.place(3, None, 0), cx));
    assert_eq!(pivot_of(&mut s, 0).row_fields, vec![2]);
    assert_eq!(s.value("A3"), CellValue::Text("行ラベル".into()));
    assert_eq!(s.value("A4"), CellValue::Text("北海道".into()));
    assert_eq!(s.value("B9"), CellValue::Number(151819.0));

    // 4. 年(field 0)を行の先頭へ追加し並べ替え（行: [0, 2]）
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.place(0, Some(PivotAxis::Row), 0), cx));
    assert_eq!(pivot_of(&mut s, 0).row_fields, vec![0, 2]);

    // 5. 数量(field 5)を値エリアに追加（値が2つになり、Σ値(-2)がcol_fieldsに自動追加）
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.place(5, Some(PivotAxis::Values), 1), cx));
    assert_eq!(pivot_of(&mut s, 0).data_fields.len(), 2);
    assert_eq!(pivot_of(&mut s, 0).col_fields, vec![-2]);

    // 6. Undo / Redo 検証
    s.key("cmd-z"); // 数量値エリア追加取消
    assert_eq!(pivot_of(&mut s, 0).data_fields.len(), 1);
    assert!(pivot_of(&mut s, 0).col_fields.is_empty());

    s.key("cmd-z"); // 年行追加取消
    assert_eq!(pivot_of(&mut s, 0).row_fields, vec![2]);

    s.key("cmd-shift-z"); // 年行追加やり直し
    assert_eq!(pivot_of(&mut s, 0).row_fields, vec![0, 2]);

    s.key("cmd-shift-z"); // 数量値エリア追加やり直し
    assert_eq!(pivot_of(&mut s, 0).data_fields.len(), 2);
    assert_eq!(pivot_of(&mut s, 0).col_fields, vec![-2]);

    s.screenshot("moved");
    s.save_as("pivot002.xlsx");

    // 7. 再読込検証
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert_eq!(sh.pivots[0].row_fields, vec![0, 2]);
    assert_eq!(sh.pivots[0].col_fields, vec![-2]);
    assert_eq!(sh.pivots[0].page_fields.len(), 1);
    assert_eq!(sh.pivots[0].page_fields[0].field, 1);
    assert_eq!(sh.pivots[0].data_fields.len(), 2);
}

/// XLSX-PIVOT-003・004・015, 値フィールドの設定: custom name, summary function (average),
/// number format, and show_as ratios (percentOfTotal, percentOfCol, percentOfRow);
/// undo and redo; saved to OOXML with showDataAs and numFmtId, and reopened.
fn pivot003_value_field_settings() {
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");
    s.click_cell("B4");
    s.menu("xlsx.pivot-value-settings");
    let target = s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).and_then(|p| p.target)).expect("its panel shown");
    assert_eq!(s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).and_then(|p| p.value)), Some(0));

    // 1. Custom name (指定した名前)
    s.update(|e, window, cx| {
        if let Some(p) = e.sheet_state().and_then(|st| st.panels.pivot.as_ref()) {
            if let Some(inp) = p.value_name.as_ref() {
                inp.update(cx, |i, cx| i.set_value("売上合計", window, cx));
            }
        }
        let name = e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).and_then(|p| p.value_name.as_ref()).map(|st| st.read(cx).value().to_string()).unwrap_or_default();
        e.change_pivot(target, |p| p.set_data_field_name(0, &name), cx);
    });
    assert_eq!(pivot_of(&mut s, 0).data_fields[0].name, "売上合計");

    // 2. Summary function change (合計 -> 平均)
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.set_function(0, "average"), cx));
    assert_eq!(pivot_of(&mut s, 0).data_fields[0].function, "average");
    let avg_val = s.value("F23");
    assert_eq!(avg_val, CellValue::Number(151819.0 / 40.0), "F23 is average: {:?}", avg_val);

    // 3. Undo and Redo
    s.key("cmd-z");
    assert_eq!(pivot_of(&mut s, 0).data_fields[0].function, "sum");
    s.key("cmd-shift-z");
    assert_eq!(pivot_of(&mut s, 0).data_fields[0].function, "average");

    // Revert function to sum for ratio checks
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.set_function(0, "sum"), cx));
    assert_eq!(pivot_of(&mut s, 0).data_fields[0].function, "sum");

    // 4. Number format (#,##0.00)
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.set_number_format(0, Some("#,##0.00".to_string())), cx));
    assert_eq!(pivot_of(&mut s, 0).data_fields[0].number_format.as_deref(), Some("#,##0.00"));

    // 5. Ratio: percentOfTotal (総計に対する比率)
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.set_show_as(0, Some("percentOfTotal".to_string())), cx));
    assert_eq!(pivot_of(&mut s, 0).data_fields[0].show_as.as_deref(), Some("percentOfTotal"));
    let p = pivot_of(&mut s, 0);
    let gt = s.with(|e, _| {
        let (_, sh) = e.current_sheet().unwrap();
        sh.cell(p.location.last_row, p.location.last_column).map(|c| c.value.clone())
    });
    assert_eq!(gt, Some(CellValue::Number(1.0)), "Grand Total is 100%");
    if let CellValue::Number(r) = s.value("B4") {
        assert!(r > 0.0 && r < 1.0, "individual ratio should be between 0 and 1: {r}");
    } else {
        panic!("B4 should be ratio: {:?}", s.value("B4"));
    }

    // 6. Ratio: percentOfCol (列集計に対する比率)
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.set_show_as(0, Some("percentOfCol".to_string())), cx));
    assert_eq!(pivot_of(&mut s, 0).data_fields[0].show_as.as_deref(), Some("percentOfCol"));
    let p = pivot_of(&mut s, 0);
    let col_total = s.with(|e, _| {
        let (_, sh) = e.current_sheet().unwrap();
        sh.cell(p.location.last_row, 1).map(|c| c.value.clone())
    });
    assert_eq!(col_total, Some(CellValue::Number(1.0)), "Column total is 100%");

    // 7. Ratio: percentOfRow (行集計に対する比率)
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.set_show_as(0, Some("percentOfRow".to_string())), cx));
    assert_eq!(pivot_of(&mut s, 0).data_fields[0].show_as.as_deref(), Some("percentOfRow"));
    let p = pivot_of(&mut s, 0);
    let row_total = s.with(|e, _| {
        let (_, sh) = e.current_sheet().unwrap();
        sh.cell(3, p.location.last_column).map(|c| c.value.clone())
    });
    assert_eq!(row_total, Some(CellValue::Number(1.0)), "Row total is 100%");

    // 8. Set back to percentOfTotal with percentage number format and save
    s.update(move |e, _, cx| e.change_pivot(target, |p| {
        p.set_show_as(0, Some("percentOfTotal".to_string()));
        p.set_number_format(0, Some("0.00%".to_string()));
    }, cx));
    s.screenshot("pivot003_percent_of_total");
    let saved = s.save_as("pivot003.xlsx");
    let data = std::fs::read(&saved).unwrap();
    let table = ibis_pptx::test_files::part_text(&data, "xl/pivotTables/pivotTable1.xml").expect("table xml");
    assert!(table.contains(r#"showDataAs="percentOfTotal""#), "{table}");
    assert!(table.contains(r#"name="売上合計""#), "{table}");
    assert!(table.contains(r#"numFmtId="#), "{table}");

    // 9. Reopen and verify
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    let p_reopened = &sh.pivots[0];
    assert_eq!(p_reopened.data_fields[0].name, "売上合計");
    assert_eq!(p_reopened.data_fields[0].show_as.as_deref(), Some("percentOfTotal"));
    assert_eq!(p_reopened.data_fields[0].number_format.as_deref(), Some("0.00%"));
    assert_eq!(sh.cell(p_reopened.location.last_row, p_reopened.location.last_column).map(|c| c.value.clone()), Some(CellValue::Number(1.0)));
}

/// XLSX-PIVOT-013・014, a pivot table's ▼: 行ラベル's (地域) 北海道 not
/// shown; 商品 (chosen in the same panel) sorted 降順; 担当 as a report
/// filter, its ▼ choosing 佐藤; saved and opened again.
fn pivot013_buttons() {
    use ibis_doc_model::PivotAxis;
    use ibis_doc_model::pivot::PivotButton;
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");
    let slide = s.with(|e, _| e.current_slide);
    let places = pivot_of(&mut s, 0).button_places();
    assert_eq!(places, vec![(1, 0, PivotButton::Rows), (0, 1, PivotButton::Columns)], "行ラベル and 列ラベル");
    let at = s.with(|e, _| e.cell_center(0, 1)).unwrap();
    s.update(move |e, window, cx| e.open_pivot_popup((slide, 0), PivotButton::Rows, at, window, cx));
    s.screenshot("rows-button");
    // 北海道 unticked.
    let hokkaido = {
        let p = pivot_of(&mut s, 0);
        let f = &p.fields[2];
        f.items.iter().position(|it| it.x.and_then(|x| f.shared.get(x as usize)) == Some(&CellValue::Text("北海道".into()))).unwrap()
    };
    s.update(move |e, _, cx| e.pivot_popup_tick(Some(hokkaido), false, cx));
    s.update(|e, _, cx| e.pivot_popup_ok(cx));
    assert_eq!(s.value("A3"), CellValue::Text("関西".into()), "北海道 not shown");
    assert_eq!(s.value("F19"), CellValue::Number(151819.0 - 109759.0));
    // 商品 sorted 降順 (ぶどう, みかん, りんご by code point: ぶ > み? no: ぶ U+3076 < み U+307F < り).
    s.update(move |e, window, cx| e.open_pivot_popup((slide, 0), PivotButton::Rows, at, window, cx));
    s.update(|e, _, cx| e.pivot_popup_field(3, cx));
    s.update(|e, _, cx| e.pivot_popup_sort(false, cx));
    assert_eq!(s.value("A4"), CellValue::Text("りんご".into()));
    assert_eq!(s.value("A6"), CellValue::Text("ぶどう".into()));
    // 担当 as a report filter: the table moves down; its ▼ chooses 佐藤.
    let target = (slide, 0);
    s.update(move |e, _, cx| e.change_pivot(target, |p| p.place(4, Some(PivotAxis::Page), 0), cx));
    assert_eq!(s.value("B1"), CellValue::Text("(すべて)".into()));
    s.update(move |e, window, cx| e.open_pivot_popup((slide, 0), PivotButton::Page(0), at, window, cx));
    let sato = {
        let p = pivot_of(&mut s, 0);
        let f = &p.fields[4];
        f.items.iter().position(|it| it.x.and_then(|x| f.shared.get(x as usize)) == Some(&CellValue::Text("佐藤".into()))).unwrap()
    };
    s.update(move |e, _, cx| e.pivot_popup_choose(Some(sato), cx));
    s.update(|e, _, cx| e.pivot_popup_ok(cx));
    assert_eq!(s.value("B1"), CellValue::Text("佐藤".into()));
    s.screenshot("filtered");
    let total = s.with(|e, _| {
        let (_, sh) = e.current_sheet().unwrap();
        let p = &sh.pivots[0];
        sh.cell(p.location.last_row, p.location.last_column).map(|c| c.value.clone())
    });
    s.save_as("pivot013.xlsx");
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    let p = &sh.pivots[0];
    assert!(p.fields[2].items[hokkaido].hidden, "北海道 kept hidden");
    assert_eq!(p.page_fields[0].item, Some(sato as u32));
    assert_eq!(sh.cell(p.location.last_row, p.location.last_column).map(|c| c.value.clone()), total, "the same total on opening");
}

/// XLSX-PQ-001, データ > テキスト/CSV から…: a Shift_JIS text file read with
/// its first column kept as text (the leading zeros stay), the others as
/// typed; the second column left out; put from a chosen cell; one step to
/// undo.
fn pq001_import_text() {
    let mut s = new_sheet();
    let data = ibis_pptx::csv::encode_sjis("コード,名称,数量,日付\r\n00123,りんご,3,2026/1/5\r\n00456,みかん,5,2026/2/6\r\n");
    s.click_cell("B2");
    s.update(|e, window, cx| e.show_import_text_panel(window, cx));
    s.update(move |e, _, cx| e.set_import_text_data("goods.csv".into(), data, cx));
    // The file is read as Shift_JIS (found from its bytes), the comma found.
    s.screenshot("panel");
    s.update(|e, _, cx| {
        if let Some(p) = e.wb_mut().panels.import_text.as_mut() {
            p.columns = vec![super::import_text::ColumnType::Text, super::import_text::ColumnType::Skip];
        }
        e.apply_import_text_panel(cx);
    });
    s.screenshot("imported");
    assert_eq!(s.value("B2"), CellValue::Text("コード".into()));
    assert_eq!(s.value("C2"), CellValue::Text("数量".into()), "the 名称 column is left out");
    assert_eq!(s.value("B3"), CellValue::Text("00123".into()), "kept as text: the zeros stay");
    assert_eq!(s.value("C3"), CellValue::Number(3.0));
    assert!(matches!(s.value("D3"), CellValue::Number(_)), "a date read as typed: {:?}", s.value("D3"));
    assert_eq!(s.value("B4"), CellValue::Text("00456".into()));
    s.key("cmd-z");
    assert_eq!(s.value("B3"), CellValue::Empty);
}

/// XLSX-PQ-001, a date column read in a chosen order: 日/月/年 for the
/// first column (what is not a date in that order stays text), 年月日 with
/// no separators for the second; both shown as yyyy/m/d.
fn pq001_import_text_dates() {
    use super::import_text::ColumnType;
    use ibis_calc::input::DateOrder;
    let mut s = new_sheet();
    let data = b"d,e\r\n5/1/2026,20260105\r\n31/2/2026,20261231\r\n".to_vec();
    s.click_cell("A1");
    s.update(|e, window, cx| e.show_import_text_panel(window, cx));
    s.update(move |e, _, cx| e.set_import_text_data("dates.csv".into(), data, cx));
    s.update(|e, _, cx| {
        if let Some(p) = e.wb_mut().panels.import_text.as_mut() {
            p.columns = vec![ColumnType::Date(DateOrder::Dmy), ColumnType::Date(DateOrder::Ymd)];
        }
        e.apply_import_text_panel(cx);
    });
    s.screenshot("imported");
    assert_eq!(s.value("A2"), CellValue::Number(46027.0), "5 Jan 2026");
    assert_eq!(s.value("B2"), CellValue::Number(46027.0));
    assert_eq!(s.value("A3"), CellValue::Text("31/2/2026".into()), "not a date in that order: text");
    assert_eq!(s.value("B3"), CellValue::Number(46387.0), "31 Dec 2026");
}

/// XLSX-CELL-005, the formula bar: a press on it types into the selected
/// cell (the cell edit begins, with the content), the bar shows the caret
/// and the selection; part of the text made bold there and kept.
fn cell005_formula_bar_edit() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("A6");
    // The bar is under the toolbar (the window is 1200 × 720 points): its field right of the name box.
    let at = (600.0, 123.0);
    s.click(at);
    assert!(s.with(|e, _| e.cell_edit().is_some_and(|c| c.text == "東町店")), "pressing the bar begins the edit");
    s.key("shift-left");
    s.key("cmd-b");
    s.screenshot("bar");
    let runs = s.with(|e, _| e.cell_edit().and_then(|c| c.runs.clone())).expect("runs while editing");
    assert_eq!(runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["東町", "店"]);
    assert_eq!(runs[1].bold, Some(true));
    s.key("enter");
    assert!(s.with(|e, _| e.current_sheet().and_then(|(_, sheet)| sheet.cell(5, 0).and_then(|c| c.runs.clone()))).is_some());
}

/// XLSX-FORMULA-006, データ > ブックのリンク…: the links listed; 値の更新 from
/// a file (a workbook with other values in the cells the link caches), the
/// formulas calculated again; リンクの解除 makes the formulas values; one
/// step to undo each; saved and opened again.
fn formula006_external_links() {
    let mut s = Scenario::open("test/editing/xlsx-external-link.xlsx");
    s.update(|e, window, cx| e.open_external_links_panel(window, cx));
    s.screenshot("links");
    assert_eq!(s.with(|e, _| e.live_external_books().iter().map(|(_, b)| b.name.clone()).collect::<Vec<_>>()), ["予算.xlsx", "actual.xlsx"]);
    assert_eq!(s.value("J45"), CellValue::Number(60.0));
    // 値の更新 from a workbook with Data!A1:A3 = 1, 2, 3 and B1 = "x".
    let update = ibis_pptx::csv::workbook_from("1,x\n2\n3\n", "Data").expect("a workbook");
    s.update(move |e, _, cx| e.refresh_external_book_from(0, &update, cx));
    assert_eq!(s.value("J45"), CellValue::Number(6.0), "SUM of the new cached cells");
    assert_eq!(s.value("K45"), CellValue::Text("x".into()));
    s.screenshot("updated");
    s.key("cmd-z");
    assert_eq!(s.value("J45"), CellValue::Number(60.0));
    // リンクの解除 of the first.
    s.update(|e, _, cx| e.break_external_link(0, cx));
    assert_eq!(s.with(|e, _| e.live_external_books().len()), 1);
    s.save_as("formula006.xlsx");
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert!(sh.cell(44, 9).is_some_and(|c| c.formula.is_none() && c.value == CellValue::Number(60.0)));
    assert_eq!(reopened.workbook.as_ref().unwrap().external_books.len(), 1);
}

/// XLSX-OPTIONS-003, the right button on a cell: the menu opens (Excel for
/// Mac's items), the cell pressed is selected unless it is within the
/// selection; 数式と値のクリア, 行を挿入, 選択したセルの値でフィルター.
fn options003_cell_menu() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B4");
    let at = s.with(|e, _| e.cell_center(0, 5)).expect("A6 on screen");
    s.right_click((f32::from(at.x), f32::from(at.y)));
    assert!(s.with(|e, _| e.sheet_state().unwrap().header_popup.is_some()), "the menu opens");
    assert_eq!(s.with(|e, _| e.current_cell()), Some((0, 5)), "the cell pressed is selected");
    s.screenshot("menu");
    s.key("escape");
    let ids = s.with(|e, _| e.cell_menu().iter().filter_map(|m| match m { crate::shell::chrome::MenuEntry::Item(i) => Some(i.id), crate::shell::chrome::MenuEntry::Submenu(x) => Some(x.id), _ => None }).collect::<Vec<_>>());
    assert_eq!(ids[..4], ["xlsx.cell-cut", "xlsx.cell-copy", "xlsx.cell-paste", "xlsx.paste-special"], "{ids:?}");
    let call = |s: &mut Scenario, id: &'static str| {
        let entries = s.with(|e, _| e.cell_menu());
        let handler = crate::shell::scenario::find_item(&entries, id).and_then(|i| i.handler.clone()).unwrap_or_else(|| panic!("no menu item {id}"));
        s.update(move |e, window, cx| handler(e, window, cx));
    };
    assert_eq!(s.value("A6"), CellValue::Text("東町店".into()));
    call(&mut s, "xlsx.cell-clear");
    assert_eq!(s.value("A6"), CellValue::Empty);
    s.key("cmd-z");
    assert_eq!(s.value("A6"), CellValue::Text("東町店".into()));
    call(&mut s, "xlsx.cell-insert-row");
    assert_eq!(s.value("A7"), CellValue::Text("東町店".into()), "a row inserted above");
    s.key("cmd-z");
    // フィルター > 選択したセルの値でフィルター: the other stores' rows are hidden.
    s.click_cell("A6");
    call(&mut s, "xlsx.cell-filter-value");
    let hidden = s.with(|e, _| e.current_sheet().map(|(_, sh)| sh.filtered_rows.len()).unwrap_or(0));
    assert!(hidden >= 1, "rows are filtered: {hidden}");
    // 選択したセルの色でフィルター: the cells with no fill (as A6 has none) are kept.
    s.key("cmd-z");
    s.click_cell("A6");
    call(&mut s, "xlsx.cell-filter-fill");
    let hidden = s.with(|e, _| e.current_sheet().map(|(_, sh)| sh.filtered_rows.len()).unwrap_or(0));
    assert!(hidden >= 1, "the cells with another fill are hidden: {hidden}");
}

/// XLSX-OPTIONS-003, the right button on a sheet's tab: the menu has Excel
/// for Mac's items; 挿入 adds a sheet, 非表示 hides this one.
fn options003_tab_menu() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let ids = s.with(|e, _| e.tab_menu().iter().filter_map(|m| match m { crate::shell::chrome::MenuEntry::Item(i) => Some(i.id), crate::shell::chrome::MenuEntry::Submenu(x) => Some(x.id), _ => None }).collect::<Vec<_>>());
    assert_eq!(ids[..4], ["xlsx.insert-sheet", "xlsx.delete-sheet", "xlsx.rename-sheet", "xlsx.duplicate-sheet"], "{ids:?}");
    assert!(ids.contains(&"xlsx.tab-colors") && ids.contains(&"xlsx.hide-sheet") && ids.contains(&"xlsx.tab-protect"), "{ids:?}");
    let count = |s: &mut Scenario| s.with(|e, _| e.presentation().slides.len());
    let before = count(&mut s);
    let entries = s.with(|e, _| e.tab_menu());
    let handler = crate::shell::scenario::find_item(&entries, "xlsx.insert-sheet").and_then(|i| i.handler.clone()).expect("the item");
    s.update(move |e, window, cx| handler(e, window, cx));
    assert_eq!(count(&mut s), before + 1, "a sheet inserted");
}

/// XLSX-OPTIONS-003, the right button on a chart over the sheet: the menu has
/// 切り取り, コピー, 貼り付け, 削除, 順序, 代替テキスト; 削除 removes the chart.
fn options003_shape_menu() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let chart = s.with(|e, _| e.slide().shapes.iter().find(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))).map(|sh| sh.id)).expect("a chart on the sheet");
    s.update(move |e, _, _| e.set_selection(vec![chart]));
    let ids = s.with(|e, _| e.shape_menu().iter().filter_map(|m| match m { crate::shell::chrome::MenuEntry::Item(i) => Some(i.id), crate::shell::chrome::MenuEntry::Submenu(x) => Some(x.id), _ => None }).collect::<Vec<_>>());
    assert_eq!(ids[..4], ["edit.cut", "edit.copy", "edit.paste", "edit.delete"], "{ids:?}");
    assert!(ids.contains(&"xlsx.order") && ids.contains(&"xlsx.alt-text"), "{ids:?}");
    let entries = s.with(|e, _| e.shape_menu());
    let handler = crate::shell::scenario::find_item(&entries, "edit.delete").and_then(|i| i.handler.clone()).expect("the item");
    s.update(move |e, window, cx| handler(e, window, cx));
    assert!(s.with(|e, _| !e.slide().shapes.iter().any(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))), "the chart is deleted");
}

/// XLSX-OPTIONS-006, エラー チェック: text that reads as a number is marked
/// (a green triangle, the button at the selected cell); 数値に変換 makes it
/// a number; エラーを無視 stops the mark; one step to undo each.
fn options006_error_check() {
    let mut s = new_sheet();
    s.click_cell("B2");
    s.enter("'123", "enter");
    s.enter("'45", "enter");
    assert_eq!(s.value("B2"), CellValue::Text("123".into()));
    let flagged = |s: &mut Scenario| s.with(|e, _| e.current_sheet().map(|(_, sh)| ibis_doc_model::error_check::flagged_cells(sh).len()).unwrap_or(0));
    assert_eq!(flagged(&mut s), 2);
    s.click_cell("B2");
    assert_eq!(s.with(|e, _| e.current_error()), Some(ibis_doc_model::error_check::ErrorKind::NumberAsText));
    s.screenshot("marked");
    let call = |s: &mut Scenario, id: &'static str| {
        let entries = s.with(|e, _| e.error_menu());
        let handler = crate::shell::scenario::find_item(&entries, id).and_then(|i| i.handler.clone()).unwrap_or_else(|| panic!("no item {id}"));
        s.update(move |e, window, cx| handler(e, window, cx));
    };
    call(&mut s, "xlsx.error-convert");
    assert_eq!(s.value("B2"), CellValue::Number(123.0));
    assert_eq!(flagged(&mut s), 1);
    s.key("cmd-z");
    assert_eq!(s.value("B2"), CellValue::Text("123".into()));
    s.click_cell("B3");
    call(&mut s, "xlsx.error-ignore");
    assert_eq!(flagged(&mut s), 1, "B3 ignored, B2 still marked");
    s.save_as("options006.xlsx");
    let reopened = s.reopen();
    let total: usize = reopened.slides.iter().filter_map(sheet_of_slide).map(|sh| ibis_doc_model::error_check::flagged_cells(sh).len()).sum();
    let ignored: usize = reopened.slides.iter().filter_map(sheet_of_slide).map(|sh| sh.ignored_errors.len()).sum();
    assert_eq!((total, ignored), (1, 1), "B2 marked, B3 ignored after a save");
}

/// XLSX-IMAGE-002, pictures in cells (セルに配置): drawn inside their cells
/// (skipped while the Excel-made file is not in the tree).
fn image002_cell_pictures() {
    if !crate::shell::scenario::repo_root().join("test/editing/xlsx-cell-images-excel.xlsx").exists() {
        return;
    }
    let mut s = Scenario::open("test/editing/xlsx-cell-images-excel.xlsx");
    let pictures = s.with(|e, _| e.slide().shapes.iter().filter(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Picture(_))).count());
    assert!(pictures >= 1, "a picture in a cell");
    // Scroll to the first cell that has one.
    let at = s.with(|e, _| e.current_sheet().and_then(|(_, sh)| sh.cell_images.first().map(|i| (i.column, i.row)))).expect("a cell picture");
    s.update(move |e, _, cx| {
        e.set_cell(at.0, at.1);
        e.scroll_range_into_view(true);
        cx.notify();
    });
    s.screenshot("cell-pictures");
}

/// XLSX-FILE-010, an old workbook (.xls, Excel 97-2003): opened as a
/// workbook (values, formulas, formats, a merge, the frozen pane), saved as
/// .xlsx and opened again.
fn file010_legacy_xls() {
    let mut s = Scenario::open("test/editing/xlsx-legacy-sample.xls");
    s.screenshot("opened");
    assert_eq!(s.value("A1"), CellValue::Text("架空商店 月次売上表".into()));
    assert_eq!(s.value("E3"), CellValue::Number(1440.0));
    assert_eq!(s.formula("E3").as_deref(), Some("C3*D3"));
    // Editing recalculates as for any workbook: 数量 of the first row changed.
    s.click_cell("C3");
    s.enter("10", "enter");
    assert_eq!(s.value("E3"), CellValue::Number(1200.0));
    s.save_as("file010.xlsx");
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert_eq!(sh.cell(2, 4).and_then(|c| c.formula.clone()).as_deref(), Some("C3*D3"));
    assert_eq!(sh.cell(2, 4).map(|c| c.value.clone()), Some(CellValue::Number(1200.0)));
}

/// FILE-010: the conditional formats, data validations, hyperlinks and notes
/// of an Excel 97-2003 workbook (made by Excel for Mac: test 担当).
fn file010_legacy_features() {
    let mut s = Scenario::open("test/editing/xlsx-legacy-features.xls");
    s.screenshot("opened");
    let sheet = s.with(|e, _| e.current_sheet().map(|(_, sh)| sh.clone())).expect("a sheet");
    assert_eq!(sheet.conditional.len(), 3);
    assert_eq!(sheet.validations.len(), 2);
    assert_eq!(sheet.hyperlinks.len(), 2);
    assert_eq!(sheet.comments.len(), 2);
    // Saved as an .xlsx and opened again: the same.
    s.save_as("file010-features.xlsx");
    let reopened = s.reopen();
    let again = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert_eq!((again.conditional.len(), again.validations.len(), again.hyperlinks.len(), again.comments.len()), (3, 2, 2, 2));
}

/// XLSX-SEC-006 開封パスワード: a workbook Excel protected asks for its
/// password (a wrong one is told), opens with the right one, an edit saves
/// protected again, the password can be taken off, and a new one set.
fn sec006_open_password() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/protected-xlsx");
    std::fs::create_dir_all(&root).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-encrypted.xlsx");
    assert_eq!(crate::read_file(&path).err().as_deref(), Some(crate::NEEDS_PASSWORD));
    assert_eq!(crate::read_file_with(&path, Some("wrong")).err().as_deref(), Some(crate::WRONG_PASSWORD));
    let loaded = crate::read_file_with(&path, Some("ibis123")).expect("opens with its password");
    assert_eq!(loaded.password.as_deref(), Some("ibis123"));
    let sheets = loaded.presentation.slides.len();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let opened = path.clone();
    s.update(|e, w, cx| e.open_path_with_password(opened, "ibis123".into(), None, w, cx));
    for _ in 0..100 {
        s.settle();
        if s.with(|e, _| e.protection_password.is_some()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(s.with(|e, _| e.protection_password.clone()).as_deref(), Some("ibis123"));
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), sheets);
    s.screenshot("opened");
    // An edit, saved: protected again, and the cell is there once decrypted.
    s.click_cell("J1");
    s.enter("確認", "enter");
    let saved = s.save_as(root.join("saved.xlsx").to_str().unwrap());
    let bytes = std::fs::read(&saved).unwrap();
    assert!(ibis_pptx::is_encrypted(&bytes), "saved protected again");
    let plain = ibis_pptx::decrypt(&bytes, "ibis123").unwrap();
    let again = ibis_pptx::read(&plain).unwrap();
    assert_eq!(again.slides.len(), sheets);
    assert_eq!(sheet_of_slide(&again.slides[0]).unwrap().cell(0, 9).map(|c| c.value.clone()), Some(CellValue::Text("確認".into())));
    // Taken off: saved in the clear.
    s.update(|e, w, cx| e.set_protection(None, w, cx));
    let bytes = std::fs::read(&saved).unwrap();
    assert!(!ibis_pptx::is_encrypted(&bytes) && ibis_pptx::read(&bytes).unwrap().slides.len() == sheets, "saved without the password");
    // A new password.
    s.update(|e, w, cx| e.set_protection(Some("新しい".into()), w, cx));
    assert!(ibis_pptx::is_encrypted(&std::fs::read(&saved).unwrap()));
}

/// The middle of button `item` of the slicer `name`, in the window.
fn slicer_button(s: &mut Scenario, name: &str, item: usize) -> (f32, f32) {
    s.with(|e, _| {
        let layout = e.slide_layout().expect("the slide is laid out");
        let sh = e.slide().shapes.iter().find(|sh| matches!(&sh.kind, ibis_doc_model::ShapeKind::Slicer(x) if x.name == name)).expect("the slicer");
        let ibis_doc_model::ShapeKind::Slicer(sl) = &sh.kind else { unreachable!() };
        let l = ibis_doc_model::slicer::layout(sl, sh.frame.width, sh.frame.height);
        let (col, row) = (item as i64 % l.columns as i64, item as i64 / l.columns as i64);
        let x = sh.frame.x + l.body.0 + col * (l.button_width + l.gap) + l.button_width / 2;
        let y = sh.frame.y + l.body.1 + row * (l.button_height + l.gap) + l.button_height / 2;
        (f32::from(layout.origin.x + layout.to_px(x)), f32::from(layout.origin.y + layout.to_px(y)))
    })
}

fn slicer_items_of(s: &mut Scenario, name: &str) -> Vec<(String, bool)> {
    s.with(|e, _| {
        e.slide().shapes.iter().find_map(|sh| match &sh.kind {
            ibis_doc_model::ShapeKind::Slicer(x) if x.name == name => Some(x.items.iter().map(|i| (i.label.clone(), i.selected)).collect()),
            _ => None,
        }).expect("the slicer")
    })
}

/// XLSX-TABLE-019・PIVOT-015, スライサー (a workbook Excel made: a pivot
/// table's slicer 商品 on one sheet, a table's 地域 on another): read and
/// drawn; a button pressed filters the pivot table (an item hidden) or the
/// table (its rows hidden); the slicer follows; the heading dragged moves
/// it; saved and opened again.
fn slicer001_buttons() {
    let mut s = Scenario::open("test/editing/xlsx-slicers-excel.xlsx");
    s.screenshot("opened");
    assert_eq!(slicer_items_of(&mut s, "商品"), vec![("A".to_string(), true), ("B".to_string(), true)]);
    assert_eq!(s.value("A5"), CellValue::Text("A".into()));
    // B alone: the pivot table loses A.
    let at = slicer_button(&mut s, "商品", 1);
    s.click(at);
    assert_eq!(slicer_items_of(&mut s, "商品"), vec![("A".to_string(), false), ("B".to_string(), true)]);
    assert_eq!(s.value("A5"), CellValue::Text("B".into()), "A is not shown");
    s.screenshot("pivot-filtered");
    // Undo: both again.
    s.key("cmd-z");
    assert!(slicer_items_of(&mut s, "商品").iter().all(|i| i.1));
    let at = slicer_button(&mut s, "商品", 1);
    s.click(at);
    // The heading dragged: the slicer moves.
    let frame_of = |s: &mut Scenario, name: &str| s.with(|e, _| e.slide().shapes.iter().find(|sh| matches!(&sh.kind, ibis_doc_model::ShapeKind::Slicer(x) if x.name == name)).map(|sh| sh.frame)).expect("the slicer");
    let before = frame_of(&mut s, "商品");
    let from = s.with(|e, _| {
        let layout = e.slide_layout().expect("the slide is laid out");
        let f = e.slide().shapes.iter().find(|sh| matches!(&sh.kind, ibis_doc_model::ShapeKind::Slicer(x) if x.name == "商品")).map(|sh| sh.frame).unwrap();
        (f32::from(layout.origin.x + layout.to_px(f.x + f.width / 3)), f32::from(layout.origin.y + layout.to_px(f.y + 152_400)))
    });
    s.drag(from, (from.0 + 60.0, from.1 + 40.0));
    let after = frame_of(&mut s, "商品");
    assert!(after.x > before.x && after.y > before.y, "moved: {before:?} -> {after:?}");
    // The table's slicer on the other sheet: 東 alone.
    let table_slide = s.with(|e, _| e.presentation().slides.iter().find(|sl| sl.name.as_deref() == Some("データ")).map(|sl| sl.id)).expect("the sheet");
    s.update(move |e, window, cx| e.select_slide(table_slide, window, cx));
    s.screenshot("table-sheet");
    let items = slicer_items_of(&mut s, "地域");
    let east = items.iter().position(|i| i.0 == "東").expect("東");
    let at = slicer_button(&mut s, "地域", east);
    s.click(at);
    let kept: Vec<String> = slicer_items_of(&mut s, "地域").into_iter().filter(|i| i.1).map(|i| i.0).collect();
    assert_eq!(kept, ["東"]);
    let hidden = s.with(|e, _| e.current_sheet().map(|(_, sh)| sh.filtered_rows.len()).unwrap_or(0));
    assert!(hidden > 0, "rows of the other regions are hidden");
    s.screenshot("table-filtered");
    // Saved and opened again: the filters and the new place stay.
    s.save_as("slicer001.xlsx");
    let reopened = s.reopen();
    let pivot_sheet = reopened.slides.iter().filter_map(sheet_of_slide).find(|sh| !sh.pivots.is_empty()).expect("the pivot sheet");
    assert!(pivot_sheet.pivots[0].fields[1].items.iter().any(|it| it.hidden));
    let table_sheet = reopened.slides.iter().filter_map(sheet_of_slide).find(|sh| !sh.tables.is_empty()).expect("the table sheet");
    assert!(table_sheet.tables[0].filter.as_ref().is_some_and(|f| !f.columns.is_empty()));
    let moved = reopened.slides.iter().flat_map(|sl| sl.shapes.iter()).find_map(|sh| matches!(&sh.kind, ibis_doc_model::ShapeKind::Slicer(x) if x.name == "商品").then_some(sh.frame)).expect("the slicer again");
    assert!((moved.x - after.x).abs() < 20_000 && (moved.y - after.y).abs() < 20_000, "{moved:?} vs {after:?}");
}

/// 挿入 > グラフ over the selected cells (XLSX-CHART): a column chart of a
/// small table, then a pie chart of the table around the selected cell;
/// their series follow the cells, saved and opened again with the chart
/// parts and no workbook of their own.
fn chart001_insert_over_cells() {
    let mut s = new_sheet();
    for (row, values) in [["", "4月", "5月"], ["北", "10", "40"], ["南", "20", "30"], ["東", "30", "20"]].into_iter().enumerate() {
        for (col, value) in values.into_iter().enumerate() {
            if value.is_empty() {
                continue;
            }
            s.click_cell(&ibis_doc_model::cell_name(col as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    // The headings of the first column are text; the corner is empty.
    s.click_cell("A1");
    s.key("shift-right");
    s.key("shift-right");
    s.key("shift-down");
    s.key("shift-down");
    s.key("shift-down");
    s.menu("pptx.insert-chart.column");
    let charts = |s: &mut Scenario| s.with(|e, _| e.slide().shapes.iter().filter_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()), _ => None }).collect::<Vec<_>>());
    let made = charts(&mut s);
    assert_eq!(made.len(), 1);
    let series: Vec<_> = made[0].groups.iter().flat_map(|g| g.series.iter()).collect();
    assert_eq!(series.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["4月", "5月"]);
    assert_eq!(series[0].categories, ["北", "南", "東"]);
    assert_eq!(series[1].values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(40.0), Some(30.0), Some(20.0)]);
    s.screenshot("column");
    // A single cell selected: the table around it.
    s.click_cell("B2");
    s.menu("pptx.insert-chart.pie");
    assert_eq!(charts(&mut s).len(), 2);
    s.screenshot("pie");
    // Numbers in the first column: a scatter chart takes them as X values.
    for (row, values) in [["x", "y"], ["1", "10"], ["2", "25"], ["3", "15"]].into_iter().enumerate() {
        for (col, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(col as u32 + 6, row as u32));
            s.enter(value, "enter");
        }
    }
    s.click_cell("G1");
    s.key("shift-right");
    s.key("shift-down");
    s.key("shift-down");
    s.key("shift-down");
    s.menu("pptx.insert-chart.scatter");
    let scattered = charts(&mut s);
    assert_eq!(scattered.len(), 3);
    let last = scattered.last().unwrap().groups.iter().flat_map(|g| g.series.iter()).next().unwrap().clone();
    assert_eq!(last.name, "y");
    assert_eq!(last.x_values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(1.0), Some(2.0), Some(3.0)]);
    s.screenshot("scatter");
    // Undo takes the scatter chart away again, redo brings it back.
    s.key("cmd-z");
    assert_eq!(charts(&mut s).len(), 2);
    s.key("cmd-shift-z");
    assert_eq!(charts(&mut s).len(), 3);
    s.save_as("chart001.xlsx");
    let reopened = s.reopen();
    let again: Vec<_> = reopened.slides.iter().flat_map(|sl| sl.shapes.iter()).filter_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some(c.clone()), _ => None }).collect();
    assert!(again.len() >= 3, "{}", again.len());
    let first = &again[again.len() - 3];
    assert_eq!(first.groups.iter().flat_map(|g| g.series.iter()).count(), 2);
}

/// XLSX-TABLE-002・003・004・007, テーブルのデザイン: the table renamed (a
/// formula naming it follows), 集計行 on (集計, the last column's SUBTOTAL)
/// and a column's function changed, another style and options, the header
/// row off and on; saved and opened again.
fn table_design() {
    let mut s = new_sheet();
    for (row, values) in [["品名", "数量", "金額"], ["a", "1", "100"], ["b", "5", "250"], ["c", "3", "60"]].into_iter().enumerate() {
        for (col, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(col as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    s.click_cell("E1");
    s.enter("=SUM(テーブル1[金額])", "enter");
    s.click_cell("A2");
    s.key("cmd-t");
    s.key("enter");
    assert_eq!(s.value("E1"), CellValue::Number(410.0));
    s.menu("xlsx.table-design");
    let fill = |s: &mut Scenario, r: &str| s.with(|e, _| { let sh = sheet_of_slide(e.slide()).unwrap(); let (c, row) = parse(r); sh.cell(row, c).and_then(|x| sh.styles[x.style as usize].fill) });
    let table = |s: &mut Scenario| s.with(|e, _| sheet_of_slide(e.slide()).unwrap().tables[0].clone());
    // Renamed: the formula follows.
    s.update(|e, window, cx| {
        if let Some(p) = e.wb_mut().panels.table_design.as_ref() {
            let input = p.name.clone();
            input.update(cx, |i, cx| i.set_value("売上", window, cx));
        }
        e.rename_table(cx);
    });
    assert_eq!(table(&mut s).name, "売上");
    assert_eq!(s.formula("E1").as_deref(), Some("SUM(売上[金額])"));
    // A name not to be taken.
    s.update(|e, window, cx| {
        if let Some(p) = e.wb_mut().panels.table_design.as_ref() {
            let input = p.name.clone();
            input.update(cx, |i, cx| i.set_value("A1", window, cx));
        }
        e.rename_table(cx);
    });
    assert_eq!(table(&mut s).name, "売上");
    // 集計行.
    s.update(|e, _, cx| e.toggle_totals_row(0, cx));
    assert_eq!(table(&mut s).range.last_row, 4);
    assert_eq!(s.value("A5"), CellValue::Text("集計".into()));
    assert_eq!(s.formula("C5").as_deref(), Some("SUBTOTAL(109,売上[金額])"));
    assert_eq!(s.value("C5"), CellValue::Number(410.0));
    s.update(|e, _, cx| e.set_table_total(0, 1, Some("average"), cx));
    assert_eq!(s.value("B5"), CellValue::Number(3.0));
    // Another style, its options.
    let before = fill(&mut s, "A1");
    s.update(|e, _, cx| e.set_table_style(0, |st| st.name = "TableStyleMedium7".into(), cx));
    assert_ne!(fill(&mut s, "A1"), before, "the heading in the new style's colour");
    s.update(|e, _, cx| e.set_table_style(0, |st| st.row_stripes = false, cx));
    assert_eq!(fill(&mut s, "B2"), fill(&mut s, "B3"), "no stripes");
    s.update(|e, _, cx| e.set_table_style(0, |st| st.first_column = true, cx));
    assert_eq!(table(&mut s).style.unwrap().first_column, true);
    s.screenshot("design");
    // The header row off and on again.
    s.update(|e, _, cx| e.toggle_header_row(0, cx));
    assert_eq!((table(&mut s).header_rows, table(&mut s).range.first_row), (0, 1));
    assert_eq!(s.value("A1"), CellValue::Empty);
    s.update(|e, _, cx| e.toggle_header_row(0, cx));
    assert_eq!(s.value("C1"), CellValue::Text("金額".into()));
    assert_eq!(s.value("E1"), CellValue::Number(410.0));
    s.save_as("table-design.xlsx");
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[1]).unwrap();
    let t = &sh.tables[0];
    assert_eq!((t.name.as_str(), t.totals_rows, t.header_rows), ("売上", 1, 1));
    assert_eq!(t.totals.get(1).and_then(|x| x.function.clone()).as_deref(), Some("average"));
    assert_eq!(t.totals.first().and_then(|x| x.label.clone()).as_deref(), Some("集計"));
    let st = t.style.clone().unwrap();
    assert_eq!((st.name.as_str(), st.row_stripes, st.first_column), ("TableStyleMedium7", false, true));
    assert_eq!(sh.cell(0, 4).and_then(|c| c.formula.clone()).as_deref(), Some("SUM(売上[金額])"));
}

/// XLSX-FORMULA-024, 数式オートコンプリート: `=su` lists SUBSTITUTE,
/// SUBTOTAL, SUM…; ↓ ↓ and Tab put in `SUM(`; inside it, its arguments'
/// hint; Esc closes the list and the entry goes on.
fn formula024_autocomplete() {
    let mut s = new_sheet();
    s.click_cell("A1");
    s.type_text("=su");
    let list = |s: &mut Scenario| s.with(|e, _| e.completion_list().map(|(_, _, l)| l.into_iter().map(|c| c.text).collect::<Vec<_>>()));
    let shown = list(&mut s).expect("the list");
    assert_eq!(&shown[..3], &["SUBSTITUTE".to_string(), "SUBTOTAL".to_string(), "SUM".to_string()]);
    s.key("down");
    s.key("down");
    s.screenshot("list");
    s.key("tab");
    assert_eq!(s.with(|e, _| e.cell_edit().map(|c| c.text.clone())).as_deref(), Some("=SUM("));
    assert!(list(&mut s).is_none(), "closed once put in");
    assert!(s.with(|e, _| e.cell_edit().map(|c| super::autocomplete::call_at(&c.text, c.caret_byte()))).flatten().is_some(), "the hint of SUM's arguments");
    s.type_text("1,2)");
    s.screenshot("typed");
    s.key("enter");
    assert_eq!(s.value("A1"), CellValue::Number(3.0));
    // Esc: the list closes, the entry goes on.
    s.click_cell("A2");
    s.type_text("=ave");
    assert!(list(&mut s).is_some());
    s.key("escape");
    assert!(list(&mut s).is_none());
    assert!(s.with(|e, _| e.cell_edit().is_some()), "still typing");
    s.type_text("rage(4,6)");
    s.key("enter");
    assert_eq!(s.value("A2"), CellValue::Number(5.0));
    // Tab with no list: to the next cell as before.
    s.click_cell("A3");
    s.type_text("7");
    s.key("tab");
    assert_eq!(s.selected(), "B3");
}

/// XSC-25, 条件付き書式 from the menu: 上位 10 項目 and a data bar on one
/// column, a colour scale on another, an icon set, 重複する値, 指定の値より
/// 大きい… through the panel; saved and read back.
fn xsc25_conditional_quick() {
    let mut s = new_sheet();
    for r in 0..12u32 {
        for (c, v) in [(0, (r * 7 % 12) as f64), (1, r as f64), (2, (r % 4) as f64)] {
            s.click_cell(&ibis_doc_model::cell_name(c, r));
            s.enter(&v.to_string(), "enter");
        }
    }
    let select = |s: &mut Scenario, from: &str, to: &str| {
        s.click_cell(from);
        let (c, r) = parse(to);
        let (c0, r0) = parse(from);
        for _ in r0..r {
            s.key("shift-down");
        }
        for _ in c0..c {
            s.key("shift-right");
        }
    };
    select(&mut s, "A1", "A12");
    s.menu("xlsx.cf-top10-percent");
    s.menu("xlsx.cf-bar");
    select(&mut s, "B1", "B12");
    s.menu("xlsx.cf-scale");
    select(&mut s, "C1", "C12");
    s.menu("xlsx.cf-icons");
    s.menu("xlsx.cf-duplicate");
    select(&mut s, "B1", "B12");
    s.menu("xlsx.cf-greater");
    s.type_text("8");
    s.update(|e, _, cx| e.apply_conditional(cx));
    s.screenshot("conditional-quick");
    let kinds = |p: &ibis_doc_model::Presentation| sheet_of_slide(&p.slides[1]).unwrap().conditional.iter().map(|f| std::mem::discriminant(&f.rules[0].kind)).count();
    let n = s.with(|e, _| kinds(e.presentation()));
    assert_eq!(n, 6);
    s.save_as("xsc25-quick.xlsx");
    let saved = s.reopen();
    assert_eq!(kinds(&saved), 6);
    let sheet = sheet_of_slide(&saved.slides[1]).unwrap();
    assert!(matches!(sheet.conditional[0].rules[0].kind, ibis_doc_model::CfKind::Top10 { rank: 10, percent: true, bottom: false }));
    assert!(matches!(&sheet.conditional[5].rules[0].kind, ibis_doc_model::CfKind::CellIs { operator, formulas } if operator == "greaterThan" && formulas == &vec!["8".to_string()]));
}

/// XSC-26, 範囲に変換: the table goes, its look stays, a SUM of its column
/// becomes a range; undone, the table is back.
fn xsc26_table_to_range() {
    let mut s = new_sheet();
    for (row, values) in [["名前", "点"], ["a", "1"], ["b", "5"], ["c", "3"]].into_iter().enumerate() {
        for (col, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(col as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    s.click_cell("A2");
    s.key("cmd-t");
    s.key("enter");
    s.click_cell("D1");
    s.enter("=SUM(テーブル1[点])", "enter");
    s.click_cell("A2");
    let fill = s.with(|e, _| { let sh = sheet_of_slide(e.slide()).unwrap(); sh.cell(0, 0).and_then(|x| sh.styles[x.style as usize].fill) });
    s.menu("xlsx.table-to-range");
    assert!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().tables.is_empty()));
    assert_eq!(s.formula("D1").as_deref(), Some("SUM($B$2:$B$4)"));
    assert_eq!(s.value("D1"), CellValue::Number(9.0));
    assert_eq!(s.with(|e, _| { let sh = sheet_of_slide(e.slide()).unwrap(); sh.cell(0, 0).and_then(|x| sh.styles[x.style as usize].fill) }), fill, "the look stays");
    s.save_as("xsc26-to-range.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[1]).unwrap().clone();
    assert!(sheet.tables.is_empty());
    assert_eq!(sheet.cell(0, 0).and_then(|x| sheet.styles[x.style as usize].fill), fill);
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().tables.len()), 1);
}

/// XSC-26, a table's ▼: a new table has them; one keeps 5 and up in 点,
/// sorted descending from its panel; saved with the table's autoFilter and
/// read back; フィルター in the table takes its ▼ off (the rows back).
fn xsc26_table_filter() {
    use crate::xlsx::filter::FilterTarget;
    let mut s = new_sheet();
    for (row, values) in [["名前", "点"], ["a", "1"], ["b", "5"], ["c", "3"], ["d", "7"]].into_iter().enumerate() {
        for (col, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(col as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    s.click_cell("A2");
    s.key("cmd-t");
    s.key("enter");
    assert!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().tables[0].filter.is_some()), "the ▼ of a new table");
    // The ▼ of 点 (B1), clicked: the table's panel.
    let corner = s.with(|e, _| e.cell_corner(1, 0)).unwrap();
    s.click((f32::from(corner.x) - 6.0, f32::from(corner.y) - 6.0));
    assert_eq!(s.with(|e, _| e.wb().filter_panel.as_ref().map(|p| p.target)), Some(FilterTarget::Table(0)));
    s.update(|e, _, cx| e.filter_panel_apply(Some(ibis_doc_model::ColumnFilter::Custom { and: false, conditions: vec![("greaterThanOrEqual".into(), "5".into())] }), cx));
    assert_eq!(s.size_pt("2"), 0.0);
    assert_eq!(s.size_pt("4"), 0.0);
    assert!(s.size_pt("3") > 0.0 && s.size_pt("5") > 0.0);
    s.click((f32::from(corner.x) - 6.0, f32::from(corner.y) - 6.0));
    s.update(|e, _, cx| e.filter_panel_sort(ibis_doc_model::sheet_command::SortBy::Value(false), cx));
    assert_eq!(s.value("A3"), CellValue::Text("d".into()));
    s.screenshot("table-filter");
    s.save_as("xsc26-table-filter.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[1]).unwrap().clone();
    let f = sheet.tables[0].filter.clone().expect("saved");
    assert_eq!(f.columns.len(), 1);
    assert!(sheet.filtered_rows.len() == 2, "{:?}", sheet.filtered_rows);
    s.click_cell("A1");
    s.menu("xlsx.filter");
    assert!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().tables[0].filter.is_none()));
    assert!(s.size_pt("2") > 0.0, "every row back");
}

/// XSC-26, テーブル: ⌘T on a list makes a table (its first row the headings,
/// in TableStyleMedium2); a value typed under it and a formula typed right
/// of it grow it (the formula filling the new column); a formula with
/// structured references follows; saved and read back, each step undone.
fn xsc26_make_table() {
    let mut s = new_sheet();
    for (row, values) in [["名前", "点"], ["a", "1"], ["b", "5"], ["c", "3"]].into_iter().enumerate() {
        for (col, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(col as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    s.click_cell("A2");
    s.key("cmd-t");
    assert!(s.with(|e, _| e.wb().table_dialog.as_ref().is_some_and(|d| d.header)), "the dialog, the headings guessed");
    s.screenshot("table-dialog");
    s.key("enter");
    let table = |s: &mut Scenario| s.with(|e, _| sheet_of_slide(e.slide()).unwrap().tables.first().cloned());
    let t = table(&mut s).expect("a table");
    assert_eq!((t.name.as_str(), t.range.last_row, t.range.last_column, t.columns.clone()), ("テーブル1", 3, 1, vec!["名前".to_string(), "点".to_string()]));
    // The heading row in the style's accent 1, the stripes on the first data row.
    let fill = |s: &mut Scenario, r: &str| s.with(|e, _| { let sh = sheet_of_slide(e.slide()).unwrap(); let (c, row) = parse(r); sh.cell(row, c).and_then(|x| sh.styles[x.style as usize].fill) });
    let accent1 = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().theme_colors.iter().find(|(n, _)| n == "accent1").map(|(_, c)| *c));
    assert!(accent1.is_some());
    assert_eq!(fill(&mut s, "A1"), accent1);
    assert!(fill(&mut s, "B2").is_some() && fill(&mut s, "B3").is_none(), "row stripes");
    // Typed under it: one row more.
    s.click_cell("A5");
    s.enter("d", "enter");
    assert_eq!(table(&mut s).unwrap().range.last_row, 4);
    s.click_cell("B5");
    s.enter("4", "enter");
    // A formula right of it: a column more, 列3, filled down.
    s.click_cell("C2");
    s.enter("=B2*2", "enter");
    let t = table(&mut s).unwrap();
    assert_eq!((t.range.last_column, t.columns[2].as_str()), (2, "列3"));
    // The old right edge an inner line now (none in this style), the new one on C.
    let right = |s: &mut Scenario, r: &str| s.with(|e, _| { let sh = sheet_of_slide(e.slide()).unwrap(); let (c, row) = parse(r); sh.cell(row, c).and_then(|x| sh.styles[x.style as usize].right) });
    assert!(right(&mut s, "B3").is_none() && right(&mut s, "C3").is_some());
    for (r, n) in [("C3", 10.0), ("C4", 6.0), ("C5", 8.0)] {
        assert_eq!(s.value(r), CellValue::Number(n), "{r}");
    }
    s.click_cell("E1");
    s.enter("=SUM(テーブル1[点])", "enter");
    assert_eq!(s.value("E1"), CellValue::Number(13.0));
    s.screenshot("table-grown");
    s.save_as("xsc26-table.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[1]).unwrap().clone();
    let t = sheet.tables.first().expect("saved");
    assert_eq!((t.name.as_str(), t.range.last_row, t.range.last_column, t.style.as_ref().map(|x| x.name.as_str())), ("テーブル1", 4, 2, Some("TableStyleMedium2")));
    assert_eq!(sheet.cell(0, 4).unwrap().value, CellValue::Number(13.0));
    // Undone: the formula, the column, …, the table.
    s.click_cell("A1");
    for _ in 0..4 {
        s.key("cmd-z");
    }
    assert_eq!(table(&mut s).unwrap().range.last_column, 1);
}

/// XLSX-TABLE-006: A sheet with a table: inserting and deleting rows and
/// columns adjusts the table's range, columns, and filter; saving and reopening
/// preserves the updated structure; undo and redo work correctly.
fn table006_structure_edits() {
    let mut s = new_sheet();
    for (row, values) in [["項目", "数値"], ["A", "10"], ["B", "20"], ["C", "30"]].into_iter().enumerate() {
        for (col, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(col as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    s.click_cell("A1");
    s.key("cmd-t");
    s.key("enter");
    let table = |s: &mut Scenario| s.with(|e, _| sheet_of_slide(e.slide()).unwrap().tables.first().cloned());
    let t = table(&mut s).expect("a table created");
    assert_eq!((t.name.as_str(), t.range.last_row, t.range.last_column), ("テーブル1", 3, 1));

    // 1. Insert row above at row 1 (inside data rows): expands table to last_row=4
    s.click_cell("A2");
    s.menu("xlsx.insert-row-above");
    let t = table(&mut s).unwrap();
    assert_eq!(t.range.last_row, 4);

    // 2. Delete row at row 2: shrinks table back to last_row=3
    s.click_cell("A3");
    s.menu("xlsx.delete-row");
    let t = table(&mut s).unwrap();
    assert_eq!(t.range.last_row, 3);

    // 3. Insert column left at col 1 (B列、テーブル内): expands table to last_column=2, columns.len()=3
    s.click_cell("B1");
    s.menu("xlsx.insert-column-left");
    let t = table(&mut s).unwrap();
    assert_eq!((t.range.last_column, t.columns.len()), (2, 3));

    // 4. Delete column at col 1: shrinks table back to last_column=1, columns.len()=2
    s.click_cell("B1");
    s.menu("xlsx.delete-column");
    let t = table(&mut s).unwrap();
    assert_eq!((t.range.last_column, t.columns.len()), (1, 2));

    // 5. Save and reopen roundtrip
    s.save_as("table006-structure.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[1]).unwrap().clone();
    let t = sheet.tables.first().expect("saved table");
    assert_eq!((t.name.as_str(), t.range.last_row, t.range.last_column, t.columns.len()), ("テーブル1", 3, 1, 2));

    // 6. Undo/Redo
    s.key("cmd-z"); // undo column delete
    assert_eq!(table(&mut s).unwrap().columns.len(), 3);
    s.key("cmd-shift-z"); // redo column delete
    assert_eq!(table(&mut s).unwrap().columns.len(), 2);
}

/// XSC-05, threaded comments: the pointer on a cell with a thread shows
/// its posts (the placeholder note Excel keeps beside it not), on a plain
/// cell nothing; rows inserted above move the thread.
fn xsc05_threaded_comments() {
    let mut s = Scenario::open("test/editing/xlsx-threads.xlsx");
    let corner = s.with(|e, _| e.cell_corner(1, 1)).unwrap();
    s.hover((f32::from(corner.x) - 10.0, f32::from(corner.y) - 8.0));
    assert_eq!(s.with(|e, _| e.wb().comment_hover), Some((1, 1)));
    s.screenshot("thread-card");
    let plain = s.with(|e, _| e.cell_corner(0, 12)).unwrap();
    s.hover((f32::from(plain.x) - 10.0, f32::from(plain.y) - 8.0));
    assert_eq!(s.with(|e, _| e.wb().comment_hover), None);
    s.click_cell("A1");
    s.menu("xlsx.insert-row-above");
    assert_eq!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().threads.iter().map(|t| (t.row, t.column)).collect::<Vec<_>>()), vec![(2, 1), (4, 1)]);
}

/// XSC-24 with the ▼ panel as Excel's, on a sales log of 3,000 rows: the
/// panel opens, the search narrows the list and Enter keeps what it found;
/// (すべて選択) and one value; テキスト フィルター 「を含む」; トップテン;
/// 今月; a colour; sorting from the panel; clearing; saved and read back.
/// Each step answers within half a second.
fn xsc24_filter_panel() {
    use crate::xlsx::filter::{ColumnKind, Section};
    use ibis_doc_model::ColumnFilter;
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let staff = ["佐藤", "鈴木", "高橋", "田中", "伊藤", "渡辺", "山本", "中村"];
    let customers = ["青葉商事", "みどり工業", "さくら商事", "ひかり物産", "あおい建設"];
    let mut csv = String::from("日付,担当,顧客,金額\n");
    for i in 0..3000 {
        let (month, day) = (7 + i % 4, 1 + i % 28);
        csv.push_str(&format!("2026/{month}/{day},{},{},{}\n", staff[i % 8], customers[i % 5], 1000 + i * 7));
    }
    std::fs::write(dir.join("xsc24-sales.csv"), csv).unwrap();
    let mut s = Scenario::open("target/scenario-input/xsc24-sales.csv");
    s.click_cell("A1");
    s.menu("xlsx.filter");
    assert_eq!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().auto_filter.as_ref().map(|f| f.range.last_row)), Some(3000));
    let visible = |s: &mut Scenario| s.with(|e, _| { let sh = sheet_of_slide(e.slide()).unwrap(); (1..=3000u32).filter(|r| sh.rows[*r as usize] > 0).count() });
    let timed = |s: &mut Scenario, what: &str, f: &mut dyn FnMut(&mut Scenario)| {
        let started = crate::clock::Instant::now();
        f(s);
        s.settle();
        let took = started.elapsed().as_millis();
        eprintln!("xsc24_filter_panel: {what} {took} ms");
        assert!(took < 500, "{what} took {took} ms");
    };
    // The ▼ of 担当 (B1), clicked: the panel, every name ticked.
    let corner = s.with(|e, _| e.cell_corner(1, 0)).unwrap();
    timed(&mut s, "opening the panel", &mut |s| s.click((f32::from(corner.x) - 6.0, f32::from(corner.y) - 6.0)));
    let (values, kind) = s.with(|e, _| { let p = e.wb().filter_panel.as_ref().unwrap(); (p.values.len(), p.kind) });
    assert_eq!((values, kind), (8, ColumnKind::Text));
    s.screenshot("filter-panel");
    // Typed into the search box, Enter: only 佐藤.
    timed(&mut s, "searching", &mut |s| s.type_text("佐"));
    assert_eq!(s.with(|e, cx| e.wb().filter_panel.as_ref().unwrap().shown(&e.wb().filter_panel.as_ref().unwrap().search.read(cx).value())), vec!["佐藤".to_string()]);
    timed(&mut s, "OK", &mut |s| s.key("enter"));
    assert!(s.with(|e, _| e.wb().filter_panel.is_none()));
    assert_eq!(visible(&mut s), 375);
    // Again: (すべて選択) off, 鈴木 and 田中 on, OK.
    s.click((f32::from(corner.x) - 6.0, f32::from(corner.y) - 6.0));
    s.update(|e, _, cx| {
        e.filter_panel_tick(None, false, cx);
        e.filter_panel_tick(Some("鈴木"), true, cx);
        e.filter_panel_tick(Some("田中"), true, cx);
        e.filter_panel_ok(cx);
    });
    assert_eq!(visible(&mut s), 750);
    // 「"担当" からフィルターをクリア」.
    s.click((f32::from(corner.x) - 6.0, f32::from(corner.y) - 6.0));
    s.update(|e, _, cx| e.filter_panel_apply(None, cx));
    assert_eq!(visible(&mut s), 3000);
    // 顧客 (C1): テキスト フィルター > 指定の値を含む… 「商事」.
    let corner_c = s.with(|e, _| e.cell_corner(2, 0)).unwrap();
    s.click((f32::from(corner_c.x) - 6.0, f32::from(corner_c.y) - 6.0));
    s.update(|e, window, cx| {
        e.filter_panel_section(Section::Conditions, cx);
        e.filter_panel_condition([Some(10), None], false, window, cx);
    });
    s.type_text("商事");
    s.screenshot("filter-condition-dialog");
    s.update(|e, _, cx| e.filter_dialog_ok(cx));
    assert_eq!(visible(&mut s), 1200);
    // 金額 (D1): 数値フィルター > トップテン…, the top 10 items.
    let corner_d = s.with(|e, _| e.cell_corner(3, 0)).unwrap();
    s.click((f32::from(corner_d.x) - 6.0, f32::from(corner_d.y) - 6.0));
    assert_eq!(s.with(|e, _| e.wb().filter_panel.as_ref().unwrap().kind), ColumnKind::Number);
    s.update(|e, window, cx| e.filter_panel_top(window, cx));
    s.update(|e, _, cx| e.filter_dialog_ok(cx));
    // The top 10 amounts among all rows, of which the 商事 ones show.
    let kept = s.with(|e, _| { let sh = sheet_of_slide(e.slide()).unwrap(); (1..=3000u32).filter(|r| sh.rows[*r as usize] > 0).map(|r| sh.cell(r, 2).unwrap().text.clone()).collect::<Vec<_>>() });
    assert!(!kept.is_empty() && kept.len() <= 10 && kept.iter().all(|t| t.contains("商事")), "{kept:?}");
    s.click((f32::from(corner_d.x) - 6.0, f32::from(corner_d.y) - 6.0));
    s.update(|e, _, cx| e.filter_panel_apply(None, cx));
    s.click((f32::from(corner_c.x) - 6.0, f32::from(corner_c.y) - 6.0));
    s.update(|e, _, cx| e.filter_panel_apply(None, cx));
    assert_eq!(visible(&mut s), 3000);
    // 日付 (A1): 日付フィルター > 今月, today being 2026-10-07.
    ibis_calc::eval::convert::set_today(ibis_calc::eval::convert::serial_from_date(2026, 10, 7, false) - 25569);
    let corner_a = s.with(|e, _| e.cell_corner(0, 0)).unwrap();
    s.click((f32::from(corner_a.x) - 6.0, f32::from(corner_a.y) - 6.0));
    assert_eq!(s.with(|e, _| e.wb().filter_panel.as_ref().unwrap().kind), ColumnKind::Date);
    s.update(|e, _, cx| e.filter_panel_apply(Some(ColumnFilter::Period("thisMonth".into())), cx));
    assert_eq!(visible(&mut s), 750);
    ibis_calc::eval::convert::set_today(i64::MIN);
    // Sorted from the panel: 金額 descending, the hidden rows staying put.
    s.click((f32::from(corner_d.x) - 6.0, f32::from(corner_d.y) - 6.0));
    timed(&mut s, "sorting", &mut |s| s.update(|e, _, cx| e.filter_panel_sort(ibis_doc_model::sheet_command::SortBy::Value(false), cx)));
    let amounts = s.with(|e, _| { let sh = sheet_of_slide(e.slide()).unwrap(); (1..=3000u32).filter(|r| sh.rows[*r as usize] > 0).map(|r| match sh.cell(r, 3).unwrap().value { CellValue::Number(n) => n, _ => 0.0 }).collect::<Vec<_>>() });
    assert!(amounts.windows(2).all(|w| w[0] >= w[1]) && amounts.len() == 750);
    s.save_as("xsc24-panel.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.auto_filter.clone().map(|f| f.columns), Some(vec![(0, ColumnFilter::Period("thisMonth".into()))]));
}

/// XSC-23: the shops sorted by April, descending (rows move with their
/// totals' formulas); back by name; the grand totals stay right.
fn xsc23_sort() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // A3:E8 by April (the sort command as the menu gives it), the heading kept.
    s.click_cell("A3");
    for _ in 0..5 {
        s.key("shift-down");
    }
    for _ in 0..4 {
        s.key("shift-right");
    }
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let range = e.selected_range().unwrap();
        e.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::Sort { slide, range, keys: vec![(1, false)], header: true })]).unwrap();
        cx.notify();
    });
    assert_eq!(s.value("A4"), CellValue::Text("駅前店".into()));
    assert_eq!(s.value("A8"), CellValue::Text("西町店".into()));
    assert_eq!(s.formula("E4").as_deref(), Some("SUM(B4:D4)"), "the row's formula moves with it");
    assert_eq!(s.value("E4"), CellValue::Number(6_660_000.0));
    assert_eq!(s.value("B9"), CellValue::Number(6_620_000.0), "the total stays");
    // By the selected cell's column from the menu (the table around it, its heading row kept).
    s.click_cell("A5");
    s.menu("xlsx.sort");
    assert_eq!(s.value("A3"), CellValue::Text("店舗".into()), "the heading stays");
    s.save_as("xsc23.xlsx");
    s.key("cmd-z");
    s.key("cmd-z");
    assert_eq!(s.value("A4"), CellValue::Text("北町店".into()));
}

/// XSC-24: a filter on the shops' table: April above its average kept (the
/// other rows hidden), SUBTOTAL(109,…) sums what is shown; one shop by
/// name; the filter taken off; saved.
fn xsc24_filter() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("G4");
    s.enter("=SUBTOTAL(109,B4:B8)", "enter");
    s.click_cell("A3");
    for _ in 0..5 {
        s.key("shift-down");
    }
    for _ in 0..4 {
        s.key("shift-right");
    }
    s.menu("xlsx.filter");
    assert!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().auto_filter.is_some()));
    s.screenshot("filter");
    // The ▼ of 4月 (B3): its menu opens.
    let corner = s.with(|e, _| e.cell_corner(1, 2)).unwrap();
    s.click((f32::from(corner.x) - 6.0, f32::from(corner.y) - 6.0));
    assert!(s.with(|e, _| e.sheet_state().unwrap().filter_panel.is_some()), "the panel opens");
    s.key("escape");
    s.update(|e, _, cx| e.filter_column(1, Some(ibis_doc_model::ColumnFilter::AboveAverage(true)), cx));
    // April: 1,250,000 980,000 1,530,000 760,000 2,100,000, average 1,324,000.
    assert_eq!(s.size_pt("4"), 0.0);
    assert!(s.size_pt("6") > 0.0 && s.size_pt("8") > 0.0);
    assert_eq!(s.value("G4"), CellValue::Number(3_630_000.0), "SUBTOTAL leaves the hidden rows out");
    s.update(|e, _, cx| e.filter_column(0, Some(ibis_doc_model::ColumnFilter::Values(vec!["駅前店".into()])), cx));
    assert_eq!(s.size_pt("6"), 0.0);
    assert_eq!(s.value("G4"), CellValue::Number(2_100_000.0));
    s.save_as("xsc24.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.auto_filter.as_ref().map(|f| f.columns.len()), Some(2));
    assert_eq!(sheet.rows[5], 0);
    s.menu("xlsx.filter");
    assert!(s.size_pt("4") > 0.0, "off: every row back");
    assert_eq!(s.value("G4"), CellValue::Number(6_620_000.0));
}

/// Sidebar fields and buttons are driven with real window clicks/typing.
fn cf_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().conditional_edit.as_ref().unwrap().ui.borrow().get(&id).unwrap());
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}
fn cf_field(s: &mut Scenario, id: &str, text: &str) {
    cf_click(s, id);
    s.key("cmd-a");
    s.type_text(text);
}
fn cf_model(s: &mut Scenario) -> Vec<ibis_doc_model::ConditionalFormat> {
    s.with(|e, _| sheet_of_slide(e.slide()).unwrap().conditional.clone())
}
fn cf_reads(s: &mut Scenario, name: &str, expected: &[ibis_doc_model::ConditionalFormat]) {
    s.save_as(name);
    let p = s.reopen();
    assert_eq!(sheet_of_slide(&p.slides[1]).unwrap().conditional, expected);
}
fn cf_drawn(s: &mut Scenario, row: u32, fill: u32) {
    s.with(|e, _| {
        let (sheet, _) = super::conditional::apply(sheet_of_slide(e.slide()).unwrap()).unwrap();
        let c = sheet.cell(row, 0).unwrap();
        assert_eq!(sheet.styles[c.style as usize].fill, Some(ibis_doc_model::Rgb(fill)));
    });
}
/// XSC-25: create, edit threshold/colour/range, overlapping priorities and
/// stopIfTrue, delete; each undo/redo and save/reload keeps the rules.
fn xsc25_conditional() {
    let mut s = new_sheet();
    for v in ["5", "15", "25"] { s.enter(v, "enter"); }
    s.click_cell("A1"); s.key("shift-down"); s.key("shift-down");
    s.menu("xlsx.conditional");
    cf_field(&mut s, "formula", "A1>10");
    cf_field(&mut s, "fill", "FFC7CE");
    cf_click(&mut s, "stop");
    s.screenshot("conditional-before-apply");
    cf_click(&mut s, "apply");
    let first = cf_model(&mut s);
    assert_eq!(first.len(), 1, "{:?}", s.with(|e, cx| e.wb().conditional_edit.as_ref().map(|p| (p.range.read(cx).value().to_string(), p.formula.read(cx).value().to_string(), p.fill.read(cx).value().to_string())))); assert!(first[0].rules[0].stop);
    cf_drawn(&mut s, 1, 0xFFC7CE);
    cf_reads(&mut s, "xsc25-create.xlsx", &first);
    s.click_cell("A1"); s.key("cmd-z"); assert!(cf_model(&mut s).is_empty());
    s.key("cmd-shift-z"); assert_eq!(cf_model(&mut s), first);
    s.menu("xlsx.conditional");
    cf_click(&mut s, "edit-0");
    cf_field(&mut s, "formula", "A2>20");
    cf_field(&mut s, "fill", "FFFF00");
    cf_field(&mut s, "range", "A2:A3");
    cf_click(&mut s, "apply");
    let edited = cf_model(&mut s);
    assert_eq!(edited[0].ranges[0].first_row, 1);
    assert_eq!(edited[0].rules[0].priority, first[0].rules[0].priority);
    cf_drawn(&mut s, 2, 0xFFFF00);
    cf_reads(&mut s, "xsc25-edit.xlsx", &edited);
    s.click_cell("A1"); s.key("cmd-z"); assert_eq!(cf_model(&mut s), first);
    s.key("cmd-shift-z"); assert_eq!(cf_model(&mut s), edited);
    s.click_cell("A2"); s.key("shift-down");
    s.menu("xlsx.conditional");
    cf_field(&mut s, "formula", "A2>0");
    cf_field(&mut s, "fill", "C6EFCE");
    cf_click(&mut s, "apply");
    let overlap = cf_model(&mut s);
    assert!(overlap[0].rules[0].priority < overlap[1].rules[0].priority);
    cf_drawn(&mut s, 1, 0xC6EFCE); // first is false, next applies
    cf_drawn(&mut s, 2, 0xFFFF00); // first is true and stops
    cf_reads(&mut s, "xsc25-overlap.xlsx", &overlap);
    s.click_cell("A2"); s.key("shift-down"); s.menu("xlsx.conditional");
    cf_click(&mut s, "clear"); assert!(cf_model(&mut s).is_empty());
    cf_reads(&mut s, "xsc25-delete.xlsx", &[]);
    s.click_cell("A2"); s.key("cmd-z"); assert_eq!(cf_model(&mut s), overlap);
    cf_reads(&mut s, "xsc25-delete-undo.xlsx", &overlap);
    s.key("cmd-shift-z"); assert!(cf_model(&mut s).is_empty());
    cf_reads(&mut s, "xsc25-delete-redo.xlsx", &[]);
}

/// Mixed references and the captured range follow a row inserted through UI.
fn xsc25_conditional_insert() {
    let mut s = new_sheet();
    s.enter("5", "enter"); s.enter("15", "enter"); s.enter("25", "enter");
    s.click_cell("A2"); s.key("shift-down"); s.menu("xlsx.conditional");
    cf_field(&mut s, "formula", "AND(A2>10,$A2>10,A$2>10,$A$2>10)");
    cf_click(&mut s, "apply");
    let before = cf_model(&mut s);
    cf_reads(&mut s, "xsc25-mixed-before.xlsx", &before);
    s.click_cell("A2"); s.menu("xlsx.insert-row-above");
    let after = cf_model(&mut s);
    assert_eq!(after[0].ranges[0], ibis_doc_model::CellRange { first_row: 2, last_row: 3, first_column: 0, last_column: 0 });
    assert_eq!(after[0].rules[0].kind, ibis_doc_model::CfKind::Expression("AND(A3>10,$A3>10,A$3>10,$A$3>10)".into()));
    cf_drawn(&mut s, 2, 0xFFC7CE); cf_drawn(&mut s, 3, 0xFFC7CE);
    cf_reads(&mut s, "xsc25-mixed-insert.xlsx", &after);
    s.key("cmd-z"); assert_eq!(cf_model(&mut s), before);
    cf_reads(&mut s, "xsc25-mixed-undo.xlsx", &before);
    s.key("cmd-shift-z"); assert_eq!(cf_model(&mut s), after);
    cf_reads(&mut s, "xsc25-mixed-redo.xlsx", &after);
}

/// The rule manager changes evaluation order and stop flags through clicks.
fn xsc25_conditional_manage() {
    let mut s = new_sheet(); s.enter("15", "enter"); s.click_cell("A1");
    s.menu("xlsx.conditional"); cf_field(&mut s, "formula", "A1>10");
    cf_field(&mut s, "fill", "FFC7CE"); cf_click(&mut s, "apply");
    s.menu("xlsx.conditional"); cf_field(&mut s, "formula", "A1>0");
    cf_field(&mut s, "fill", "C6EFCE"); cf_click(&mut s, "apply");
    let before = cf_model(&mut s); cf_drawn(&mut s, 0, 0xFFC7CE);
    s.menu("xlsx.conditional"); cf_click(&mut s, "down-0-0");
    let moved = cf_model(&mut s);
    assert_eq!(moved[0].rules[0].priority, 2); assert_eq!(moved[1].rules[0].priority, 1);
    cf_drawn(&mut s, 0, 0xC6EFCE); cf_reads(&mut s, "xsc25-manage-move.xlsx", &moved);
    s.click_cell("A1"); s.key("cmd-z"); assert_eq!(cf_model(&mut s), before);
    s.key("cmd-shift-z"); assert_eq!(cf_model(&mut s), moved);
    s.menu("xlsx.conditional"); cf_click(&mut s, "rule-stop-1-0");
    let stopped = cf_model(&mut s); assert!(stopped[1].rules[0].stop);
    cf_reads(&mut s, "xsc25-manage-stop.xlsx", &stopped);
    s.click_cell("A1"); s.key("cmd-z"); assert_eq!(cf_model(&mut s), moved);
    s.key("cmd-shift-z"); assert_eq!(cf_model(&mut s), stopped);
    s.menu("xlsx.conditional"); cf_click(&mut s, "up-0-0");
    let restored = cf_model(&mut s);
    assert_eq!(restored[0].rules[0].priority, 1); assert_eq!(restored[1].rules[0].priority, 2);
    cf_drawn(&mut s, 0, 0xFFC7CE); cf_reads(&mut s, "xsc25-manage-up.xlsx", &restored);
    s.screenshot("conditional-managed");
}

/// CF-018: Conditional formatting rule duplication and deletion through manager clicks.
fn cf018_duplicate_and_delete_rule() {
    let mut s = new_sheet(); s.enter("15", "enter"); s.click_cell("A1");
    s.menu("xlsx.conditional"); cf_field(&mut s, "formula", "A1>10");
    cf_field(&mut s, "fill", "FFC7CE"); cf_click(&mut s, "apply");
    s.menu("xlsx.conditional"); cf_field(&mut s, "formula", "A1>0");
    cf_field(&mut s, "fill", "C6EFCE"); cf_click(&mut s, "apply");

    let initial = cf_model(&mut s);
    assert_eq!(initial.len(), 2);
    assert_eq!(initial[0].rules[0].priority, 1);
    assert_eq!(initial[1].rules[0].priority, 2);

    // Duplicate rule 0 (A1>10)
    s.menu("xlsx.conditional");
    cf_click(&mut s, "duplicate-0-0");
    let duplicated = cf_model(&mut s);
    assert_eq!(duplicated.len(), 3);
    assert_eq!(duplicated[0].rules[0].priority, 1);
    assert_eq!(duplicated[1].rules[0].priority, 3);
    assert_eq!(duplicated[2].rules[0].priority, 2);
    assert_eq!(duplicated[2].rules[0].kind, duplicated[0].rules[0].kind);
    assert_eq!(duplicated[2].rules[0].style, duplicated[0].rules[0].style);
    cf_reads(&mut s, "cf018-duplicate.xlsx", &duplicated);

    // Undo duplication
    s.click_cell("A1"); s.key("cmd-z"); assert_eq!(cf_model(&mut s), initial);

    // Redo duplication
    s.key("cmd-shift-z"); assert_eq!(cf_model(&mut s), duplicated);

    // Edit the duplicated rule using "コピーとして追加" (duplicate-apply)
    s.menu("xlsx.conditional");
    cf_click(&mut s, "edit-2");
    cf_field(&mut s, "formula", "A1>5");
    cf_field(&mut s, "fill", "FFFF00");
    cf_click(&mut s, "duplicate-apply");
    let after_copy_add = cf_model(&mut s);
    assert_eq!(after_copy_add.len(), 4);

    // Delete the duplicated rule (index 2, rule 0)
    s.menu("xlsx.conditional");
    cf_click(&mut s, "delete-2-0");
    let after_delete = cf_model(&mut s);
    assert_eq!(after_delete.len(), 3);

    // Undo deletion
    s.click_cell("A1"); s.key("cmd-z"); assert_eq!(cf_model(&mut s), after_copy_add);

    // Redo deletion
    s.key("cmd-shift-z"); assert_eq!(cf_model(&mut s), after_delete);
    cf_reads(&mut s, "cf018-delete.xlsx", &after_delete);
    s.screenshot("conditional-cf018");
}

/// CF-011: Conditional formatting threshold types (min, max, num, percent, percentile, formula)
/// for colorScale, dataBar, and iconSet rules evaluation, rendering, save/reopen and undo/redo.
fn cf011_threshold_types() {
    let mut s = new_sheet();
    // A1:A5 = [10, 20, 30, 40, 50]
    for v in ["10", "20", "30", "40", "50"] { s.enter(v, "enter"); }
    // B1 = 25
    s.click_cell("B1");
    s.enter("25", "enter");

    // Add ColorScale with min, percentile(50), max
    let slide_id = s.with(|e, _| e.slide().id);
    let color_scale_format = ibis_doc_model::ConditionalFormat {
        source_index: None,
        ranges: vec![ibis_doc_model::CellRange { first_row: 0, last_row: 4, first_column: 0, last_column: 0 }],
        rules: vec![ibis_doc_model::CfRule {
            kind: ibis_doc_model::CfKind::ColorScale(vec![
                (ibis_doc_model::CfValue { kind: "min".into(), value: None, gte: true }, ibis_doc_model::Rgb(0xF8696B)),
                (ibis_doc_model::CfValue { kind: "percentile".into(), value: Some("50".into()), gte: true }, ibis_doc_model::Rgb(0xFFEB84)),
                (ibis_doc_model::CfValue { kind: "max".into(), value: None, gte: true }, ibis_doc_model::Rgb(0x63BE7B)),
            ]),
            priority: 1,
            stop: false,
            style: None,
        }],
    };

    // Add DataBar with num(10) and formula("B1")
    let data_bar_format = ibis_doc_model::ConditionalFormat {
        source_index: None,
        ranges: vec![ibis_doc_model::CellRange { first_row: 0, last_row: 4, first_column: 0, last_column: 0 }],
        rules: vec![ibis_doc_model::CfRule {
            kind: ibis_doc_model::CfKind::data_bar(
                ibis_doc_model::CfValue { kind: "num".into(), value: Some("10".into()), gte: true },
                ibis_doc_model::CfValue { kind: "formula".into(), value: Some("B1".into()), gte: true },
                ibis_doc_model::Rgb(0x638EC6),
                true,
            ),
            priority: 2,
            stop: false,
            style: None,
        }],
    };

    // Add IconSet with percent(0), percentile(33), formula("AVERAGE(A1:A5)")
    let icon_set_format = ibis_doc_model::ConditionalFormat {
        source_index: None,
        ranges: vec![ibis_doc_model::CellRange { first_row: 0, last_row: 4, first_column: 0, last_column: 0 }],
        rules: vec![ibis_doc_model::CfRule {
            kind: ibis_doc_model::CfKind::icon_set(
                "3TrafficLights1",
                vec![
                    ibis_doc_model::CfValue { kind: "percent".into(), value: Some("0".into()), gte: true },
                    ibis_doc_model::CfValue { kind: "percentile".into(), value: Some("33".into()), gte: true },
                    ibis_doc_model::CfValue { kind: "formula".into(), value: Some("AVERAGE(A1:A5)".into()), gte: true },
                ],
                false,
                true,
            ),
            priority: 3,
            stop: false,
            style: None,
        }],
    };

    s.update(|e, _, cx| {
        e.document.execute(vec![
            ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::AddConditional { slide: slide_id, format: color_scale_format }),
            ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::AddConditional { slide: slide_id, format: data_bar_format }),
            ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::AddConditional { slide: slide_id, format: icon_set_format }),
        ]).unwrap();
        cx.notify();
    });

    let model = cf_model(&mut s);
    assert_eq!(model.len(), 3);

    // Verify drawn colors for ColorScale
    cf_drawn(&mut s, 0, 0xF8696B); // A1 (min 10) -> Red
    cf_drawn(&mut s, 2, 0xFFEB84); // A3 (50th percentile 30) -> Yellow
    cf_drawn(&mut s, 4, 0x63BE7B); // A5 (max 50) -> Green

    // Undo / Redo
    s.key("cmd-z");
    assert!(cf_model(&mut s).is_empty());
    s.key("cmd-shift-z");
    assert_eq!(cf_model(&mut s).len(), 3);

    // Save and Reopen
    s.save_as("cf011-threshold-types.xlsx");
    let reopened = s.reopen();
    let re_sh = sheet_of_slide(&reopened.slides[1]).unwrap();
    assert_eq!(re_sh.conditional.len(), 3);

    if let ibis_doc_model::CfKind::ColorScale(pts) = &re_sh.conditional[0].rules[0].kind {
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[0].0.kind, "min");
        assert_eq!(pts[1].0.kind, "percentile");
        assert_eq!(pts[1].0.value.as_deref(), Some("50"));
        assert_eq!(pts[2].0.kind, "max");
    } else {
        panic!("expected ColorScale");
    }

    if let ibis_doc_model::CfKind::DataBar { min, max, .. } = &re_sh.conditional[1].rules[0].kind {
        assert_eq!(min.kind, "num");
        assert_eq!(min.value.as_deref(), Some("10"));
        assert_eq!(max.kind, "formula");
        assert_eq!(max.value.as_deref(), Some("B1"));
    } else {
        panic!("expected DataBar");
    }

    if let ibis_doc_model::CfKind::IconSet { values, .. } = &re_sh.conditional[2].rules[0].kind {
        assert_eq!(values.len(), 3);
        assert_eq!(values[0].kind, "percent");
        assert_eq!(values[1].kind, "percentile");
        assert_eq!(values[2].kind, "formula");
        assert_eq!(values[2].value.as_deref(), Some("AVERAGE(A1:A5)"));
    } else {
        panic!("expected IconSet");
    }

    s.screenshot("conditional-cf011");
}

/// P1 XF-09: every line style and palette color can be selected and applied.
/// 分割ボタン（文字の色・塗りつぶし）: ▼ のパレットで選んだ色が、左半分の「最後に使った色」になり、左を押すと
/// すぐ別のセルに付く。初めは既定の色。
fn split_color_buttons() {
    use ibis_doc_model::Rgb;
    let mut s = new_sheet();
    s.click_cell("A1");
    // At first the button's own colour (red for the text).
    s.toolbar("text-color");
    assert_eq!(style_at(&mut s, "A1").color, Rgb(0xff0000), "the colour at first");
    // A colour of the palette; the left half then does that one.
    s.toolbar_menu("text-color", "xlsx.color");
    s.click_cell("B1");
    s.toolbar("text-color");
    let first = super::toolbar::COLORS[0].1;
    assert_eq!(style_at(&mut s, "B1").color, Rgb(first));
    // The fill: its own at first, then the picked.
    s.toolbar("fill");
    assert_eq!(style_at(&mut s, "B1").fill, Some(Rgb(0xffff00)));
    s.screenshot("split");
}

fn xsc13_border_styles() {
    use ibis_doc_model::{Border, BorderLine, Rgb};
    let mut s = new_sheet(); s.size(1500., 800.);
    let mut expected = Vec::new();
    for (row, line) in BorderLine::ALL.into_iter().enumerate() {
        for (column, &(label, color)) in super::toolbar::COLORS.iter().enumerate() {
            let address = ibis_doc_model::cell_name(column as u32, row as u32);
            s.click_cell(&address);
            s.toolbar_menu("borders", line.ooxml());
            s.toolbar_menu("borders", &crate::shell::i18n::text(label));
            s.toolbar_menu("borders", "xlsx.border-bottom");
            let border = Some(Border { line, color: Rgb(color) });
            assert_eq!(style_at(&mut s, &address).bottom, border);
            s.key("cmd-z"); assert_eq!(style_at(&mut s, &address).bottom, None);
            s.key("cmd-shift-z"); assert_eq!(style_at(&mut s, &address).bottom, border);
            expected.push((row as u32, column as u32, border));
        }
    }
    s.save_as("xsc13-border-styles.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    for (row, column, expected) in expected {
        assert_eq!(sheet.styles[sheet.cell(row, column).unwrap().style as usize].bottom, expected);
    }
    s.click_cell("N1");
    for id in ["xlsx.border-top", "xlsx.border-left", "xlsx.border-right", "xlsx.border-diagonal-up", "xlsx.border-diagonal-down"] {
        s.toolbar_menu("borders", id);
    }
    let style = style_at(&mut s, "N1");
    assert!(style.top.is_some() && style.left.is_some() && style.right.is_some() && style.diagonal_up.is_some() && style.diagonal_down.is_some());
    s.toolbar_menu("borders", "xlsx.border-none");
    assert_eq!(style_at(&mut s, "N1").diagonal_up, None);
    s.key("cmd-z"); assert_eq!(style_at(&mut s, "N1"), style);
    s.save_as("xsc13-border-edges.xlsx"); s.reopen();
    s.screenshot("border-styles");
}

/// Existing model values must be reachable through actual toolbar operations.
fn xsc13_alignment_options() {
    use ibis_doc_model::CellAlign;
    let mut s = new_sheet();
    for (row, id, align) in [(0, "xlsx.align-center-across", CellAlign::CenterAcross), (1, "xlsx.align-distributed", CellAlign::Distributed)] {
        let address = ibis_doc_model::cell_name(0, row);
        s.click_cell(&address);
        let before = style_at(&mut s, &address);
        s.toolbar_menu("halign", id);
        assert_eq!(style_at(&mut s, &address).align, align);
        s.key("cmd-z"); assert_eq!(style_at(&mut s, &address), before);
        s.key("cmd-shift-z"); assert_eq!(style_at(&mut s, &address).align, align);
        s.toolbar_menu("halign", "xlsx.shrink");
        assert!(style_at(&mut s, &address).shrink);
        s.key("cmd-z"); assert!(!style_at(&mut s, &address).shrink);
        s.key("cmd-shift-z"); assert!(style_at(&mut s, &address).shrink);
    }
    s.save_as("xsc13-alignment-options.xlsx");
    let p = s.reopen(); let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    for (row, align) in [(0, CellAlign::CenterAcross), (1, CellAlign::Distributed)] {
        let style = &sheet.styles[sheet.cell(row, 0).unwrap().style as usize];
        assert_eq!(style.align, align); assert!(style.shrink);
    }
}

fn xsc13_alignment_all_values() {
    use ibis_doc_model::{CellAlign, CellVerticalAlign};
    let mut s = new_sheet();
    let horizontal = [("general", CellAlign::General), ("left", CellAlign::Left), ("center", CellAlign::Center), ("right", CellAlign::Right), ("fill", CellAlign::Fill), ("justify", CellAlign::Justify), ("center-across", CellAlign::CenterAcross), ("distributed", CellAlign::Distributed)];
    let vertical = [("top", CellVerticalAlign::Top), ("center", CellVerticalAlign::Center), ("bottom", CellVerticalAlign::Bottom), ("justify", CellVerticalAlign::Justify), ("distributed", CellVerticalAlign::Distributed)];
    for (row, (hid, h)) in horizontal.into_iter().enumerate() {
        for (col, (vid, v)) in vertical.into_iter().enumerate() {
            let address = ibis_doc_model::cell_name(col as u32, row as u32);
            s.click_cell(&address); s.type_text("配置確認"); s.key("enter"); s.click_cell(&address);
            let before = style_at(&mut s, &address);
            s.toolbar_menu("halign", &format!("xlsx.align-{hid}"));
            assert_eq!(style_at(&mut s, &address).align, h);
            if h != before.align {
                s.key("cmd-z"); assert_eq!(style_at(&mut s, &address), before);
                s.key("cmd-shift-z"); assert_eq!(style_at(&mut s, &address).align, h);
            }
            let before = style_at(&mut s, &address);
            s.toolbar_menu("valign", &format!("xlsx.valign-{vid}"));
            assert_eq!(style_at(&mut s, &address).vertical, v);
            if v != before.vertical {
                s.key("cmd-z"); assert_eq!(style_at(&mut s, &address), before);
                s.key("cmd-shift-z"); assert_eq!(style_at(&mut s, &address).vertical, v);
            }
        }
    }
    s.save_as("xsc13-alignment-all-values.xlsx");
    let p = s.reopen(); let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    for (row, (_, h)) in horizontal.into_iter().enumerate() {
        for (col, (_, v)) in vertical.into_iter().enumerate() {
            let style = &sheet.styles[sheet.cell(row as u32, col as u32).unwrap().style as usize];
            assert_eq!((style.align, style.vertical), (h, v));
        }
    }
}

fn xsc13_underline_kinds() {
    use ibis_doc_model::CellUnderline;
    let mut s = new_sheet();
    for (column, u) in CellUnderline::ALL.into_iter().enumerate() {
        let address = ibis_doc_model::cell_name(column as u32, 0);
        s.click_cell(&address); s.type_text("gyp 下線"); s.key("enter"); s.click_cell(&address);
        let before = style_at(&mut s, &address);
        s.toolbar_menu("underline-kind", u.ooxml());
        assert_eq!(style_at(&mut s, &address).underline, u);
        if u != before.underline {
            s.key("cmd-z"); assert_eq!(style_at(&mut s, &address), before);
            s.key("cmd-shift-z"); assert_eq!(style_at(&mut s, &address).underline, u);
        }
    }
    s.save_as("xsc13-underline-kinds.xlsx"); let p = s.reopen(); let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    for (column, u) in CellUnderline::ALL.into_iter().enumerate() {
        assert_eq!(sheet.styles[sheet.cell(0, column as u32).unwrap().style as usize].underline, u);
    }
    s.click_cell("E1"); s.key("cmd-u"); assert_eq!(style_at(&mut s, "E1").underline, CellUnderline::None);
    s.key("cmd-z"); assert_eq!(style_at(&mut s, "E1").underline, CellUnderline::DoubleAccounting);
    s.toolbar("underline"); assert_eq!(style_at(&mut s, "E1").underline, CellUnderline::None);
    s.key("cmd-z"); assert_eq!(style_at(&mut s, "E1").underline, CellUnderline::DoubleAccounting);
}

fn xsc55_implicit_operators() {
    let mut s = new_sheet();
    for (address, text) in [("A1","10"),("A2","20"),("A3","30"),("B1","=@A1:A3"),("B2","=@A1:A3"),("B3","=@A1:A3"),("B4","=@A1:A3"),("D1","=SEQUENCE(3)"),("E1","=SUM(D1#)"),("E2","=@D1#"),("F1","=@SEQUENCE(3)")] {
        s.click_cell(address); s.enter(text,"enter");
    }
    assert_eq!(s.value("B1"),CellValue::Number(10.)); assert_eq!(s.value("B2"),CellValue::Number(20.)); assert_eq!(s.value("B3"),CellValue::Number(30.));
    assert_eq!(s.value("B4"),CellValue::Error(ibis_doc_model::CellError::Value));
    assert_eq!(s.value("E1"),CellValue::Number(6.)); assert_eq!(s.value("E2"),CellValue::Number(2.)); assert_eq!(s.value("F1"),CellValue::Number(1.));
    s.key("cmd-z"); assert_eq!(s.value("F1"),CellValue::Empty);
    s.key("cmd-shift-z"); assert_eq!(s.value("F1"),CellValue::Number(1.));
    s.save_as("xsc55-implicit-operators.xlsx");
    let p = s.reopen(); let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    for (row,col,formula,expected) in [(0,1,"@A1:A3",10.),(1,1,"@A1:A3",20.),(1,4,"@D1#",2.),(0,5,"@SEQUENCE(3)",1.)] {
        let c=sheet.cell(row,col).unwrap(); assert_eq!(c.formula.as_deref(),Some(formula)); assert_eq!(c.value,CellValue::Number(expected));
    }
}

/// Cached external results are preserved; this is not an external calculation test.
fn xsc39_external_cached_values() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    for (address,text) in [("A10","1"),("B10","=7"),("B11","=11"),("C10","=SUM(B10:B11)")] {s.click_cell(address);s.enter(text,"enter");}
    let path=s.save_as("xsc39-external-initial.xlsx");
    let fixture=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit/external-cache-ui-synthetic-original.xlsx");
    std::fs::create_dir_all(fixture.parent().unwrap()).unwrap();
    // Prepare only this generated fixture with Python's standard ZIP/XML library.
    // No external calculation, app automation, or additional Rust dependency.
    let status=std::process::Command::new("python3").arg("-c").arg(r#"
import sys, zipfile, re
with zipfile.ZipFile(sys.argv[1]) as source, zipfile.ZipFile(sys.argv[2], 'w') as output:
    for info in source.infolist():
        data = source.read(info.filename)
        if info.filename == 'xl/worksheets/sheet1.xml':
            text = data.decode('utf-8')
            text = re.sub(r'(<c r="B10"[^>]*><f>)[^<]*(</f>)', r'\g<1>_xlfn.FIELDVALUE(A10,"Price")\g<2>', text)
            text = re.sub(r'(<c r="B11"[^>]*><f>)[^<]*(</f>)', r'\g<1>_xlfn._xlws.PY(A10,0)\g<2>', text)
            data = text.encode('utf-8')
        output.writestr(info, data)
"#).arg(path).arg(&fixture).status().unwrap();
    assert!(status.success(), "prepare generated external-cache fixture");
    drop(s); // Release the scenario lock before opening the generated fixture.
    let mut s=Scenario::open(fixture.to_str().unwrap());
    let verify=|s:&mut Scenario|{assert_eq!(s.value("B10"),CellValue::Number(7.));assert_eq!(s.value("B11"),CellValue::Number(11.));assert_eq!(s.value("C10"),CellValue::Number(18.));};
    verify(&mut s); s.click_cell("A10");s.enter("2","enter");verify(&mut s);
    s.key("cmd-z");assert_eq!(s.value("A10"),CellValue::Number(1.));verify(&mut s);
    s.key("cmd-shift-z");assert_eq!(s.value("A10"),CellValue::Number(2.));verify(&mut s);
    s.click_cell("B10");s.key("cmd-b");verify(&mut s);
    s.key("cmd-z");verify(&mut s);s.key("cmd-shift-z");verify(&mut s);
    s.save_as("xsc39-external-preserved.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[0]).unwrap();
    for (row,expected) in [(9,7.),(10,11.)] {assert_eq!(sheet.cell(row,1).unwrap().value,CellValue::Number(expected));}
    assert_eq!(sheet.cell(9,1).unwrap().formula.as_deref(),Some("FIELDVALUE(A10,\"Price\")"));
    assert_eq!(sheet.cell(10,1).unwrap().formula.as_deref(),Some("PY(A10,0)"));
}

fn xsc55_flatten_options() {
    let mut s=new_sheet();
    for (address,text) in [("A1","1"),("C1","=NA()"),("A2","2"),("B2","0"),("C2","=\"\"")] {s.click_cell(address);s.enter(text,"enter");}
    let mut checks=Vec::new();
    for (f,function) in ["TOCOL","TOROW"].into_iter().enumerate() {
        for ignore in 0..=3 { for scan in 0..=1 {
            let row=5+(f*8+ignore*2+scan)*8;
            let address=ibis_doc_model::cell_name(0,row as u32);
            s.click_cell(&address);s.enter(&format!("={function}(A1:C2,{ignore},{})",scan==1),"enter");
            let base=if scan==1 {vec![CellValue::Number(1.),CellValue::Number(2.),CellValue::Empty,CellValue::Number(0.),CellValue::Error(ibis_doc_model::CellError::NA),CellValue::Text(String::new())]} else {vec![CellValue::Number(1.),CellValue::Empty,CellValue::Error(ibis_doc_model::CellError::NA),CellValue::Number(2.),CellValue::Number(0.),CellValue::Text(String::new())]};
            let expected:Vec<_>=base.into_iter().filter(|v|!((v.is_empty()&&matches!(ignore,1|3))||(matches!(v,CellValue::Error(_))&&matches!(ignore,2|3)))).map(|v|if v.is_empty(){CellValue::Number(0.)}else{v}).collect();
            for (i,value) in expected.into_iter().enumerate() {
                let (r,c)=if f==0 {(row+i,0)}else{(row,i)};
                let a=ibis_doc_model::cell_name(c as u32,r as u32);assert_eq!(s.value(&a),value,"{function}/{ignore}/{scan}");checks.push((r as u32,c as u32,value));
            }
            s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),CellValue::Number(1.));
        }}
    }
    s.click_cell("A141");s.enter("=TOCOL(A1:C2,0,NA())","enter");assert_eq!(s.value("A141"),CellValue::Error(ibis_doc_model::CellError::NA));
    s.save_as("xsc55-flatten-options.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();
    for (r,c,value) in checks {assert_eq!(sheet.cell(r,c).unwrap().value,value);}
    assert_eq!(sheet.cell(140,0).unwrap().value,CellValue::Error(ibis_doc_model::CellError::NA));
}

fn xsc39_weekend_options() {
    let mut s=new_sheet();
    let cases=[("0000011",10.,45293.),("0000000",14.,45293.),("1010101",6.,45293.),("1111111",0.,0.)];
    for (row,(weekend,count,date)) in cases.into_iter().enumerate() {
        let a=ibis_doc_model::cell_name(0,row as u32);s.click_cell(&a);s.enter(&format!("=NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,14),\"{weekend}\")"),"enter");assert_eq!(s.value(&a),CellValue::Number(count));
        s.key("cmd-z");assert_eq!(s.value(&a),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&a),CellValue::Number(count));
        let b=ibis_doc_model::cell_name(1,row as u32);let expected=if weekend=="1111111" {CellValue::Error(ibis_doc_model::CellError::Value)}else{CellValue::Number(date)};
        s.click_cell(&b);s.enter(&format!("=WORKDAY.INTL(DATE(2024,1,1),1,\"{weekend}\")"),"enter");assert_eq!(s.value(&b),expected);
        s.key("cmd-z");assert_eq!(s.value(&b),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&b),expected);
    }
    s.save_as("xsc39-weekend-options.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();
    for (row,(weekend,count,date)) in cases.into_iter().enumerate() {
        assert_eq!(sheet.cell(row as u32,0).unwrap().value,CellValue::Number(count));assert_eq!(sheet.cell(row as u32,1).unwrap().value,if weekend=="1111111" {CellValue::Error(ibis_doc_model::CellError::Value)}else{CellValue::Number(date)});
    }
}

fn xsc39_datedif_units() {
    let mut s=new_sheet();let units=[("Y",1.),("M",14.),("D",440.),("MD",14.),("YM",2.),("YD",75.)];
    for (row,(unit,expected)) in units.into_iter().enumerate() {
        let address=ibis_doc_model::cell_name(0,row as u32);s.click_cell(&address);s.enter(&format!("=DATEDIF(DATE(2001,6,1),DATE(2002,8,15),\"{unit}\")"),"enter");assert_eq!(s.value(&address),CellValue::Number(expected));
        s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),CellValue::Number(expected));
    }
    s.save_as("xsc39-datedif-units.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();
    for (row,(_,expected)) in units.into_iter().enumerate(){assert_eq!(sheet.cell(row as u32,0).unwrap().value,CellValue::Number(expected));}
}

fn xsc39_information_options() {
    let mut s=new_sheet();let formulas=["INFO(1)","INFO(\"invalid\")","CELL(1,A1)","CELL(\"invalid\",A1)","INFO(\"memavail\")","INFO(\"memused\")","INFO(\"totmem\")"];
    for (row,formula) in formulas.into_iter().enumerate() {
        let address=ibis_doc_model::cell_name(1,row as u32);let expected=CellValue::Error(if row<4 {ibis_doc_model::CellError::Value}else{ibis_doc_model::CellError::NA});
        s.click_cell(&address);s.enter(&format!("={formula}"),"enter");assert_eq!(s.value(&address),expected);
        s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),expected);
    }
    s.save_as("xsc39-information-options.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();
    for row in 0..7 {assert_eq!(sheet.cell(row,1).unwrap().value,CellValue::Error(if row<4 {ibis_doc_model::CellError::Value}else{ibis_doc_model::CellError::NA}));}
}

fn xsc55_sort_options() {
    let mut s=new_sheet();let mut checks=vec![];let mut row=0;
    for index in [1,2] {for order in [-1,1] {for by in [false,true] {
        let address=ibis_doc_model::cell_name(0,row);s.click_cell(&address);s.enter(&format!("=SORT({{4,1;2,3}},{index},{order},{by})"),"enter");
        let swap=(index==1&&order==1)||(index==2&&order==-1);let expected=match(by,swap){(false,true)=>[[2.,3.],[4.,1.]],(true,true)=>[[1.,4.],[3.,2.]],_=>[[4.,1.],[2.,3.]]};
        for (r,values) in expected.into_iter().enumerate(){for (c,value) in values.into_iter().enumerate(){let a=ibis_doc_model::cell_name(c as u32,row+r as u32);assert_eq!(s.value(&a),CellValue::Number(value));checks.push((row+r as u32,c as u32,value));}}
        s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),CellValue::Number(expected[0][0]));row+=3;
    }}}
    s.click_cell("A25");s.enter("=SORT({4,1;2,3},1,1,NA())","enter");assert_eq!(s.value("A25"),CellValue::Error(ibis_doc_model::CellError::NA));
    s.save_as("xsc55-sort-options.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for (r,c,value) in checks {assert_eq!(sheet.cell(r,c).unwrap().value,CellValue::Number(value));}assert_eq!(sheet.cell(24,0).unwrap().value,CellValue::Error(ibis_doc_model::CellError::NA));
}

fn xsc55_sort_errors() {
    let mut s=new_sheet();s.click_cell("A1");s.enter("=SORT({#N/A;#DIV/0!;#VALUE!})","enter");
    for (address,error) in [("A1",ibis_doc_model::CellError::Div0),("A2",ibis_doc_model::CellError::Value),("A3",ibis_doc_model::CellError::NA)]{assert_eq!(s.value(address),CellValue::Error(error));}
    s.key("cmd-z");assert_eq!(s.value("A1"),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value("A1"),CellValue::Error(ibis_doc_model::CellError::Div0));
    s.save_as("xsc55-sort-errors.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for (row,error) in [ibis_doc_model::CellError::Div0,ibis_doc_model::CellError::Value,ibis_doc_model::CellError::NA].into_iter().enumerate(){assert_eq!(sheet.cell(row as u32,0).unwrap().value,CellValue::Error(error));}
}

fn xsc55_textsplit_options() {
    let mut s=new_sheet();let mut checks=vec![];let mut row=0;
    for ignore in [false,true] {for mode in [0,1] {for custom in [false,true] {
        let address=ibis_doc_model::cell_name(0,row);let pad=if custom {",\"pad\""}else{""};s.click_cell(&address);s.enter(&format!("=TEXTSPLIT(\"aXbxxc|dXe\",\"x\",\"|\",{ignore},{mode}{pad})"),"enter");
        let rows=match (ignore,mode) {(false,0)=>vec![vec!["aXb","","c"],vec!["dXe"]],(true,0)=>vec![vec!["aXb","c"],vec!["dXe"]],(false,1)=>vec![vec!["a","b","","c"],vec!["d","e"]],_=>vec![vec!["a","b","c"],vec!["d","e"]]};
        let width=rows[0].len();for (r,values) in rows.iter().enumerate(){for col in 0..width {let expected=values.get(col).map(|v|CellValue::Text((*v).into())).unwrap_or_else(||if custom {CellValue::Text("pad".into())}else{CellValue::Error(ibis_doc_model::CellError::NA)});let a=ibis_doc_model::cell_name(col as u32,row+r as u32);assert_eq!(s.value(&a),expected);checks.push((row+r as u32,col as u32,expected));}}
        s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),CellValue::Text(if mode==0 {"aXb"}else{"a"}.into()));row+=3;
    }}}
    s.click_cell("A25");s.enter("=TEXTSPLIT(\"a,b\",\",\",,NA())","enter");assert_eq!(s.value("A25"),CellValue::Error(ibis_doc_model::CellError::NA));
    s.save_as("xsc55-textsplit-options.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for (r,c,value) in checks{assert_eq!(sheet.cell(r,c).unwrap().value,value);}assert_eq!(sheet.cell(24,0).unwrap().value,CellValue::Error(ibis_doc_model::CellError::NA));
}

fn xsc55_textsplit_shape() {
    let mut s=new_sheet();let cases=[("A1","TEXTSPLIT(\"Hello World\",\"Hello World\",\"Hello World\")",CellValue::Text(String::new())),("A4","TEXTSPLIT(\"a,b,c\",\",\",\",\")",CellValue::Text("a".into())),("A7","TEXTSPLIT(\"m\",\",\",,FALSE,0,{1,2;3,4})",CellValue::Error(ibis_doc_model::CellError::Value)),("A10","TEXTSPLIT(\"m\",\",\",,FALSE,0,C1:C3)",CellValue::Text("m".into()))];
    for (address,formula,expected) in cases {s.click_cell(address);s.enter(&format!("={formula}"),"enter");assert_eq!(s.value(address),expected);s.key("cmd-z");assert_eq!(s.value(address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(address),expected);}
    assert_eq!(s.value("B1"),CellValue::Text(String::new()));assert_eq!(s.value("A2"),CellValue::Empty);assert_eq!(s.value("C4"),CellValue::Text("c".into()));
    s.save_as("xsc55-textsplit-shape.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();assert_eq!(sheet.cell(0,1).unwrap().value,CellValue::Text(String::new()));assert_eq!(sheet.cell(3,2).unwrap().value,CellValue::Text("c".into()));assert_eq!(sheet.cell(6,0).unwrap().value,CellValue::Error(ibis_doc_model::CellError::Value));
}

fn xsc55_text_casefold() {
    let mut s=new_sheet();let prefixes=["K","İ","日本","🙂","ΟΣ|"];
    for (i,prefix) in prefixes.into_iter().enumerate() {
        for (row,column,function,expected) in [(i as u32*4,0,"TEXTBEFORE",prefix),(i as u32*4,1,"TEXTAFTER","tail"),(i as u32*4+1,0,"TEXTSPLIT",prefix)] {
            let address=ibis_doc_model::cell_name(column,row);let options=if function=="TEXTSPLIT" {",,FALSE,1"}else{",1,1"};
            s.click_cell(&address);s.enter(&format!("={function}(\"{prefix}Xtail\",\"x\"{options})"),"enter");assert_eq!(s.value(&address),CellValue::Text(expected.into()));s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),CellValue::Text(expected.into()));
        }
        assert_eq!(s.value(&ibis_doc_model::cell_name(1,i as u32*4+1)),CellValue::Text("tail".into()));
    }
    s.save_as("xsc55-text-casefold.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for (i,prefix) in prefixes.into_iter().enumerate(){for row in [i as u32*4,i as u32*4+1]{assert_eq!(sheet.cell(row,0).unwrap().value,CellValue::Text(prefix.into()));assert_eq!(sheet.cell(row,1).unwrap().value,CellValue::Text("tail".into()));}}
}

fn xsc39_subtotal_literals() {
    let mut s=new_sheet();
    for (i,f) in ["LEN(\"SUBTOTAL(\")","LEN(\"AGGREGATE(\")","SUBTOTAL(9,A1:A2)","AGGREGATE(9,0,A1:A2)"].into_iter().enumerate(){s.click_cell(&format!("A{}",i+1));s.enter(&format!("={f}"),"enter");}
    let checks=[("SUBTOTAL(9,A1:A4)",19.0),("SUBTOTAL(3,A1:A4)",2.0),("AGGREGATE(9,0,A1:A4)",19.0),("AGGREGATE(3,0,A1:A4)",2.0),("AGGREGATE(9,4,A1:A4)",57.0)];
    for (i,(f,expected)) in checks.iter().enumerate(){let address=format!("B{}",i+1);s.click_cell(&address);s.enter(&format!("={f}"),"enter");assert_eq!(s.value(&address),CellValue::Number(*expected));s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),CellValue::Number(*expected));}
    s.save_as("xsc39-subtotal-literals.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for (row,(_,expected)) in checks.into_iter().enumerate(){assert_eq!(sheet.cell(row as u32,1).unwrap().value,CellValue::Number(expected));}
}

fn xsc39_subtotal_count() {
    let mut s=new_sheet();for (address,input) in [("A1","=NA()"),("A2","7"),("A3","text")]{s.click_cell(address);s.enter(input,"enter");}
    let expected=[CellValue::Number(1.0),CellValue::Number(1.0),CellValue::Number(3.0),CellValue::Number(3.0),CellValue::Error(ibis_doc_model::CellError::NA)];
    for (row,code) in [2,102,3,103,9].into_iter().enumerate(){let address=format!("B{}",row+1);s.click_cell(&address);s.enter(&format!("=SUBTOTAL({code},A1:A3)"),"enter");assert_eq!(s.value(&address),expected[row]);s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),expected[row]);}
    s.save_as("xsc39-subtotal-count.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for (row,value) in expected.into_iter().enumerate(){assert_eq!(sheet.cell(row as u32,1).unwrap().value,value);}
}

fn xsc39_subtotal_hidden() {
    let mut s=new_sheet();for (i,n) in [2,4,6].into_iter().enumerate(){s.click_cell(&format!("A{}",i+1));s.enter(&n.to_string(),"enter");}
    for (i,code) in (1..=11).chain(101..=111).enumerate(){s.click_cell(&ibis_doc_model::cell_name((1+i/11) as u32,(i%11) as u32));s.enter(&format!("=SUBTOTAL({code},A1:A3)"),"enter");}
    let all=[4.0,3.0,3.0,6.0,2.0,48.0,2.0,(8.0_f64/3.0).sqrt(),12.0,4.0,8.0/3.0];let visible=[4.0,2.0,2.0,6.0,2.0,12.0,8.0_f64.sqrt(),2.0,8.0,8.0,4.0];
    let check=|s:&mut Scenario,hidden:bool|{for i in 0..22{let expected=if hidden && i>=11{visible[i%11]}else{all[i%11]};let CellValue::Number(n)=s.value(&ibis_doc_model::cell_name((1+i/11) as u32,(i%11) as u32))else{panic!("not numeric {i}")};assert!((n-expected).abs()<1e-12,"{i}: {n} != {expected}");}};
    check(&mut s,false);s.click_cell("A2");s.key("ctrl-9");check(&mut s,true);s.key("cmd-z");check(&mut s,false);s.key("cmd-shift-z");check(&mut s,true);
    s.save_as("xsc39-subtotal-hidden.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for i in 0..22{let expected=if i>=11{visible[i%11]}else{all[i%11]};let CellValue::Number(n)=sheet.cell((i%11) as u32,(1+i/11) as u32).unwrap().value else{panic!("not numeric {i}")};assert!((n-expected).abs()<1e-12);}assert_eq!(sheet.rows[1],0);
}

fn xsc39_aggregate_enums() {
    let mut s=new_sheet();for (i,n) in [2,4,4,6].into_iter().enumerate(){s.click_cell(&format!("A{}",i+1));s.enter(&n.to_string(),"enter");}
    let all=[4.0,4.0,4.0,6.0,2.0,192.0,(8.0_f64/3.0).sqrt(),2.0_f64.sqrt(),16.0,8.0/3.0,2.0,4.0,4.0,4.0,4.0,3.5,3.5,2.5,2.5];let visible=[14.0/3.0,3.0,3.0,6.0,4.0,96.0,(4.0_f64/3.0).sqrt(),(8.0_f64/9.0).sqrt(),14.0,4.0/3.0,8.0/9.0,4.0,4.0,4.0,4.0,4.0,4.0,4.0,4.0];
    for function in 1..=19{for option in 0..8{let address=ibis_doc_model::cell_name(option+1,function-1);let k=match function{14|15=>",2",16|18=>",0.25",17|19=>",1",_=>""};s.click_cell(&address);s.enter(&format!("=AGGREGATE({function},{option},A1:A4{k})"),"enter");s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");}}
    let check=|s:&mut Scenario,hidden:bool|{for function in 1..=19{for option in 0..8{let expected=if hidden && option%2==1{visible[function as usize-1]}else{all[function as usize-1]};let CellValue::Number(n)=s.value(&ibis_doc_model::cell_name(option+1,function-1))else{panic!("non numeric {function}/{option}")};assert!((n-expected).abs()<1e-12,"{function}/{option}: {n} != {expected}");}}};
    check(&mut s,false);s.click_cell("A1");s.key("ctrl-9");check(&mut s,true);s.key("cmd-z");check(&mut s,false);s.key("cmd-shift-z");check(&mut s,true);
    s.save_as("xsc39-aggregate-enums.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for function in 1..=19{for option in 0..8{let expected=if option%2==1{visible[function as usize-1]}else{all[function as usize-1]};let CellValue::Number(n)=sheet.cell(function-1,option+1).unwrap().value else{panic!("non numeric {function}/{option}")};assert!((n-expected).abs()<1e-12);}}
}

fn xsc24_subtotal_filter() {
    let mut s=new_sheet();for (address,input) in [("A1","header"),("A2","2"),("A3","4"),("A4","6")]{s.click_cell(address);s.enter(input,"enter");}
    for (row,code) in [9,109,2,102,3,103].into_iter().enumerate(){s.click_cell(&format!("B{}",row+1));s.enter(&format!("=SUBTOTAL({code},A2:A4)"),"enter");}
    s.click_cell("A3");s.key("ctrl-9");s.click_cell("A1");for _ in 0..2{s.key("shift-down");}s.menu("xlsx.filter");
    s.update(|e,_,cx|e.filter_column(0,Some(ibis_doc_model::ColumnFilter::Values(vec!["4".into(),"6".into()])),cx));
    let check=|s:&mut Scenario|{for (row,n) in [10.0,6.0,2.0,1.0,2.0,1.0].into_iter().enumerate(){assert_eq!(s.value(&format!("B{}",row+1)),CellValue::Number(n));}};check(&mut s);s.key("cmd-z");assert_eq!(s.value("B1"),CellValue::Number(12.0));assert_eq!(s.value("B2"),CellValue::Number(8.0));s.key("cmd-shift-z");check(&mut s);
    s.save_as("xsc24-subtotal-filter.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for (row,n) in [10.0,6.0,2.0,1.0,2.0,1.0].into_iter().enumerate(){assert_eq!(sheet.cell(row as u32,1).unwrap().value,CellValue::Number(n));}assert_eq!(sheet.filtered_rows.iter().copied().collect::<Vec<_>>(),vec![1]);
    s.menu("xlsx.filter");assert_eq!(s.value("B1"),CellValue::Number(12.0));assert_eq!(s.value("B2"),CellValue::Number(8.0));assert_eq!(s.size_pt("3"),0.0);
}

fn xsc08_jis_name() {
    let mut s=new_sheet();let cases=[("abc","ａｂｃ"),("ｱｲｳ ABC","アイウ　ＡＢＣ"),("ｶﾞﾊﾟｳﾞ","ガパウ゛"),("日本語ＡＢＣ","日本語ＡＢＣ"),("ｶA🙂","カＡ🙂"),("","")];
    for (row,(input,expected)) in cases.into_iter().enumerate(){if !input.is_empty(){s.click_cell(&format!("A{}",row+1));s.enter(input,"enter");}for (column,name) in [(1,"JIS"),(2,"DBCS")]{let address=ibis_doc_model::cell_name(column,row as u32);s.click_cell(&address);s.enter(&format!("={name}(A{})",row+1),"enter");assert_eq!(s.value(&address),CellValue::Text(expected.into()));s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");}}
    for (row,formula,expected) in [(6,"JIS(123)",CellValue::Text("１２３".into())),(7,"JIS(NA())",CellValue::Error(ibis_doc_model::CellError::NA)),(8,"JIS()",CellValue::Error(ibis_doc_model::CellError::Value)),(9,"JIS(\"a\",\"b\")",CellValue::Error(ibis_doc_model::CellError::Value))]{let address=format!("B{}",row+1);s.click_cell(&address);s.enter(&format!("={formula}"),"enter");assert_eq!(s.value(&address),expected);s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");}
    s.save_as("xsc08-jis-name.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for (row,(_,expected)) in cases.into_iter().enumerate(){for column in [1,2]{assert_eq!(sheet.cell(row as u32,column).unwrap().value,CellValue::Text(expected.into()));}}assert_eq!(sheet.cell(0,1).unwrap().formula.as_deref(),Some("JIS(A1)"));assert_eq!(sheet.cell(0,2).unwrap().formula.as_deref(),Some("DBCS(A1)"));
}

fn xsc39_aggregate_resize() {
    let mut s=new_sheet();for (i,n) in [2,4,6].into_iter().enumerate(){s.click_cell(&format!("A{}",i+1));s.enter(&n.to_string(),"enter");}
    for (row,f) in ["SUBTOTAL(9,A1:A3)","SUBTOTAL(109,A1:A3)","AGGREGATE(9,1,A1:A3)","AGGREGATE(9,4,A1:A3)","B2*2","SUM(A1:A3)"].into_iter().enumerate(){s.click_cell(&format!("B{}",row+1));s.enter(&format!("={f}"),"enter");}
    let check=|s:&mut Scenario,hidden:bool|{for (row,n) in (if hidden{[12.0,8.0,8.0,12.0,16.0,12.0]}else{[12.0,12.0,12.0,12.0,24.0,12.0]}).into_iter().enumerate(){assert_eq!(s.value(&format!("B{}",row+1)),CellValue::Number(n));}};
    let (x,y)=s.header_line("2");let top=s.with(|e,_|e.sheet_state().unwrap().header_geometry.borrow().rows.iter().find(|r|r.0==1).unwrap().1);s.drag((x,y),(x,top));assert_eq!(s.size_pt("2"),0.0);check(&mut s,true);s.key("cmd-z");check(&mut s,false);s.key("cmd-shift-z");check(&mut s,true);
    s.save_as("xsc39-aggregate-resize.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for (row,n) in [12.0,8.0,8.0,12.0,16.0,12.0].into_iter().enumerate(){assert_eq!(sheet.cell(row as u32,1).unwrap().value,CellValue::Number(n));}assert_eq!(sheet.rows[1],0);
}

fn xsc55_wrap_count() {
    let mut s=new_sheet();let mut row=0;let mut checks=vec![];
    for function in ["WRAPROWS","WRAPCOLS"]{for vector in ["{1,2}","{1;2}"]{for pad in ["",",\"pad\""]{let address=ibis_doc_model::cell_name(0,row);s.click_cell(&address);s.enter(&format!("={function}({vector},5{pad})"),"enter");assert_eq!(s.value(&address),CellValue::Number(1.0));let (r,c)=if function=="WRAPROWS"{(row,1)}else{(row+1,0)};let second=ibis_doc_model::cell_name(c,r);assert_eq!(s.value(&second),CellValue::Number(2.0));s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);assert_eq!(s.value(&second),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&second),CellValue::Number(2.0));checks.push((row,r,c));row+=3;}}}
    s.save_as("xsc55-wrap-count.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for (row,r,c) in checks{assert_eq!(sheet.cell(row,0).unwrap().value,CellValue::Number(1.0));assert_eq!(sheet.cell(r,c).unwrap().value,CellValue::Number(2.0));}assert_eq!(sheet.cells.len(),16);
}

fn xsc55_expand_options() {
    let mut s=new_sheet();let mut row=0;let mut checks=vec![];
    for rows in [2,3]{for columns in [2,3]{for pad in ["",",0",",\"\"",",\"pad\""]{let address=ibis_doc_model::cell_name(0,row);s.click_cell(&address);s.enter(&format!("=EXPAND({{1,2;3,4}},{rows},{columns}{pad})"),"enter");
        for r in 0..rows{for c in 0..columns{let expected=if r<2 && c<2{CellValue::Number((r*2+c+1) as f64)}else{match pad{""=>CellValue::Error(ibis_doc_model::CellError::NA),",0"=>CellValue::Number(0.0),",\"\""=>CellValue::Text(String::new()),_=>CellValue::Text("pad".into())}};assert_eq!(s.value(&ibis_doc_model::cell_name(c,row+r)),expected);checks.push((row+r,c,expected));}}
        s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),CellValue::Number(1.0));row+=4;
    }}}
    for f in ["EXPAND({1,2},1E20,2)","EXPAND({1;2},2,1E20)","EXPAND(1,1E20,1E20)"]{let address=ibis_doc_model::cell_name(0,row);s.click_cell(&address);s.enter(&format!("={f}"),"enter");assert_eq!(s.value(&address),CellValue::Error(ibis_doc_model::CellError::Num));s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");row+=1;}
    s.save_as("xsc55-expand-options.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for (r,c,value) in checks{assert_eq!(sheet.cell(r,c).unwrap().value,value);}for row in 64..67{assert_eq!(sheet.cell(row,0).unwrap().value,CellValue::Error(ibis_doc_model::CellError::Num));}
}

fn xsc55_regex_lookup_unsupported() {
    let mut s=new_sheet();let checks=[("XLOOKUP(\"a.c\",{\"abc\",\"a.c\"},{10,20},99,3,1)",CellValue::Number(10.0)),("XLOOKUP(\"a.c\",{\"abc\",\"a.c\"},{10,20},99,3,-1)",CellValue::Number(20.0)),("XMATCH(\"a.c\",{\"abc\",\"a.c\"},3,1)",CellValue::Number(1.0)),("XMATCH(\"a.c\",{\"abc\",\"a.c\"},3,-1)",CellValue::Number(2.0))];
    for (i,(f,expected)) in checks.iter().enumerate(){let address=format!("A{}",i+1);s.click_cell(&address);s.enter(&format!("={f}"),"enter");assert_eq!(&s.value(&address),expected);s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(&s.value(&address),expected);}
    s.save_as("xsc55-regex-lookup-unsupported.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for (row,(_,expected)) in checks.iter().enumerate(){assert_eq!(&sheet.cell(row as u32,0).unwrap().value,expected);}
}

fn xsc55_xlookup_shape() {
    let mut s=new_sheet();let mut row=0;for (keys,returns) in [("{1;2}","{10;20;30}"),("{1,2}","{10,20,30}")]{for key in [1,9]{for mode in [-1,0,1]{for search in [-1,1]{let address=format!("A{}",row+1);s.click_cell(&address);s.enter(&format!("=XLOOKUP({key},{keys},{returns},99,{mode},{search})"),"enter");assert_eq!(s.value(&address),CellValue::Error(ibis_doc_model::CellError::Value));s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),CellValue::Error(ibis_doc_model::CellError::Value));row+=1;}}}}
    s.save_as("xsc55-xlookup-shape.xlsx");let p=s.reopen();let sheet=sheet_of_slide(&p.slides[1]).unwrap();for row in 0..24{assert_eq!(sheet.cell(row,0).unwrap().value,CellValue::Error(ibis_doc_model::CellError::Value));}
}

fn xsc55_xlookup_empty() {
    let mut s=new_sheet();s.click_cell("B1");s.enter("=XLOOKUP(A1,{1;2;3},{2.1;3.9;6.2},99,1,1)","enter");assert_eq!(s.value("B1"),CellValue::Number(99.0));s.key("cmd-z");assert_eq!(s.value("B1"),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value("B1"),CellValue::Number(99.0));s.save_as("xsc55-xlookup-empty-blank.xlsx");assert_eq!(sheet_of_slide(&s.reopen().slides[1]).unwrap().cell(0,1).unwrap().value,CellValue::Number(99.0));s.click_cell("A1");s.enter("0","enter");assert_eq!(s.value("B1"),CellValue::Number(2.1));s.key("cmd-z");assert_eq!(s.value("B1"),CellValue::Number(99.0));s.key("cmd-shift-z");assert_eq!(s.value("B1"),CellValue::Number(2.1));s.save_as("xsc55-xlookup-empty-zero.xlsx");assert_eq!(sheet_of_slide(&s.reopen().slides[1]).unwrap().cell(0,1).unwrap().value,CellValue::Number(2.1));
}

fn xsc55_sortby_shape_orders() {
    let mut s=new_sheet();let mut checks=vec![];let mut row=0;
    for (source,valid,bad) in [("{1;2;3;4}","{1;2;3;4}","{1,2;3,4}"),("{1,2,3,4}","{1,2,3,4}","{1,2;3,4}"),("{1;2;3;4}","{1;2;3;4}","{1,2,3,4}"),("{1,2,3,4}","{1,2,3,4}","{1;2;3;4}")]{for position in 0..3{let keys=(0..3).map(|i|if i==position{bad}else{valid}).collect::<Vec<_>>();let address=ibis_doc_model::cell_name(0,row);s.click_cell(&address);s.enter(&format!("=SORTBY({source},{},1,{},1,{},1)",keys[0],keys[1],keys[2]),"enter");let v=CellValue::Error(ibis_doc_model::CellError::Value);assert_eq!(s.value(&address),v);s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),v);checks.push((row,0,v));row+=1;}}
    for sep in [";",","]{for (a,b,expected) in [(1,1,[4.,2.,1.,3.]),(1,-1,[2.,4.,3.,1.]),(-1,1,[1.,3.,4.,2.]),(-1,-1,[3.,1.,2.,4.])]{let src=format!("{{{}}}",["1","2","3","4"].join(sep));let k1=format!("{{{}}}",["2","1","2","1"].join(sep));let k2=format!("{{{}}}",["1","2","2","1"].join(sep));let address=ibis_doc_model::cell_name(0,row);s.click_cell(&address);s.enter(&format!("=SORTBY({src},{k1},{a},{k2},{b})"),"enter");for (i,n) in expected.into_iter().enumerate(){let (r,c)=(if sep==";"{row+i as u32}else{row},if sep==";"{0}else{i as u32});let v=CellValue::Number(n);assert_eq!(s.value(&ibis_doc_model::cell_name(c,r)),v);checks.push((r,c,v));}s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),CellValue::Number(expected[0]));row+=5;}}
    s.save_as("xsc55-sortby-shape-orders.xlsx");let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();for (r,c,v) in checks{assert_eq!(sh.cell(r,c).unwrap().value,v);}
}

fn xsc55_filter_empty() {
    let mut s=new_sheet();for (address,input) in [("A1","10"),("A2","20"),("A3","30"),("C1","=FILTER(A1:A3,B1:B3,99)")]{s.click_cell(address);s.enter(input,"enter");}assert_eq!(s.value("C1"),CellValue::Number(99.0));s.key("cmd-z");assert_eq!(s.value("C1"),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value("C1"),CellValue::Number(99.0));s.save_as("xsc55-filter-empty-blank.xlsx");assert_eq!(sheet_of_slide(&s.reopen().slides[1]).unwrap().cell(0,2).unwrap().value,CellValue::Number(99.0));s.click_cell("B2");s.enter("TRUE","enter");assert_eq!(s.value("C1"),CellValue::Number(20.0));s.key("cmd-z");assert_eq!(s.value("C1"),CellValue::Number(99.0));s.key("cmd-shift-z");assert_eq!(s.value("C1"),CellValue::Number(20.0));s.save_as("xsc55-filter-empty-selected.xlsx");assert_eq!(sheet_of_slide(&s.reopen().slides[1]).unwrap().cell(0,2).unwrap().value,CellValue::Number(20.0));
}

fn xsc55_arraytotext_empty() {
    let mut s=new_sheet();for (row,source) in ["TAKE({1;2},0)","FILTER({1;2},{FALSE;FALSE})"].into_iter().enumerate(){let mut formulas=vec![(source.to_string(),ibis_doc_model::CellError::Calc)];for suffix in ["",",0",",1"]{formulas.push((format!("ARRAYTOTEXT({source}{suffix})"),ibis_doc_model::CellError::Value));}for (col,(f,e)) in formulas.into_iter().enumerate(){let address=ibis_doc_model::cell_name(col as u32,row as u32);s.click_cell(&address);s.enter(&format!("={f}"),"enter");assert_eq!(s.value(&address),CellValue::Error(e));s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),CellValue::Error(e));}}
    s.save_as("xsc55-arraytotext-empty.xlsx");let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();for row in 0..2{assert_eq!(sh.cell(row,0).unwrap().value,CellValue::Error(ibis_doc_model::CellError::Calc));for col in 1..4{assert_eq!(sh.cell(row,col).unwrap().value,CellValue::Error(ibis_doc_model::CellError::Value));}}
}

fn xsc55_choose_lines_options() {
    let mut s=new_sheet();let mut checks=vec![];let mut row=0;let cases=[("1",vec![1]),("-1",vec![3]),("2.9",vec![2]),("{3,1,3}",vec![3,1,3]),("{3;1;3}",vec![3,1,3]),("{3,1;2,3}",vec![3,1,2,3])];for name in ["CHOOSEROWS","CHOOSECOLS"]{for (input,lines) in &cases{let address=ibis_doc_model::cell_name(0,row);s.click_cell(&address);s.enter(&format!("={name}({{1,2,3;4,5,6;7,8,9}},{input})"),"enter");for (out,line) in lines.iter().enumerate(){for cross in 0..3{let (r,c,n)=if name=="CHOOSEROWS"{(row+out as u32,cross,((*line-1)*3+cross+1) as f64)}else{(row+cross,out as u32,(cross*3+*line) as f64)};let v=CellValue::Number(n);assert_eq!(s.value(&ibis_doc_model::cell_name(c,r)),v);checks.push((r,c,v));}}let first=s.value(&address);s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),first);row+=5;}
    for (index,e) in [("0",ibis_doc_model::CellError::Value),("4",ibis_doc_model::CellError::Value),("-4",ibis_doc_model::CellError::Value),("NA()",ibis_doc_model::CellError::NA),("{0,#N/A}",ibis_doc_model::CellError::NA),("{#N/A,0}",ibis_doc_model::CellError::NA)]{let address=ibis_doc_model::cell_name(0,row);s.click_cell(&address);s.enter(&format!("={name}({{1,2,3;4,5,6;7,8,9}},{index})"),"enter");assert_eq!(s.value(&address),CellValue::Error(e));s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),CellValue::Error(e));checks.push((row,0,CellValue::Error(e)));row+=1;}}
    s.save_as("xsc55-choose-lines-options.xlsx");let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();for (r,c,v) in checks{assert_eq!(sh.cell(r,c).unwrap().value,v);}
}

fn sort_click(s: &mut Scenario, id: &str) {
    let id=id.to_string();let b=s.with(|e,_|*e.wb().sort_edit.as_ref().unwrap().ui.borrow().get(&id).unwrap());s.click((f32::from(b.origin.x+b.size.width/2.0),f32::from(b.origin.y+b.size.height/2.0)));
}
fn sort_field(s: &mut Scenario, id: &str, text: &str) {sort_click(s,id);s.key("cmd-a");s.type_text(text);}
/// XSC-23, 並べ替え with Excel's keys: the column picked by its heading,
/// the yellow cells on top, then by the score descending.
fn xsc23_sort_keys() {
    let mut s = new_sheet();
    for (row, values) in [["名前", "点"], ["a", "1"], ["b", "5"], ["c", "3"], ["d", "4"], ["e", "2"]].into_iter().enumerate() {
        for (col, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(col as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    // b and e (rows 3 and 6) filled yellow.
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let ranges = [2u32, 5].map(|r| ibis_doc_model::CellRange { first_row: r, last_row: r, first_column: 0, last_column: 1 }).to_vec();
        e.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetFormat { slide, ranges, change: ibis_doc_model::StyleChange::Fill(Some(ibis_doc_model::Rgb(0xFFFF00))) })]).unwrap();
        cx.notify();
    });
    s.click_cell("A1");
    s.menu("xlsx.sort-manage");
    sort_field(&mut s, "range", "A1:B6");
    // 列: 点, by its heading; キー: セルの色 yellow, on top.
    sort_click(&mut s, "pick-0");
    sort_click(&mut s, "pick-0-B");
    assert_eq!(s.with(|e, cx| e.wb().sort_edit.as_ref().unwrap().levels[0].0.read(cx).value().to_string()), "B");
    sort_click(&mut s, "key-0");
    s.screenshot("sort-keys");
    let options = s.with(|e, _| e.wb().sort_edit.as_ref().unwrap().ui.borrow().keys().filter(|k| k.starts_with("key-0-")).count());
    assert_eq!(options, 3, "セルの値, and the two fills (none, yellow)");
    // The fills are listed as found: none first (row 2), then yellow.
    sort_click(&mut s, "key-0-2");
    assert_eq!(s.with(|e, _| e.wb().sort_edit.as_ref().unwrap().keys[0]), crate::xlsx::sort_edit::LevelKey::CellColor(Some(ibis_doc_model::Rgb(0xFFFF00))));
    // Then 点, descending.
    sort_click(&mut s, "add");
    sort_field(&mut s, "column-1", "B");
    sort_click(&mut s, "order-1");
    sort_click(&mut s, "apply");
    assert!(s.with(|e, _| e.wb().sort_edit.as_ref().unwrap().error.is_empty()));
    let names: Vec<CellValue> = (2..=6).map(|r| s.value(&format!("A{r}"))).collect();
    assert_eq!(names, ["b", "e", "d", "c", "a"].map(|t| CellValue::Text(t.into())).to_vec());
}

fn xsc23_sort_manage() {
    for header in [false,true]{for ascending in [false,true]{for second in [false,true]{let mut s=new_sheet();for (row,values) in [["Group","Score","ID"],["b","2","r1"],["a","2","r2"],["b","1","r3"],["a","1","r4"]].into_iter().enumerate(){for (col,value) in values.into_iter().enumerate(){s.click_cell(&ibis_doc_model::cell_name(col as u32,row as u32));s.enter(value,"enter");}}
        s.menu("xlsx.sort-manage");sort_field(&mut s,"range","A1:C5");sort_field(&mut s,"column-0","A");sort_click(&mut s,"add");
        if header && ascending && !second{sort_click(&mut s,"add");sort_click(&mut s,"remove-2");sort_click(&mut s,"down-0");assert_eq!(s.with(|e,cx|e.wb().sort_edit.as_ref().unwrap().levels[0].0.read(cx).value().to_string()),"B");sort_click(&mut s,"up-1");assert_eq!(s.with(|e,cx|e.wb().sort_edit.as_ref().unwrap().levels[0].0.read(cx).value().to_string()),"A");sort_field(&mut s,"column-1","D");sort_click(&mut s,"apply");assert!(!s.with(|e,_|e.wb().sort_edit.as_ref().unwrap().error.is_empty()));assert_eq!(s.value("C2"),CellValue::Text("r1".into()));sort_field(&mut s,"column-1","A");sort_click(&mut s,"apply");assert!(!s.with(|e,_|e.wb().sort_edit.as_ref().unwrap().error.is_empty()));sort_field(&mut s,"range","bad");sort_click(&mut s,"apply");assert!(!s.with(|e,_|e.wb().sort_edit.as_ref().unwrap().error.is_empty()));sort_field(&mut s,"range","A1:C5");}
        sort_field(&mut s,"column-1","B");if !header{sort_click(&mut s,"header");}if !ascending{sort_click(&mut s,"order-0");}if !second{sort_click(&mut s,"order-1");}let configured=s.with(|e,cx|{let p=e.wb().sort_edit.as_ref().unwrap();(p.header,p.range.read(cx).value().to_string(),p.levels.iter().map(|(i,o)|(i.read(cx).value().to_string(),*o)).collect::<Vec<_>>())});assert_eq!(configured.0,header,"{configured:?}");assert_eq!(configured.2,vec![("A".into(),ascending),("B".into(),second)],"{configured:?}");if header && ascending && second{s.screenshot("sort-manage-panel");}sort_click(&mut s,"apply");assert!(s.with(|e,_|e.wb().sort_edit.as_ref().unwrap().error.is_empty()));
        let mut expected=match (ascending,second){(true,true)=>vec!["r4","r2","r3","r1"],(true,false)=>vec!["r2","r4","r1","r3"],(false,true)=>vec!["r3","r1","r4","r2"],(false,false)=>vec!["r1","r3","r2","r4"]};if header || !ascending{expected.insert(0,"ID");}else{expected.push("ID");}for (row,id) in expected.iter().enumerate(){assert_eq!(s.value(&format!("C{}",row+1)),CellValue::Text((*id).into()));}s.click_cell("A1");s.key("cmd-z");assert_eq!(s.value("C2"),CellValue::Text("r1".into()));s.key("cmd-shift-z");for (row,id) in expected.iter().enumerate(){assert_eq!(s.value(&format!("C{}",row+1)),CellValue::Text((*id).into()));}
        s.save_as(&format!("xsc23-sort-manage-{header}-{ascending}-{second}.xlsx"));let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();for (row,id) in expected.iter().enumerate(){assert_eq!(sh.cell(row as u32,2).unwrap().value,CellValue::Text((*id).into()));}sort_click(&mut s,"close");assert!(s.with(|e,_|e.wb().sort_edit.is_none()));
    }}}
}

fn xsc23_sort_filter() {
    let mut s=new_sheet();for (address,text) in [("A1","数"),("A2","2"),("A3","4"),("A4","6"),("C1","=SUBTOTAL(9,A2:A4)")]{s.click_cell(address);s.enter(text,"enter");}s.click_cell("A1");for _ in 0..3{s.key("shift-down");}s.menu("xlsx.filter");s.update(|e,_,cx|e.filter_column(0,Some(ibis_doc_model::ColumnFilter::Values(vec!["4".into(),"6".into()])),cx));assert_eq!(s.value("C1"),CellValue::Number(10.0));s.click_cell("A1");s.menu("xlsx.sort-manage");sort_field(&mut s,"range","A1:A4");sort_field(&mut s,"column-0","A");assert!(s.with(|e,_|e.wb().sort_edit.as_ref().unwrap().header));sort_click(&mut s,"order-0");sort_click(&mut s,"apply");for (address,n) in [("A2",2.),("A3",6.),("A4",4.),("C1",10.)]{assert_eq!(s.value(address),CellValue::Number(n));}s.click_cell("A1");s.key("cmd-z");assert_eq!(s.value("A3"),CellValue::Number(4.0));s.key("cmd-shift-z");assert_eq!(s.value("A3"),CellValue::Number(6.0));
    s.save_as("xsc23-sort-filter-before-edit.xlsx");let p=s.reopen();assert_eq!(sheet_of_slide(&p.slides[1]).unwrap().filtered_rows.iter().copied().collect::<Vec<_>>(),vec![1]);let slide=p.slides[1].id;let mut d=ibis_doc_model::Document::new(p);d.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetCells {slide,edits:vec![ibis_doc_model::CellEdit {row:3,column:0,input:ibis_doc_model::CellInput::Value(CellValue::Number(3.0))}]})]).unwrap();assert_eq!(sheet_of_slide(&d.presentation().slides[1]).unwrap().cell(0,2).unwrap().value,CellValue::Number(9.0));s.click_cell("A4");s.enter("3","enter");assert_eq!(s.value("C1"),CellValue::Number(9.0));s.key("cmd-z");assert_eq!(s.value("C1"),CellValue::Number(10.0));s.key("cmd-shift-z");assert_eq!(s.value("C1"),CellValue::Number(9.0));s.save_as("xsc23-sort-filter-after-edit.xlsx");let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();assert_eq!(sh.filtered_rows.iter().copied().collect::<Vec<_>>(),vec![1]);assert_eq!(sh.cell(0,2).unwrap().value,CellValue::Number(9.0));
}

fn xsc55_spill_structure() {
    for columns in [false,true]{for (insert,at) in [(true,0),(true,2),(false,1)]{let mut s=new_sheet();s.click_cell("B2");s.enter("=SEQUENCE(3,3)","enter");s.click_cell("F6");s.enter("=SUM($B$2#)","enter");s.click_cell("C3");s.toolbar("bold");let selected=if columns{ibis_doc_model::cell_name(at,1)}else{ibis_doc_model::cell_name(1,at)};s.click_cell(&selected);s.menu(match (columns,insert){(true,true)=>"xlsx.insert-column-left",(false,true)=>"xlsx.insert-row-above",(true,false)=>"xlsx.delete-column",(false,false)=>"xlsx.delete-row"});let sum=if columns{if insert{"G6"}else{"E6"}}else{if insert{"F7"}else{"F5"}};
        if insert{let (r,c)=if at==0{if columns{(1,2)}else{(2,1)}}else{(1,1)};for row in 0..3{for col in 0..3{assert_eq!(s.value(&ibis_doc_model::cell_name(c+col,r+row)),CellValue::Number((row*3+col+1) as f64));}}assert_eq!(s.value(sum),CellValue::Number(45.0));}else{for row in 1..4{for col in 1..4{assert_eq!(s.value(&ibis_doc_model::cell_name(col,row)),CellValue::Empty);}}assert_eq!(s.value(sum),CellValue::Error(ibis_doc_model::CellError::Ref));let address=if columns{"B3"}else{"C2"};assert!(style_at(&mut s,address).bold);}
        s.key("cmd-z");assert_eq!(s.value("D4"),CellValue::Number(9.0));assert_eq!(s.value("F6"),CellValue::Number(45.0));s.key("cmd-shift-z");assert_eq!(s.value(sum),if insert{CellValue::Number(45.0)}else{CellValue::Error(ibis_doc_model::CellError::Ref)});s.save_as(&format!("xsc55-spill-structure-{columns}-{insert}-{at}.xlsx"));let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();let expected=s.with(|e,_|sheet_of_slide(e.slide()).unwrap().spills.clone());assert_eq!(sh.spills,expected);if !insert{for row in 1..4{for col in 1..4{assert!(sh.cell(row,col).is_none_or(|c|c.value.is_empty()));}}}
    }}
}

fn xsc55_spill_partial_delete() {
    for columns in [false,true] {
        let mut s=new_sheet();s.click_cell("B2");s.enter("=SEQUENCE(3,3)","enter");s.click_cell("F6");s.enter("=SUM($B$2#)","enter");s.click_cell(if columns{"C2"}else{"B3"});s.menu(if columns{"xlsx.delete-column"}else{"xlsx.delete-row"});
        let sum=if columns{"E6"}else{"F5"};assert_eq!(s.value(sum),CellValue::Number(45.0));
        for row in 0..3{for col in 0..3{assert_eq!(s.value(&ibis_doc_model::cell_name(col+1,row+1)),CellValue::Number((row*3+col+1)as f64));}}
        s.key("cmd-z");assert_eq!(s.value("F6"),CellValue::Number(45.0));s.key("cmd-shift-z");assert_eq!(s.value(sum),CellValue::Number(45.0));s.save_as(&format!("xsc55-spill-partial-{columns}.xlsx"));let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();assert_eq!(sh.spills.get(&(1,1)),Some(&ibis_doc_model::CellRange {first_row:1,last_row:3,first_column:1,last_column:3}));for row in 0..3{for col in 0..3{assert_eq!(sh.cell(row+1,col+1).unwrap().value,CellValue::Number((row*3+col+1)as f64));}}
    }
}

fn xsc08_match_options() {
    let mut s=new_sheet();let cases:Vec<_>=scenario_cases!("../../../../test/editing/xlsx-match-options.tsv").lines().skip(1).map(|line|line.split_once('\t').unwrap()).collect();
    for (row,(formula,expected)) in cases.iter().enumerate(){let address=format!("A{}",row+1);let expected=expected.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(if *expected=="#N/A"{ibis_doc_model::CellError::NA}else{ibis_doc_model::CellError::Value}));s.click_cell(&address);s.enter(&format!("={formula}"),"enter");assert_eq!(s.value(&address),expected,"{formula}");s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),expected);}
    s.save_as("xsc08-match-options.xlsx");let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();for(row,(formula,expected))in cases.iter().enumerate(){let expected=expected.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(if *expected=="#N/A"{ibis_doc_model::CellError::NA}else{ibis_doc_model::CellError::Value}));let cell=sh.cell(row as u32,0).unwrap();assert_eq!(cell.value,expected);assert_eq!(cell.formula.as_deref(),Some(*formula));}
}

fn xsc55_unique_options() {
    for(case,line)in scenario_cases!("../../../../test/editing/xlsx-unique-options.tsv").lines().skip(1).enumerate(){let fields:Vec<_>=line.split('\t').collect();let mut s=new_sheet();for(row,values)in [[1,10,1],[2,20,2],[1,10,1]].into_iter().enumerate(){for(column,n)in values.into_iter().enumerate(){s.click_cell(&ibis_doc_model::cell_name(column as u32,row as u32));s.enter(&n.to_string(),"enter");}}s.click_cell("E1");s.enter(&format!("={}",fields[0]),"enter");
        let check=|s:&mut Scenario,text:&str|{let rows:Vec<Vec<f64>>=text.split(';').map(|r|r.split(',').map(|v|v.parse().unwrap()).collect()).collect();for row in 0..3{for col in 0..3{assert_eq!(s.value(&ibis_doc_model::cell_name(col+4,row)),rows.get(row as usize).and_then(|x|x.get(col as usize)).map_or(CellValue::Empty,|n|CellValue::Number(*n)));}}};
        check(&mut s,fields[1]);s.save_as(&format!("xsc55-unique-{case}-before.xlsx"));let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();let rows:Vec<_>=fields[1].split(';').collect();assert_eq!(sh.spills.get(&(0,4)),Some(&ibis_doc_model::CellRange {first_row:0,last_row:rows.len()as u32-1,first_column:4,last_column:4+rows[0].split(',').count()as u32-1}));for(row,values)in rows.iter().enumerate(){for(col,n)in values.split(',').enumerate(){assert_eq!(sh.cell(row as u32,col as u32+4).unwrap().value,CellValue::Number(n.parse().unwrap()));}}assert_eq!(sh.cell(0,4).unwrap().formula.as_deref(),Some(fields[0]));s.click_cell("C3");s.enter("3","enter");check(&mut s,fields[2]);s.key("cmd-z");check(&mut s,fields[1]);s.key("cmd-shift-z");check(&mut s,fields[2]);s.save_as(&format!("xsc55-unique-{case}-after.xlsx"));let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();assert_eq!(sh.spills.get(&(0,4)),Some(&ibis_doc_model::CellRange {first_row:0,last_row:2,first_column:4,last_column:6}));for(row,values)in [[1.,10.,1.],[2.,20.,2.],[1.,10.,3.]].into_iter().enumerate(){for(col,n)in values.into_iter().enumerate(){assert_eq!(sh.cell(row as u32,col as u32+4).unwrap().value,CellValue::Number(n));}}
    }
}

fn xsc55_makearray_dimensions() {
    for columns in [false,true]{let mut s=new_sheet();s.click_cell("A1");s.enter("2","enter");s.click_cell("C3");let formula=if columns{"MAKEARRAY(3,A1,LAMBDA(r,c,r*10+c))"}else{"MAKEARRAY(A1,3,LAMBDA(r,c,r*10+c))"};s.enter(&format!("={formula}"),"enter");let(rows,cols)=if columns{(3,2)}else{(2,3)};for r in 0..rows{for c in 0..cols{assert_eq!(s.value(&ibis_doc_model::cell_name(c+2,r+2)),CellValue::Number(((r+1)*10+c+1)as f64));}}s.save_as(&format!("xsc55-makearray-{columns}-before.xlsx"));s.click_cell("A1");s.enter("2.9","enter");assert_eq!(s.value("C3"),CellValue::Error(ibis_doc_model::CellError::Value));for r in 2..5{for c in 2..5{if(r,c)!=(2,2){assert_eq!(s.value(&ibis_doc_model::cell_name(c,r)),CellValue::Empty);}}}s.key("cmd-z");assert_eq!(s.value("C3"),CellValue::Number(11.0));s.key("cmd-shift-z");assert_eq!(s.value("C3"),CellValue::Error(ibis_doc_model::CellError::Value));s.save_as(&format!("xsc55-makearray-{columns}-after.xlsx"));let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();assert_eq!(sh.cell(2,2).unwrap().value,CellValue::Error(ibis_doc_model::CellError::Value));assert_eq!(sh.cell(2,2).unwrap().formula.as_deref(),Some(formula));for r in 2..5{for c in 2..5{if(r,c)!=(2,2){assert!(sh.cell(r,c).is_none_or(|x|x.value.is_empty()));}}}
    }
}

fn xsc55_lambda_bindings() {
    let mut s=new_sheet();for(address,text)in[("A1","1"),("A2","2"),("B1","3"),("B2","4")]{s.click_cell(address);s.enter(text,"enter");}let formula=ibis_calc::formula::from_file("_xlfn.MAP(A1:A2,B1:B2,_xlfn.LAMBDA(_xlpm.x0,_xlpm.x1,_xlpm.x0+_xlpm.x1))");s.click_cell("D1");s.enter(&format!("={formula}"),"enter");assert_eq!(s.value("D1"),CellValue::Number(4.0));assert_eq!(s.value("D2"),CellValue::Number(6.0));s.click_cell("A1");s.enter("10","enter");assert_eq!(s.value("D1"),CellValue::Number(13.0));s.key("cmd-z");assert_eq!(s.value("D1"),CellValue::Number(4.0));s.key("cmd-shift-z");assert_eq!(s.value("D1"),CellValue::Number(13.0));s.save_as("xsc55-lambda-bindings.xlsx");let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();assert_eq!(sh.cell(0,3).unwrap().value,CellValue::Number(13.0));assert_eq!(sh.cell(1,3).unwrap().value,CellValue::Number(6.0));assert_eq!(sh.cell(0,3).unwrap().formula.as_deref(),Some(formula.as_str()));
}

fn xsc55_map_shapes() {
    let mut s=new_sheet();s.click_cell("J10");s.enter("5","enter");let cases:Vec<_>=scenario_cases!("../../../../test/editing/xlsx-map-shapes.tsv").lines().skip(1).map(|line|line.split_once('\t').unwrap()).collect();for(i,(formula,expected))in cases.iter().enumerate(){let address=format!("A{}",i*3+1);let check=|s:&mut Scenario|{for(row,values)in expected.split(';').enumerate(){for(col,v)in values.split(',').enumerate(){let expected=v.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==v).unwrap()));assert_eq!(s.value(&ibis_doc_model::cell_name(col as u32,i as u32*3+row as u32)),expected);}}};s.click_cell(&address);s.enter(&format!("={formula}"),"enter");check(&mut s);s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");check(&mut s);}s.save_as("xsc55-map-shapes.xlsx");let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();for(i,(formula,expected))in cases.iter().enumerate(){assert_eq!(sh.cell(i as u32*3,0).unwrap().formula.as_deref(),Some(*formula));for(row,values)in expected.split(';').enumerate(){for(col,v)in values.split(',').enumerate(){let expected=v.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==v).unwrap()));assert_eq!(sh.cell(i as u32*3+row as u32,col as u32).unwrap().value,expected);}}}
}

fn xsc55_lambda_arity() {
    let mut s=new_sheet();let cases:Vec<_>=scenario_cases!("../../../../test/editing/xlsx-lambda-arity.tsv").lines().skip(1).map(|line|line.split_once('\t').unwrap()).collect();for(row,(formula,v))in cases.iter().enumerate(){let expected=v.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==*v).unwrap()));let address=format!("A{}",row+1);s.click_cell(&address);s.enter(&format!("={formula}"),"enter");assert_eq!(s.value(&address),expected,"{formula}");s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),expected);}s.save_as("xsc55-lambda-arity.xlsx");let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();for(row,(formula,v))in cases.iter().enumerate(){let expected=v.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==*v).unwrap()));let cell=sh.cell(row as u32,0).unwrap();assert_eq!(cell.value,expected);assert_eq!(cell.formula.as_deref(),Some(*formula));}
}

fn xsc55_isomitted() {
    let cases: Vec<_> = scenario_cases!("../../../../test/editing/xlsx-isomitted.tsv")
        .lines().skip(1).map(|line| line.split_once('\t').unwrap()).collect();
    for (group, cases) in cases.chunks(6).enumerate() {
        let mut s = new_sheet();
        s.click_cell("A2");
        s.enter("7", "enter");
        for (i, (formula, expected)) in cases.iter().enumerate() {
            let address = format!("D{}", i * 3 + 1);
            let check = |s: &mut Scenario| {
                for (row, values) in expected.split(';').enumerate() {
                    for (col, v) in values.split(',').enumerate() {
                        assert_eq!(s.value(&ibis_doc_model::cell_name(col as u32 + 3, i as u32 * 3 + row as u32)), CellValue::Number(v.parse().unwrap()), "{formula}");
                    }
                }
            };
            s.click_cell(&address);
            assert_eq!(s.selected(), address, "before enter {formula}");
            s.enter(&format!("={formula}"), "enter");
            assert_eq!(s.formula(&address).as_deref(), Some(*formula));
            check(&mut s);
            s.click_cell(&address);
            s.key("cmd-z");
            assert_eq!(s.value(&address), CellValue::Empty);
            s.key("cmd-shift-z");
            assert_eq!(s.formula(&address).as_deref(), Some(*formula));
            check(&mut s);
        }
        s.save_as(&format!("xsc55-isomitted-{group}.xlsx"));
        let p = s.reopen();
        let sh = sheet_of_slide(&p.slides[1]).unwrap();
        for (i, (formula, expected)) in cases.iter().enumerate() {
            assert_eq!(sh.cell(i as u32 * 3, 3).unwrap().formula.as_deref(), Some(*formula));
            for (row, values) in expected.split(';').enumerate() {
                for (col, v) in values.split(',').enumerate() {
                    assert_eq!(sh.cell(i as u32 * 3 + row as u32, col as u32 + 3).unwrap().value, CellValue::Number(v.parse().unwrap()));
                }
            }
        }
    }
}

fn xsc01_scrollbar_edge_selection() {
    let mut s = new_sheet();
    s.click_cell("A2");
    s.enter("7", "enter");
    for (i, line) in scenario_cases!("../../../../test/editing/xlsx-isomitted.tsv").lines().skip(1).enumerate() {
        let (formula, _) = line.split_once('\t').unwrap();
        let address = format!("D{}", i * 3 + 1);
        s.click_cell(&address);
        if s.selected() != address {
            s.screenshot("selection-mismatch");
            let geometry = s.with(|e, _| {
                let p = e.cell_center(3, i as u32 * 3).unwrap();
                (p, e.cell_under(p), e.workspace_bounds, e.workspace_scroll.offset())
            });
            panic!("requested {address}, selected {}, geometry {geometry:?}", s.selected());
        }
        s.enter(&format!("={formula}"), "enter");
        assert_eq!(s.formula(&address).as_deref(), Some(formula));
        s.click_cell(&address);
        s.key("cmd-z");
        s.key("cmd-shift-z");
        assert_eq!(s.formula(&address).as_deref(), Some(formula));
    }
    s.save_as("xsc01-scrollbar-edge.xlsx");
    let p=s.reopen();
    let sh=sheet_of_slide(&p.slides[1]).unwrap();
    for(i,line)in scenario_cases!("../../../../test/editing/xlsx-isomitted.tsv").lines().skip(1).enumerate(){let(formula,_)=line.split_once('\t').unwrap();assert_eq!(sh.cell(i as u32*3,3).unwrap().formula.as_deref(),Some(formula));}
}

fn xsc55_spill_cross_sheet() {
    for columns in [false,true] {for(insert,at)in[(true,0),(true,2),(false,1),(false,2)] {
        let mut s=new_sheet();
        let (source,source_name)=s.with(|e,_|(e.current_slide,e.slide().name.clone().unwrap()));
        s.click_cell("B2");s.enter("=SEQUENCE(3,3)","enter");
        s.menu("xlsx.insert-sheet");
        let (observer,observer_name)=s.with(|e,_|(e.current_slide,e.slide().name.clone().unwrap()));
        for(address,formula)in[("A1",format!("SUM({source_name}!$B$2#)")),("A2",format!("{source_name}!B2+{source_name}!$B2+{source_name}!B$2+{source_name}!$B$2")),("A4",format!("MAP({source_name}!$B$2#,LAMBDA(value,value*2))")),("A8","SUM(A4#)".into())] {s.click_cell(address);s.enter(&format!("={formula}"),"enter");}
        let check=|s:&mut Scenario,deleted:bool|{for(address,n)in[("A1",45.0),("A2",4.0),("A8",90.0)]{assert_eq!(s.value(address),if deleted{CellValue::Error(ibis_doc_model::CellError::Ref)}else{CellValue::Number(n)});}for r in 0..3{for c in 0..3{let expected=if deleted{if(r,c)==(0,0){CellValue::Error(ibis_doc_model::CellError::Ref)}else{CellValue::Empty}}else{CellValue::Number(((r*3+c+1)*2)as f64)};assert_eq!(s.value(&ibis_doc_model::cell_name(c,r+3)),expected);}}};
        check(&mut s,false);s.key("ctrl-pageup");assert_eq!(s.with(|e,_|e.current_slide),source);
        s.click_cell(&if columns{ibis_doc_model::cell_name(at,1)}else{ibis_doc_model::cell_name(1,at)});
        s.menu(match(columns,insert){(true,true)=>"xlsx.insert-column-left",(false,true)=>"xlsx.insert-row-above",(true,false)=>"xlsx.delete-column",(false,false)=>"xlsx.delete-row"});
        s.key("ctrl-pagedown");assert_eq!(s.with(|e,_|e.current_slide),observer);
        let deleted=!insert&&at==1;check(&mut s,deleted);s.key("cmd-z");check(&mut s,false);s.key("cmd-shift-z");check(&mut s,deleted);
        s.save_as(&format!("xsc55-spill-cross-sheet-{columns}-{insert}-{at}.xlsx"));let p=s.reopen();let sh=sheet_of_slide(p.slides.iter().find(|slide|slide.name.as_deref()==Some(observer_name.as_str())).unwrap()).unwrap();
        for(address,n)in[("A1",45.0),("A2",4.0),("A8",90.0)]{let(c,r)=parse(address);assert_eq!(sh.cell(r,c).unwrap().value,if deleted{CellValue::Error(ibis_doc_model::CellError::Ref)}else{CellValue::Number(n)});}
        for r in 0..3{for c in 0..3{let expected=if deleted{if(r,c)==(0,0){CellValue::Error(ibis_doc_model::CellError::Ref)}else{CellValue::Empty}}else{CellValue::Number(((r*3+c+1)*2)as f64)};assert_eq!(sh.cell(r+3,c).map(|x|x.value.clone()).unwrap_or_default(),expected);}}
    }}
}

fn xsc55_dynamic_reference() {
    let mut s=new_sheet();s.click_cell("A1");s.enter("3","enter");s.click_cell("B2");s.enter("=SEQUENCE(A1)","enter");let cases:Vec<_>=scenario_cases!("../../../../test/editing/xlsx-dynamic-reference.tsv").lines().skip(1).map(|line|line.split('\t').collect::<Vec<_>>()).collect();
    let check=|s:&mut Scenario,col:usize|{for(i,f)in cases.iter().enumerate(){let expected:Vec<f64>=f[col].split(';').map(|v|v.parse().unwrap()).collect();for r in 0..3{assert_eq!(s.value(&ibis_doc_model::cell_name(3+i as u32*3,r+2)),expected.get(r as usize).map_or(CellValue::Empty,|n|CellValue::Number(*n)),"{} {r}",f[0]);}}};
    for(i,f)in cases.iter().enumerate(){let address=ibis_doc_model::cell_name(3+i as u32*3,2);s.click_cell(&address);assert_eq!(s.selected(),address);s.enter(&format!("={}",f[0]),"enter");}check(&mut s,1);s.save_as("xsc55-dynamic-reference-before.xlsx");s.click_cell("A1");s.enter("2","enter");check(&mut s,2);s.key("cmd-z");check(&mut s,1);s.key("cmd-shift-z");check(&mut s,2);s.save_as("xsc55-dynamic-reference-after.xlsx");let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();for(i,f)in cases.iter().enumerate(){assert_eq!(sh.cell(2,3+i as u32*3).unwrap().formula.as_deref(),Some(f[0]));let expected:Vec<f64>=f[2].split(';').map(|v|v.parse().unwrap()).collect();for r in 0..3{assert_eq!(sh.cell(r+2,3+i as u32*3).map(|x|x.value.clone()).unwrap_or_default(),expected.get(r as usize).map_or(CellValue::Empty,|n|CellValue::Number(*n)));}}
}


fn xsc55_text_boundary() {
    let mut s=new_sheet();let cases:Vec<_>=scenario_cases!("../../../../test/editing/xlsx-text-boundary.tsv").lines().skip(1).map(|line|line.split_once('\t').unwrap()).collect();for(row,(formula,v))in cases.iter().enumerate(){let expected=v.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==*v).unwrap()));let address=format!("A{}",row+1);s.click_cell(&address);s.enter(&format!("={formula}"),"enter");assert_eq!(s.value(&address),expected,"{formula}");s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(&address),expected);}s.save_as("xsc55-text-boundary.xlsx");let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();for(row,(formula,v))in cases.iter().enumerate(){let expected=v.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==*v).unwrap()));let cell=sh.cell(row as u32,0).unwrap();assert_eq!(cell.value,expected);assert_eq!(cell.formula.as_deref(),Some(*formula));}
}

fn xsc55_compatibility_len() {
    for (path,version) in [("test/editing/xlsx-compatibility-v1.xlsx",1),("test/editing/xlsx-sales.xlsx",2)] {
        let mut s=Scenario::open(path);
        s.menu("xlsx.insert-sheet");
        let cases=[("😀x",3.,2.),("😀\u{fe0f}",3.,2.),("😀🏽",4.,2.),("👩‍👩‍👧‍👧",11.,7.),("日本語",3.,3.),("",0.,0.)];
        for (i,(text,legacy,modern)) in cases.iter().enumerate() {
            let source=format!("A{}",i+1);let output=format!("B{}",i+1);
            let expected=CellValue::Number(if version==2 {*modern} else {*legacy});
            s.click_cell(&source);s.enter(text,"enter");
            s.click_cell(&output);s.enter(&format!("=LEN({source})"),"enter");
            assert_eq!(s.value(&output),expected);
            s.key("cmd-z");assert_eq!(s.value(&output),CellValue::Empty);
            s.key("cmd-shift-z");assert_eq!(s.value(&output),expected);
            s.click_cell(&source);s.enter("abc","enter");assert_eq!(s.value(&output),CellValue::Number(3.));
            s.key("cmd-z");assert_eq!(s.value(&output),expected);
            s.key("cmd-shift-z");assert_eq!(s.value(&output),CellValue::Number(3.));
            s.key("cmd-z");assert_eq!(s.value(&output),expected);
        }
        s.save_as(&format!("xsc55-compatibility-len-v{version}.xlsx"));
        let p=s.reopen();assert_eq!(p.workbook.as_deref().unwrap().compatibility_version,Some(version));
        let sh=sheet_of_slide(&p.slides[1]).unwrap();
        for(i,(_,legacy,modern))in cases.iter().enumerate() {
            let cell=sh.cell(i as u32,1).unwrap();
            assert_eq!(cell.formula.as_deref(),Some(format!("LEN(A{})",i+1).as_str()));
            assert_eq!(cell.value,CellValue::Number(if version==2 {*modern} else {*legacy}));
        }
    }
}

fn xsc55_compatibility_slice() {
    for(path,version)in[("test/editing/xlsx-compatibility-v1.xlsx",1),("test/editing/xlsx-sales.xlsx",2)] {
        let mut s=Scenario::open(path);s.menu("xlsx.insert-sheet");
        let cases:Vec<_>=scenario_cases!("../../../../test/editing/xlsx-compatibility-slice.tsv").lines().skip(1).map(|line|line.split('\t').collect::<Vec<_>>()).collect();
        for(row,f)in cases.iter().enumerate() {
            let text=f[version as usize];let expected=ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==text).map_or_else(||CellValue::Text(text.into()),|e|CellValue::Error(*e));
            let address=format!("A{}",row+1);s.click_cell(&address);s.enter(&format!("={}",f[0]),"enter");
            assert_eq!(s.value(&address),expected,"version {version}: {}",f[0]);
            s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);
            s.key("cmd-shift-z");assert_eq!(s.value(&address),expected);
        }
        s.save_as(&format!("xsc55-compatibility-slice-v{version}.xlsx"));let p=s.reopen();
        assert_eq!(p.workbook.as_deref().unwrap().compatibility_version,Some(version));let sh=sheet_of_slide(&p.slides[1]).unwrap();
        for(row,f)in cases.iter().enumerate() {
            let text=f[version as usize];let expected=ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==text).map_or_else(||CellValue::Text(text.into()),|e|CellValue::Error(*e));
            let cell=sh.cell(row as u32,0).unwrap();assert_eq!(cell.formula.as_deref(),Some(f[0]));assert_eq!(cell.value,expected);
        }
    }
}

fn xsc55_compatibility_search() {
    for(path,version)in[("test/editing/xlsx-compatibility-v1.xlsx",1),("test/editing/xlsx-sales.xlsx",2)] {
        let mut s=Scenario::open(path);s.menu("xlsx.insert-sheet");
        let cases:Vec<_>=scenario_cases!("../../../../test/editing/xlsx-compatibility-search.tsv").lines().skip(1).map(|line|line.split('\t').collect::<Vec<_>>()).collect();
        for(row,f)in cases.iter().enumerate() {
            let text=f[version as usize];let expected=text.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==text).unwrap()));
            let address=format!("A{}",row+1);s.click_cell(&address);s.enter(&format!("={}",f[0]),"enter");
            assert_eq!(s.value(&address),expected,"version {version}: {}",f[0]);
            s.key("cmd-z");assert_eq!(s.value(&address),CellValue::Empty);
            s.key("cmd-shift-z");assert_eq!(s.value(&address),expected);
        }
        s.save_as(&format!("xsc55-compatibility-search-v{version}.xlsx"));let p=s.reopen();
        assert_eq!(p.workbook.as_deref().unwrap().compatibility_version,Some(version));let sh=sheet_of_slide(&p.slides[1]).unwrap();
        for(row,f)in cases.iter().enumerate() {
            let text=f[version as usize];let expected=text.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==text).unwrap()));
            let cell=sh.cell(row as u32,0).unwrap();assert_eq!(cell.formula.as_deref(),Some(f[0]));assert_eq!(cell.value,expected);
        }
    }
}

fn xsc55_compatibility_toggle() {
    let mut s=new_sheet();let cases:Vec<_>=scenario_cases!("../../../../test/editing/xlsx-compatibility-toggle.tsv").lines().skip(1).map(|line|line.split('\t').collect::<Vec<_>>()).collect();
    for(i,f)in cases.iter().enumerate(){s.click_cell(&format!("A{}",i*3+1));s.enter(&format!("={}",f[0]),"enter");}
    let check=|s:&mut Scenario,version:u32| {
        assert_eq!(s.menu_state("xlsx.compatibility-v1"),Some((version!=1,version==1)));
        assert_eq!(s.menu_state("xlsx.compatibility-v2"),Some((version!=2,version==2)));
        for(i,f)in cases.iter().enumerate(){let text=f[version as usize+1];let expected=if f[1]=="T" {vec![CellValue::Text(text.into())]} else {text.split(';').map(|v|CellValue::Number(v.parse().unwrap())).collect()};for r in 0..2{assert_eq!(s.value(&format!("A{}",i*3+1+r)),expected.get(r).cloned().unwrap_or_default(),"version {version}: {}",f[0]);}}
    };
    check(&mut s,2);
    for(version,id)in[(1,"xlsx.compatibility-v1"),(2,"xlsx.compatibility-v2")] {
        s.click_cell("A1");s.menu(id);check(&mut s,version);
        s.key("cmd-z");check(&mut s,3-version);
        s.key("cmd-shift-z");check(&mut s,version);
        s.save_as(&format!("xsc55-compatibility-toggle-v{version}.xlsx"));let p=s.reopen();assert_eq!(p.workbook.as_deref().unwrap().compatibility_version,Some(version));
        let sh=sheet_of_slide(&p.slides[1]).unwrap();for(i,f)in cases.iter().enumerate(){assert_eq!(sh.cell(i as u32*3,0).unwrap().formula.as_deref(),Some(f[0]));let text=f[version as usize+1];let expected=if f[1]=="T" {vec![CellValue::Text(text.into())]} else {text.split(';').map(|v|CellValue::Number(v.parse().unwrap())).collect()};for r in 0..2{assert_eq!(sh.cell(i as u32*3+r,0).map(|c|c.value.clone()).unwrap_or_default(),expected.get(r as usize).cloned().unwrap_or_default());}}
    }
}


fn xsc55_dynamic_reopen() {
    let cases: Vec<_> = scenario_cases!("../../../../test/editing/xlsx-dynamic-reopen.tsv").lines().skip(1).map(|line| line.split('\t').collect::<Vec<_>>()).collect();
    let check = |s: &mut Scenario, col: usize| {
        for (i, f) in cases.iter().enumerate() {
            let expected: Vec<f64> = f[col].split(';').map(|v| v.parse().unwrap()).collect();
            for r in 0..3 { assert_eq!(s.value(&ibis_doc_model::cell_name(3 + i as u32 * 3, r + 2)), expected.get(r as usize).map_or(CellValue::Empty, |v| CellValue::Number(*v)), "{} row {}", f[0], r); }
        }
    };
    let mut s = new_sheet();
    s.click_cell("A1"); s.enter("1", "enter");
    s.click_cell("B2"); s.enter("=SEQUENCE(A1)", "enter");
    for (i, f) in cases.iter().enumerate() { s.click_cell(&ibis_doc_model::cell_name(3 + i as u32 * 3, 2)); s.enter(&format!("={}", f[0]), "enter"); }
    check(&mut s, 1);
    s.save_as("xsc55-dynamic-reopen-source.xlsx");
    s.reload_saved();
    s.click_cell("A1"); s.key("ctrl-pagedown");
    assert_eq!(s.with(|e, _| e.document.presentation().slides.iter().position(|slide| slide.id == e.current_slide)), Some(1));
    check(&mut s, 1);
    s.click_cell("A1"); s.enter("3", "enter"); check(&mut s, 2);
    s.key("cmd-z"); check(&mut s, 1);
    s.key("cmd-shift-z"); check(&mut s, 2);
    s.save_as("xsc55-dynamic-reopen-grown.xlsx");
    let p = s.reopen(); let sh = sheet_of_slide(&p.slides[1]).unwrap();
    for (i, f) in cases.iter().enumerate() {
        let col = 3 + i as u32 * 3;
        assert_eq!(sh.cell(2, col).unwrap().formula.as_deref(), Some(f[0]));
        // A formula that gives one value is saved plain (as Excel saves it).
        assert_eq!(sh.spills.contains_key(&(2, col)), ibis_calc::formula::arrays::can_return_array(f[0], &|_| false), "{}", f[0]);
        let expected: Vec<f64> = f[2].split(';').map(|v| v.parse().unwrap()).collect();
        for r in 0..3 { assert_eq!(sh.cell(r + 2, col).map(|c| c.value.clone()).unwrap_or_default(), expected.get(r as usize).map_or(CellValue::Empty, |v| CellValue::Number(*v))); }
    }
}


fn xsc39_week_return_types() {
    let mut s = new_sheet(); let cases: Vec<_> = scenario_cases!("../../../../test/editing/xlsx-week-return-types.tsv").lines().skip(1).map(|line| line.split('\t').collect::<Vec<_>>()).collect();
    s.click_cell("B1"); s.enter("=DATE(2021,1,1)", "enter");
    for (r, f) in cases.iter().enumerate() { s.click_cell(&format!("A{}", r + 1)); s.enter(&format!("={}", f[0]), "enter"); }
    let check = |s: &mut Scenario, col: usize| { for (r, f) in cases.iter().enumerate() { let expected = f[col].parse::<f64>().map(CellValue::Number).unwrap_or(CellValue::Error(ibis_doc_model::CellError::Num)); assert_eq!(s.value(&format!("A{}", r + 1)), expected, "{}", f[0]); } };
    check(&mut s, 1); s.save_as("xsc39-week-return-types-before.xlsx");
    s.reload_saved(); s.click_cell("A1"); s.key("ctrl-pagedown"); check(&mut s, 1);
    s.click_cell("B1"); s.enter("=DATE(2021,1,4)", "enter"); check(&mut s, 2);
    s.key("cmd-z"); check(&mut s, 1); s.key("cmd-shift-z"); check(&mut s, 2);
    s.save_as("xsc39-week-return-types-after.xlsx"); let p = s.reopen(); let sh = sheet_of_slide(&p.slides[1]).unwrap();
    for (r, f) in cases.iter().enumerate() { let cell = sh.cell(r as u32, 0).unwrap(); assert_eq!(cell.formula.as_deref(), Some(f[0])); let expected = f[2].parse::<f64>().map(CellValue::Number).unwrap_or(CellValue::Error(ibis_doc_model::CellError::Num)); assert_eq!(cell.value, expected); }
}


fn xsc39_day_count_options() {
    let mut s = new_sheet(); let cases: Vec<_> = scenario_cases!("../../../../test/editing/xlsx-day-count-options.tsv").lines().skip(1).map(|line| line.split('\t').collect::<Vec<_>>()).collect();
    s.click_cell("B1"); s.enter("=DATE(2019,1,1)", "enter"); s.click_cell("C1"); s.enter("=DATE(2020,1,1)", "enter");
    for (r, f) in cases.iter().enumerate() { s.click_cell(&format!("A{}", r + 1)); s.enter(&format!("={}", f[0]), "enter"); }
    let check = |s: &mut Scenario, col: usize| { for (r, f) in cases.iter().enumerate() { let expected = f[col].parse::<f64>().map(CellValue::Number).unwrap_or(CellValue::Error(ibis_doc_model::CellError::Num)); assert_eq!(s.value(&format!("A{}", r + 1)), expected, "{}", f[0]); } };
    check(&mut s, 1); s.save_as("xsc39-day-count-before.xlsx"); s.reload_saved(); s.click_cell("A1"); s.key("ctrl-pagedown"); check(&mut s, 1);
    s.click_cell("C1"); s.enter("=DATE(2021,1,1)", "enter"); s.click_cell("B1"); s.enter("=DATE(2020,1,1)", "enter"); check(&mut s, 2);
    s.key("cmd-z"); s.key("cmd-z"); check(&mut s, 1); s.key("cmd-shift-z"); s.key("cmd-shift-z"); check(&mut s, 2);
    s.save_as("xsc39-day-count-after.xlsx"); let p = s.reopen(); let sh = sheet_of_slide(&p.slides[1]).unwrap();
    for (r, f) in cases.iter().enumerate() { let cell = sh.cell(r as u32, 0).unwrap(); assert_eq!(cell.formula.as_deref(), Some(f[0])); let expected = f[2].parse::<f64>().map(CellValue::Number).unwrap_or(CellValue::Error(ibis_doc_model::CellError::Num)); assert_eq!(cell.value, expected); }
}


fn xsc55_new_dynamic_metadata() {
    let mut s=Scenario::open("test/editing/xlsx-nondynamic-metadata.xlsx");s.click_cell("A1");s.key("ctrl-pagedown");
    assert_eq!(s.with(|e,_|e.slide().name.clone()),Some("新dynamic保持".into()));
    for (address,formula,n) in [("F10","SEQUENCE(A1)",1.),("H10","F10#*2",2.)] {s.click_cell(address);s.enter(&format!("={formula}"),"enter");assert_eq!(s.value(address),CellValue::Number(n));s.key("cmd-z");assert_eq!(s.value(address),CellValue::Empty);s.key("cmd-shift-z");assert_eq!(s.value(address),CellValue::Number(n));}
    let check=|s:&mut Scenario,size:u32| {s.with(|e,_| {let sh=sheet_of_slide(e.slide()).unwrap();assert!(!sh.spills.contains_key(&(1,1)));assert!(sh.spills.contains_key(&(9,5)));assert!(sh.spills.contains_key(&(9,7)));});assert_eq!(s.value("B2"),CellValue::Number(1.));assert_eq!(s.value("B3"),CellValue::Empty);for r in 0..3 {let expected=if r<size{CellValue::Number((r+1)as f64)}else{CellValue::Empty};assert_eq!(s.value(&format!("F{}",r+10)),expected);assert_eq!(s.value(&format!("H{}",r+10)),if r<size{CellValue::Number(((r+1)*2)as f64)}else{CellValue::Empty});}};
    check(&mut s,1);s.save_as("xsc55-new-dynamic-one.xlsx");s.reload_saved();s.click_cell("A1");s.key("ctrl-pagedown");check(&mut s,1);
    s.click_cell("A1");s.enter("3","enter");check(&mut s,3);s.key("cmd-z");check(&mut s,1);s.key("cmd-shift-z");check(&mut s,3);
    s.save_as("xsc55-new-dynamic-three.xlsx");let p=s.reopen();let sh=sheet_of_slide(&p.slides[1]).unwrap();assert!(!sh.spills.contains_key(&(1,1)));for(c,f)in[(5,"SEQUENCE(A1)"),(7,"F10#*2")] {assert!(sh.spills.contains_key(&(9,c)));assert_eq!(sh.cell(9,c).unwrap().formula.as_deref(),Some(f));for r in 0..3 {assert_eq!(sh.cell(r+9,c).unwrap().value,CellValue::Number(((r+1)*if c==5{1}else{2})as f64));}}
}


fn xsc55_composite_formula_literal() {
    let mut s=Scenario::open("test/editing/xlsx-composite-metadata.xlsx"); s.click_cell("A1"); s.key("ctrl-pagedown");
    assert_eq!(s.with(|e,_|e.slide().name.clone()),Some("複合保持".into()));
    let check_formula=|s:&mut Scenario,n:f64| {assert_eq!(s.value("B2"),CellValue::Number(n));assert_eq!(s.value("D3"),CellValue::Number(n*2.));s.with(|e,_|assert!(sheet_of_slide(e.slide()).unwrap().spills.contains_key(&(1,1))));};
    let check_literal=|s:&mut Scenario| {assert_eq!(s.value("B2"),CellValue::Number(42.));assert_eq!(s.shown("D3"),"#REF!");s.with(|e,_|{let sh=sheet_of_slide(e.slide()).unwrap();assert!(!sh.spills.contains_key(&(1,1)));assert!(sh.cell(1,1).unwrap().formula.is_none());});};
    check_formula(&mut s,1.); s.click_cell("B2"); s.enter("=SEQUENCE(A1)*3","enter"); check_formula(&mut s,3.);
    s.key("cmd-z"); check_formula(&mut s,1.); s.key("cmd-shift-z"); check_formula(&mut s,3.);
    s.save_as("xsc55-composite-rewritten.xlsx"); s.reload_saved(); s.click_cell("A1"); s.key("ctrl-pagedown"); check_formula(&mut s,3.);
    s.click_cell("B2"); s.enter("42","enter"); check_literal(&mut s); s.key("cmd-z"); check_formula(&mut s,3.); s.key("cmd-shift-z"); check_literal(&mut s);
    s.save_as("xsc55-composite-literal.xlsx"); s.reload_saved(); s.click_cell("A1"); s.key("ctrl-pagedown"); check_literal(&mut s);
    s.click_cell("B2"); s.enter("=SEQUENCE(A1)*5","enter"); check_formula(&mut s,5.); s.key("cmd-z"); check_literal(&mut s); s.key("cmd-shift-z"); check_formula(&mut s,5.);
    s.save_as("xsc55-composite-reactivated.xlsx"); s.reload_saved(); s.click_cell("A1"); s.key("ctrl-pagedown"); check_formula(&mut s,5.);
    s.click_cell("A1");s.enter("3","enter");assert_eq!(s.value("B4"),CellValue::Number(15.));
    s.save_as("xsc55-composite-reactivated-grown.xlsx");s.reload_saved();s.click_cell("A1");s.key("ctrl-pagedown");assert_eq!(s.value("B4"),CellValue::Number(15.));
}

fn xsc55_compatibility_whitespace() {
    let mut s=Scenario::open("test/editing/xlsx-compatibility-whitespace.xlsx");
    let cases=[("LEN(\"😀x\")",CellValue::Number(3.),CellValue::Number(2.)),("MID(\"😀x\",1,1)",CellValue::Text("�".into()),CellValue::Text("😀".into())),("REPLACE(\"😀x\",1,1,\"Q\")",CellValue::Text("Q�x".into()),CellValue::Text("Qx".into())),("FIND(\"x\",\"😀x\")",CellValue::Number(3.),CellValue::Number(2.)),("SEARCH(\"?x\",\"😀x\")",CellValue::Number(2.),CellValue::Number(1.))];
    let check=|s:&mut Scenario,v:u32| {assert_eq!(s.menu_state("xlsx.compatibility-v1"),Some((v!=1,v==1)));assert_eq!(s.menu_state("xlsx.compatibility-v2"),Some((v!=2,v==2)));for(i,(_,old,new))in cases.iter().enumerate(){assert_eq!(s.value(&format!("A{}",i+1)),if v==1{old.clone()}else{new.clone()});}};
    assert_eq!(s.menu_state("xlsx.compatibility-v2"),Some((false,true)));
    for(i,(f,_,_))in cases.iter().enumerate(){s.click_cell(&format!("A{}",i+1));s.enter(&format!("={f}"),"enter");}check(&mut s,2);
    for(v,id)in[(1,"xlsx.compatibility-v1"),(2,"xlsx.compatibility-v2")] {
        s.click_cell("A1");s.menu(id);check(&mut s,v);s.key("cmd-z");check(&mut s,3-v);s.key("cmd-shift-z");check(&mut s,v);
        s.save_as(&format!("xsc55-whitespace-v{v}.xlsx"));s.reload_saved();check(&mut s,v);
    }
}

fn xsc39_aggregate_count_options() {
    let mut s=new_sheet();for(address,input)in[("A1","5"),("A2","=NA()"),("A3","text"),("A4","=\"\""),("A5","=SUBTOTAL(3,A1:A4)"),("A6","=TRUE"),("A8","8")] {s.click_cell(address);s.enter(input,"enter");}
    for f in 2..=3 {for o in 0..8 {let a=ibis_doc_model::cell_name(f-1,o);s.click_cell(&a);s.enter(&format!("=AGGREGATE({f},{o},A1:A8)"),"enter");s.key("cmd-z");assert_eq!(s.value(&a),CellValue::Empty);s.key("cmd-shift-z");}}
    let check=|s:&mut Scenario,error_hidden:bool,number_hidden:bool| {for o in 0..8 {let nested=o<4;let skip_errors=matches!(o,2|3|6|7);let hides=o%2==1;let count=3-i32::from(nested)-i32::from(hides&&number_hidden);let counta=7-i32::from(nested)-i32::from(skip_errors)-i32::from(hides&&number_hidden)-i32::from(hides&&error_hidden&&!skip_errors);assert_eq!(s.value(&format!("B{}",o+1)),CellValue::Number(count as f64));assert_eq!(s.value(&format!("C{}",o+1)),CellValue::Number(counta as f64));}};
    check(&mut s,false,false);s.save_as("xsc39-aggregate-count-shown.xlsx");
    s.click_cell("A2");s.key("ctrl-9");check(&mut s,true,false);s.key("cmd-z");check(&mut s,false,false);s.key("cmd-shift-z");check(&mut s,true,false);
    s.click_cell("A8");s.key("ctrl-9");check(&mut s,true,true);s.key("cmd-z");check(&mut s,true,false);s.key("cmd-shift-z");check(&mut s,true,true);
    s.save_as("xsc39-aggregate-count-hidden.xlsx");s.reload_saved();s.click_cell("A1");s.key("ctrl-pagedown");check(&mut s,true,true);
    s.click_cell("A1");for _ in 0..7 {s.key("shift-down");}s.key("ctrl-shift-9");check(&mut s,false,false);s.key("cmd-z");check(&mut s,true,true);s.key("cmd-shift-z");check(&mut s,false,false);
    s.save_as("xsc39-aggregate-count-restored.xlsx");s.reload_saved();s.click_cell("A1");s.key("ctrl-pagedown");check(&mut s,false,false);
}

fn xsc25_conditional_cell_is() {
    let mut s=new_sheet();for v in ["5","15","25"] {s.enter(v,"enter");}
    let ops=[("lessThan",[true,false,false]),("lessThanOrEqual",[true,true,false]),("equal",[false,true,false]),("notEqual",[true,false,true]),("greaterThanOrEqual",[false,true,true]),("greaterThan",[false,false,true]),("between",[false,true,false]),("notBetween",[true,false,true])];
    for(i,(op,holds))in ops.into_iter().enumerate() {
        s.click_cell("A1");s.key("shift-down");s.key("shift-down");s.menu("xlsx.conditional");let previous=cf_model(&mut s);if !previous.is_empty(){cf_click(&mut s,"edit-0");}
        cf_click(&mut s,"kind");for _ in 0..i+2 {s.key("down");}s.key("enter");
        cf_field(&mut s,"formula","15");if i>=6 {if i==6 {cf_click(&mut s,"apply");assert_eq!(cf_model(&mut s),previous);cf_field(&mut s,"formula2","(");cf_click(&mut s,"apply");assert_eq!(cf_model(&mut s),previous);}cf_field(&mut s,"formula2","20");}cf_field(&mut s,"fill","FFC7CE");cf_field(&mut s,"range","A1:A3");cf_click(&mut s,"stop");cf_click(&mut s,"apply");
        let expected=cf_model(&mut s);assert_eq!(expected.len(),1);assert_eq!(expected[0].rules[0].kind,ibis_doc_model::CfKind::CellIs {operator:op.into(),formulas:if i>=6 {vec!["15".into(),"20".into()]}else{vec!["15".into()]}});assert_eq!(expected[0].rules[0].priority,1);assert_eq!(expected[0].rules[0].stop,i%2==0);
        for(row,hold)in holds.into_iter().enumerate(){if hold {cf_drawn(&mut s,row as u32,0xFFC7CE);}else{s.with(|e,_|{let(sh,_)=super::conditional::apply(sheet_of_slide(e.slide()).unwrap()).unwrap();let c=sh.cell(row as u32,0).unwrap();assert_ne!(sh.styles[c.style as usize].fill,Some(ibis_doc_model::Rgb(0xFFC7CE)));});}}
        s.click_cell("A1");s.key("cmd-z");assert_eq!(cf_model(&mut s),previous);s.key("cmd-shift-z");assert_eq!(cf_model(&mut s),expected);
        s.save_as(&format!("xsc25-cellis-{op}.xlsx"));s.reload_saved();s.click_cell("A1");s.key("ctrl-pagedown");assert_eq!(cf_model(&mut s),expected);
    }
    s.menu("xlsx.conditional");cf_click(&mut s,"edit-0");cf_field(&mut s,"formula","10");cf_field(&mut s,"formula2","22");cf_field(&mut s,"fill","FFFF00");cf_field(&mut s,"range","A2:A3");cf_click(&mut s,"apply");let edited=cf_model(&mut s);assert_eq!(edited[0].ranges[0].first_row,1);cf_drawn(&mut s,2,0xFFFF00);
    s.click_cell("A1");s.key("cmd-z");assert_eq!(cf_model(&mut s)[0].ranges[0].first_row,0);s.key("cmd-shift-z");assert_eq!(cf_model(&mut s),edited);s.save_as("xsc25-cellis-edited.xlsx");s.reload_saved();s.click_cell("A1");s.key("ctrl-pagedown");assert_eq!(cf_model(&mut s),edited);
    s.click_cell("A2");s.key("shift-down");s.menu("xlsx.conditional");cf_click(&mut s,"clear");assert!(cf_model(&mut s).is_empty());s.click_cell("A2");s.key("cmd-z");assert_eq!(cf_model(&mut s),edited);s.key("cmd-shift-z");assert!(cf_model(&mut s).is_empty());s.save_as("xsc25-cellis-deleted.xlsx");s.reload_saved();s.click_cell("A1");s.key("ctrl-pagedown");assert!(cf_model(&mut s).is_empty());
}

fn xsc25_conditional_opaque_edit() {
    let mut s=Scenario::open("test/editing/xlsx-conditional-opaque.xlsx");s.click_cell("A1");s.key("ctrl-pagedown");let old=cf_model(&mut s);assert_eq!(old.len(),1);cf_drawn(&mut s,1,0xFFC7CE);cf_drawn(&mut s,2,0xFFC7CE);
    s.menu("xlsx.conditional");cf_click(&mut s,"edit-0");cf_click(&mut s,"kind");for _ in 0..6 {s.key("down");}s.key("enter");cf_field(&mut s,"formula","20");cf_field(&mut s,"range","A2:A3");cf_click(&mut s,"stop");cf_click(&mut s,"apply");let edited=cf_model(&mut s);assert_eq!(edited[0].rules[0].kind,ibis_doc_model::CfKind::CellIs {operator:"greaterThanOrEqual".into(),formulas:vec!["20".into()]});assert_eq!(edited[0].rules[0].style,old[0].rules[0].style);assert_eq!(edited[0].source_index,old[0].source_index);assert_eq!(edited[0].ranges[0].first_row,1);assert!(edited[0].rules[0].stop);cf_drawn(&mut s,2,0xFFC7CE);
    s.click_cell("A1");s.key("cmd-z");assert_eq!(cf_model(&mut s),old);s.key("cmd-shift-z");assert_eq!(cf_model(&mut s),edited);
    s.save_as("xsc25-opaque-edited.xlsx");s.reload_saved();s.click_cell("A1");s.key("ctrl-pagedown");assert_eq!(cf_model(&mut s),edited);cf_drawn(&mut s,2,0xFFC7CE);
    s.menu("xlsx.conditional");cf_click(&mut s,"edit-0");cf_field(&mut s,"fill","00B050");cf_click(&mut s,"apply");let colored=cf_model(&mut s);assert_eq!(colored[0].rules[0].style.as_ref().unwrap().fill,Some(ibis_doc_model::Rgb(0x00B050)));cf_drawn(&mut s,2,0x00B050);s.click_cell("A1");s.key("cmd-z");assert_eq!(cf_model(&mut s),edited);s.key("cmd-shift-z");assert_eq!(cf_model(&mut s),colored);s.save_as("xsc25-opaque-colored.xlsx");s.reload_saved();s.click_cell("A1");s.key("ctrl-pagedown");assert_eq!(cf_model(&mut s),colored);cf_drawn(&mut s,2,0x00B050);
}

fn xsc05_cell_comments() {
    let mut s = new_sheet();
    s.click_cell("B2");
    s.enter("100", "enter");
    let slide = s.with(|e, _| e.slide().id);
    s.update(|e, _, _| {
        e.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetComment {
            slide,
            row: 1,
            column: 1,
            comment: Some(ibis_doc_model::SheetComment {
                row: 1,
                column: 1,
                author: "Tester".into(),
                text: "Target cell note".into(),
            }),
        })]).unwrap();
    });
    let c = s.with(|e, _| {
        let sh = sheet_of_slide(e.slide()).unwrap();
        sh.comment_at(1, 1).cloned()
    });
    assert_eq!(c.as_ref().map(|x| x.text.as_str()), Some("Target cell note"));

    s.save_as("xsc05-comments.xlsx");
    let p = s.reopen();
    let sh = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(sh.comment_at(1, 1).map(|x| x.text.as_str()), Some("Target cell note"));

    // Undo
    s.key("cmd-z");
    let c_after_undo = s.with(|e, _| {
        let sh = sheet_of_slide(e.slide()).unwrap();
        sh.comment_at(1, 1).cloned()
    });
    assert!(c_after_undo.is_none());

    // Redo
    s.key("cmd-shift-z");
    let c_after_redo = s.with(|e, _| {
        let sh = sheet_of_slide(e.slide()).unwrap();
        sh.comment_at(1, 1).cloned()
    });
    assert_eq!(c_after_redo.as_ref().map(|x| x.text.as_str()), Some("Target cell note"));
}

/// Times changing sheets as the user did (#7): on a workbook named by
/// IBIS_PERF_XLSX (an absolute path; skipped without it), the chart sheet
/// (the third) zoomed with the wheel down to 35%, then the first sheet's
/// tab clicked: the time until the sheet is drawn (three frames).
fn perf_sheet_switch() {
    let Some(path) = std::env::var_os("IBIS_PERF_XLSX") else { return };
    let mut s = Scenario::open(&path.to_string_lossy());
    s.update(|e, window, cx| {
        let third = e.presentation().slides[2].id;
        e.select_slide(third, window, cx);
    });
    // The wheel zooms in steps, each drawn.
    for step in 0..12 {
        let zoom = 1.0 - step as f32 * (0.65 / 11.0);
        s.update(|e, window, cx| e.set_zoom(Some(zoom), window, cx));
    }
    let started = crate::clock::Instant::now();
    s.update(|e, window, cx| {
        let first = e.presentation().slides[0].id;
        e.select_slide(first, window, cx);
    });
    eprintln!("perf_sheet_switch: {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
}

/// Times zooming a sheet with the wheel (#6): on a workbook named by
/// IBIS_PERF_XLSX (skipped without it), the first sheet zoomed from 85% to
/// 35% in steps, each drawn: each step's time, and then the same steps again
/// (zooms seen before).
fn perf_sheet_zoom() {
    let Some(path) = std::env::var_os("IBIS_PERF_XLSX") else { return };
    let mut s = Scenario::open(&path.to_string_lossy());
    // IBIS_PERF_ROUNDS: more rounds, to profile.
    let rounds = std::env::var("IBIS_PERF_ROUNDS").ok().and_then(|r| r.parse().ok()).unwrap_or(2);
    for round in 0..rounds {
        let mut times = Vec::new();
        for step in 0..11 {
            let zoom = 0.85 - step as f32 * 0.05;
            let started = crate::clock::Instant::now();
            s.update(|e, window, cx| e.set_zoom(Some(zoom), window, cx));
            times.push(format!("{:.0}%:{:.0}", zoom * 100.0, started.elapsed().as_secs_f64() * 1000.0));
        }
        eprintln!("perf_sheet_zoom round {round} (zoom:ms): {}", times.join(" "));
        // The same three frames with nothing changed (redrawn from what is kept).
        let started = crate::clock::Instant::now();
        s.update(|_, window, _| window.refresh());
        eprintln!("perf_sheet_zoom unchanged: {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
    }
}

/// Pictures of a workbook's first sheet on the screen (IBIS_PERF_XLSX;
/// skipped without it) at several zooms, scrolled down and across: to
/// compare the screen's drawing before and after a change to it
/// (`target/scenario/xlsx::perf_sheet_shots/`).
fn perf_sheet_shots() {
    let Some(path) = std::env::var_os("IBIS_PERF_XLSX") else { return };
    let mut s = Scenario::open(&path.to_string_lossy());
    for zoom in [1.0f32, 0.85, 0.5, 0.35, 2.0] {
        for (i, offset) in [(0.0f32, 0.0f32), (-150.0, -900.0), (0.0, -4000.0)].into_iter().enumerate() {
            s.update(|e, window, cx| {
                e.set_zoom(Some(zoom), window, cx);
                e.workspace_scroll.set_offset(point(px(offset.0), px(offset.1)));
                e.clamp_workspace_scroll();
            });
            s.screenshot(&format!("z{:.0}-{i}", zoom * 100.0));
        }
    }
}

/// #5: with panes frozen, the wheel turned up at the sheet's top (past it)
/// moves nothing: the frozen columns (the left pane) stay with the sheet
/// beside them, rather than going down by the turn and back (GPUI holds
/// the scrolling in bounds only as it lays out, after the panes are made).
fn fix5_frozen_wheel_past_top() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B4");
    s.menu("xlsx.freeze");
    let split = s.with(|e, _| e.workspace_content().and_then(|c| e.frozen_split(c))).expect("frozen panes");
    let ws = s.with(|e, _| e.workspace_bounds).expect("the workspace");
    let left = split.0.expect("frozen columns");
    // The left pane, less the edges where the scrollbars show.
    let pane = |s: &mut Scenario| {
        let (width, picture) = s.picture();
        let k = width as f32 / 1200.0;
        let (x0, x1) = ((f32::from(ws.origin.x) * k) as u32, ((f32::from(ws.origin.x) + left - 2.0) * k) as u32);
        let (y0, y1) = ((f32::from(ws.origin.y) * k) as u32, ((f32::from(ws.origin.y + ws.size.height) - 20.0) * k) as u32);
        (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).map(|(x, y)| { let i = ((y * width + x) * 4) as usize; [picture[i], picture[i + 1], picture[i + 2], picture[i + 3]] }).collect::<Vec<_>>()
    };
    let before = pane(&mut s);
    let middle = (f32::from(ws.origin.x + ws.size.width / 2.0), f32::from(ws.origin.y + ws.size.height / 2.0));
    for _ in 0..3 {
        s.wheel(middle, 120.0);
        assert_eq!(s.with(|e, _| e.workspace_scroll.offset().y), px(0.0), "the sheet stays at its top");
        assert!(pane(&mut s) == before, "the frozen columns stay where they were");
    }
    // Scrolled down and back up past the top: the same again.
    s.wheel(middle, -300.0);
    s.wheel(middle, 600.0);
    assert!(pane(&mut s) == before, "the frozen columns come back to the top, no further");
}

/// #2: a cell border is as many of the screen's pixels thick as Excel
/// draws its style, at every zoom (the user's decision 2026-10-07): the
/// thin lines between the rows of the sales table (A3:E9) 1 logical pixel
/// (2 on a Retina screen), the medium line over it 2, zoom by zoom from 50%
/// to 200%.
fn fix2_border_width_steady_with_zoom() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let mut report = Vec::new();
    for step in 0..=26 {
        // (A worksheet's 100% is 72 px an inch: from 70% it is the 52% the
        // lines were measured from.)
        let zoom = 0.7 + step as f32 * 0.05;
        s.update(|e, window, cx| {
            e.set_zoom(Some(zoom), window, cx);
            e.workspace_scroll.set_offset(point(px(0.), px(0.)));
        });
        // The left of column C (numbers, on the right): no text there.
        let (column, rows) = s.with(|e, _| {
            let c = e.cell_center(2, 3).expect("C4");
            let d = e.cell_center(3, 3).expect("D4");
            let ys: Vec<f32> = (1..=8).map(|r| f32::from(e.cell_center(2, r).expect("a cell").y)).collect();
            (f32::from(c.x) - (f32::from(d.x) - f32::from(c.x)) * 0.35, ys)
        });
        let (width, picture) = s.picture();
        let k = width as f32 / 1200.0;
        let x = (column * k) as u32;
        let dark = |y: u32| {
            let i = ((y * width + x) * 4) as usize;
            (picture[i] as u32 + picture[i + 1] as u32 + picture[i + 2] as u32) < 300
        };
        // The dark run between each pair of rows' middles: rows 2|3 (the
        // table's top, medium), then 4|5 … 8|9 (thin).
        let run = |w: &[f32]| ((w[0] * k) as u32..(w[1] * k) as u32).filter(|&y| dark(y)).count() as u32;
        let runs: Vec<u32> = rows.windows(2).map(run).collect();
        let thin = &runs[2..];
        // The medium line on the table's right (column E's right edge),
        // across row 5: the dark pixels from 4 pixels before it to 2 after.
        let (a_left, y5) = s.with(|e, _| {
            let (d, f) = (e.cell_center(3, 4).unwrap(), e.cell_center(5, 4).unwrap());
            // Near the row's top: the text sits at its bottom.
            let next = e.cell_center(3, 5).unwrap();
            ((f32::from(d.x) + f32::from(f.x)) / 2.0 + (f32::from(f.x) - f32::from(d.x)) / 4.0, f32::from(d.y) - (f32::from(next.y) - f32::from(d.y)) / 2.0 + 3.0)
        });
        let y = (y5 * k) as u32;
        // The longest run of dark pixels near there (the line).
        let (mut medium, mut current) = (0u32, 0u32);
        for x in ((a_left - 24.0) * k) as u32..((a_left + 24.0) * k) as u32 {
            let i = ((y * width + x) * 4) as usize;
            if (picture[i] as u32 + picture[i + 1] as u32 + picture[i + 2] as u32) < 300 {
                current += 1;
                medium = medium.max(current);
            } else {
                current = 0;
            }
        }
        report.push(format!("{:.0}%:{medium}/{thin:?}", zoom * 100.0));
        assert!(thin.iter().all(|&r| r as f32 == k), "thin lines 1 pixel at {:.0}%: {thin:?}", zoom * 100.0);
        // (while the line is in the window)
        assert!((a_left + 24.0) * k >= width as f32 - 60.0 * k || medium as f32 == 2.0 * k, "the medium line 2 pixels at {:.0}%: {medium} {runs:?}", zoom * 100.0);
    }
    eprintln!("fix2 lines (zoom: medium/thin, device pixels): {}", report.join(" "));
}

fn dedupe_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.dedupe.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}

/// XLSX-TABLE-016 データ > 重複の削除: a table of names and regions, its
/// current region taken from a cell in it, with a heading row; by all the
/// columns (nothing goes: the third column differs), then by 名前 and 地域
/// (text without regard to case), the first of each kept and the rest
/// moved up; Excel's message; undone, redone, saved and read back.
fn table016_remove_duplicates() {
    let mut s = new_sheet();
    let rows = [["名前", "地域", "番号"], ["佐藤", "東", "1"], ["さとう", "西", "2"], ["佐藤", "東", "3"], ["Kato", "西", "4"], ["kato", "西", "5"]];
    for (row, values) in rows.into_iter().enumerate() {
        for (column, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(column as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    s.click_cell("B3");
    s.menu("xlsx.remove-duplicates");
    let (range, header, columns) = s.with(|e, cx| {
        let p = e.wb().panels.dedupe.as_ref().expect("the panel");
        (p.range.read(cx).value().to_string(), p.header, p.columns.clone())
    });
    assert_eq!((range.as_str(), header, columns), ("A1:C6", true, vec![0, 1, 2]));
    s.screenshot("remove-duplicates-panel");
    dedupe_click(&mut s, "apply");
    let message = |s: &mut Scenario| s.with(|e, _| e.wb().panels.dedupe.as_ref().unwrap().message.clone());
    assert_eq!(message(&mut s), "重複する値は見つかりませんでした。");
    dedupe_click(&mut s, "column-C");
    dedupe_click(&mut s, "apply");
    assert_eq!(message(&mut s), "2 個の重複する値が見つかり、削除されました。3 個の一意の値が残っています。");
    let numbers = |s: &mut Scenario| (2..=6).map(|r| s.value(&format!("C{r}"))).collect::<Vec<_>>();
    let n = |v: f64| CellValue::Number(v);
    assert_eq!(numbers(&mut s), vec![n(1.0), n(2.0), n(4.0), CellValue::Empty, CellValue::Empty]);
    assert_eq!(s.value("A1"), CellValue::Text("名前".into()));
    s.click_cell("E1");
    s.key("cmd-z");
    assert_eq!(numbers(&mut s), vec![n(1.0), n(2.0), n(3.0), n(4.0), n(5.0)]);
    s.key("cmd-shift-z");
    assert_eq!(numbers(&mut s), vec![n(1.0), n(2.0), n(4.0), CellValue::Empty, CellValue::Empty]);
    // No column chosen: refused, nothing changes.
    dedupe_click(&mut s, "none");
    dedupe_click(&mut s, "apply");
    assert_eq!(message(&mut s), "列を 1 つ以上選んでください。");
    s.save_as("table016.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(sheet.cell(3, 2).map(|c| c.value.clone()), Some(n(4.0)));
    assert!(sheet.cell(4, 2).is_none_or(|c| c.value.is_empty()));
    dedupe_click(&mut s, "close");
    assert!(s.with(|e, _| e.wb().panels.dedupe.is_none()));
}

fn text_columns_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.text_columns.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}
fn text_columns_field(s: &mut Scenario, id: &str, text: &str) {
    text_columns_click(s, id);
    s.key("cmd-a");
    s.type_text(text);
}

/// XLSX-TABLE-017 データ > 区切り位置: names, cities and amounts typed
/// with commas split into three columns (the amount a number, a quoted
/// comma kept); split again over them only after being asked; dates split
/// at fixed positions, as text (a leading zero kept); undone and redone,
/// saved and read back.
fn table017_text_to_columns() {
    let mut s = new_sheet();
    for (row, text) in ["佐藤,東京,100", "鈴木,\"大阪,北区\",250", "田中,名古屋,75"].into_iter().enumerate() {
        s.click_cell(&format!("A{}", row + 1));
        s.enter(text, "enter");
    }
    s.click_cell("A1");
    s.key("shift-down");
    s.key("shift-down");
    s.menu("xlsx.text-to-columns");
    text_columns_click(&mut s, "text-columns-tab");
    text_columns_click(&mut s, "text-columns-comma");
    let preview = s.with(|e, _| e.text_columns_source().len());
    assert_eq!(preview, 3);
    s.screenshot("text-to-columns-panel");
    text_columns_click(&mut s, "apply");
    let message = |s: &mut Scenario| s.with(|e, _| e.wb().panels.text_columns.as_ref().unwrap().message.clone());
    assert_eq!(message(&mut s), "3 行を区切りました。");
    assert_eq!(s.value("A2"), CellValue::Text("鈴木".into()));
    assert_eq!(s.value("B2"), CellValue::Text("大阪,北区".into()));
    assert_eq!(s.value("C1"), CellValue::Number(100.0));
    assert_eq!(s.value("C3"), CellValue::Number(75.0));
    // Split again at B1: C holds something, so it asks first.
    text_columns_field(&mut s, "destination", "B1");
    text_columns_click(&mut s, "apply");
    assert!(message(&mut s).starts_with("ここには既にデータがあります"));
    assert_eq!(s.value("C1"), CellValue::Number(100.0));
    text_columns_click(&mut s, "close");
    // Fixed width, as text.
    s.click_cell("A6");
    s.enter("20261007", "enter");
    s.click_cell("A6");
    s.menu("xlsx.text-to-columns");
    text_columns_click(&mut s, "text-columns-fixed");
    text_columns_field(&mut s, "breaks", "4, 6");
    text_columns_click(&mut s, "text-columns-format");
    text_columns_click(&mut s, "apply");
    assert_eq!(s.value("A6"), CellValue::Text("2026".into()));
    assert_eq!(s.value("C6"), CellValue::Text("07".into()));
    s.click_cell("E1");
    s.key("cmd-z");
    assert_eq!(s.value("C6"), CellValue::Empty);
    assert_eq!(s.value("A6"), CellValue::Number(20261007.0));
    s.key("cmd-shift-z");
    assert_eq!(s.value("C6"), CellValue::Text("07".into()));
    s.save_as("table017.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(sheet.cell(1, 1).map(|c| c.value.clone()), Some(CellValue::Text("大阪,北区".into())));
    assert_eq!(sheet.cell(5, 2).map(|c| c.value.clone()), Some(CellValue::Text("07".into())));
}

/// XLSX-CELL-012 形式を選択して貼り付け > 空白セルを無視する: a column
/// with a gap copied onto filled cells: with the setting on, the cell
/// under the gap keeps its value (すべて and 値); off, it is cleared.
fn cell012_paste_skip_blanks() {
    let mut s = new_sheet();
    for (cell, text) in [("A1", "1"), ("A3", "3"), ("C1", "10"), ("C2", "20"), ("C3", "30"), ("E2", "200")] {
        s.click_cell(cell);
        s.enter(text, "enter");
    }
    s.click_cell("A1");
    s.key("shift-down");
    s.key("shift-down");
    s.key("cmd-c");
    assert_eq!(s.menu_state("xlsx.paste-skip-blanks").map(|m| m.1), Some(false));
    s.menu("xlsx.paste-skip-blanks");
    assert_eq!(s.menu_state("xlsx.paste-skip-blanks").map(|m| m.1), Some(true));
    s.click_cell("C1");
    s.menu("xlsx.paste-all");
    let column = |s: &mut Scenario, c: &str| (1..=3).map(|r| s.value(&format!("{c}{r}"))).collect::<Vec<_>>();
    let n = |v: f64| CellValue::Number(v);
    assert_eq!(column(&mut s, "C"), vec![n(1.0), n(20.0), n(3.0)]);
    s.click_cell("E1");
    s.menu("xlsx.paste-values");
    assert_eq!(column(&mut s, "E"), vec![n(1.0), n(200.0), n(3.0)]);
    s.key("cmd-z");
    assert_eq!(column(&mut s, "E"), vec![CellValue::Empty, n(200.0), CellValue::Empty]);
    // Off again: the gap clears the cell under it.
    s.menu("xlsx.paste-skip-blanks");
    s.menu("xlsx.paste-values");
    assert_eq!(column(&mut s, "E"), vec![n(1.0), CellValue::Empty, n(3.0)]);
}

fn goto_special_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.goto_special.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}

/// XLSX-CELL-018 編集 > 条件を選択してジャンプ: in a table with gaps, the
/// blank cells selected (several ranges) and filled with 0 by ⌃Enter; the
/// numbers among the constants selected and made bold together; formulas;
/// the last cell; nothing found; a click elsewhere drops the extra ranges.
fn cell018_goto_special() {
    let mut s = new_sheet();
    for (cell, text) in [("A1", "名前"), ("B1", "数"), ("A2", "a"), ("B2", "1"), ("A3", "b"), ("A4", "c"), ("B4", "3"), ("C2", "=B2*2")] {
        s.click_cell(cell);
        s.enter(text, "enter");
    }
    s.click_cell("A1");
    for _ in 0..3 {
        s.key("shift-down");
    }
    s.key("shift-right");
    s.key("shift-right");
    s.menu("xlsx.goto-special");
    goto_special_click(&mut s, "blanks");
    goto_special_click(&mut s, "apply");
    // C1, B3:C3, C4 (C2 has a formula): three ranges.
    let ranges = s.with(|e, _| e.selected_ranges());
    let r = |a: u32, b: u32, c: u32, d: u32| ibis_doc_model::CellRange { first_row: a, last_row: b, first_column: c, last_column: d };
    assert_eq!(ranges, vec![r(0, 0, 2, 2), r(2, 2, 1, 2), r(3, 3, 2, 2)]);
    s.screenshot("goto-special-blanks");
    s.type_text("0");
    s.key("ctrl-enter");
    for cell in ["C1", "B3", "C3", "C4"] {
        assert_eq!(s.value(cell), CellValue::Number(0.0), "{cell}");
    }
    assert_eq!(s.value("C2"), CellValue::Number(2.0));
    s.key("cmd-z");
    assert_eq!(s.value("B3"), CellValue::Empty);
    // Constants that are numbers, within A1:C4 again: B2, B4.
    s.click_cell("A1");
    for _ in 0..3 {
        s.key("shift-down");
    }
    s.key("shift-right");
    s.key("shift-right");
    goto_special_click(&mut s, "constants");
    goto_special_click(&mut s, "text");
    goto_special_click(&mut s, "apply");
    assert_eq!(s.with(|e, _| e.selected_ranges()), vec![r(1, 1, 1, 1), r(3, 3, 1, 1)]);
    s.key("cmd-b");
    let bold = |s: &mut Scenario, row: u32, column: u32| s.with(move |e, _| e.current_sheet().and_then(|(_, sh)| sh.cell(row, column).map(|c| sh.styles[c.style as usize].bold)).unwrap_or(false));
    assert!(bold(&mut s, 1, 1) && bold(&mut s, 3, 1));
    assert!(!bold(&mut s, 0, 1));
    // Formulas (all types again): C2, from one cell (the used range).
    goto_special_click(&mut s, "text");
    s.click_cell("E5");
    assert_eq!(s.with(|e, _| e.selected_ranges().len()), 1, "a click drops the extra ranges");
    goto_special_click(&mut s, "formulas");
    goto_special_click(&mut s, "apply");
    assert_eq!(s.selected(), "C2");
    goto_special_click(&mut s, "last-cell");
    goto_special_click(&mut s, "apply");
    assert_eq!(s.selected(), "C4");
    goto_special_click(&mut s, "comments");
    goto_special_click(&mut s, "apply");
    assert_eq!(s.with(|e, _| e.wb().panels.goto_special.as_ref().unwrap().message.clone()), "該当するセルが見つかりません。");

    // Row differences within D1:F1: D1 is base (10.0), E1 (10.0), F1 (20.0). F1 is different.
    s.click_cell("D1");
    s.enter("10", "enter");
    s.click_cell("E1");
    s.enter("10", "enter");
    s.click_cell("F1");
    s.enter("20", "enter");
    s.click_cell("D1");
    s.key("shift-right");
    s.key("shift-right");
    goto_special_click(&mut s, "row-differences");
    goto_special_click(&mut s, "apply");
    assert_eq!(s.with(|e, _| e.selected_ranges()), vec![r(0, 0, 5, 5)]);

    // Column differences within D1:D3: D1 is base (10.0), D2 (10.0), D3 (30.0). D3 is different.
    s.click_cell("D2");
    s.enter("10", "enter");
    s.click_cell("D3");
    s.enter("30", "enter");
    s.click_cell("D1");
    s.key("shift-down");
    s.key("shift-down");
    goto_special_click(&mut s, "column-differences");
    goto_special_click(&mut s, "apply");
    assert_eq!(s.with(|e, _| e.selected_ranges()), vec![r(2, 2, 3, 3)]);

    // Precedents of C2 (=B2*2): B2.
    s.click_cell("C2");
    goto_special_click(&mut s, "precedents");
    goto_special_click(&mut s, "apply");
    assert_eq!(s.selected(), "B2");

    // Dependents of B2: C2.
    s.click_cell("B2");
    goto_special_click(&mut s, "dependents");
    goto_special_click(&mut s, "apply");
    assert_eq!(s.selected(), "C2");

    // Objects: when none exist, message is shown.
    goto_special_click(&mut s, "objects");
    goto_special_click(&mut s, "apply");
    assert_eq!(s.with(|e, _| e.wb().panels.goto_special.as_ref().unwrap().message.clone()), "該当するオブジェクトが見つかりません。");

    // Insert a shape, then select objects: shape is selected.
    s.menu("xlsx.shape.rect");
    assert!(s.with(|e, _| e.selected_sheet_shape()).is_some());
    s.click_cell("A1");
    goto_special_click(&mut s, "objects");
    goto_special_click(&mut s, "apply");
    assert!(s.with(|e, _| !e.selected_ids().is_empty()));
}

/// XLSX-SHEET-016 表示 > 分割: split at D6; the wheel over the top pane
/// scrolls it alone (the sheet below stays), over the sheet the sheet
/// alone; a click in the scrolled top pane picks the cell shown there;
/// freezing takes the split away, and 分割 again splits and unsplits.
fn sheet016_split_window() {
    let mut s = new_sheet();
    s.click_cell("D6");
    s.menu("xlsx.split");
    assert_eq!(s.with(|e, _| e.split_panes().map(|p| p.at)), Some((3, 5)));
    assert_eq!(s.menu_state("xlsx.split").map(|m| m.1), Some(true));
    let (ws, split) = s.with(|e, _| (e.workspace_bounds.unwrap(), e.workspace_content().and_then(|c| e.frozen_split(c)).unwrap()));
    let (left, top) = (split.0.unwrap(), split.1.unwrap());
    let in_top = (f32::from(ws.origin.x) + left + 100.0, f32::from(ws.origin.y) + top / 2.0 + 10.0);
    let in_main = (f32::from(ws.origin.x) + left + 100.0, f32::from(ws.origin.y) + top + 100.0);
    // A click in the top pane, before it scrolls.
    s.click(in_top);
    let before = s.selected();
    let row_height = s.with(|e, _| {
        let c = e.cell_center(4, 2).unwrap();
        let d = e.cell_center(4, 3).unwrap();
        f32::from(d.y - c.y)
    });
    s.wheel(in_top, -row_height * 2.0);
    s.update(|_, _, _| {});
    assert_eq!(s.with(|e, _| e.workspace_scroll.offset().y), px(0.0), "the sheet below stays");
    assert!(s.with(|e, _| e.pane_shift().y) < px(0.0), "the top pane scrolled");
    s.click(in_top);
    let after = s.selected();
    let row = |a: &str| a.trim_start_matches(|c: char| c.is_ascii_alphabetic()).parse::<u32>().unwrap();
    assert_eq!(row(&after), row(&before) + 2, "{before} → {after}");
    s.screenshot("split");
    // The wheel over the sheet below scrolls it, not the top pane.
    let shift = s.with(|e, _| e.pane_shift());
    s.wheel(in_main, -200.0);
    s.update(|_, _, _| {});
    assert!(s.with(|e, _| e.workspace_scroll.offset().y) < px(0.0));
    assert_eq!(s.with(|e, _| e.pane_shift()), shift);
    // Freezing replaces it.
    s.click_cell("B2");
    s.menu("xlsx.freeze");
    assert!(s.with(|e, _| e.split_panes().is_none()));
    s.menu("xlsx.split");
    assert!(s.with(|e, _| e.split_panes().is_some() && e.current_sheet().unwrap().1.frozen.is_none()));
    s.menu("xlsx.split");
    assert!(s.with(|e, _| e.split_panes().is_none()));
}

/// XLSX-FORMAT-014 表示形式 > セルのスタイル: 良い on two cells (green text
/// on green, from ECMA-376's presetCellStyles.xml with the theme), 見出し 1
/// on a heading (bold, 15 pt, a thick line under it), 桁区切り (a number
/// format only, the fill kept), 標準 back to plain; each one step; saved
/// and read back.
fn format014_cell_styles() {
    let mut s = new_sheet();
    for (cell, text) in [("A1", "見出し"), ("A2", "1234.5"), ("A3", "2")] {
        s.click_cell(cell);
        s.enter(text, "enter");
    }
    let style = |s: &mut Scenario, row: u32| s.with(move |e, _| e.current_sheet().and_then(|(_, sh)| sh.cell(row, 0).map(|c| sh.styles[c.style as usize].clone())).unwrap_or_default());
    s.click_cell("A2");
    s.key("shift-down");
    s.menu("xlsx.cell-style.good");
    let good = style(&mut s, 2);
    assert_eq!((good.fill, good.color), (Some(ibis_doc_model::Rgb(0xC6EFCE)), ibis_doc_model::Rgb(0x006100)));
    s.menu("xlsx.cell-style.comma");
    let comma = style(&mut s, 1);
    assert_ne!(comma.number_format, "General");
    assert_eq!(comma.fill, Some(ibis_doc_model::Rgb(0xC6EFCE)), "a number style keeps the fill");
    s.key("cmd-z");
    assert_eq!(style(&mut s, 1).number_format, "General", "one step undone");
    s.click_cell("A1");
    s.menu("xlsx.cell-style.heading1");
    let heading = style(&mut s, 0);
    assert!(heading.bold && heading.font_size == 1500, "{heading:?}");
    assert_eq!(heading.bottom.map(|b| b.line), Some(ibis_doc_model::BorderLine::Thick));
    s.click_cell("A3");
    s.menu("xlsx.cell-style.normal");
    let plain = style(&mut s, 2);
    assert_eq!((plain.fill, plain.bold), (None, false));
    s.save_as("format014.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    let saved = |row: u32| sheet.styles[sheet.cell(row, 0).unwrap().style as usize].clone();
    assert_eq!(saved(1).fill, Some(ibis_doc_model::Rgb(0xC6EFCE)));
    assert!(saved(0).bold);
}

fn cell_style_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.new_cell_style.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}

fn cell_style_field(s: &mut Scenario, id: &str, text: &str) {
    cell_style_click(s, id);
    s.key("cmd-a");
    s.type_text(text);
}

/// XLSX-FORMAT-015 セルのスタイル: 新しいセル スタイル… でアクティブ セルの
/// 書式（良い: 薄緑の塗りつぶし・濃緑の文字）を元にカスタム スタイル「強調 1」を
/// 作成、ユーザー設定グループから別のセルに適用、元に戻す/やり直し、
/// 保存して開き直しても保持されることの確認。
fn format015_custom_cell_style() {
    let mut s = new_sheet();
    s.click_cell("A1");
    s.enter("元スタイル", "enter");
    s.click_cell("A1");
    s.menu("xlsx.cell-style.good");
    let good_style = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.cell(0, 0).map(|c| sheet.styles[c.style as usize].clone()).unwrap()
    });
    assert_eq!((good_style.fill, good_style.color), (Some(ibis_doc_model::Rgb(0xC6EFCE)), ibis_doc_model::Rgb(0x006100)));

    // 新しいセル スタイル… を開く
    s.menu("xlsx.new-cell-style");
    s.screenshot("new-cell-style-panel");

    // スタイル名を「強調 1」に変更
    cell_style_field(&mut s, "style-name", "強調 1");

    // 「保護」のチェックボックスをトグルして除外してみる
    cell_style_click(&mut s, "include-protection");

    // OK を押して作成
    cell_style_click(&mut s, "apply");

    // ワークブックにカスタム スタイルが追加されたことを確認
    let custom_styles = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().custom_cell_styles.clone());
    assert_eq!(custom_styles.len(), 1);
    assert_eq!(custom_styles[0].name, "強調 1");
    assert_eq!(custom_styles[0].format.fill, Some(ibis_doc_model::Rgb(0xC6EFCE)));
    assert_eq!(custom_styles[0].format.color, ibis_doc_model::Rgb(0x006100));
    assert_eq!(custom_styles[0].include_protection, false);
    assert_eq!(custom_styles[0].include_fill, true);

    // 元に戻す (AddCustomCellStyle の undo)
    s.click_cell("A1");
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().workbook.as_ref().unwrap().custom_cell_styles.len()), 0);

    // やり直し (AddCustomCellStyle の redo)
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.presentation().workbook.as_ref().unwrap().custom_cell_styles.len()), 1);

    // 別のセル B5 にカスタム スタイルを適用
    s.click_cell("B5");
    s.enter("対象セル", "enter");
    s.click_cell("B5");
    s.menu("xlsx.custom-cell-style.強調 1");

    let b5_style = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.cell(4, 1).map(|c| sheet.styles[c.style as usize].clone()).unwrap()
    });
    assert_eq!(b5_style.fill, Some(ibis_doc_model::Rgb(0xC6EFCE)));
    assert_eq!(b5_style.color, ibis_doc_model::Rgb(0x006100));

    // 書式適用の元に戻す/やり直し
    s.key("cmd-z");
    let b5_undone = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.cell(4, 1).map(|c| sheet.styles[c.style as usize].clone()).unwrap()
    });
    assert_eq!(b5_undone.fill, None);

    s.key("cmd-shift-z");
    let b5_redone = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.cell(4, 1).map(|c| sheet.styles[c.style as usize].clone()).unwrap()
    });
    assert_eq!(b5_redone.fill, Some(ibis_doc_model::Rgb(0xC6EFCE)));

    // 保存して開き直す
    s.save_as("format015.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    let b5_saved = sheet.styles[sheet.cell(4, 1).unwrap().style as usize].clone();
    assert_eq!(b5_saved.fill, Some(ibis_doc_model::Rgb(0xC6EFCE)));
    assert_eq!(b5_saved.color, ibis_doc_model::Rgb(0x006100));

    let wb = p.workbook.as_ref().unwrap();
    assert_eq!(wb.custom_cell_styles.len(), 1);
    assert_eq!(wb.custom_cell_styles[0].name, "強調 1");
    assert_eq!(wb.custom_cell_styles[0].format.fill, Some(ibis_doc_model::Rgb(0xC6EFCE)));
    assert_eq!(wb.custom_cell_styles[0].format.color, ibis_doc_model::Rgb(0x006100));
    assert_eq!(wb.custom_cell_styles[0].include_protection, false);
}

/// XLSX-CF-003 条件付き書式 > セルの強調表示ルール > 日付… (今日、昨日、明日、過去 7 日間など):
/// 選択範囲の日付セルに対して、日付の条件付き書式ルールを適用。
/// 該当セルに濃い赤の文字・薄い赤の背景（light_red）が反映され、
/// 元に戻す/やり直し、および保存して開き直しても type="timePeriod" として保持されることを確認。
fn cf003_date_occurring() {
    let mut s = new_sheet();
    // A1 = TODAY(), A2 = TODAY() - 1 (昨日), A3 = TODAY() + 1 (明日), A4 = TODAY() + 30
    s.click_cell("A1");
    s.enter("=TODAY()", "enter");
    s.click_cell("A2");
    s.enter("=TODAY()-1", "enter");
    s.click_cell("A3");
    s.enter("=TODAY()+1", "enter");
    s.click_cell("A4");
    s.enter("=TODAY()+30", "enter");

    // A1:A4 を選択
    s.click_cell("A1");
    for _ in 0..3 {
        s.key("shift-down");
    }

    // 「今日」のルールを適用
    s.menu("xlsx.cf-date.today");

    // A1 に条件付き書式が適用されていることを確認 (CfKind::TimePeriod today)
    let cfs = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.conditional.clone()
    });
    assert_eq!(cfs.len(), 1);
    assert_eq!(cfs[0].rules.len(), 1);
    if let ibis_doc_model::CfKind::TimePeriod { period, formula } = &cfs[0].rules[0].kind {
        assert_eq!(period, "today");
        assert_eq!(formula, "FLOOR(A1,1)=TODAY()");
    } else {
        panic!("expected TimePeriod rule, got {:?}", cfs[0].rules[0].kind);
    }
    assert_eq!(cfs[0].rules[0].style.as_ref().unwrap().fill, Some(ibis_doc_model::Rgb(0xFFC7CE)));
    assert_eq!(cfs[0].rules[0].style.as_ref().unwrap().color, Some(ibis_doc_model::Rgb(0x9C0006)));

    // 描画オーバーレイ/条件付き書式の反映を確認
    let applied = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        crate::xlsx::conditional::apply(sheet)
    });
    assert!(applied.is_some(), "conditional format applied");
    let (mod_sheet, _) = applied.unwrap();
    // A1 (row 0, col 0) にスタイルが適用され、fill が 0xFFC7CE になっている
    let style_a1 = &mod_sheet.styles[mod_sheet.cell(0, 0).unwrap().style as usize];
    assert_eq!(style_a1.fill, Some(ibis_doc_model::Rgb(0xFFC7CE)));
    // A2 (row 1, col 0) には today のスタイルは適用されない (fill は None)
    let style_a2 = &mod_sheet.styles[mod_sheet.cell(1, 0).unwrap().style as usize];
    assert_eq!(style_a2.fill, None);

    // 元に戻す (undo)
    s.click_cell("A1");
    s.key("cmd-z");
    let cfs_undone = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.conditional.clone()
    });
    assert_eq!(cfs_undone.len(), 0);

    // やり直し (redo)
    s.key("cmd-shift-z");
    let cfs_redone = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.conditional.clone()
    });
    assert_eq!(cfs_redone.len(), 1);

    // 「昨日」のルールも追加
    s.click_cell("A1");
    for _ in 0..3 {
        s.key("shift-down");
    }
    s.menu("xlsx.cf-date.yesterday");

    let cfs_both = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.conditional.clone()
    });
    assert_eq!(cfs_both.len(), 2);

    s.screenshot("cf-date-occurring");

    // 保存して開き直し
    s.save_as("cf003_date.xlsx");
    let saved = s.reopen();
    let saved_sheet = sheet_of_slide(&saved.slides[1]).unwrap();
    assert_eq!(saved_sheet.conditional.len(), 2);
    let today_rule = saved_sheet.conditional.iter().find_map(|cf| {
        cf.rules.iter().find(|r| matches!(&r.kind, ibis_doc_model::CfKind::TimePeriod { period, .. } if period == "today"))
    }).expect("today rule persisted");
    assert_eq!(today_rule.style.as_ref().unwrap().fill, Some(ibis_doc_model::Rgb(0xFFC7CE)));

    let yesterday_rule = saved_sheet.conditional.iter().find_map(|cf| {
        cf.rules.iter().find(|r| matches!(&r.kind, ibis_doc_model::CfKind::TimePeriod { period, .. } if period == "yesterday"))
    }).expect("yesterday rule persisted");
    assert_eq!(yesterday_rule.style.as_ref().unwrap().fill, Some(ibis_doc_model::Rgb(0xFFC7CE)));
}

/// XLSX-CELL-016 データ > フラッシュ フィル (⌃E): beside a table of full
/// names and mail addresses with a heading row, the first name typed once;
/// ⌃E fills the rest (one step, undone); another column, "LAST, F."
/// from two columns; nothing to learn from: the status says so.
fn cell016_flash_fill() {
    let mut s = new_sheet();
    let rows = [["名前", "メール", "名"], ["Nancy Davolio", "nancy@example.com", "Nancy"], ["Andrew Fuller", "andrew@example.com", ""], ["Janet Leverling", "janet@example.com", ""]];
    for (r, values) in rows.iter().enumerate() {
        for (c, v) in values.iter().enumerate() {
            if !v.is_empty() {
                s.click_cell(&ibis_doc_model::cell_name(c as u32, r as u32));
                s.enter(v, "enter");
            }
        }
    }
    s.click_cell("C3");
    s.key("ctrl-e");
    assert_eq!(s.value("C3"), CellValue::Text("Andrew".into()));
    assert_eq!(s.value("C4"), CellValue::Text("Janet".into()));
    assert!(s.status_texts().iter().any(|t| t.contains("2 個のセル")), "{:?}", s.status_texts());
    s.key("cmd-z");
    assert_eq!(s.value("C4"), CellValue::Empty);
    s.key("cmd-shift-z");
    // "DAVOLIO, N." in D2: the other rows follow (by the menu).
    s.click_cell("D2");
    s.enter("DAVOLIO, N.", "enter");
    s.click_cell("D3");
    s.menu("xlsx.flash-fill");
    assert_eq!(s.value("D3"), CellValue::Text("FULLER, A.".into()));
    assert_eq!(s.value("D4"), CellValue::Text("LEVERLING, J.".into()));
    // A column with no example: nothing to learn.
    s.click_cell("F3");
    s.key("ctrl-e");
    assert_eq!(s.value("F3"), CellValue::Empty);
}

fn validation_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().validation.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}
fn validation_field(s: &mut Scenario, id: &str, text: &str) {
    validation_click(s, id);
    s.key("cmd-a");
    s.type_text(text);
}

/// XLSX-VALID-001・011 データの入力規則: whole numbers between 1 and 10
/// with a stop message of its own (11 refused, its message shown; 5 kept);
/// a text length of at most 3 as a warning (kept, warned); a list from a
/// range; read back into the panel; saved and read again; cleared.
fn valid001_rules_and_messages() {
    let mut s = new_sheet();
    for (cell, text) in [("E1", "東"), ("E2", "西"), ("E3", "南")] {
        s.click_cell(cell);
        s.enter(text, "enter");
    }
    s.click_cell("A1");
    s.key("shift-down");
    s.menu("xlsx.validation");
    s.update(|e, _, cx| {
        let p = e.wb_mut().validation.as_mut().unwrap();
        p.kind = "whole";
        p.operator = "between";
        cx.notify();
    });
    validation_field(&mut s, "items", "1");
    validation_field(&mut s, "second", "10");
    validation_click(&mut s, "tab-2");
    validation_field(&mut s, "error-title", "数");
    validation_field(&mut s, "error-text", "1 から 10 の整数");
    s.screenshot("validation-panel");
    validation_click(&mut s, "apply");
    s.click_cell("A1");
    s.enter("11", "enter");
    assert_eq!(s.value("A1"), CellValue::Empty, "refused");
    assert!(s.status_texts().iter().any(|t| t.contains("1 から 10 の整数")), "{:?}", s.status_texts());
    s.key("escape");
    s.click_cell("A2");
    s.enter("5", "enter");
    assert_eq!(s.value("A2"), CellValue::Number(5.0));
    // Text of at most 3 characters, a warning: kept, with the warning.
    s.click_cell("B1");
    s.menu("xlsx.validation");
    s.update(|e, _, cx| {
        let p = e.wb_mut().validation.as_mut().unwrap();
        p.kind = "textLength";
        p.operator = "lessThanOrEqual";
        p.style = "warning";
        cx.notify();
    });
    validation_field(&mut s, "items", "3");
    validation_click(&mut s, "apply");
    s.click_cell("B1");
    s.enter("abcd", "enter");
    assert_eq!(s.value("B1"), CellValue::Text("abcd".into()), "a warning keeps it");
    // A list from a range.
    s.click_cell("C1");
    s.menu("xlsx.validation");
    s.update(|e, _, cx| {
        e.wb_mut().validation.as_mut().unwrap().kind = "list";
        cx.notify();
    });
    validation_field(&mut s, "items", "=E1:E3");
    validation_click(&mut s, "apply");
    s.click_cell("C1");
    s.enter("北", "enter");
    assert_eq!(s.value("C1"), CellValue::Empty);
    s.key("escape");
    s.click_cell("C1");
    s.enter("西", "enter");
    assert_eq!(s.value("C1"), CellValue::Text("西".into()));
    // Dates within 2026; a custom formula (even numbers) on F1:F3.
    s.click_cell("D1");
    s.menu("xlsx.validation");
    s.update(|e, _, cx| {
        let p = e.wb_mut().validation.as_mut().unwrap();
        p.kind = "date";
        p.operator = "between";
        cx.notify();
    });
    validation_field(&mut s, "items", "2026/1/1");
    validation_field(&mut s, "second", "2026/12/31");
    validation_click(&mut s, "apply");
    s.click_cell("D1");
    s.enter("2025/5/1", "enter");
    assert_eq!(s.value("D1"), CellValue::Empty, "a date out of 2026 refused");
    s.key("escape");
    s.click_cell("D1");
    s.enter("2026/5/1", "enter");
    assert!(matches!(s.value("D1"), CellValue::Number(_)));
    s.click_cell("F1");
    s.key("shift-down");
    s.key("shift-down");
    s.menu("xlsx.validation");
    s.update(|e, _, cx| {
        e.wb_mut().validation.as_mut().unwrap().kind = "custom";
        cx.notify();
    });
    validation_field(&mut s, "items", "=MOD(F1,2)=0");
    validation_click(&mut s, "apply");
    s.click_cell("F3");
    s.enter("3", "enter");
    assert_eq!(s.value("F3"), CellValue::Empty, "odd refused (the formula moved to F3)");
    s.key("escape");
    s.click_cell("F3");
    s.enter("4", "enter");
    assert_eq!(s.value("F3"), CellValue::Number(4.0));
    // The panel shows A1's rule again.
    s.click_cell("A1");
    s.menu("xlsx.validation");
    let (kind, first) = s.with(|e, cx| {
        let p = e.wb().validation.as_ref().unwrap();
        (p.kind, p.items.read(cx).value().to_string())
    });
    assert_eq!((kind, first.as_str()), ("whole", "1"));
    s.save_as("valid001.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    let kinds: Vec<&str> = sheet.validations.iter().map(|v| v.kind.as_str()).collect();
    assert!(kinds.contains(&"whole") && kinds.contains(&"textLength") && kinds.contains(&"list"), "{kinds:?}");
    let whole = sheet.validations.iter().find(|v| v.kind == "whole").unwrap();
    assert_eq!((whole.formula1.as_deref(), whole.formula2.as_deref()), (Some("1"), Some("10")));
    assert_eq!(whole.error.as_ref().map(|e| e.2.as_str()), Some("1 から 10 の整数"));
    // すべてクリア on A1:A2.
    s.key("shift-down");
    validation_click(&mut s, "clear");
    assert!(s.with(|e, _| e.current_rule().is_none()));
}

/// XLSX-VALID-010 入力規則のコピーと形式を選択して貼り付け:
/// - A1 に整数 (1..=10) の入力規則を設定
/// - A1 をコピーして B1 に通常貼り付け -> B1 にも規則がコピーされ、15 の入力が拒否、5 の入力が許可される
/// - C1 に既存値 "元の値" を入力後、A1 をコピーして C1 に「形式を選択して貼り付け > 入力規則」
///   -> C1 の値 "元の値" が維持され、入力規則のみ適用（15 拒否、8 許可）
/// - A1 を切り取り（カット）して D1 に貼り付け -> A1 の規則が消え 50 が入る、D1 には規則が適用される
/// - 保存・開き直しラウンドトリップでコピーされた入力規則が保持されていることを確認
fn valid010_copy_and_paste_validation() {
    let mut s = new_sheet();
    s.click_cell("A1");
    s.menu("xlsx.validation");
    s.update(|e, _, cx| {
        let p = e.wb_mut().validation.as_mut().unwrap();
        p.kind = "whole";
        p.operator = "between";
        cx.notify();
    });
    validation_field(&mut s, "items", "1");
    validation_field(&mut s, "second", "10");
    validation_click(&mut s, "tab-2");
    validation_field(&mut s, "error-text", "1 から 10 の整数");
    validation_click(&mut s, "apply");

    // A1 入力規則の確認
    s.click_cell("A1");
    s.enter("15", "enter");
    assert_eq!(s.value("A1"), CellValue::Empty, "A1 refuses 15");
    s.key("escape");

    // 1. 通常貼り付けによる入力規則のコピー: A1 をコピーして B1 へ
    s.click_cell("A1");
    s.menu("edit.copy");
    s.click_cell("B1");
    s.menu("edit.paste");

    // B1 にも同じ規則が適用されている
    s.click_cell("B1");
    s.enter("15", "enter");
    assert_eq!(s.value("B1"), CellValue::Empty, "B1 refuses 15");
    s.key("escape");
    s.click_cell("B1");
    s.enter("5", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(5.0), "B1 accepts 5");

    // 2. 形式を選択して貼り付け > 入力規則: C1 に値を設定後、A1 をコピーして C1 へ
    s.click_cell("C1");
    s.enter("元の値", "enter");
    assert_eq!(s.value("C1"), CellValue::Text("元の値".into()));

    s.click_cell("A1");
    s.menu("edit.copy");
    s.click_cell("C1");
    s.menu("xlsx.paste-validation");
    // 値は "元の値" のまま維持されている
    assert_eq!(s.value("C1"), CellValue::Text("元の値".into()), "value preserved");

    // C1 に新たな値を入れると入力規則で検査される
    s.click_cell("C1");
    s.enter("20", "enter");
    assert_eq!(s.value("C1"), CellValue::Text("元の値".into()), "C1 refuses 20 and keeps original");
    s.key("escape");
    s.click_cell("C1");
    s.enter("8", "enter");
    assert_eq!(s.value("C1"), CellValue::Number(8.0), "C1 accepts 8");

    // 3. 切り取り（カット）と貼り付け: A1 をカットして D1 へ
    s.click_cell("A1");
    s.menu("edit.cut");
    s.click_cell("D1");
    s.menu("edit.paste");

    // A1 の入力規則は消えているので 50 が入る
    s.click_cell("A1");
    s.enter("50", "enter");
    assert_eq!(s.value("A1"), CellValue::Number(50.0), "A1 no longer restricted");

    // D1 には入力規則が移動している
    s.click_cell("D1");
    s.enter("50", "enter");
    assert_eq!(s.value("D1"), CellValue::Empty, "D1 refuses 50");
    s.key("escape");
    s.click_cell("D1");
    s.enter("3", "enter");
    assert_eq!(s.value("D1"), CellValue::Number(3.0), "D1 accepts 3");

    // 4. 保存・開き直しラウンドトリップ検証
    s.save_as("valid010.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    assert!(!sheet.validations.is_empty(), "validations saved and reloaded");
    let whole_ranges: Vec<CellRange> = sheet.validations.iter().filter(|v| v.kind == "whole").flat_map(|v| v.ranges.clone()).collect();
    assert!(whole_ranges.iter().any(|r| r.first_row == 0 && r.first_column == 1), "B1 has rule");
    assert!(whole_ranges.iter().any(|r| r.first_row == 0 && r.first_column == 2), "C1 has rule");
    assert!(whole_ranges.iter().any(|r| r.first_row == 0 && r.first_column == 3), "D1 has rule");
}

/// XLSX-VALID-012 貼付・再計算で規則を回避する場合の評価:
/// - 直接手動入力では入力規則（1〜10の整数）による検証で不正値（99）が拒否される
/// - 別セルからの貼り付け（形式を選択して値貼り付けなど）では規則が回避され、不正値がセルに設定される
/// - 数式の再計算（=C1*10）でも規則が回避され、計算結果（50）がセルに設定される
/// - 規則を回避して設定された無効データは「無効データのマーク表示」（xlsx.validation.circle-invalid）で評価・赤丸マークされる
/// - 「マークのクリア」（xlsx.validation.clear-circles）で消去される
/// - 再計算で適合値（10）に戻ると無効セル一覧から自動で除外される
/// - 元に戻す（Undo）で再計算値が再び不適合になると無効セルに復帰する
/// - 保存・開き直しラウンドトリップでデータ整合性が保持される
fn valid012_paste_and_recalc_bypass() {
    let mut s = new_sheet();

    // 1. A1 に「1から10の整数」の入力規則を設定
    s.click_cell("A1");
    s.menu("xlsx.validation");
    s.update(|e, _, cx| {
        let p = e.wb_mut().validation.as_mut().unwrap();
        p.kind = "whole";
        p.operator = "between";
        cx.notify();
    });
    validation_field(&mut s, "items", "1");
    validation_field(&mut s, "second", "10");
    validation_click(&mut s, "tab-2");
    validation_field(&mut s, "error-text", "1 から 10 の整数");
    validation_click(&mut s, "apply");

    // 2. 直接手動入力時の評価: 不正値 99 は拒否される
    s.click_cell("A1");
    s.enter("99", "enter");
    assert_eq!(s.value("A1"), CellValue::Empty, "A1 directly refuses 99");
    assert!(s.status_texts().iter().any(|t| t.contains("1 から 10 の整数")), "{:?}", s.status_texts());
    s.key("escape");

    // 正常値 5 は受け入れられる
    s.click_cell("A1");
    s.enter("5", "enter");
    assert_eq!(s.value("A1"), CellValue::Number(5.0), "A1 accepts 5");

    // 3. 貼り付けによる規則回避:
    // B1 に 99 を用意し、B1 をコピーして A1 に形式を選択して値貼り付け（xlsx.paste-values）
    s.click_cell("B1");
    s.enter("99", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(99.0));

    s.click_cell("B1");
    s.menu("edit.copy");
    s.click_cell("A1");
    s.menu("xlsx.paste-values");
    // 入力規則チェックを回避して不正値 99 が A1 に設定される（貼付による規則回避）
    assert_eq!(s.value("A1"), CellValue::Number(99.0), "paste-values bypasses validation rule");

    // 4. 再計算による規則回避:
    // A2 にも入力規則（1..10 の整数）を設定
    s.click_cell("A2");
    s.menu("xlsx.validation");
    s.update(|e, _, cx| {
        let p = e.wb_mut().validation.as_mut().unwrap();
        p.kind = "whole";
        p.operator = "between";
        cx.notify();
    });
    validation_field(&mut s, "items", "1");
    validation_field(&mut s, "second", "10");
    validation_click(&mut s, "apply");

    // C1 に 1 を入力
    s.click_cell("C1");
    s.enter("1", "enter");
    assert_eq!(s.value("C1"), CellValue::Number(1.0));

    // A2 に数式 =C1*10 を入力（この時点では 1*10 = 10 で規則 1..10 に適合）
    s.click_cell("A2");
    s.enter("=C1*10", "enter");
    assert_eq!(s.value("A2"), CellValue::Number(10.0), "A2 initial recalc is 10 (valid)");

    // C1 を 5 に変更 -> A2 は 50 に再計算される。再計算は入力規則に阻害されず反映される（再計算による規則回避）
    s.click_cell("C1");
    s.enter("5", "enter");
    assert_eq!(s.value("C1"), CellValue::Number(5.0));
    assert_eq!(s.value("A2"), CellValue::Number(50.0), "recalculation bypasses validation rule (50)");

    // 5. 回避された無効データのマーク評価（Circle Invalid Data）
    // 現時点で無効データとなっているセルを検査:
    // A1 (col 0, row 0): 99 (規則 1..10 違反)
    // A2 (col 0, row 1): 50 (規則 1..10 違反)
    let invalid = s.with(|e, _| e.invalid_cells());
    assert!(invalid.contains(&(0, 0)), "A1 is invalid: {:?}", invalid);
    assert!(invalid.contains(&(0, 1)), "A2 is invalid: {:?}", invalid);

    // 初期状態ではマークフラグは false
    assert!(!s.with(|e, _| e.validation_circles()));
    assert!(s.with(|e, _| e.render_validation_circles(1.0)).is_none());

    // メニュー「データ > 無効データのマーク表示」（xlsx.validation.circle-invalid）を実行
    s.menu("xlsx.validation.circle-invalid");
    assert!(s.with(|e, _| e.validation_circles()));
    assert!(s.with(|e, _| e.render_validation_circles(1.0)).is_some());

    // メニュー「データ > マークのクリア」（xlsx.validation.clear-circles）を実行
    s.menu("xlsx.validation.clear-circles");
    assert!(!s.with(|e, _| e.validation_circles()));
    assert!(s.with(|e, _| e.render_validation_circles(1.0)).is_none());

    // 6. 再計算で適合値に戻った場合の挙動検証:
    // C1 を 1 に戻す -> A2 は 10（適合値）に再計算される
    s.click_cell("C1");
    s.enter("1", "enter");
    assert_eq!(s.value("A2"), CellValue::Number(10.0));
    let invalid2 = s.with(|e, _| e.invalid_cells());
    assert!(invalid2.contains(&(0, 0)), "A1 still invalid");
    assert!(!invalid2.contains(&(0, 1)), "A2 is now valid (10), so no longer marked invalid: {:?}", invalid2);

    // 7. 元に戻す・やり直し（Undo）検証:
    // C1 の入力を元に戻すと A2 は再び 50（不適合）に戻る
    s.key("cmd-z");
    assert_eq!(s.value("A2"), CellValue::Number(50.0));
    assert!(s.with(|e, _| e.invalid_cells()).contains(&(0, 1)), "A2 invalid again after undo");

    // 8. 保存と再オープン検証:
    s.save_as("valid012_bypass.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(sheet.cell(0, 0).map(|c| &c.value), Some(&CellValue::Number(99.0)));
    assert_eq!(sheet.cell(1, 0).map(|c| &c.value), Some(&CellValue::Number(50.0)));
    assert!(!sheet.validations.is_empty(), "validations saved and reloaded");
}

fn note_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.note.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}

/// XLSX-SEC-009・CELL-013 メモ: 挿入 > 新しいメモ on B2 (typed, kept by the
/// user's name), 編集 then shows it and changes it; a second one on C3;
/// コメントとメモのクリア over B2:C3 takes both off (one step, undone);
/// saved and read back.
fn sec009_notes() {
    let mut s = new_sheet();
    s.click_cell("B2");
    assert!(s.menu_state("xlsx.note-new").is_some_and(|m| m.0));
    s.menu("xlsx.note-new");
    note_click(&mut s, "text");
    s.type_text("Check this");
    note_click(&mut s, "save");
    let note = |s: &mut Scenario, r: u32, c: u32| s.with(move |e, _| e.current_sheet().and_then(|(_, sh)| sh.comment_at(r, c).map(|n| (n.author.clone(), n.text.clone()))));
    let (author, body) = note(&mut s, 1, 1).expect("a note on B2");
    assert_eq!(body, "Check this");
    assert!(!author.is_empty());
    // 編集: the note shown, changed.
    assert!(s.menu_state("xlsx.note-edit").is_some());
    s.menu("xlsx.note-edit");
    assert_eq!(s.with(|e, cx| e.wb().panels.note.as_ref().unwrap().state.read(cx).value().to_string()), "Check this");
    note_click(&mut s, "text");
    s.key("cmd-a");
    s.type_text("Checked");
    note_click(&mut s, "save");
    assert_eq!(note(&mut s, 1, 1).map(|n| n.1).as_deref(), Some("Checked"));
    s.click_cell("C3");
    s.menu("xlsx.note-new");
    note_click(&mut s, "text");
    s.type_text("Second");
    note_click(&mut s, "save");
    let saved = s.save_as("sec009.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(sheet.comment_at(1, 1).map(|n| n.text.as_str()), Some("Checked"));
    assert_eq!(sheet.comment_at(2, 2).map(|n| n.text.as_str()), Some("Second"));
    // What Excel needs to show them: each note's VML shape (its row and
    // column), the sheet's legacyDrawing, its relationship, the content type.
    let part = |name: &str| String::from_utf8(std::process::Command::new("unzip").arg("-p").arg(&saved).arg(name).output().unwrap().stdout).unwrap_or_default();
    let listing = String::from_utf8(std::process::Command::new("unzip").arg("-l").arg(&saved).output().unwrap().stdout).unwrap();
    let vml_name = listing.split_whitespace().find(|w| w.ends_with(".vml")).expect("a VML drawing").to_string();
    let vml = part(&vml_name);
    assert_eq!(vml.matches("ObjectType=\"Note\"").count(), 2, "{vml}");
    assert!(vml.contains("<x:Row>1</x:Row>") && vml.contains("<x:Column>1</x:Column>") && vml.contains("<x:Row>2</x:Row>"));
    let sheet_name = listing.split_whitespace().find(|w| w.starts_with("xl/worksheets/sheet") && w.ends_with(".xml") && part(w).contains("legacyDrawing")).expect("a sheet with legacyDrawing").to_string();
    let rels = part(&sheet_name.replace("worksheets/", "worksheets/_rels/").replace(".xml", ".xml.rels"));
    assert!(rels.contains("relationships/vmlDrawing"), "{rels}");
    assert!(part("\\[Content_Types\\].xml").contains("vmlDrawing"));
    // Cleared over B2:C3, in one step.
    s.click_cell("B2");
    s.key("shift-down");
    s.key("shift-right");
    s.menu("xlsx.clear-comments");
    assert!(note(&mut s, 1, 1).is_none() && note(&mut s, 2, 2).is_none());
    s.key("cmd-z");
    assert!(note(&mut s, 1, 1).is_some() && note(&mut s, 2, 2).is_some());
    // A third note on the file read back: its drawing kept (the first two
    // shapes as they were), one shape added.
    let first = vml.split("<v:shape ").nth(1).map(|x| x.split('"').nth(1).unwrap_or("").to_string()).unwrap_or_default();
    s.update(|e, window, cx| e.go_to_reference("D4", window, cx));
    s.menu("xlsx.note-new");
    note_click(&mut s, "text");
    s.type_text("Third");
    note_click(&mut s, "save");
    let saved3 = s.save_as("sec009-three.xlsx");
    let vml3 = String::from_utf8(std::process::Command::new("unzip").arg("-p").arg(&saved3).arg(&vml_name).output().unwrap().stdout).unwrap();
    assert_eq!(vml3.matches("ObjectType=\"Note\"").count(), 3, "{vml3}");
    assert!(vml3.contains(&format!("id=\"{first}\"")) && vml3.contains("<x:Row>3</x:Row>"), "{first}");
    s.update(|e, window, cx| e.go_to_reference("D4", window, cx));
    s.menu("xlsx.note-delete");
    // One taken off and saved: one shape left; none: the drawing goes.
    // (Selected by the name box: a note's card may cover the cells beside it.)
    s.update(|e, window, cx| e.go_to_reference("C3", window, cx));
    s.menu("xlsx.note-delete");
    assert!(note(&mut s, 1, 1).is_some() && note(&mut s, 2, 2).is_none());
    let saved = s.save_as("sec009-one.xlsx");
    let vml = String::from_utf8(std::process::Command::new("unzip").arg("-p").arg(&saved).arg(&vml_name).output().unwrap().stdout).unwrap();
    assert_eq!(vml.matches("ObjectType=\"Note\"").count(), 1);
    s.update(|e, window, cx| e.go_to_reference("B2", window, cx));
    s.menu("xlsx.note-delete");
    let saved = s.save_as("sec009-none.xlsx");
    let listing = String::from_utf8(std::process::Command::new("unzip").arg("-l").arg(&saved).output().unwrap().stdout).unwrap();
    assert!(!listing.contains(".vml"), "{listing}");
    let sheet = String::from_utf8(std::process::Command::new("unzip").arg("-p").arg(&saved).arg(&sheet_name).output().unwrap().stdout).unwrap();
    assert!(!sheet.contains("legacyDrawing"));
}

/// A sheet Excel protected (password pw1: its SHA-512 hash) opened, and
/// unprotected from 表示 > シート保護の解除… with that password (a wrong
/// one first).
fn sec002_unprotect_excel_file() {
    let mut s = Scenario::open("test/editing/xlsx-protected-by-excel.xlsx");
    assert!(s.with(|e, _| e.sheet_protection().is_some_and(|p| p.hash.is_some())));
    s.click_cell("A1");
    s.type_text("9");
    s.key("enter");
    assert!(s.status_texts().iter().any(|t| t.contains("保護されているシート")));
    s.key("escape");
    s.menu("xlsx.unprotect-sheet");
    protect_field(&mut s, "password", "pw2");
    protect_click(&mut s, "apply");
    assert!(s.with(|e, _| e.sheet_protection().is_some()));
    protect_field(&mut s, "password", "pw1");
    protect_click(&mut s, "apply");
    assert!(s.with(|e, _| e.sheet_protection().is_none()));
    s.click_cell("A1");
    s.enter("9", "enter");
    assert_eq!(s.value("A1"), CellValue::Number(9.0));
}

fn protect_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.protect.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}
fn protect_field(s: &mut Scenario, id: &str, text: &str) {
    protect_click(s, id);
    s.key("cmd-a");
    s.type_text(text);
}

/// XLSX-SEC-001・002・003 シートの保護: B1 unlocked, C1's formula hidden;
/// protected with a password (asked twice): a locked cell refuses typing,
/// Delete and bold (Excel's message), the unlocked one takes a value, rows
/// cannot be inserted, C1's formula is not shown; saved with the
/// password's SHA-512 hash only; a wrong password does not unprotect, the
/// right one does.
fn sec002_protect_sheet() {
    let mut s = new_sheet();
    s.click_cell("A1");
    s.enter("10", "enter");
    s.click_cell("C1");
    s.enter("=A1*2", "enter");
    s.click_cell("B1");
    s.menu("xlsx.lock-cells");
    assert_eq!(s.menu_state("xlsx.lock-cells").map(|m| m.1), Some(false), "B1 unlocked");
    s.click_cell("C1");
    s.menu("xlsx.hide-formulas");
    s.menu("xlsx.protect-sheet");
    protect_field(&mut s, "password", "pw1");
    protect_field(&mut s, "confirm", "pw2");
    protect_click(&mut s, "apply");
    assert!(s.with(|e, _| e.sheet_protection().is_none()), "the passwords differ");
    protect_field(&mut s, "confirm", "pw1");
    protect_click(&mut s, "apply");
    assert!(s.with(|e, _| e.sheet_protection().is_some()));
    // A locked cell: typing, Delete and bold refused.
    s.click_cell("A1");
    s.type_text("5");
    s.key("enter");
    assert_eq!(s.value("A1"), CellValue::Number(10.0));
    assert!(s.status_texts().iter().any(|t| t.contains("保護されているシート")), "{:?}", s.status_texts());
    s.click_cell("A1");
    s.key("delete");
    assert_eq!(s.value("A1"), CellValue::Number(10.0));
    s.key("cmd-b");
    assert!(!s.with(|e, _| e.current_style().bold));
    s.menu("xlsx.insert-row-above");
    assert_eq!(s.value("A1"), CellValue::Number(10.0), "no row inserted");
    // The unlocked cell takes a value.
    s.click_cell("B1");
    s.enter("7", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(7.0));
    // C1's formula hidden from the bar.
    s.click_cell("C1");
    assert!(s.with(|e, _| e.formula_hidden_here()));
    s.save_as("sec002.xlsx");
    let saved = std::fs::read(crate::shell::scenario::repo_root().join("target/scenario/xlsx::sec002_protect_sheet/sec002.xlsx")).unwrap();
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    let protection = sheet.protection.clone().expect("protected when read back");
    let hash = protection.hash.clone().expect("a SHA-512 hash");
    assert_eq!((hash.algorithm.as_str(), hash.spin_count), ("SHA-512", 100_000));
    assert!(super::password::opens(&protection, "pw1"));
    assert!(!saved.windows(3).any(|w| w == b"pw1"), "the password itself is not in the file");
    assert!(sheet.cell(0, 1).is_some_and(|c| sheet.styles[c.style as usize].unlocked));
    // Unprotecting: a wrong password, then the right one.
    s.menu("xlsx.unprotect-sheet");
    protect_field(&mut s, "password", "nope");
    protect_click(&mut s, "apply");
    assert!(s.with(|e, _| e.sheet_protection().is_some()));
    protect_field(&mut s, "password", "pw1");
    protect_click(&mut s, "apply");
    assert!(s.with(|e, _| e.sheet_protection().is_none()));
    s.click_cell("A1");
    s.enter("5", "enter");
    assert_eq!(s.value("A1"), CellValue::Number(5.0));
}

fn protect_wb_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.protect_workbook.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}
fn protect_wb_field(s: &mut Scenario, id: &str, text: &str) {
    protect_wb_click(s, id);
    s.key("cmd-a");
    s.type_text(text);
}

/// XLSX-SEC-005 ブック構成の保護:
/// protected with a password (asked twice): sheet insertion, deletion, rename,
/// move, hide are refused (Excel's message and disabled menus), cell editing
/// is still permitted; saved with the password's SHA-512 hash;
/// round-trip verify; wrong password does not unprotect, right password does.
fn sec005_protect_workbook() {
    let mut s = new_sheet();
    assert!(!s.with(|e, _| e.workbook_structure_locked()));
    s.menu("xlsx.protect-workbook");
    protect_wb_field(&mut s, "password", "pw1");
    protect_wb_field(&mut s, "confirm", "pw2");
    protect_wb_click(&mut s, "apply");
    assert!(!s.with(|e, _| e.workbook_structure_locked()), "passwords differ");

    protect_wb_field(&mut s, "confirm", "pw1");
    protect_wb_click(&mut s, "apply");
    assert!(s.with(|e, _| e.workbook_structure_locked()), "workbook structure locked");

    // Menus should be disabled
    assert_eq!(s.menu_state("xlsx.insert-sheet").map(|m| m.0), Some(false), "insert-sheet disabled");
    assert_eq!(s.menu_state("xlsx.delete-sheet").map(|m| m.0), Some(false), "delete-sheet disabled");

    // Structural operations are refused (e.g. from tab buttons or shortcuts)
    let sheet_count = s.with(|e, _| e.presentation().slides.len());
    s.update(|e, window, cx| e.insert_sheet(window, cx));
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), sheet_count, "no new sheet");
    assert!(s.status_texts().iter().any(|t| t.contains("ブックは保護されているため")));

    // Cell editing is still permitted
    s.click_cell("A1");
    s.enter("123", "enter");
    assert_eq!(s.value("A1"), CellValue::Number(123.0));

    // Save and reload roundtrip
    s.save_as("sec005.xlsx");
    let saved = std::fs::read(crate::shell::scenario::repo_root().join("target/scenario/xlsx::sec005_protect_workbook/sec005.xlsx")).unwrap();
    let p = s.reopen();
    let wb = p.workbook.as_ref().expect("workbook");
    let protection = wb.protection.as_ref().expect("protection roundtrip");
    assert!(protection.lock_structure);
    let hash = protection.hash.as_ref().expect("SHA-512 hash");
    assert_eq!((hash.algorithm.as_str(), hash.spin_count), ("SHA-512", 100_000));
    assert!(super::password::opens_workbook(protection, "pw1"));
    assert!(!saved.windows(3).any(|w| w == b"pw1"), "password cleartext not in file");

    // Unprotect: wrong password then right password
    s.menu("xlsx.unprotect-workbook");
    protect_wb_field(&mut s, "password", "wrong");
    protect_wb_click(&mut s, "apply");
    assert!(s.with(|e, _| e.workbook_structure_locked()), "still locked with wrong password");

    protect_wb_field(&mut s, "password", "pw1");
    protect_wb_click(&mut s, "apply");
    assert!(!s.with(|e, _| e.workbook_structure_locked()), "unprotected");

    // Now sheet insertion works
    s.menu("xlsx.insert-sheet");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), sheet_count + 1, "sheet inserted after unprotect");
}

fn allow_ranges_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.allow_edit_ranges.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}

fn allow_ranges_field(s: &mut Scenario, id: &str, text: &str) {
    let input = s.with(|e, _| {
        let p = e.wb().panels.allow_edit_ranges.as_ref().expect("the allow_edit_ranges panel");
        match id {
            "title" => p.title.clone(),
            "sqref" => p.sqref.clone(),
            "password" => p.password.clone(),
            _ => panic!("unknown allow_ranges field {id}"),
        }
    });
    s.update(|_, window, cx| input.update(cx, |i, cx| i.set_value(text.to_string(), window, cx)));
}

/// XLSX-SEC-004 範囲の編集の許可:
/// Allow Users to Edit Ranges when worksheet is protected.
/// Ranges with or without password can be configured, saved with SHA-512,
/// and edited while other locked cells remain protected.
fn sec004_allow_edit_ranges() {
    let mut s = new_sheet();
    s.click_cell("A1");
    s.enter("100", "enter");
    s.click_cell("B1");
    s.enter("200", "enter");
    s.click_cell("C1");
    s.enter("300", "enter");

    // 1. Open Allow Edit Ranges panel and add Range1 on B1:B5 with password
    s.menu("xlsx.allow-edit-ranges");
    allow_ranges_field(&mut s, "title", "Range1");
    allow_ranges_field(&mut s, "sqref", "B1:B5");
    allow_ranges_field(&mut s, "password", "range_pw");
    allow_ranges_click(&mut s, "add");

    // Add Range2 on C1:C5 without password
    allow_ranges_field(&mut s, "title", "Range2");
    allow_ranges_field(&mut s, "sqref", "C1:C5");
    allow_ranges_field(&mut s, "password", "");
    allow_ranges_click(&mut s, "add");

    let count = s.with(|e, _| e.current_sheet().unwrap().1.protected_ranges.len());
    assert_eq!(count, 2, "two protected ranges configured");

    // Close panel
    allow_ranges_click(&mut s, "close");

    // 2. Protect the sheet
    s.menu("xlsx.protect-sheet");
    protect_click(&mut s, "apply");
    assert!(s.with(|e, _| e.sheet_protection().is_some()), "sheet is protected");

    // 3. A1 is locked and outside any allowed range -> editing refused
    s.click_cell("A1");
    s.enter("999", "enter");
    assert_eq!(s.value("A1"), CellValue::Number(100.0), "A1 locked");
    assert!(s.status_texts().iter().any(|t| t.contains("保護されているシート")));

    // 4. B1 is in Range1 (password-protected) -> before unlock, editing refused
    s.click_cell("B1");
    s.enter("999", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(200.0), "B1 refused before unlock");

    // 5. Unlock Range1 with password
    let bad_pw = s.update(|e, _, _| e.unlock_protected_range("Range1", "wrong_pw"));
    assert!(!bad_pw, "wrong password rejected");

    let ok_pw = s.update(|e, _, _| e.unlock_protected_range("Range1", "range_pw"));
    assert!(ok_pw, "correct password unlocks range");

    // Now B1 can be edited
    s.click_cell("B1");
    s.enter("250", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(250.0), "B1 updated after unlock");

    // 6. C1 is in Range2 (no password) -> can be edited directly without password
    s.click_cell("C1");
    s.enter("350", "enter");
    assert_eq!(s.value("C1"), CellValue::Number(350.0), "C1 edited freely");

    // 7. Save and reopen roundtrip
    s.save_as("sec004.xlsx");
    let saved = std::fs::read(crate::shell::scenario::repo_root().join("target/scenario/xlsx::sec004_allow_edit_ranges/sec004.xlsx")).unwrap();
    let p = s.reopen();
    let sh = sheet_of_slide(&p.slides[1]).expect("sheet");
    assert_eq!(sh.protected_ranges.len(), 2, "protected ranges survived reopen");
    let r1 = sh.protected_ranges.iter().find(|r| r.name == "Range1").expect("Range1");
    assert!(r1.hash.is_some(), "Range1 has password hash");
    let r2 = sh.protected_ranges.iter().find(|r| r.name == "Range2").expect("Range2");
    assert!(r2.hash.is_none(), "Range2 has no password");
    assert!(!saved.windows(8).any(|w| w == b"range_pw"), "cleartext password not in file");
}

fn series_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.series.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}
fn series_field(s: &mut Scenario, id: &str, text: &str) {
    series_click(s, id);
    s.key("cmd-a");
    s.type_text(text);
}

/// XLSX-CELL-015 連続データの作成: 1 down A1:A5 by 3 (1, 4, 7, 10, 13);
/// 2 along B1 multiplied by 2 up to 100 from one cell; month ends from
/// 2026/1/31 down C1:C4 (2/28, 3/31, 4/30), still dates; undone in one step.
fn cell015_series() {
    let mut s = new_sheet();
    s.click_cell("A1");
    s.enter("1", "enter");
    s.click_cell("A1");
    for _ in 0..4 {
        s.key("shift-down");
    }
    s.menu("xlsx.series");
    series_field(&mut s, "step", "3");
    series_click(&mut s, "apply");
    let column = |s: &mut Scenario, c: &str, n: u32| (1..=n).map(|r| s.value(&format!("{c}{r}"))).collect::<Vec<_>>();
    let n = |v: f64| CellValue::Number(v);
    assert_eq!(column(&mut s, "A", 5), vec![n(1.0), n(4.0), n(7.0), n(10.0), n(13.0)]);
    // From one cell, along the row, ×2 until 100.
    s.click_cell("B1");
    s.enter("2", "enter");
    s.click_cell("B1");
    s.menu("xlsx.series");
    series_click(&mut s, "series-rows");
    series_click(&mut s, "series-growth");
    series_field(&mut s, "step", "2");
    series_field(&mut s, "stop", "100");
    series_click(&mut s, "apply");
    let row: Vec<CellValue> = ["B1", "C1", "D1", "E1", "F1", "G1", "H1"].iter().map(|c| s.value(c)).collect();
    assert_eq!(row, vec![n(2.0), n(4.0), n(8.0), n(16.0), n(32.0), n(64.0), CellValue::Empty]);
    // Month ends.
    s.click_cell("C2");
    s.enter("2026/1/31", "enter");
    s.click_cell("C2");
    for _ in 0..3 {
        s.key("shift-down");
    }
    s.menu("xlsx.series");
    series_click(&mut s, "series-date");
    series_click(&mut s, "unit-month");
    series_click(&mut s, "apply");
    let shown: Vec<String> = ["C2", "C3", "C4", "C5"].iter().map(|c| s.shown(c)).collect();
    assert_eq!(shown, vec!["2026/1/31", "2026/2/28", "2026/3/31", "2026/4/30"], "{shown:?}");
    s.key("cmd-z");
    assert_eq!(s.value("C3"), CellValue::Empty);
}

/// XLSX-WHATIF-014 U:線形・指数傾向のフィル:
/// 編集 > フィル > 連続データの作成…（xlsx.series）における
/// 1. 線形傾向（Linear Trend）: 既存の数値（10, 25）からトレンドオンで加算（傾き15）を展開
/// 2. 指数傾向（Growth Trend）: 既存の数値（10, 20）からトレンドオンで乗算（公比2）を展開
/// 3. 行方向の指数傾向: C1(3), D1(9) から横方向に公比3を展開
/// 4. 元に戻す（Undo）・やり直し（Redo）、およびファイル保存・再オープンラウンドトリップのデータ整合性を検証
fn whatif014_trend_linear_and_growth() {
    let mut s = new_sheet();

    // 1. 列方向の線形傾向（Linear Trend）: 10, 25 から 10, 25, 40, 55, 70
    s.click_cell("A1");
    s.enter("10", "enter");
    s.click_cell("A2");
    s.enter("25", "enter");

    s.click_cell("A1");
    for _ in 0..4 {
        s.key("shift-down");
    }
    s.menu("xlsx.series");
    series_click(&mut s, "series-trend");
    series_click(&mut s, "series-linear");
    series_click(&mut s, "apply");

    let n = |v: f64| CellValue::Number(v);
    let col_a: Vec<CellValue> = ["A1", "A2", "A3", "A4", "A5"].iter().map(|c| s.value(c)).collect();
    assert_eq!(col_a, vec![n(10.0), n(25.0), n(40.0), n(55.0), n(70.0)], "linear trend down columns");

    // 2. 列方向の指数傾向（Growth Trend）: 10, 20 から 10, 20, 40, 80, 160
    s.click_cell("B1");
    s.enter("10", "enter");
    s.click_cell("B2");
    s.enter("20", "enter");

    s.click_cell("B1");
    for _ in 0..4 {
        s.key("shift-down");
    }
    s.menu("xlsx.series");
    series_click(&mut s, "series-trend");
    series_click(&mut s, "series-growth");
    series_click(&mut s, "apply");

    let col_b: Vec<CellValue> = ["B1", "B2", "B3", "B4", "B5"].iter().map(|c| s.value(c)).collect();
    assert_eq!(col_b, vec![n(10.0), n(20.0), n(40.0), n(80.0), n(160.0)], "growth trend down columns");

    // 3. 行方向の指数傾向（Growth Trend across rows）: 3, 9 から 3, 9, 27, 81
    s.click_cell("C1");
    s.enter("3", "enter");
    s.click_cell("D1");
    s.enter("9", "enter");

    s.click_cell("C1");
    for _ in 0..3 {
        s.key("shift-right");
    }
    s.menu("xlsx.series");
    series_click(&mut s, "series-rows");
    series_click(&mut s, "series-trend");
    series_click(&mut s, "series-growth");
    series_click(&mut s, "apply");

    let row_cd: Vec<CellValue> = ["C1", "D1", "E1", "F1"].iter().map(|c| s.value(c)).collect();
    assert_eq!(row_cd, vec![n(3.0), n(9.0), n(27.0), n(81.0)], "growth trend across rows");

    // 4. 元に戻す（Undo）とやり直し（Redo）
    s.key("cmd-z");
    assert_eq!(s.value("E1"), CellValue::Empty, "undo clears filled cells");
    assert_eq!(s.value("F1"), CellValue::Empty);
    s.key("cmd-shift-z");
    assert_eq!(s.value("E1"), n(27.0), "redo restores filled cells");
    assert_eq!(s.value("F1"), n(81.0));

    // 5. 保存と再オープンラウンドトリップ
    s.save_as("whatif014_trend.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(sheet.cell(4, 0).map(|c| &c.value), Some(&n(70.0)), "A5 saved");
    assert_eq!(sheet.cell(4, 1).map(|c| &c.value), Some(&n(160.0)), "B5 saved");
    assert_eq!(sheet.cell(0, 5).map(|c| &c.value), Some(&n(81.0)), "F1 saved");
}

fn advanced_filter_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.advanced_filter.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}
fn advanced_filter_field(s: &mut Scenario, id: &str, text: &str) {
    advanced_filter_click(s, id);
    s.key("cmd-a");
    s.type_text(text);
}

/// XLSX-TABLE-015 データ > 詳細設定…:
/// Advanced filter with criteria range:
/// 1. In-place filter with AND condition (地域="東" AND 点数>75)
///    - Checks matching rows stay visible and non-matching rows are hidden
///    - Checks undo (rows restored) and redo (re-hidden)
/// 2. Copy to another location (抽出先: 指定した範囲, copy_to = H1)
///    - Checks extracted rows at H1:J3
/// 3. Save to xlsx and reopen:
///    - Verifies extracted cells persisted in sheet!
fn table015_advanced_filter() {
    let mut s = new_sheet();
    let rows = [
        ["名前", "地域", "点数"],
        ["佐藤", "東", "80"],
        ["鈴木", "西", "90"],
        ["田中", "東", "70"],
        ["高橋", "東", "85"],
        ["渡辺", "西", "60"],
    ];
    for (row, values) in rows.into_iter().enumerate() {
        for (column, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(column as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    // Criteria in E1:F2 (地域="東", 点数=">75")
    s.click_cell("E1"); s.enter("地域", "enter");
    s.click_cell("F1"); s.enter("点数", "enter");
    s.click_cell("E2"); s.enter("東", "enter");
    s.click_cell("F2"); s.enter(">75", "enter");

    s.click_cell("A1");
    for _ in 0..5 { s.key("shift-down"); }
    for _ in 0..2 { s.key("shift-right"); }
    s.menu("xlsx.advanced-filter");

    let list_val = s.with(|e, cx| {
        e.wb().panels.advanced_filter.as_ref().unwrap().list_range.read(cx).value().to_string()
    });
    assert_eq!(list_val, "A1:C6");

    advanced_filter_field(&mut s, "criteria-input", "E1:F2");
    advanced_filter_click(&mut s, "apply");

    let message = |s: &mut Scenario| s.with(|e, _| e.wb().panels.advanced_filter.as_ref().unwrap().message.clone());
    assert_eq!(message(&mut s), "フィルターを実行しました。");

    // 佐藤 (row 2, 80) is visible, 鈴木 (row 3, 西) is hidden, 田中 (row 4, 70 <= 75) is hidden,
    // 高橋 (row 5, 85) is visible, 渡辺 (row 6, 西) is hidden
    assert!(s.size_pt("2") > 0.0);
    assert_eq!(s.size_pt("3"), 0.0);
    assert_eq!(s.size_pt("4"), 0.0);
    assert!(s.size_pt("5") > 0.0);
    assert_eq!(s.size_pt("6"), 0.0);

    // Undo / Redo
    s.click_cell("A1");
    s.key("cmd-z");
    assert!(s.size_pt("3") > 0.0);
    assert!(s.size_pt("4") > 0.0);
    s.key("cmd-shift-z");
    assert_eq!(s.size_pt("3"), 0.0);
    assert_eq!(s.size_pt("4"), 0.0);

    // Copy to other location: H1
    advanced_filter_click(&mut s, "action-copy");
    advanced_filter_field(&mut s, "copy-to-input", "H1");
    advanced_filter_click(&mut s, "apply");

    let n = |v: f64| CellValue::Number(v);
    let t = |s: &str| CellValue::Text(s.into());
    assert_eq!(s.value("H1"), t("名前"));
    assert_eq!(s.value("I1"), t("地域"));
    assert_eq!(s.value("J1"), t("点数"));
    assert_eq!(s.value("H2"), t("佐藤"));
    assert_eq!(s.value("I2"), t("東"));
    assert_eq!(s.value("J2"), n(80.0));
    assert_eq!(s.value("H3"), t("高橋"));
    assert_eq!(s.value("I3"), t("東"));
    assert_eq!(s.value("J3"), n(85.0));

    s.save_as("table015.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(sheet.cell(1, 7).map(|c| c.value.clone()), Some(t("佐藤")));
    assert_eq!(sheet.cell(1, 9).map(|c| c.value.clone()), Some(n(80.0)));

    advanced_filter_click(&mut s, "close");
    assert!(s.with(|e, _| e.wb().panels.advanced_filter.is_none()));
}

fn consolidate_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.consolidate.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}
fn consolidate_field(s: &mut Scenario, id: &str, text: &str) {
    consolidate_click(s, id);
    s.key("cmd-a");
    s.type_text(text);
}

/// XLSX-TABLE-018 データ > 統合…:
/// Consolidate data from several ranges/sheets:
/// 1. Consolidate by top/left labels:
///    - Source 1 at A1:C3 (りんご, みかん)
///    - Source 2 at E1:G3 (りんご, バナナ)
///    - Destination at A8
///    - Function: Sum
///    - Labels: top_labels = true, left_labels = true
/// 2. Verifies consolidated table aggregation
/// 3. Undo / Redo verification
/// 4. Save and reopen: verifies <dataConsolidate> attributes and dataRefs!
fn table018_consolidate() {
    let mut s = new_sheet();
    let src1 = [
        ["品名", "4月", "5月"],
        ["りんご", "100", "150"],
        ["みかん", "80", "120"],
    ];
    for (r, vals) in src1.into_iter().enumerate() {
        for (c, v) in vals.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(c as u32, r as u32));
            s.enter(v, "enter");
        }
    }
    let src2 = [
        ["品名", "4月", "5月"],
        ["りんご", "200", "250"],
        ["バナナ", "50", "70"],
    ];
    for (r, vals) in src2.into_iter().enumerate() {
        for (c, v) in vals.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(c as u32 + 4, r as u32));
            s.enter(v, "enter");
        }
    }

    s.click_cell("A8");
    s.menu("xlsx.consolidate");

    consolidate_field(&mut s, "reference-input", "A1:C3");
    consolidate_click(&mut s, "add-ref");

    consolidate_field(&mut s, "reference-input", "E1:G3");
    consolidate_click(&mut s, "add-ref");

    let refs = s.with(|e, _| e.wb().panels.consolidate.as_ref().unwrap().references.clone());
    assert_eq!(refs, vec!["A1:C3", "E1:G3"]);

    consolidate_click(&mut s, "top-labels");
    consolidate_click(&mut s, "left-labels");

    consolidate_click(&mut s, "apply");
    let msg = s.with(|e, _| e.wb().panels.consolidate.as_ref().unwrap().message.clone());
    assert_eq!(msg, "統合を実行しました。");

    let n = |v: f64| CellValue::Number(v);
    let t = |s: &str| CellValue::Text(s.into());

    // Row 8: headers: [empty, 4月, 5月]
    assert_eq!(s.value("B8"), t("4月"));
    assert_eq!(s.value("C8"), t("5月"));
    // Row 9: りんご: [りんご, 300, 400]
    assert_eq!(s.value("A9"), t("りんご"));
    assert_eq!(s.value("B9"), n(300.0));
    assert_eq!(s.value("C9"), n(400.0));
    // Row 10: みかん: [みかん, 80, 120]
    assert_eq!(s.value("A10"), t("みかん"));
    assert_eq!(s.value("B10"), n(80.0));
    assert_eq!(s.value("C10"), n(120.0));
    // Row 11: バナナ: [バナナ, 50, 70]
    assert_eq!(s.value("A11"), t("バナナ"));
    assert_eq!(s.value("B11"), n(50.0));
    assert_eq!(s.value("C11"), n(70.0));

    // Undo / Redo
    s.click_cell("A1");
    s.key("cmd-z");
    assert_eq!(s.value("B9"), CellValue::Empty);
    s.key("cmd-shift-z");
    assert_eq!(s.value("B9"), n(300.0));

    s.save_as("table018.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(sheet.cell(8, 1).map(|c| c.value.clone()), Some(n(300.0)));
    assert_eq!(sheet.cell(10, 1).map(|c| c.value.clone()), Some(n(50.0)));
    let dc = sheet.data_consolidate.as_ref().expect("data_consolidate saved and restored");
    assert_eq!(dc.function, ibis_doc_model::ConsolidateFunction::Sum);
    assert!(dc.top_labels);
    assert!(dc.left_labels);
    assert_eq!(dc.data_refs, vec!["A1:C3", "E1:G3"]);

    consolidate_click(&mut s, "close");
    assert!(s.with(|e, _| e.wb().panels.consolidate.is_none()));
}

/// XLSX-TABLE-014: 大文字と小文字を区別する並べ替え
fn table014_case_sensitive_sort() {
    let mut s = new_sheet();
    for (row, values) in [["Item", "ID"], ["b", "1"], ["B", "2"], ["a", "3"], ["A", "4"]].into_iter().enumerate() {
        for (col, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(col as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    s.click_cell("A1");
    s.menu("xlsx.sort-manage");
    sort_field(&mut s, "range", "A1:B5");
    sort_field(&mut s, "column-0", "A");
    assert!(s.with(|e, _| e.wb().sort_edit.as_ref().unwrap().header));
    assert!(!s.with(|e, _| e.wb().sort_edit.as_ref().unwrap().match_case));

    // Case-insensitive sort ascending (default): stable order within same case-folded letter (a then A, b then B)
    sort_click(&mut s, "apply");
    assert_eq!(s.value("A2"), CellValue::Text("a".into()));
    assert_eq!(s.value("A3"), CellValue::Text("A".into()));
    assert_eq!(s.value("A4"), CellValue::Text("b".into()));
    assert_eq!(s.value("A5"), CellValue::Text("B".into()));

    // Toggle match_case on
    sort_click(&mut s, "match-case");
    assert!(s.with(|e, _| e.wb().sort_edit.as_ref().unwrap().match_case));
    s.screenshot("sort-case-sensitive-panel");

    // Apply case-sensitive sort: uppercase precedes lowercase (A, a, B, b)
    sort_click(&mut s, "apply");
    assert_eq!(s.value("A2"), CellValue::Text("A".into()));
    assert_eq!(s.value("A3"), CellValue::Text("a".into()));
    assert_eq!(s.value("A4"), CellValue::Text("B".into()));
    assert_eq!(s.value("A5"), CellValue::Text("b".into()));

    // Undo / Redo
    s.click_cell("A1");
    s.key("cmd-z");
    assert_eq!(s.value("A2"), CellValue::Text("a".into()));
    s.key("cmd-shift-z");
    assert_eq!(s.value("A2"), CellValue::Text("A".into()));

    // Save and reopen
    s.save_as("table014-case-sensitive.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(sheet.cell(1, 0).unwrap().value, CellValue::Text("A".into()));
    assert_eq!(sheet.cell(2, 0).unwrap().value, CellValue::Text("a".into()));
    assert_eq!(sheet.cell(3, 0).unwrap().value, CellValue::Text("B".into()));
    assert_eq!(sheet.cell(4, 0).unwrap().value, CellValue::Text("b".into()));

    sort_click(&mut s, "close");
    assert!(s.with(|e, _| e.wb().sort_edit.is_none()));
}

fn table013_custom_list_sort() {
    let mut s = new_sheet();
    let rows = [
        ("A1", "曜日"), ("B1", "番号"),
        ("A2", "水"),   ("B2", "1"),
        ("A3", "月"),   ("B3", "2"),
        ("A4", "金"),   ("B4", "3"),
        ("A5", "日"),   ("B5", "4"),
        ("A6", "その他"), ("B6", "5"),
        ("A7", "火"),   ("B7", "6"),
    ];
    for (pos, val) in rows {
        s.click_cell(pos);
        s.enter(val, "enter");
    }

    s.menu("xlsx.sort-manage");
    sort_field(&mut s, "range", "A1:B7");
    sort_field(&mut s, "column-0", "A");
    assert!(s.with(|e, _| e.wb().sort_edit.as_ref().unwrap().header));

    // Open custom list settings for level 0
    sort_click(&mut s, "custom-btn-0");
    assert_eq!(s.with(|e, _| e.wb().sort_edit.as_ref().unwrap().custom_open), Some(0));

    // Click preset 0 ("日〜土")
    sort_click(&mut s, "preset-0-0");
    let preset_val = s.with(|e, cx| e.wb().sort_edit.as_ref().unwrap().custom_lists[0].read(cx).value().to_string());
    assert!(preset_val.contains("日") && preset_val.contains("土"));
    s.screenshot("sort-custom-list-panel");

    // Apply custom list sort (ascending): 日 (4), 月 (2), 火 (6), 水 (1), 金 (3), その他 (5)
    sort_click(&mut s, "apply");
    assert!(s.with(|e, _| e.wb().sort_edit.as_ref().unwrap().error.is_empty()));
    assert_eq!(s.value("A2"), CellValue::Text("日".into()));
    assert_eq!(s.value("B2"), CellValue::Number(4.0));
    assert_eq!(s.value("A3"), CellValue::Text("月".into()));
    assert_eq!(s.value("B3"), CellValue::Number(2.0));
    assert_eq!(s.value("A4"), CellValue::Text("火".into()));
    assert_eq!(s.value("B4"), CellValue::Number(6.0));
    assert_eq!(s.value("A5"), CellValue::Text("水".into()));
    assert_eq!(s.value("B5"), CellValue::Number(1.0));
    assert_eq!(s.value("A6"), CellValue::Text("金".into()));
    assert_eq!(s.value("B6"), CellValue::Number(3.0));
    assert_eq!(s.value("A7"), CellValue::Text("その他".into()));
    assert_eq!(s.value("B7"), CellValue::Number(5.0));

    // Reverse list order (descending): 金 (3), 水 (1), 火 (6), 月 (2), 日 (4), その他 (5)
    sort_click(&mut s, "order-0");
    sort_click(&mut s, "apply");
    assert!(s.with(|e, _| e.wb().sort_edit.as_ref().unwrap().error.is_empty()));
    assert_eq!(s.value("A2"), CellValue::Text("金".into()));
    assert_eq!(s.value("B2"), CellValue::Number(3.0));
    assert_eq!(s.value("A3"), CellValue::Text("水".into()));
    assert_eq!(s.value("B3"), CellValue::Number(1.0));
    assert_eq!(s.value("A4"), CellValue::Text("火".into()));
    assert_eq!(s.value("B4"), CellValue::Number(6.0));
    assert_eq!(s.value("A5"), CellValue::Text("月".into()));
    assert_eq!(s.value("B5"), CellValue::Number(2.0));
    assert_eq!(s.value("A6"), CellValue::Text("日".into()));
    assert_eq!(s.value("B6"), CellValue::Number(4.0));
    assert_eq!(s.value("A7"), CellValue::Text("その他".into()));
    assert_eq!(s.value("B7"), CellValue::Number(5.0));

    // Undo / Redo
    s.click_cell("A1");
    s.key("cmd-z");
    assert_eq!(s.value("A2"), CellValue::Text("日".into()));
    s.key("cmd-shift-z");
    assert_eq!(s.value("A2"), CellValue::Text("金".into()));

    // Clear custom list: reverts to normal value sort
    sort_click(&mut s, "custom-clear-0");
    assert!(s.with(|e, cx| e.wb().sort_edit.as_ref().unwrap().custom_lists[0].read(cx).value().is_empty()));

    // Save and reopen
    s.save_as("table013-custom-list-sort.xlsx");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(sheet.cell(1, 0).unwrap().value, CellValue::Text("金".into()));
    assert_eq!(sheet.cell(1, 1).unwrap().value, CellValue::Number(3.0));

    sort_click(&mut s, "close");
    assert!(s.with(|e, _| e.wb().sort_edit.is_none()));
}

fn formula016_calculation_options() {
    let mut s = new_sheet();
    // A1 = 10, B1 = =A1*2
    s.click_cell("A1");
    s.enter("10", "enter");
    s.click_cell("B1");
    s.enter("=A1*2", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(20.0));

    // Verify initial calculation mode is Auto (checked and disabled)
    assert_eq!(s.menu_state("xlsx.calc-auto"), Some((false, true)));
    assert_eq!(s.menu_state("xlsx.calc-manual"), Some((true, false)));
    assert_eq!(s.menu_state("xlsx.calc-auto-no-table"), Some((true, false)));

    // Switch to Manual mode via menu
    s.menu("xlsx.calc-manual");
    assert_eq!(s.menu_state("xlsx.calc-manual"), Some((false, true)));
    assert_eq!(s.menu_state("xlsx.calc-auto"), Some((true, false)));

    // Edit A1 = 50: B1 must NOT recalculate in Manual mode
    s.click_cell("A1");
    s.enter("50", "enter");
    assert_eq!(s.value("A1"), CellValue::Number(50.0));
    assert_eq!(s.value("B1"), CellValue::Number(20.0));

    // Calculate Now via menu (F9)
    s.menu("xlsx.calculate-now");
    assert_eq!(s.value("B1"), CellValue::Number(100.0));

    // Undo Calculate Now
    s.key("cmd-z");
    assert_eq!(s.value("B1"), CellValue::Number(20.0));

    // Redo Calculate Now
    s.key("cmd-shift-z");
    assert_eq!(s.value("B1"), CellValue::Number(100.0));

    // Edit A1 = 70: B1 remains 100 in Manual mode
    s.click_cell("A1");
    s.enter("70", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(100.0));

    // Calculate Sheet via menu (Shift+F9)
    s.menu("xlsx.calculate-sheet");
    assert_eq!(s.value("B1"), CellValue::Number(140.0));

    // Switch to AutoNoTable
    s.menu("xlsx.calc-auto-no-table");
    assert_eq!(s.menu_state("xlsx.calc-auto-no-table"), Some((false, true)));

    // In AutoNoTable, regular formulas auto-calculate
    s.click_cell("A1");
    s.enter("5", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(10.0));

    // Switch back to Auto
    s.menu("xlsx.calc-auto");
    assert_eq!(s.menu_state("xlsx.calc-auto"), Some((false, true)));

    // Save and reopen in Auto mode
    s.save_as("formula016-calc-mode-auto.xlsx");
    let p = s.reopen();
    assert_eq!(p.workbook.as_deref().unwrap().calc_mode, ibis_doc_model::CalcMode::Auto);

    // Switch to Manual and save
    s.menu("xlsx.calc-manual");
    s.save_as("formula016-calc-mode-manual.xlsx");
    let p2 = s.reopen();
    assert_eq!(p2.workbook.as_deref().unwrap().calc_mode, ibis_doc_model::CalcMode::Manual);
}

/// XLSX-FORMULA-019: 表示桁数で計算 (Precision as displayed / calcPr/@fullPrecision="0"):
/// - Initial calculation precision is full (15 digits, fullPrecision: true).
/// - A1 = 10.456 with format 0.00 displays 10.46, internal value is 10.456, B1 = =A1*10 is 104.56.
/// - Toggle "Precision as Displayed" via Data menu:
///   - Checkbox is checked.
///   - Existing numeric cells rounded to displayed precision: A1 becomes 10.46.
///   - Formulas update: B1 becomes 104.60.
/// - Undo / Redo restores original values and full precision state.
/// - With precision as displayed active, entering new numbers (C1 = 5.67 with format 0.0) rounds directly to 5.7.
/// - Save and reopen: fullPrecision="0" persists in OOXML calcPr and rounded values persist.
/// - Toggle back to full precision and verify save/reopen roundtrip.
fn formula019_precision_as_displayed() {
    let mut s = new_sheet();

    // Verify initial full precision
    assert_eq!(s.menu_state("xlsx.precision-as-displayed"), Some((true, false)), "menu is enabled, unchecked");
    assert!(s.with(|e, _| e.presentation().workbook.as_deref().unwrap().full_precision));

    // A1 = 10.456, set format 0.00
    s.click_cell("A1");
    s.enter("10.456", "enter");
    s.click_cell("A1");
    custom_format(&mut s, "0.00");
    assert_eq!(s.shown("A1"), "10.46");
    assert_eq!(s.value("A1"), CellValue::Number(10.456), "internal value retains full precision initially");

    // B1 = =A1*10
    s.click_cell("B1");
    s.enter("=A1*10", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(104.56));

    // Turn on precision as displayed via Data menu
    s.menu("xlsx.precision-as-displayed");
    assert_eq!(s.menu_state("xlsx.precision-as-displayed"), Some((true, true)), "menu is checked");
    assert!(!s.with(|e, _| e.presentation().workbook.as_deref().unwrap().full_precision));

    // A1 rounded to displayed precision (10.46) and B1 recalculated to 104.60
    assert_eq!(s.value("A1"), CellValue::Number(10.46));
    assert_eq!(s.value("B1"), CellValue::Number(10.46 * 10.0));

    // Undo restores full precision and original values
    s.key("cmd-z");
    assert_eq!(s.menu_state("xlsx.precision-as-displayed"), Some((true, false)));
    assert!(s.with(|e, _| e.presentation().workbook.as_deref().unwrap().full_precision));
    assert_eq!(s.value("A1"), CellValue::Number(10.456));
    assert_eq!(s.value("B1"), CellValue::Number(104.56));

    // Redo restores precision as displayed
    s.key("cmd-shift-z");
    assert_eq!(s.menu_state("xlsx.precision-as-displayed"), Some((true, true)));
    assert!(!s.with(|e, _| e.presentation().workbook.as_deref().unwrap().full_precision));
    assert_eq!(s.value("A1"), CellValue::Number(10.46));
    assert_eq!(s.value("B1"), CellValue::Number(10.46 * 10.0));

    // Enter C1 with format 0.0 while precision as displayed is active
    s.click_cell("C1");
    custom_format(&mut s, "0.0");
    s.click_cell("C1");
    s.enter("5.67", "enter");
    assert_eq!(s.value("C1"), CellValue::Number(5.7), "entered value is rounded to displayed format");

    // Save and reopen
    s.save_as("formula019-precision-as-displayed.xlsx");
    let p = s.reopen();
    assert!(!p.workbook.as_deref().unwrap().full_precision, "fullPrecision=0 saved and reopened");
    assert_eq!(s.value("A1"), CellValue::Number(10.46));
    assert_eq!(s.value("B1"), CellValue::Number(10.46 * 10.0));
    assert_eq!(s.value("C1"), CellValue::Number(5.7));

    // Toggle back to full precision
    s.menu("xlsx.precision-as-displayed");
    assert_eq!(s.menu_state("xlsx.precision-as-displayed"), Some((true, false)));
    assert!(s.with(|e, _| e.presentation().workbook.as_deref().unwrap().full_precision));
    s.save_as("formula019-full-precision.xlsx");
    let p2 = s.reopen();
    assert!(p2.workbook.as_deref().unwrap().full_precision, "full precision restored");
}

/// XLSX-FORMULA-017: 反復計算と循環参照 (Iterative Calculation and Circular References):
/// - Cyclic formula B1 = =B1+1 defaults to 0.0 with iterative calculation disabled.
/// - Enable iterative calculation via Data menu: B1 evaluates to 100.0 (maxIterations=100).
/// - Enter IF counter C1 = =IF(C1<10, C1+1, C1) which converges to 10.0 in 10 iterations.
/// - Downstream non-cyclic formula D1 = =B1+C1 evaluates to 110.0.
/// - Undo / Redo restores iterative calculation state and values.
/// - Save and reopen: verify calcPr/@iterate="1" persists and values evaluate correctly.
fn formula017_iterative_calculation() {
    let mut s = new_sheet();

    // Verify initial iterative calculation state (disabled)
    assert_eq!(s.menu_state("xlsx.iterative-calculation"), Some((true, false)), "iterative calculation is disabled initially");
    assert!(!s.with(|e, _| e.presentation().workbook.as_deref().unwrap().iterate));

    // B1 = =B1+1 (self-referential circular formula)
    s.click_cell("B1");
    s.enter("=B1+1", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(0.0), "circular reference without iterative calculation shows 0");

    // Enable iterative calculation via Data menu
    s.menu("xlsx.iterative-calculation");
    assert_eq!(s.menu_state("xlsx.iterative-calculation"), Some((true, true)), "menu is checked");
    assert!(s.with(|e, _| e.presentation().workbook.as_deref().unwrap().iterate));
    assert_eq!(s.value("B1"), CellValue::Number(100.0), "B1 recalculated with 100 iterations");

    // Enter C1 = =IF(C1<10, C1+1, C1)
    s.click_cell("C1");
    s.enter("=IF(C1<10, C1+1, C1)", "enter");
    assert_eq!(s.value("C1"), CellValue::Number(10.0), "C1 converges to 10.0 in 10 iterations");

    // Enter downstream formula D1 = =B1+C1
    s.click_cell("D1");
    s.enter("=B1+C1", "enter");
    assert_eq!(s.value("D1"), CellValue::Number(110.0), "downstream formula uses converged values");

    // Undo: removes D1
    s.key("cmd-z");
    assert_eq!(s.value("D1"), CellValue::Empty);

    // Undo: removes C1
    s.key("cmd-z");
    assert_eq!(s.value("C1"), CellValue::Empty);

    // Undo: turns off iterative calculation
    s.key("cmd-z");
    assert_eq!(s.menu_state("xlsx.iterative-calculation"), Some((true, false)));
    assert!(!s.with(|e, _| e.presentation().workbook.as_deref().unwrap().iterate));
    assert_eq!(s.value("B1"), CellValue::Number(0.0));

    // Redo: turns on iterative calculation
    s.key("cmd-shift-z");
    assert_eq!(s.menu_state("xlsx.iterative-calculation"), Some((true, true)));
    assert!(s.with(|e, _| e.presentation().workbook.as_deref().unwrap().iterate));
    assert_eq!(s.value("B1"), CellValue::Number(100.0));

    // Redo: restores C1
    s.key("cmd-shift-z");
    assert_eq!(s.value("C1"), CellValue::Number(10.0));

    // Save and reopen
    s.save_as("formula017-iterative.xlsx");
    let p = s.reopen();
    assert!(p.workbook.as_deref().unwrap().iterate, "iterate=1 persisted in saved OOXML");
    assert_eq!(s.value("B1"), CellValue::Number(100.0));
    assert_eq!(s.value("C1"), CellValue::Number(10.0));

    // Toggle back off
    s.menu("xlsx.iterative-calculation");
    assert_eq!(s.menu_state("xlsx.iterative-calculation"), Some((true, false)));
    assert!(!s.with(|e, _| e.presentation().workbook.as_deref().unwrap().iterate));
    assert_eq!(s.value("B1"), CellValue::Number(0.0));
    s.save_as("formula017-iterative-off.xlsx");
    let p2 = s.reopen();
    assert!(!p2.workbook.as_deref().unwrap().iterate, "iterate=0 persisted after disabling");
}

/// XLSX-FORMULA-022: U:参照元・参照先トレース (Trace Precedents & Trace Dependents, Remove Arrows):
/// - A1=10, A2=20, A3=A1+A2, B3=A3*2, A5=SUM(A1:A2)
/// - Trace Precedents on A3 -> arrows from A1 and A2 to A3
/// - Trace Dependents on A3 -> arrow from A3 to B3
/// - Remove Precedent Arrows -> only dependent arrow remains
/// - Remove All Arrows -> all arrows cleared
/// - Range reference precedent on A5 -> box outline and arrow to A5
/// - Remove Dependent Arrows -> precedent arrow remains
/// - Menu items xlsx.trace-precedents, xlsx.trace-dependents, xlsx.remove-arrows connect correctly
fn formula022_trace_arrows() {
    let mut s = new_sheet();
    s.click_cell("A1");
    s.enter("10", "enter");
    s.click_cell("A2");
    s.enter("20", "enter");
    s.click_cell("A3");
    s.enter("=A1+A2", "enter");
    s.click_cell("B3");
    s.enter("=A3*2", "enter");

    assert_eq!(s.value("A3"), CellValue::Number(30.0));
    assert_eq!(s.value("B3"), CellValue::Number(60.0));

    // Initial state: no trace arrows, render_trace_arrows returns None
    assert!(s.with(|e, _| e.trace_arrows().is_empty()));
    assert!(s.with(|e, _| e.render_trace_arrows(1.0)).is_none());

    // Trace precedents on A3
    s.click_cell("A3");
    s.menu("xlsx.trace-precedents");

    let arrows = s.with(|e, _| e.trace_arrows().to_vec());
    assert_eq!(arrows.len(), 2, "A3 has 2 precedent arrows (A1 and A2)");
    assert!(arrows.iter().all(|a| a.to == (0, 2) && a.kind == super::trace::TraceArrowKind::Precedent));
    assert!(arrows.iter().any(|a| a.from_range == (0, 0, 0, 0))); // A1
    assert!(arrows.iter().any(|a| a.from_range == (0, 1, 0, 1))); // A2
    assert!(s.with(|e, _| e.render_trace_arrows(1.0)).is_some(), "overlay renders arrows");

    // Trace dependents on A3 (B3 depends on A3)
    s.menu("xlsx.trace-dependents");
    let arrows = s.with(|e, _| e.trace_arrows().to_vec());
    assert_eq!(arrows.len(), 3, "2 precedents + 1 dependent");
    assert!(arrows.iter().any(|a| a.to == (1, 2) && a.from_range == (0, 2, 0, 2) && a.kind == super::trace::TraceArrowKind::Dependent));

    // Remove precedent arrows only
    s.menu("xlsx.remove-precedent-arrows");
    let arrows = s.with(|e, _| e.trace_arrows().to_vec());
    assert_eq!(arrows.len(), 1, "only dependent arrow remains");
    assert_eq!(arrows[0].kind, super::trace::TraceArrowKind::Dependent);

    // Remove all arrows
    s.menu("xlsx.remove-all-arrows");
    assert!(s.with(|e, _| e.trace_arrows().is_empty()));
    assert!(s.with(|e, _| e.render_trace_arrows(1.0)).is_none());

    // Range reference precedent on A5
    s.click_cell("A5");
    s.enter("=SUM(A1:A2)", "enter");
    s.click_cell("A5");
    s.menu("xlsx.trace-precedents");

    let arrows = s.with(|e, _| e.trace_arrows().to_vec());
    assert_eq!(arrows.len(), 1, "A5 has 1 range precedent arrow");
    assert_eq!(arrows[0].from_range, (0, 0, 0, 1), "A1:A2 range");
    assert_eq!(arrows[0].to, (0, 4), "A5 target cell");
    assert_eq!(arrows[0].kind, super::trace::TraceArrowKind::Precedent);
    assert!(s.with(|e, _| e.render_trace_arrows(1.0)).is_some());

    // Remove dependent arrows (should not affect precedent arrow)
    s.menu("xlsx.remove-dependent-arrows");
    assert_eq!(s.with(|e, _| e.trace_arrows().len()), 1);

    // Remove all arrows via general remove-all-arrows menu
    s.menu("xlsx.remove-all-arrows");
    assert!(s.with(|e, _| e.trace_arrows().is_empty()));
    assert!(s.with(|e, _| e.render_trace_arrows(1.0)).is_none());
}

/// XLSX-FORMULA-023: 数式検証とウォッチ（Evaluate Formula & Watch Window）
/// - 数式の検証:
///   - 選択セルの数式に対してステップ評価（Evaluate Session）を開始
///   - 各ステップでの中間式の文字列表現および下線対象範囲（Redex span）の確認
///   - ステップイン（Step In）による参照先セルの数式展開と評価
///   - ステップアウト（Step Out）による親数式への復帰と結果置換
///   - 最終計算結果値への到達およびやり直し（Restart）
///   - 数式を含まないセルでの適切な案内メッセージ表示
/// - ウォッチ ウィンドウ:
///   - 選択セルのウォッチリストへの追加（シート名、セル参照、値、数式）
///   - セル値変更・再計算時の自動同期リフレッシュ
///   - 登録エントリのクリックによる該当セルへのジャンプ
///   - 個別削除およびすべて削除
fn formula023_evaluate_and_watch() {
    let mut s = new_sheet();
    s.click_cell("B1");
    s.enter("5", "enter");
    s.click_cell("A1");
    s.enter("=B1*2", "enter");
    s.click_cell("A2");
    s.enter("20", "enter");
    s.click_cell("A3");
    s.enter("=A1+A2*3", "enter");

    assert_eq!(s.value("B1"), CellValue::Number(5.0));
    assert_eq!(s.value("A1"), CellValue::Number(10.0));
    assert_eq!(s.value("A2"), CellValue::Number(20.0));
    assert_eq!(s.value("A3"), CellValue::Number(70.0));

    // 1. 数式の検証 (Evaluate Formula) on A3
    s.click_cell("A3");
    s.menu("xlsx.evaluate-formula");

    // パネルが開いていることを検証
    s.with(|e, _| {
        let p = e.sheet_state().and_then(|st| st.panels.evaluate_formula.as_ref()).expect("evaluate panel open");
        assert_eq!(p.info.formula_text, "=A1 + A2 * 3");
        assert!(!p.info.is_finished);
        assert_eq!(p.info.depth, 0);
    });

    // Step 1: ステップを進める
    s.update(|e, _, _| e.evaluate_step());
    s.with(|e, _| {
        let p = e.sheet_state().and_then(|st| st.panels.evaluate_formula.as_ref()).unwrap();
        assert!(!p.info.is_finished);
    });

    // 最後まで step を実行
    for _ in 0..10 {
        let finished = s.with(|e, _| e.sheet_state().and_then(|st| st.panels.evaluate_formula.as_ref()).map_or(true, |p| p.info.is_finished));
        if finished { break; }
        s.update(|e, _, _| e.evaluate_step());
    }

    s.with(|e, _| {
        let p = e.sheet_state().and_then(|st| st.panels.evaluate_formula.as_ref()).unwrap();
        assert!(p.info.is_finished);
        assert_eq!(p.info.result_value, Some(CellValue::Number(70.0)));
        assert_eq!(p.info.formula_text, "70");
    });

    // やり直し (Restart)
    s.update(|e, _, _| e.evaluate_restart());
    s.with(|e, _| {
        let p = e.sheet_state().and_then(|st| st.panels.evaluate_formula.as_ref()).unwrap();
        assert_eq!(p.info.formula_text, "=A1 + A2 * 3");
        assert!(!p.info.is_finished);
    });

    // ステップインの検証 (セル A4 = =A1 + 1)
    s.click_cell("A4");
    s.enter("=A1+1", "enter");
    s.click_cell("A4");
    s.menu("xlsx.evaluate-formula");

    s.with(|e, _| {
        let p = e.sheet_state().and_then(|st| st.panels.evaluate_formula.as_ref()).unwrap();
        assert_eq!(p.info.formula_text, "=A1 + 1");
        assert!(p.info.can_step_in, "A1 has formula =B1*2, so can_step_in is true");
        assert_eq!(p.info.step_in_target, Some("A1".into()));
    });

    // Step In into A1
    s.update(|e, _, _| e.evaluate_step_in());
    s.with(|e, _| {
        let p = e.sheet_state().and_then(|st| st.panels.evaluate_formula.as_ref()).unwrap();
        assert_eq!(p.info.depth, 1);
        assert_eq!(p.info.formula_text, "=B1 * 2");
    });

    // Step Out back to A4
    s.update(|e, _, _| e.evaluate_step_out());
    s.with(|e, _| {
        let p = e.sheet_state().and_then(|st| st.panels.evaluate_formula.as_ref()).unwrap();
        assert_eq!(p.info.depth, 0);
        assert_eq!(p.info.formula_text, "=10 + 1");
    });

    // 数式のないセル (B1: 数値 5) で開いた場合
    s.click_cell("B1");
    s.menu("xlsx.evaluate-formula");
    s.with(|e, _| {
        let p = e.sheet_state().and_then(|st| st.panels.evaluate_formula.as_ref()).unwrap();
        assert!(p.session.is_none());
        assert!(p.message.is_some());
    });

    // 2. ウォッチ ウィンドウ (Watch Window)
    s.menu("xlsx.watch-window");
    s.with(|e, _| {
        assert!(e.sheet_state().and_then(|st| st.panels.watch_window.as_ref()).is_some());
        assert!(e.sheet_state().unwrap().panels.watches.is_empty());
    });

    // A4 セルをウォッチに追加
    s.click_cell("A4");
    s.update(|e, _, _| e.add_watch_cell());
    s.with(|e, _| {
        let watches = &e.sheet_state().unwrap().panels.watches;
        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0].cell_ref, "$A$4");
        assert_eq!(watches[0].value_text, "11");
        assert_eq!(watches[0].formula_text, Some("=A1+1".into()));
    });

    // B1 セルをウォッチに追加
    s.click_cell("B1");
    s.update(|e, _, _| e.add_watch_cell());
    s.with(|e, _| {
        let watches = &e.sheet_state().unwrap().panels.watches;
        assert_eq!(watches.len(), 2);
        assert_eq!(watches[1].cell_ref, "$B$1");
        assert_eq!(watches[1].value_text, "5");
    });

    // B1 の値を 50 に変更 -> ウォッチが自動同期リフレッシュされる
    s.click_cell("B1");
    s.enter("50", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(50.0));
    assert_eq!(s.value("A1"), CellValue::Number(100.0));
    assert_eq!(s.value("A4"), CellValue::Number(101.0));

    s.update(|e, _, _| e.refresh_watches());
    s.with(|e, _| {
        let watches = &e.sheet_state().unwrap().panels.watches;
        assert_eq!(watches[0].value_text, "101", "A4 recalculated value in watch");
        assert_eq!(watches[1].value_text, "50", "B1 updated value in watch");
    });

    // ジャンプ機能: ウォッチの A4 をクリック
    let a4_watch_id = s.with(|e, _| e.sheet_state().unwrap().panels.watches[0].id);
    s.update(|e, _, _| e.jump_to_watch(a4_watch_id));
    assert_eq!(s.with(|e, _| e.current_cell()), Some((0, 3)), "current cell jumped to A4 (col 0, row 3)");

    // 個別削除
    s.update(|e, _, _| e.delete_watch(a4_watch_id));
    s.with(|e, _| {
        let watches = &e.sheet_state().unwrap().panels.watches;
        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0].cell_ref, "$B$1");
    });

    // すべて削除
    s.update(|e, _, _| e.clear_all_watches());
    s.with(|e, _| {
        assert!(e.sheet_state().unwrap().panels.watches.is_empty());
    });
}

/// XLSX-FORMULA-008: 名前付き数式と定数 (Named formulas and constants):
/// - Define constant name (TaxRate = 0.10)
/// - Define formula name (TotalSales = SUM(A1:A2))
/// - Define composite formula name (WithTax = TotalSales * (1 + TaxRate))
/// - Cells referencing named formulas and constants evaluate correctly
/// - Edit defined name via UI panel (TaxRate -> 0.08) and verify dependency cascade updates
/// - Undo / Redo of defined name changes
/// - Save to .xlsx and reopen: verify defined names and calculated values persist
fn formula008_named_formulas() {
    let mut s = new_sheet();
    s.click_cell("A1");
    s.enter("1000", "enter");
    s.click_cell("A2");
    s.enter("2000", "enter");

    // Enter formulas referencing defined names before they are defined -> all evaluate to #NAME?
    s.click_cell("B1");
    s.enter("=A1*TaxRate", "enter");
    assert_eq!(s.shown("B1"), "#NAME?");

    s.click_cell("B2");
    s.enter("=TotalSales", "enter");
    assert_eq!(s.shown("B2"), "#NAME?");

    s.click_cell("B3");
    s.enter("=WithTax", "enter");
    assert_eq!(s.shown("B3"), "#NAME?");

    // Open name manager in sidebar
    s.menu("xlsx.named-ranges");

    // Define constant name TaxRate = 0.10 -> B1 recalculates to 100.0
    s.update(|e, _, cx| {
        assert!(e.set_named_formula("TaxRate", "0.10", cx));
    });
    assert_eq!(s.value("B1"), CellValue::Number(100.0));

    // Define formula name TotalSales = SUM(A1:A2) -> B2 recalculates to 3000.0
    s.update(|e, _, cx| {
        assert!(e.set_named_formula("TotalSales", "SUM(A1:A2)", cx));
    });
    assert_eq!(s.value("B2"), CellValue::Number(3000.0));

    let num = |v: &CellValue| match v {
        CellValue::Number(n) => *n,
        other => panic!("expected number, got {other:?}"),
    };

    // Define composite formula name WithTax = TotalSales * (1 + TaxRate) -> B3 recalculates to 3300.0
    s.update(|e, _, cx| {
        assert!(e.set_named_formula("WithTax", "TotalSales * (1 + TaxRate)", cx));
    });
    assert!((num(&s.value("B3")) - 3300.0).abs() < 1e-4);

    // Edit TaxRate via UI panel to 0.08
    s.update(|e, window, cx| {
        e.edit_name(0, "TaxRate", "0.08", window, cx);
        let panel = e.sheet_state().unwrap().names.as_ref().unwrap();
        let name_text = panel.new_name.read(cx).value();
        let formula_text = panel.new_formula.read(cx).value();
        assert_eq!(name_text, "TaxRate");
        assert_eq!(formula_text, "=0.08");
        assert!(e.set_named_formula(&name_text, &formula_text, cx));
    });

    // Cascade recalculation: B1 updates to 80.0, B3 updates to 3240.0
    assert_eq!(s.value("B1"), CellValue::Number(80.0));
    assert_eq!(s.value("B2"), CellValue::Number(3000.0));
    assert!((num(&s.value("B3")) - 3240.0).abs() < 1e-4);

    // Undo: TaxRate reverts to 0.10, B1 -> 100.0, B3 -> 3300.0
    s.key("cmd-z");
    assert_eq!(s.value("B1"), CellValue::Number(100.0));
    assert!((num(&s.value("B3")) - 3300.0).abs() < 1e-4);

    // Redo: TaxRate back to 0.08, B1 -> 80.0, B3 -> 3240.0
    s.key("cmd-shift-z");
    assert_eq!(s.value("B1"), CellValue::Number(80.0));
    assert!((num(&s.value("B3")) - 3240.0).abs() < 1e-4);

    let sheet_name = s.with(|e, _| e.slide().name.clone().unwrap());

    // Save and reopen
    s.save_as("formula008-named-formulas.xlsx");
    let p = s.reopen();
    let names = &p.workbook.as_deref().unwrap().names;
    assert_eq!(names.iter().find(|n| n.name == "TaxRate").unwrap().formula, "0.08");
    assert_eq!(names.iter().find(|n| n.name == "TotalSales").unwrap().formula, format!("SUM({sheet_name}!$A$1:$A$2)"));
    assert_eq!(names.iter().find(|n| n.name == "WithTax").unwrap().formula, "TotalSales*(1+TaxRate)");
    let slide = p.slides.iter().find(|sl| sl.name.as_deref() == Some(sheet_name.as_str())).unwrap();
    let sheet = sheet_of_slide(slide).unwrap();
    assert_eq!(sheet.cell(0, 1).unwrap().value, CellValue::Number(80.0));
    assert_eq!(sheet.cell(1, 1).unwrap().value, CellValue::Number(3000.0));
    assert!((num(&sheet.cell(2, 1).unwrap().value) - 3240.0).abs() < 1e-4);
}

/// XLSX-PIVOT-009: ピボットテーブルの日付・数値フィールドのグループ化とグループ解除
fn pivot009_group_and_ungroup() {
    use ibis_doc_model::{DateGroupBy, PivotAxis, PivotFieldGrouping};
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");
    let slide_id = s.with(|e, _| e.current_slide);
    let target = (slide_id, 0);

    // 1. Place date field 0 (日付) on rows, field 6 (金額) on values
    s.update(move |e, _, cx| e.change_pivot(target, |p| {
        p.place(2, None, 0); // remove 地域
        p.place(3, None, 0); // remove 担当者
        p.place(0, Some(PivotAxis::Row), 0);
    }, cx));
    assert_eq!(pivot_of(&mut s, 0).row_fields, vec![0]);

    // 2. Select A5 (a cell in the date row labels) and open grouping via menu "xlsx.pivot-group"
    s.click_cell("A5");
    s.menu("xlsx.pivot-group");
    assert!(s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot_group.as_ref()).is_some()), "pivot group panel opened");

    // 3. Configure Date grouping by Months (月) and apply
    s.update(|e, _, cx| {
        if let Some(p) = e.wb_mut().panels.pivot_group.as_mut() {
            p.is_date = true;
            p.date_by = DateGroupBy::Months;
            p.date_auto_start = true;
            p.date_auto_end = true;
        }
        e.apply_pivot_group(cx);
    });

    let p = pivot_of(&mut s, 0);
    assert_eq!(p.fields[0].grouping, Some(PivotFieldGrouping::Date {
        by: DateGroupBy::Months,
        start: Some(46027.0),
        end: Some(46378.0),
        auto_start: true,
        auto_end: true,
    }));

    // Verify row items are grouped into month labels (contains "月")
    let first_row_label = s.value("A4");
    assert!(matches!(first_row_label, CellValue::Text(ref t) if t.contains("月")), "A4 is grouped month: {:?}", first_row_label);

    // 4. Test Undo and Redo
    s.key("cmd-z");
    assert_eq!(pivot_of(&mut s, 0).fields[0].grouping, None, "grouping undone");
    s.key("cmd-shift-z");
    assert_eq!(pivot_of(&mut s, 0).fields[0].grouping, Some(PivotFieldGrouping::Date {
        by: DateGroupBy::Months,
        start: Some(46027.0),
        end: Some(46378.0),
        auto_start: true,
        auto_end: true,
    }), "grouping redone");

    // 5. Test numeric grouping: switch to field 5 (数量) on rows
    s.update(move |e, _, cx| e.change_pivot(target, |p| {
        p.place(0, None, 0);
        p.place(5, Some(PivotAxis::Row), 0);
    }, cx));
    s.click_cell("A4");
    s.menu("xlsx.pivot-group");
    s.update(|e, window, cx| {
        if let Some(p) = e.wb_mut().panels.pivot_group.as_mut() {
            p.is_date = false;
            p.num_start.update(cx, |i, cx| i.set_value("10", window, cx));
            p.num_end.update(cx, |i, cx| i.set_value("50", window, cx));
            p.num_interval.update(cx, |i, cx| i.set_value("10", window, cx));
            p.num_auto_start = false;
            p.num_auto_end = false;
        }
        e.apply_pivot_group(cx);
    });

    let p = pivot_of(&mut s, 0);
    assert_eq!(p.fields[5].grouping, Some(PivotFieldGrouping::Number {
        start: 10.0,
        end: 50.0,
        interval: 10.0,
        auto_start: false,
        auto_end: false,
    }));

    // Verify row items have ranges like "10-19" or "<10"
    let row_val = s.value("A4");
    assert!(matches!(row_val, CellValue::Text(ref t) if t.contains("<10") || t.contains("10-19")), "A4 is numeric range: {:?}", row_val);

    // 6. Test Ungroup via menu "xlsx.pivot-ungroup"
    s.click_cell("A4");
    s.menu("xlsx.pivot-ungroup");
    assert_eq!(pivot_of(&mut s, 0).fields[5].grouping, None, "ungrouped via menu");

    // 7. Re-apply date grouping on field 0, save and reopen
    s.update(move |e, _, cx| e.change_pivot(target, |p| {
        p.place(5, None, 0);
        p.place(0, Some(PivotAxis::Row), 0);
        p.group_field(0, PivotFieldGrouping::Date {
            by: DateGroupBy::Months,
            start: Some(46027.0),
            end: Some(46378.0),
            auto_start: true,
            auto_end: true,
        });
    }, cx));

    s.screenshot("pivot009_grouped");
    let saved = s.save_as("pivot009.xlsx");
    let data = std::fs::read(&saved).unwrap();
    let cache = ibis_pptx::test_files::part_text(&data, "xl/pivotCache/pivotCacheDefinition1.xml").expect("cache xml");
    assert!(cache.contains(r#"<fieldGroup><rangePr groupBy="months""#), "cache has fieldGroup: {cache}");

    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    let p = &sh.pivots[0];
    assert_eq!(p.fields[0].grouping, Some(PivotFieldGrouping::Date {
        by: DateGroupBy::Months,
        start: Some(46027.0),
        end: Some(46378.0),
        auto_start: true,
        auto_end: true,
    }), "reopened preserves date grouping");
}

/// XLSX-PIVOT-010: ピボットテーブル詳細設定（PivotTable Options）
fn pivot010_options() {
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");

    // 1. Check outside pivot table: clicking Z10 and invoking menu "xlsx.pivot-options" should refuse with status
    s.click_cell("Z10");
    s.menu("xlsx.pivot-options");
    assert!(s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot_options.as_ref()).is_none()), "panel not opened outside pivot");
    assert!(s.with(|e, _| e.status.as_ref().map_or(false, |msg| msg.contains("ピボットテーブルの中のセル"))), "status message shown");

    // 2. Select cell within pivot table (A4) and open options panel
    s.click_cell("A4");
    s.menu("xlsx.pivot-options");
    assert!(s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot_options.as_ref()).is_some()), "pivot options panel opened");

    // Check initial values
    let orig_name = pivot_of(&mut s, 0).name.clone();
    assert!(!orig_name.is_empty());

    // 3. Configure options in panel
    s.update(|e, window, cx| {
        if let Some(p) = e.wb_mut().panels.pivot_options.as_mut() {
            p.name.update(cx, |input, cx| input.set_value("売上サマリー".to_string(), window, cx));
            p.merge_item = true;
            p.auto_fit_columns = false;
            p.preserve_formatting = false;
            p.multiple_field_filters = true;
            p.row_grand_totals = false;
            p.col_grand_totals = false;
            p.missing_caption_enabled = true;
            p.missing_caption.update(cx, |input, cx| input.set_value("(なし)".to_string(), window, cx));
            p.error_caption_enabled = true;
            p.error_caption.update(cx, |input, cx| input.set_value("#ERR".to_string(), window, cx));
        }
        e.apply_pivot_options(cx);
    });

    // 4. Verify applied options in pivot definition
    let p = pivot_of(&mut s, 0);
    assert_eq!(p.name, "売上サマリー");
    assert!(p.merge_item);
    assert!(!p.auto_fit_columns);
    assert!(!p.preserve_formatting);
    assert!(p.multiple_field_filters);
    assert!(!p.row_grand_totals);
    assert!(!p.col_grand_totals);
    assert_eq!(p.missing_caption.as_deref(), Some("(なし)"));
    assert_eq!(p.error_caption.as_deref(), Some("#ERR"));

    // 5. Test Undo and Redo
    s.key("cmd-z");
    let undone = pivot_of(&mut s, 0);
    assert_eq!(undone.name, orig_name);
    assert!(!undone.merge_item);
    assert!(undone.auto_fit_columns);
    assert!(undone.preserve_formatting);
    assert!(!undone.multiple_field_filters);
    assert!(undone.row_grand_totals);
    assert!(undone.col_grand_totals);

    s.key("cmd-shift-z");
    let redone = pivot_of(&mut s, 0);
    assert_eq!(redone.name, "売上サマリー");
    assert!(redone.merge_item);
    assert!(!redone.auto_fit_columns);
    assert!(!redone.preserve_formatting);
    assert!(redone.multiple_field_filters);
    assert!(!redone.row_grand_totals);
    assert!(!redone.col_grand_totals);
    assert_eq!(redone.missing_caption.as_deref(), Some("(なし)"));
    assert_eq!(redone.error_caption.as_deref(), Some("#ERR"));

    // 6. Save and verify OOXML output
    s.screenshot("pivot010_options");
    let saved = s.save_as("pivot010.xlsx");
    let data = std::fs::read(&saved).unwrap();
    let pivot_xml = ibis_pptx::test_files::part_text(&data, "xl/pivotTables/pivotTable1.xml").expect("pivot table xml");
    assert!(pivot_xml.contains(r#"name="売上サマリー""#), "xml has name: {pivot_xml}");
    assert!(pivot_xml.contains(r#"mergeItem="1""#), "xml has mergeItem: {pivot_xml}");
    assert!(pivot_xml.contains(r#"applyWidthHeightFormats="0""#), "xml has applyWidthHeightFormats 0: {pivot_xml}");
    assert!(pivot_xml.contains(r#"useAutoFormatting="0""#), "xml has useAutoFormatting 0: {pivot_xml}");
    assert!(pivot_xml.contains(r#"multipleFieldFilters="1""#), "xml has multipleFieldFilters 1: {pivot_xml}");
    assert!(pivot_xml.contains(r#"rowGrandTotals="0""#), "xml has rowGrandTotals 0: {pivot_xml}");
    assert!(pivot_xml.contains(r#"colGrandTotals="0""#), "xml has colGrandTotals 0: {pivot_xml}");
    assert!(pivot_xml.contains(r#"missingCaption="(なし)""#), "xml has missingCaption: {pivot_xml}");
    assert!(pivot_xml.contains(r##"errorCaption="#ERR""##), "xml has errorCaption: {pivot_xml}");

    // 7. Reopen and verify restored properties in model
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    let p2 = &sh.pivots[0];
    assert_eq!(p2.name, "売上サマリー");
    assert!(p2.merge_item);
    assert!(!p2.auto_fit_columns);
    assert!(!p2.preserve_formatting);
    assert!(p2.multiple_field_filters);
    assert!(!p2.row_grand_totals);
    assert!(!p2.col_grand_totals);
    assert_eq!(p2.missing_caption.as_deref(), Some("(なし)"));
    assert_eq!(p2.error_caption.as_deref(), Some("#ERR"));
}

/// XLSX-WHATIF-006: シナリオの統合（Merge Scenarios）
fn whatif006_merge_scenarios() {
    let mut s = new_sheet();
    let sheet1_id = s.with(|e, _| e.current_slide);
    let sheet1_name = s.with(|e, _| e.slide().name.clone()).unwrap();

    // 1. Setup Sheet 1 and register scenario "ベース"
    s.click_cell("B1");
    s.enter("100", "enter");
    s.click_cell("B1");
    s.menu("xlsx.scenario-manager");
    scenario_manager_click(&mut s, "scenario-add");
    scenario_manager_field(&mut s, "scenario-name", "ベース");
    scenario_manager_field(&mut s, "scenario-val-B1", "100");
    scenario_manager_click(&mut s, "scenario-save");

    let count1 = s.with(|e, _| {
        let sl = e.presentation().slides.iter().find(|sl| sl.name.as_ref() == Some(&sheet1_name))?;
        sheet_of_slide(sl)?.scenarios.as_ref().map(|sc| sc.scenarios.len())
    });
    assert_eq!(count1, Some(1));
    scenario_manager_click(&mut s, "scenario-close");

    // 2. Insert Sheet 2 and register scenarios "高成長", "低成長"
    s.update(|e, window, cx| e.insert_sheet(window, cx));
    let sheet2_id = s.with(|e, _| e.current_slide);
    let sheet2_name = s.with(|e, _| e.slide().name.clone()).unwrap();
    assert_ne!(sheet1_id, sheet2_id);

    s.click_cell("B1");
    s.enter("150", "enter");
    s.click_cell("B1");
    s.menu("xlsx.scenario-manager");

    // Add "高成長"
    scenario_manager_click(&mut s, "scenario-add");
    scenario_manager_field(&mut s, "scenario-name", "高成長");
    scenario_manager_field(&mut s, "scenario-val-B1", "200");
    scenario_manager_click(&mut s, "scenario-save");

    // Add "低成長"
    scenario_manager_click(&mut s, "scenario-add");
    scenario_manager_field(&mut s, "scenario-name", "低成長");
    scenario_manager_field(&mut s, "scenario-val-B1", "50");
    scenario_manager_click(&mut s, "scenario-save");

    let count2 = s.with(|e, _| {
        let sl = e.presentation().slides.iter().find(|sl| sl.name.as_ref() == Some(&sheet2_name))?;
        sheet_of_slide(sl)?.scenarios.as_ref().map(|sc| sc.scenarios.len())
    });
    assert_eq!(count2, Some(2));
    scenario_manager_click(&mut s, "scenario-close");

    // 3. Switch back to Sheet 1
    s.update(|e, window, cx| e.select_slide(sheet1_id, window, cx));
    assert_eq!(s.with(|e, _| e.current_slide), sheet1_id);

    // 4. Open Scenario Manager and click "統合…"
    s.menu("xlsx.scenario-manager");
    scenario_manager_click(&mut s, "scenario-merge");

    // Verify in Merge mode
    assert!(s.with(|e, _| matches!(e.wb().panels.scenario_manager.as_ref().map(|p| p.mode), Some(super::scenario_manager::ScenarioDialogMode::Merge))));

    // 5. Select Sheet 2 and click OK
    let btn_id = format!("scenario-merge-sheet-{}", sheet2_id.0);
    scenario_manager_click(&mut s, &btn_id);
    scenario_manager_click(&mut s, "scenario-merge-ok");

    // 6. Verify back in List mode and Sheet 1 has all 3 scenarios
    assert!(s.with(|e, _| matches!(e.wb().panels.scenario_manager.as_ref().map(|p| p.mode), Some(super::scenario_manager::ScenarioDialogMode::List))));
    let sc_names: Vec<String> = s.with(|e, _| {
        let sl = e.presentation().slides.iter().find(|sl| sl.name.as_ref() == Some(&sheet1_name))?;
        sheet_of_slide(sl)?.scenarios.as_ref().map(|sc| sc.scenarios.iter().map(|sc| sc.name.clone()).collect())
    }).unwrap_or_default();
    assert_eq!(sc_names, vec!["ベース", "高成長", "低成長"]);

    // 7. Show scenario "高成長" (item 1) and verify cell B1 updated
    scenario_manager_click(&mut s, "scenario-item-1");
    scenario_manager_click(&mut s, "scenario-show");
    assert_eq!(s.value("B1"), CellValue::Number(200.0));

    // 8. Test Undo and Redo
    // Undo showing scenario -> B1 becomes 100
    s.click_cell("E1");
    s.key("cmd-z");
    assert_eq!(s.value("B1"), CellValue::Number(100.0));

    // Undo merge scenarios -> Sheet 1 scenarios reverted to 1 ("ベース")
    s.key("cmd-z");
    let sc_after_undo: Vec<String> = s.with(|e, _| {
        let sl = e.presentation().slides.iter().find(|sl| sl.name.as_ref() == Some(&sheet1_name))?;
        sheet_of_slide(sl)?.scenarios.as_ref().map(|sc| sc.scenarios.iter().map(|sc| sc.name.clone()).collect())
    }).unwrap_or_default();
    assert_eq!(sc_after_undo, vec!["ベース"]);

    // Redo merge scenarios -> Sheet 1 scenarios restored to 3
    s.key("cmd-shift-z");
    let sc_after_redo: Vec<String> = s.with(|e, _| {
        let sl = e.presentation().slides.iter().find(|sl| sl.name.as_ref() == Some(&sheet1_name))?;
        sheet_of_slide(sl)?.scenarios.as_ref().map(|sc| sc.scenarios.iter().map(|sc| sc.name.clone()).collect())
    }).unwrap_or_default();
    assert_eq!(sc_after_redo, vec!["ベース", "高成長", "低成長"]);

    // 9. Save and verify OOXML persistence
    s.screenshot("whatif006_merged");
    let _saved = s.save_as("whatif006.xlsx");
    let reopened = s.reopen();
    let sl_reopened = reopened.slides.iter().find(|sl| sl.name.as_ref() == Some(&sheet1_name)).unwrap();
    let sh_reopened = sheet_of_slide(sl_reopened).unwrap();
    let re_names: Vec<String> = sh_reopened.scenarios.as_ref().unwrap().scenarios.iter().map(|sc| sc.name.clone()).collect();
    assert_eq!(re_names, vec!["ベース", "高成長", "低成長"]);
}

/// XLSX-PIVOT-019: 詳細データの展開（Show Details / Drill Down）
fn pivot019_show_details() {
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");

    // 1. Outside pivot table: invoking "xlsx.pivot-show-details" refuses with status message
    s.click_cell("H5");
    s.menu("xlsx.pivot-show-details");
    assert!(s.with(|e, _| e.status.as_ref().map_or(false, |msg| msg.contains("ピボットテーブルのデータセル"))), "refusal status shown");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 1, "no new sheet created");

    // 2. Select data cell B4 (北海道 りんご 第1四半期: 360) and show details via menu
    s.click_cell("B4");
    s.menu("xlsx.pivot-show-details");

    // A new sheet "展開1" is created and selected
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 2, "drill-down sheet added");
    assert_eq!(s.with(|e, _| e.slide().name.clone()), Some("展開1".into()), "switched to new sheet");

    // Check headings at row 0 (A1..G1)
    assert_eq!(s.value("A1"), CellValue::Text("日付".into()));
    assert_eq!(s.value("B1"), CellValue::Text("四半期".into()));
    assert_eq!(s.value("C1"), CellValue::Text("地域".into()));
    assert_eq!(s.value("D1"), CellValue::Text("商品".into()));
    assert_eq!(s.value("E1"), CellValue::Text("担当".into()));
    assert_eq!(s.value("F1"), CellValue::Text("数量".into()));
    assert_eq!(s.value("G1"), CellValue::Text("金額".into()));

    // Check record at row 1 (A2..G2)
    assert_eq!(s.value("A2"), CellValue::Number(46027.0));
    assert_eq!(s.value("B2"), CellValue::Text("第1四半期".into()));
    assert_eq!(s.value("C2"), CellValue::Text("北海道".into()));
    assert_eq!(s.value("D2"), CellValue::Text("りんご".into()));
    assert_eq!(s.value("E2"), CellValue::Text("佐藤".into()));
    assert_eq!(s.value("F2"), CellValue::Number(3.0));
    assert_eq!(s.value("G2"), CellValue::Number(99999.0));

    // 3. Test Undo: reverts to original sheet and removes drill-down sheet
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 1, "sheet removed by undo");

    // 4. Test Redo: restores drill-down sheet
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 2, "sheet restored by redo");

    // 5. Test Save and Reopen roundtrip
    s.screenshot("pivot019_details");
    let _saved = s.save_as("pivot019_details.xlsx");
    let reopened = s.reopen();
    assert_eq!(reopened.slides.len(), 2);
    let first_sheet = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert_eq!(first_sheet.cell(0, 0).map(|c| c.value.clone()), Some(CellValue::Text("日付".into())));
    assert_eq!(first_sheet.cell(1, 2).map(|c| c.value.clone()), Some(CellValue::Text("北海道".into())));
    assert_eq!(first_sheet.cell(1, 3).map(|c| c.value.clone()), Some(CellValue::Text("りんご".into())));
    assert_eq!(first_sheet.cell(1, 6).map(|c| c.value.clone()), Some(CellValue::Number(99999.0)));
}

/// XLSX-PIVOT-011: 計算フィールド（Calculated Field）
/// ピボットテーブルへの計算フィールド追加、数式計算、Undo/Redo、保存と開き直しラウンドトリップ。
fn pivot011_calculated_field() {
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");
    s.click_cell("A5");

    // Remove column field (四半期) so the table doesn't expand rightward and overlap with column J data
    s.update(|e, _, cx| {
        let target = (e.current_slide, 0);
        e.change_pivot(target, |p| p.place(1, None, 0), cx);
    });

    s.menu("xlsx.pivot-calculated-field");

    // 1. Verify calculated field panel is opened
    assert!(
        s.with(|e, _| e.sheet_state().map_or(false, |st| st.panels.pivot_calc_field.is_some())),
        "calculated field panel opened"
    );

    // 2. Add calculated field "消費税" with formula "= 金額 * 0.1"
    s.update(|e, window, cx| {
        if let Some(p) = e.wb_mut().panels.pivot_calc_field.as_mut() {
            p.name_input.update(cx, |input, cx| input.set_value("消費税", window, cx));
            p.formula_input.update(cx, |input, cx| input.set_value("= 金額 * 0.1", window, cx));
        }
        e.add_or_modify_calc_field(cx);
    });

    // Verify calculated field is registered in model
    let target = s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot_calc_field.as_ref().map(|p| p.target))).unwrap();
    let pivot = s.with(|e, _| e.pivot_def(target)).unwrap();
    let calcs = pivot.calculated_fields();
    assert_eq!(calcs.len(), 1, "has 1 calculated field");
    assert_eq!(calcs[0].1, "消費税");
    assert_eq!(calcs[0].2, "= 金額 * 0.1");

    // Check header row for 消費税 column and calculated values
    assert_eq!(s.value("B2"), CellValue::Text("合計 / 金額".into()));
    assert_eq!(s.value("C2"), CellValue::Text("合計 / 消費税".into()));
    assert_eq!(s.value("B5"), CellValue::Number(1600.0));
    assert_eq!(s.value("C5"), CellValue::Number(160.0));

    // 3. Test Undo
    s.key("cmd-z");
    let pivot_after_undo = s.with(|e, _| e.pivot_def(target)).unwrap();
    assert_eq!(pivot_after_undo.calculated_fields().len(), 0, "calculated field undone");

    // 4. Test Redo
    s.key("cmd-shift-z");
    let pivot_after_redo = s.with(|e, _| e.pivot_def(target)).unwrap();
    assert_eq!(pivot_after_redo.calculated_fields().len(), 1, "calculated field restored by redo");

    // 5. Test Save and Reopen roundtrip
    s.screenshot("pivot011_calculated_field");
    let _saved = s.save_as("pivot011_calculated_field.xlsx");
    let reopened = s.reopen();
    let sheet = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert!(!sheet.pivots.is_empty(), "has pivot table in reopened file");
    let reopened_calcs = sheet.pivots[0].calculated_fields();
    assert_eq!(reopened_calcs.len(), 1, "calculated field restored after save & reopen");
    assert_eq!(reopened_calcs[0].1, "消費税");
    assert_eq!(reopened_calcs[0].2, "金額 * 0.1");
}

/// XLSX-PIVOT-012: 計算アイテム（Calculated Item）
/// ピボットテーブルのフィールド内集計アイテムの追加、数式計算、行・列・総計の更新、Undo/Redo、保存と開き直しラウンドトリップ。
fn pivot012_calculated_item() {
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");
    s.click_cell("A5");

    // Remove column field (四半期) and secondary row field (商品) to simplify layout
    s.update(|e, _, cx| {
        let target = (e.current_slide, 0);
        e.change_pivot(target, |p| {
            p.place(1, None, 0);
            p.place(3, None, 0);
        }, cx);
    });

    s.menu("xlsx.pivot-calculated-item");

    // 1. Verify calculated item panel is opened
    assert!(
        s.with(|e, _| e.sheet_state().map_or(false, |st| st.panels.pivot_calc_item.is_some())),
        "calculated item panel opened"
    );

    // 2. Select field 2 (地域), add calculated item "北日本" with formula "= 北海道"
    s.update(|e, window, cx| {
        let list_idx = e
            .wb()
            .panels
            .pivot_calc_item
            .as_ref()
            .and_then(|p| p.fields.iter().position(|(_, name)| name == "地域"))
            .unwrap_or(0);
        e.select_calc_item_field(list_idx, cx);

        if let Some(p) = e.wb_mut().panels.pivot_calc_item.as_mut() {
            p.name_input.update(cx, |input, cx| input.set_value("北日本", window, cx));
            p.formula_input.update(cx, |input, cx| input.set_value("= 北海道", window, cx));
        }
        e.add_or_modify_calc_item(cx);
    });

    // Verify calculated item is registered in model
    let target = s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot_calc_item.as_ref().map(|p| p.target))).unwrap();
    let pivot = s.with(|e, _| e.pivot_def(target)).unwrap();
    let items = pivot.calculated_items_for_field(2);
    assert_eq!(items.len(), 1, "has 1 calculated item");
    assert_eq!(items[0].0, "北日本");
    assert_eq!(items[0].1, "= 北海道");

    // Verify "北日本" appears in pivot table and has same amount as "北海道"
    let mut hokkaido_row = None;
    let mut kitanihon_row = None;
    for r in 1..=20 {
        let a_val = s.value(&format!("A{r}"));
        if a_val == CellValue::Text("北海道".into()) {
            hokkaido_row = Some(r);
        } else if a_val == CellValue::Text("北日本".into()) {
            kitanihon_row = Some(r);
        }
    }
    assert!(hokkaido_row.is_some(), "hokkaido row found");
    assert!(kitanihon_row.is_some(), "kitanihon row found");

    let h_val = s.value(&format!("B{}", hokkaido_row.unwrap()));
    let k_val = s.value(&format!("B{}", kitanihon_row.unwrap()));
    assert_eq!(k_val, h_val, "kitanihon amount equals hokkaido amount");

    // 3. Test Undo
    s.key("cmd-z");
    let pivot_after_undo = s.with(|e, _| e.pivot_def(target)).unwrap();
    assert_eq!(pivot_after_undo.calculated_items_for_field(2).len(), 0, "calculated item undone");

    // 4. Test Redo
    s.key("cmd-shift-z");
    let pivot_after_redo = s.with(|e, _| e.pivot_def(target)).unwrap();
    assert_eq!(pivot_after_redo.calculated_items_for_field(2).len(), 1, "calculated item restored by redo");

    // 5. Test Save and Reopen roundtrip
    s.screenshot("pivot012_calculated_item");
    let _saved = s.save_as("pivot012_calculated_item.xlsx");
    let reopened = s.reopen();
    let sheet = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert!(!sheet.pivots.is_empty(), "has pivot table in reopened file");
    let reopened_items = sheet.pivots[0].calculated_items_for_field(2);
    assert_eq!(reopened_items.len(), 1, "calculated item restored after save & reopen");
    assert_eq!(reopened_items[0].0, "北日本");
    assert_eq!(reopened_items[0].1, "北海道");
}

/// XLSX-PIVOT-010: 手動グループ化（Manual Grouping）
/// 文字列アイテム（関西・九州）の複数選択からのグループ化、グループ1への合算、Undo/Redo、グループ解除、保存・再読込ラウンドトリップ。
fn pivot010_manual_grouping() {
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");
    s.click_cell("A5");

    // Simplify layout: 地域 as single row field
    s.update(|e, _, cx| {
        let target = (e.current_slide, 0);
        e.change_pivot(target, |p| {
            p.place(1, None, 0);
            p.place(3, None, 0);
        }, cx);
    });

    // Find "関西" and "九州" rows
    let mut kansai_row = None;
    let mut kyushu_row = None;
    for r in 1..=20 {
        let v = s.value(&format!("A{r}"));
        if v == CellValue::Text("関西".into()) { kansai_row = Some(r); }
        else if v == CellValue::Text("九州".into()) { kyushu_row = Some(r); }
    }
    let k_row = kansai_row.expect("kansai row");
    let q_row = kyushu_row.expect("kyushu row");

    let k_val = s.value(&format!("B{k_row}"));
    let q_val = s.value(&format!("B{q_row}"));
    let expected_sum = match (k_val, q_val) {
        (CellValue::Number(a), CellValue::Number(b)) => a + b,
        _ => panic!("values must be numbers"),
    };

    // Select 関西, then Cmd-click 九州
    s.click_cell(&format!("A{k_row}"));
    let q_center = s.with(|e, _| e.cell_center(0, q_row as u32 - 1)).unwrap();
    s.dispatch_click_with((f32::from(q_center.x), f32::from(q_center.y)), gpui_kit::Modifiers { platform: true, ..Default::default() });

    // Group
    s.menu("xlsx.pivot-group");

    // Verify "グループ1" row exists and has expected sum
    let mut g1_row = None;
    for r in 1..=20 {
        if s.value(&format!("A{r}")) == CellValue::Text("グループ1".into()) {
            g1_row = Some(r);
            break;
        }
    }
    let g_r = g1_row.expect("グループ1 row exists");
    assert_eq!(s.value(&format!("B{g_r}")), CellValue::Number(expected_sum));

    // Undo
    s.key("cmd-z");
    assert!(s.value(&format!("A{g_r}")) != CellValue::Text("グループ1".into()));

    // Redo
    s.key("cmd-shift-z");
    assert_eq!(s.value(&format!("A{g_r}")), CellValue::Text("グループ1".into()));

    // Ungroup
    s.click_cell(&format!("A{g_r}"));
    s.menu("xlsx.pivot-ungroup");
    assert!(s.value(&format!("A{g_r}")) != CellValue::Text("グループ1".into()));

    // Re-group, save, and reopen
    s.click_cell(&format!("A{k_row}"));
    let q_center2 = s.with(|e, _| e.cell_center(0, q_row as u32 - 1)).unwrap();
    s.dispatch_click_with((f32::from(q_center2.x), f32::from(q_center2.y)), gpui_kit::Modifiers { platform: true, ..Default::default() });
    s.menu("xlsx.pivot-group");

    s.screenshot("pivot010_manual_grouping");
    let _saved = s.save_as("pivot010_manual_grouping.xlsx");
    let reopened = s.reopen();
    let sheet = sheet_of_slide(&reopened.slides[0]).unwrap();
    let p2 = &sheet.pivots[0];
    let groups = p2.manual_groups_for_field(2).expect("has manual groups after reopen");
    assert_eq!(groups.get("関西").map(String::as_str), Some("グループ1"));
    assert_eq!(groups.get("九州").map(String::as_str), Some("グループ1"));
}

/// XLSX-PIVOT-013: 値による並べ替え（Sort by Value / Data Field）
/// ピボットテーブルの行フィールド（地域）を集計値（合計 / 金額）で降順・昇順に並べ替え、Undo/Redo、保存・再読込ラウンドトリップ。
fn pivot013_sort_by_value() {
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");
    s.click_cell("A5");

    // Simplify layout: 地域 as single row field
    s.update(|e, _, cx| {
        let target = (e.current_slide, 0);
        e.change_pivot(target, |p| {
            p.place(1, None, 0);
            p.place(3, None, 0);
        }, cx);
    });

    // Collect initial row labels and values
    let mut initial_items = Vec::new();
    for r in 1..=20 {
        let a = s.value(&format!("A{r}"));
        let b = s.value(&format!("B{r}"));
        if let (CellValue::Text(name), CellValue::Number(val)) = (&a, &b) {
            if name != "行ラベル" && name != "総計" {
                initial_items.push((name.clone(), *val));
            }
        }
    }
    assert!(initial_items.len() >= 3, "has multiple regions");

    // 1. Sort by dataField 0 (金額) descending
    s.update(|e, window, cx| {
        let target = (e.current_slide, 0);
        let pt = gpui_kit::Point { x: gpui_kit::px(100.0), y: gpui_kit::px(100.0) };
        e.open_pivot_popup(target, ibis_doc_model::pivot::PivotButton::Rows, pt, window, cx);
        e.pivot_popup_sort_by_value(0, false, cx);
    });

    // Verify descending order: each item's value >= next item's value
    let mut desc_items = Vec::new();
    for r in 1..=20 {
        let a = s.value(&format!("A{r}"));
        let b = s.value(&format!("B{r}"));
        if let (CellValue::Text(name), CellValue::Number(val)) = (&a, &b) {
            if name != "行ラベル" && name != "総計" {
                desc_items.push((name.clone(), *val));
            }
        }
    }
    assert_eq!(desc_items.len(), initial_items.len());
    for w in desc_items.windows(2) {
        assert!(w[0].1 >= w[1].1, "descending: {} ({}) >= {} ({})", w[0].0, w[0].1, w[1].0, w[1].1);
    }

    // 2. Undo
    s.key("cmd-z");
    let mut undo_items = Vec::new();
    for r in 1..=20 {
        let a = s.value(&format!("A{r}"));
        let b = s.value(&format!("B{r}"));
        if let (CellValue::Text(name), CellValue::Number(val)) = (&a, &b) {
            if name != "行ラベル" && name != "総計" {
                undo_items.push((name.clone(), *val));
            }
        }
    }
    assert_eq!(undo_items, initial_items, "undone to initial order");

    // 3. Redo
    s.key("cmd-shift-z");
    let mut redo_items = Vec::new();
    for r in 1..=20 {
        let a = s.value(&format!("A{r}"));
        let b = s.value(&format!("B{r}"));
        if let (CellValue::Text(name), CellValue::Number(val)) = (&a, &b) {
            if name != "行ラベル" && name != "総計" {
                redo_items.push((name.clone(), *val));
            }
        }
    }
    assert_eq!(redo_items, desc_items, "redone to descending order");

    // 4. Sort by dataField 0 (金額) ascending
    s.update(|e, window, cx| {
        let target = (e.current_slide, 0);
        let pt = gpui_kit::Point { x: gpui_kit::px(100.0), y: gpui_kit::px(100.0) };
        e.open_pivot_popup(target, ibis_doc_model::pivot::PivotButton::Rows, pt, window, cx);
        e.pivot_popup_sort_by_value(0, true, cx);
    });

    let mut asc_items = Vec::new();
    for r in 1..=20 {
        let a = s.value(&format!("A{r}"));
        let b = s.value(&format!("B{r}"));
        if let (CellValue::Text(name), CellValue::Number(val)) = (&a, &b) {
            if name != "行ラベル" && name != "総計" {
                asc_items.push((name.clone(), *val));
            }
        }
    }
    assert_eq!(asc_items.len(), initial_items.len());
    for w in asc_items.windows(2) {
        assert!(w[0].1 <= w[1].1, "ascending: {} ({}) <= {} ({})", w[0].0, w[0].1, w[1].0, w[1].1);
    }

    // 5. Screenshot, Save and Reopen roundtrip
    s.screenshot("pivot013_sort_by_value");
    let _saved = s.save_as("pivot013_sort_by_value.xlsx");
    let reopened = s.reopen();
    let sheet = sheet_of_slide(&reopened.slides[0]).unwrap();
    let p2 = &sheet.pivots[0];
    assert_eq!(p2.fields[2].sort, ibis_doc_model::PivotSort::Ascending);
    assert_eq!(p2.fields[2].sort_data_field, Some(0));
}

/// XLSX-PIVOT-001・018: ピボットテーブルの元データにテーブル名・定義された名前を使用。
/// テーブル名「テーブル1」および名前付き範囲「売上データ」からピボットテーブルを作成し、
/// フィールド配置・集計値計算・Undo/Redo・保存（worksheetSource name="...")・開き直しラウンドトリップを検証。
fn pivot001_table_source() {
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");

    // 1. データ範囲 J1:P41 にテーブル「テーブル1」を作成
    s.click_cell("K5");
    s.update(|e, _, cx| {
        let range = ibis_doc_model::CellRange { first_row: 0, last_row: 40, first_column: 9, last_column: 15 };
        e.make_table(range, true, cx).unwrap();
    });

    // ワークブックに定義された名前「売上データ」も追加
    s.update(|e, _, _| {
        let mut wb = e.presentation().workbook.as_deref().cloned().unwrap_or_default();
        wb.names.push(ibis_doc_model::DefinedName {
            name: "売上データ".to_string(),
            sheet: None,
            formula: "ピボット!$J$1:$P$41".to_string(),
            hidden: false,
        });
        e.document.ids().workbook = Some(std::sync::Arc::new(wb));
    });

    // 2. テーブル名「テーブル1」からピボットテーブル作成
    s.menu("xlsx.pivot-insert");
    s.update(|e, window, cx| {
        if let Some(c) = e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).and_then(|p| p.create.as_ref()) {
            c.source.update(cx, |inp, cx| inp.set_value("テーブル1".to_string(), window, cx));
        }
    });
    s.update(|e, window, cx| e.create_pivot(window, cx));

    assert_eq!(s.with(|e, _| e.slide().name.clone()).as_deref(), Some("Sheet2"), "a new sheet created");
    let p = pivot_of(&mut s, 0);
    assert_eq!(p.source.name.as_deref(), Some("テーブル1"), "source name is テーブル1");

    let target = s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).and_then(|p| p.target)).expect("fields shown");
    s.update(move |e, _, cx| e.toggle_pivot_field(target, 2, cx)); // 地域
    s.update(move |e, _, cx| e.toggle_pivot_field(target, 6, cx)); // 金額

    assert_eq!(s.value("A3"), CellValue::Text("行ラベル".into()));
    assert_eq!(s.value("B3"), CellValue::Text("合計 / 金額".into()));
    assert_eq!(s.value("A9"), CellValue::Text("総計".into()));
    assert_eq!(s.value("B9"), CellValue::Number(151819.0));

    // 3. Undo / Redo 検証
    s.key("cmd-z"); // 金額解除
    assert_eq!(s.value("B3"), CellValue::Text("".into()));
    s.key("cmd-z"); // 地域解除
    assert_eq!(s.value("A3"), CellValue::Empty);
    s.key("cmd-shift-z"); // 地域復元
    s.key("cmd-shift-z"); // 金額復元
    assert_eq!(s.value("B9"), CellValue::Number(151819.0));

    // 4. 保存と XML 検証
    s.screenshot("pivot001_table_source");
    let saved = s.save_as("pivot001_table_source.xlsx");
    let data = std::fs::read(&saved).unwrap();
    let cache_xml = ibis_pptx::test_files::part_text(&data, "xl/pivotCache/pivotCacheDefinition2.xml").expect("pivot cache xml exists");
    assert!(cache_xml.contains(r#"name="テーブル1""#), "cache xml has table name: {cache_xml}");
    assert!(!cache_xml.contains(r#"ref=""#), "cache xml has no ref attribute when name is set: {cache_xml}");

    // 5. 開き直し検証
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert_eq!(sh.pivots.len(), 1);
    assert_eq!(sh.pivots[0].source.name.as_deref(), Some("テーブル1"));
    assert_eq!(sh.cell(8, 1).map(|c| c.value.clone()), Some(CellValue::Number(151819.0)), "reopened B9 has total value");

    // 6. 定義された名前「売上データ」からのピボット作成も検証
    let pivot_slide = s.with(|e, _| e.presentation().slides.iter().find(|sl| sl.name.as_deref() == Some("ピボット")).map(|sl| sl.id)).unwrap();
    s.update(|e, window, cx| e.select_slide(pivot_slide, window, cx));
    s.menu("xlsx.pivot-insert");
    s.update(|e, window, cx| {
        if let Some(c) = e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).and_then(|p| p.create.as_ref()) {
            c.source.update(cx, |inp, cx| inp.set_value("売上データ".to_string(), window, cx));
        }
    });
    s.update(|e, window, cx| e.create_pivot(window, cx));
    let p_dn = pivot_of(&mut s, 0);
    assert_eq!(p_dn.source.name.as_deref(), Some("売上データ"), "source name is 売上データ");

    let target_dn = s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).and_then(|p| p.target)).expect("fields shown");
    s.update(move |e, _, cx| e.toggle_pivot_field(target_dn, 2, cx));
    s.update(move |e, _, cx| e.toggle_pivot_field(target_dn, 6, cx));
    assert_eq!(s.value("B9"), CellValue::Number(151819.0));
}

/// XLSX-CELL-003: 15-digit precision rule for long numbers, zero-padding beyond 15 digits,
/// preserving quoted text strings, cascade recalculation, OOXML save and reopen roundtrip, Undo/Redo.
fn cell003_long_numbers_precision() {
    let mut s = new_sheet();
    let current_sheet_name = s.with(|e, _| e.slide().name.clone().unwrap_or_default());

    // A1: 15桁整数 -> そのまま保持
    s.click_cell("A1");
    s.enter("123456789012345", "enter");
    assert_eq!(s.value("A1"), CellValue::Number(123456789012345.0));

    // A2: 16桁整数 -> 16桁目が0になる
    s.enter("1234567890123456", "enter");
    assert_eq!(s.value("A2"), CellValue::Number(1234567890123450.0));

    // A3: 19桁整数 -> 16〜19桁目が0になる
    s.enter("1234567890123456789", "enter");
    assert_eq!(s.value("A3"), CellValue::Number(1234567890123450000.0));

    // A4: 16桁小数 -> 15桁に切り捨て
    s.enter("1.234567890123456", "enter");
    assert_eq!(s.value("A4"), CellValue::Number(1.23456789012345));

    // A5: クォート付き文字列 -> 全桁保持
    s.enter("'1234567890123456789", "enter");
    assert_eq!(s.value("A5"), CellValue::Text("1234567890123456789".into()));

    // 数式セル B1, B2
    s.click_cell("B1");
    s.enter("=A1+1", "enter");
    assert_eq!(s.value("B1"), CellValue::Number(123456789012346.0));

    s.click_cell("B2");
    s.enter("=A2+1", "enter");
    assert_eq!(s.value("B2"), CellValue::Number(1234567890123451.0));

    // A2 の更新によるカスケード再計算と Undo / Redo
    s.click_cell("A2");
    s.enter("9876543210987654", "enter");
    assert_eq!(s.value("A2"), CellValue::Number(9876543210987650.0));
    assert_eq!(s.value("B2"), CellValue::Number(9876543210987651.0));

    s.key("cmd-z");
    assert_eq!(s.value("A2"), CellValue::Number(1234567890123450.0));
    assert_eq!(s.value("B2"), CellValue::Number(1234567890123451.0));

    s.key("cmd-shift-z");
    assert_eq!(s.value("A2"), CellValue::Number(9876543210987650.0));
    assert_eq!(s.value("B2"), CellValue::Number(9876543210987651.0));

    // スクリーンショット & 保存
    s.screenshot("cell003_long_numbers_precision");
    let path = s.save_as("cell003_long_numbers_precision.xlsx");

    // 保存ファイルの XML 検証
    let data = std::fs::read(&path).unwrap();
    let has_rounded = (1..=10).any(|i| {
        ibis_pptx::test_files::part_text(&data, &format!("xl/worksheets/sheet{i}.xml"))
            .is_some_and(|xml| xml.contains("9876543210987650"))
    });
    assert!(has_rounded, "saved sheet XML contains 15-digit rounded number 9876543210987650");

    // 開き直し検証
    let reopened = s.reopen();
    let target_slide = reopened.slides.iter().find(|sl| sl.name.as_deref() == Some(&current_sheet_name)).expect("sheet exists");
    let sh = sheet_of_slide(target_slide).unwrap();
    assert_eq!(sh.cell(0, 0).map(|c| c.value.clone()), Some(CellValue::Number(123456789012345.0)));
    assert_eq!(sh.cell(1, 0).map(|c| c.value.clone()), Some(CellValue::Number(9876543210987650.0)));
    assert_eq!(sh.cell(2, 0).map(|c| c.value.clone()), Some(CellValue::Number(1234567890123450000.0)));
    assert_eq!(sh.cell(3, 0).map(|c| c.value.clone()), Some(CellValue::Number(1.23456789012345)));
    assert_eq!(sh.cell(4, 0).map(|c| c.value.clone()), Some(CellValue::Text("1234567890123456789".into())));
    assert_eq!(sh.cell(0, 1).map(|c| c.value.clone()), Some(CellValue::Number(123456789012346.0)));
    assert_eq!(sh.cell(1, 1).map(|c| c.value.clone()), Some(CellValue::Number(9876543210987651.0)));
}

/// XLSX-PIVOT-010 / XLSX-PIVOT-021: ピボットテーブル オプション（総計・行/列ラベルのカスタム表示名、更新設定）
/// および行・列フィールド設定（小計の有無、グループ先頭/末尾配置、レイアウト形式）の設定・反映・Undo/Redo・保存再オープン検証。
fn pivot010_options_and_field_settings() {
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");

    // 1. セル A4 を選択してピボットテーブル オプションを開く
    s.click_cell("A4");
    s.menu("xlsx.pivot-options");
    assert!(s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot_options.as_ref()).is_some()), "pivot options panel opened");

    // 2. カスタムキャプション（総計・行ラベル）およびデータ更新フラグを設定して適用
    s.update(|e, window, cx| {
        if let Some(p) = e.wb_mut().panels.pivot_options.as_mut() {
            p.grand_total_caption_enabled = true;
            p.grand_total_caption.update(cx, |input, cx| input.set_value("総合計".to_string(), window, cx));
            p.row_header_caption_enabled = true;
            p.row_header_caption.update(cx, |input, cx| input.set_value("区分".to_string(), window, cx));
            p.refresh_on_load = true;
        }
        e.apply_pivot_options(cx);
    });

    // モデルおよび画面セルのキャプション反映を検証
    let p = pivot_of(&mut s, 0);
    assert_eq!(p.grand_total_caption.as_deref(), Some("総合計"));
    assert_eq!(p.row_header_caption.as_deref(), Some("区分"));
    assert!(p.refresh_on_load);

    // 3. フィールド設定（小計とレイアウト）の操作
    // ピボットフィールドパネルを開く
    s.menu("xlsx.pivot-fields");
    assert!(s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).is_some()));

    // 行フィールド 0 の「設定」をクリックして詳細設定エリアを展開
    s.update(|e, _, cx| {
        if let Some(pnl) = e.wb_mut().panels.pivot.as_mut() {
            pnl.field_selected = Some((ibis_doc_model::PivotAxis::Row, 0));
        }
        cx.notify();
    });

    // 行フィールド 0 の小計をオフにし、レイアウトを表形式に変更
    let target = s.with(|e, _| e.sheet_state().and_then(|st| st.panels.pivot.as_ref()).and_then(|p| p.target)).expect("target exists");
    s.update(move |e, _, cx| {
        e.change_pivot(target, |p| {
            if let Some(fld) = p.fields.get_mut(0) {
                fld.default_subtotal = false;
                fld.compact = false;
                fld.outline = false; // 表形式
            }
        }, cx);
    });

    let p_after = pivot_of(&mut s, 0);
    assert!(!p_after.fields[0].default_subtotal, "subtotal disabled for field 0");
    assert!(!p_after.fields[0].compact, "compact disabled for field 0");
    assert!(!p_after.fields[0].outline, "outline disabled for field 0");

    // 4. Undo / Redo 検証
    s.key("cmd-z");
    let p_undone = pivot_of(&mut s, 0);
    assert!(p_undone.fields[0].default_subtotal);

    s.key("cmd-shift-z");
    let p_redone = pivot_of(&mut s, 0);
    assert!(!p_redone.fields[0].default_subtotal);

    // 5. 保存と XML 検証
    s.screenshot("pivot010_field_settings");
    let saved = s.save_as("pivot010_field_settings.xlsx");
    let data = std::fs::read(&saved).unwrap();
    let pivot_xml = ibis_pptx::test_files::part_text(&data, "xl/pivotTables/pivotTable1.xml").expect("pivot table xml exists");
    assert!(pivot_xml.contains(r#"grandTotalCaption="総合計""#), "XML contains grandTotalCaption: {pivot_xml}");
    assert!(pivot_xml.contains(r#"rowHeaderCaption="区分""#), "XML contains rowHeaderCaption: {pivot_xml}");
    assert!(pivot_xml.contains(r#"defaultSubtotal="0""#), "XML contains defaultSubtotal=0: {pivot_xml}");
    assert!(pivot_xml.contains(r#"compact="0""#), "XML contains compact=0: {pivot_xml}");

    // 6. 開き直し検証
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert_eq!(sh.pivots[0].grand_total_caption.as_deref(), Some("総合計"));
    assert_eq!(sh.pivots[0].row_header_caption.as_deref(), Some("区分"));
    assert!(!sh.pivots[0].fields[0].default_subtotal);
    assert!(!sh.pivots[0].fields[0].compact);
}

/// XLSX-FORMAT-013: 塗りつぶし色とパターン（前景色選択、グラデーション塗りの設定・保存・描画連動、Undo/Redo、再オープン）
fn format013_fill_patterns_and_gradients() {
    use ibis_doc_model::{CellGradient, Rgb, StyleChange};

    let mut s = new_sheet();
    for (cell, text) in [("A1", "パターン"), ("B1", "グラデーション縦"), ("C1", "グラデーション放射")] {
        s.click_cell(cell);
        s.enter(text, "enter");
    }

    // 1. A1: 背景色黄色 + ltUpDiag パターン + パターン前景色赤
    s.click_cell("A1");
    s.update(|e, _, cx| e.format_cells(StyleChange::Fill(Some(Rgb(0xFFFF00))), cx));
    // パターン種類設定 (ltUpDiag: 実線 右上がり 斜線 縞)
    s.update(|e, _, cx| e.format_cells(StyleChange::Pattern(Some(("ltUpDiag".into(), Rgb(0), Rgb(0xFFFF00)))), cx));
    // パターン色を赤に変更 (StyleChange::PatternColor)
    s.update(|e, _, cx| e.format_cells(StyleChange::PatternColor(Rgb(0xC00000)), cx));

    // 2. B1: 縦方向グラデーション (xlsx.gradient-vert)
    s.click_cell("B1");
    s.menu("xlsx.gradient-vert");

    // 3. C1: 中央から放射状グラデーション (xlsx.gradient-center)
    s.click_cell("C1");
    s.menu("xlsx.gradient-center");

    // モデル上の検証
    let style = |s: &mut Scenario, row: u32, col: u32| {
        s.with(|e, _| {
            let sh = sheet_of_slide(&e.document.presentation().slides[1]).unwrap();
            sh.styles[sh.cell(row, col).unwrap().style as usize].clone()
        })
    };

    let st_a1 = style(&mut s, 0, 0);
    assert_eq!(st_a1.pattern, Some(("ltUpDiag".into(), Rgb(0xC00000), Rgb(0xFFFF00))));
    assert_eq!(st_a1.fill, Some(Rgb(0xFFFF00)));

    let st_b1 = style(&mut s, 0, 1);
    assert!(matches!(st_b1.gradient, Some(CellGradient::Linear { degree: 90, .. })));
    assert_eq!(st_b1.pattern, None);

    let st_c1 = style(&mut s, 0, 2);
    assert!(matches!(st_c1.gradient, Some(CellGradient::Path { .. })));
    assert_eq!(st_c1.pattern, None);

    // 4. Undo / Redo 検証
    s.key("cmd-z"); // undo C1 gradient
    let st_c1_undone = style(&mut s, 0, 2);
    assert_eq!(st_c1_undone.gradient, None);

    s.key("cmd-shift-z"); // redo C1 gradient
    let st_c1_redone = style(&mut s, 0, 2);
    assert!(matches!(st_c1_redone.gradient, Some(CellGradient::Path { .. })));

    // 5. スクリーンショットと保存
    s.screenshot("format013_fill_patterns_and_gradients");
    s.save_as("format013_fills.xlsx");

    // 6. 開き直し検証
    let reopened = s.reopen();
    let sh = sheet_of_slide(&reopened.slides[1]).unwrap();
    let r_a1 = &sh.styles[sh.cell(0, 0).unwrap().style as usize];
    assert_eq!(r_a1.pattern, Some(("ltUpDiag".into(), Rgb(0xC00000), Rgb(0xFFFF00))));

    let r_b1 = &sh.styles[sh.cell(0, 1).unwrap().style as usize];
    assert!(matches!(r_b1.gradient, Some(CellGradient::Linear { degree: 90, .. })));

    let r_c1 = &sh.styles[sh.cell(0, 2).unwrap().style as usize];
    assert!(matches!(r_c1.gradient, Some(CellGradient::Path { .. })));
}

/// XLSX-FORMAT-016 テーマ色・フォント・効果:
/// 表示形式 > テーマの配色 からテーマパレットを切り替え（Aspect）、
/// テーマに紐づくセルの塗りつぶし・文字色およびセルスタイルの描画色が連動して変化、
/// テーマの色とフォント詳細設定パネルの確認、Undo / Redo、
/// 表示形式 > テーマの配色 > Office で標準パレットへの切り替え、
/// ファイル保存と開き直し後もテーマとスタイルが維持されることの確認。
fn format016_theme_palette_and_colors() {
    use ibis_doc_model::{Rgb, ThemeColor};
    let mut s = new_sheet();

    let initial_palette = s.with(|e, _| e.current_theme_palette_colors());
    let orig_accent1 = initial_palette[4];
    let orig_accent2 = initial_palette[5];

    // 1. セル A1 にテーマ色塗りつぶし（accent1）、セル B1 にテーマ色文字（accent2）を設定
    s.click_cell("A1");
    s.enter("アクセント1塗り", "enter");
    s.click_cell("B1");
    s.enter("アクセント2文字", "enter");

    let slide = s.with(|e, _| e.current_slide);
    let r1 = ibis_doc_model::CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 };
    let r2 = ibis_doc_model::CellRange { first_row: 0, first_column: 1, last_row: 0, last_column: 1 };
    s.update(|e, _, cx| {
        e.run_sheet(vec![
            ibis_doc_model::Command::Sheet(ibis_doc_model::sheet_command::SheetCommand::SetFormat {
                slide,
                ranges: vec![r1],
                change: ibis_doc_model::sheet_command::StyleChange::FillTheme(orig_accent1, ThemeColor { index: 4, tint: 0 }),
            }),
            ibis_doc_model::Command::Sheet(ibis_doc_model::sheet_command::SheetCommand::SetFormat {
                slide,
                ranges: vec![r2],
                change: ibis_doc_model::sheet_command::StyleChange::ColorTheme(orig_accent2, ThemeColor { index: 5, tint: 0 }),
            }),
        ], cx);
    });

    let style = |s: &mut Scenario, row: u32, col: u32| {
        s.with(|e, _| {
            let sh = sheet_of_slide(&e.document.presentation().slides[1]).unwrap();
            sh.styles[sh.cell(row, col).unwrap().style as usize].clone()
        })
    };

    assert!(!s.with(|e, _| e.is_xlsx_theme_palette_active("aspect")));

    // 2. 表示形式 > テーマの配色 > アスペクト (Aspect) に切り替え
    s.menu("xlsx.theme-palette.aspect");

    assert!(s.with(|e, _| e.is_xlsx_theme_palette_active("aspect")));

    let aspect_palette = s.with(|e, _| e.current_theme_palette_colors());
    assert_eq!(aspect_palette[4], Rgb(0xF0A22E)); // Aspect accent1
    assert_eq!(aspect_palette[5], Rgb(0xA56110)); // Aspect accent2

    // セル A1 のスタイルが新テーマのアクセント1色に連動していることを確認
    let st_a1 = style(&mut s, 0, 0);
    assert_eq!(st_a1.fill, Some(Rgb(0xF0A22E)));
    assert_eq!(st_a1.fill_theme, Some(ThemeColor { index: 4, tint: 0 }));

    // セル B1 のスタイルが新テーマのアクセント2色に連動していることを確認
    let st_b1 = style(&mut s, 0, 1);
    assert_eq!(st_b1.color, Rgb(0xA56110));
    assert_eq!(st_b1.color_theme, Some(ThemeColor { index: 5, tint: 0 }));

    // 3. 表示形式 > テーマの色とフォント… サイドバーパネルを開く
    s.menu("xlsx.theme-colors");
    assert!(s.with(|e, _| e.sidebar_open));

    // サイドバーを閉じる
    s.update(|e, w, cx| e.close_xlsx_theme_panel(w, cx));
    assert!(!s.with(|e, _| e.sidebar_open));

    // 4. Undo / Redo 検証
    s.key("cmd-z"); // Undo theme change
    assert!(!s.with(|e, _| e.is_xlsx_theme_palette_active("aspect")));
    let st_a1_undone = style(&mut s, 0, 0);
    assert_eq!(st_a1_undone.fill, Some(orig_accent1));
    let st_b1_undone = style(&mut s, 0, 1);
    assert_eq!(st_b1_undone.color, orig_accent2);

    s.key("cmd-shift-z"); // Redo theme change
    assert!(s.with(|e, _| e.is_xlsx_theme_palette_active("aspect")));
    let st_a1_redone = style(&mut s, 0, 0);
    assert_eq!(st_a1_redone.fill, Some(Rgb(0xF0A22E)));
    let st_b1_redone = style(&mut s, 0, 1);
    assert_eq!(st_b1_redone.color, Rgb(0xA56110));

    // 5. Office テーマパレットに切り替え
    s.menu("xlsx.theme-palette.office");
    assert!(s.with(|e, _| e.is_xlsx_theme_palette_active("office")));
    assert!(!s.with(|e, _| e.is_xlsx_theme_palette_active("aspect")));
    let st_a1_office = style(&mut s, 0, 0);
    assert_eq!(st_a1_office.fill, Some(Rgb(0x4472C4)));
    let st_b1_office = style(&mut s, 0, 1);
    assert_eq!(st_b1_office.color, Rgb(0xED7D31));

    // 6. スクリーンショットと保存
    s.screenshot("format016_theme_palette_and_colors");
    s.save_as("format016_theme.xlsx");

    // 7. 開き直し検証
    let reopened = s.reopen();
    let wb = reopened.workbook.as_ref().unwrap();
    assert_eq!(wb.theme_colors.iter().find(|(n, _)| n == "accent1").unwrap().1, Rgb(0x4472C4));
    assert_eq!(wb.theme_colors.iter().find(|(n, _)| n == "accent2").unwrap().1, Rgb(0xED7D31));

    let sh = sheet_of_slide(&reopened.slides[1]).unwrap();
    let r_a1 = &sh.styles[sh.cell(0, 0).unwrap().style as usize];
    assert_eq!(r_a1.fill, Some(Rgb(0x4472C4)));
    assert_eq!(r_a1.fill_theme, Some(ThemeColor { index: 4, tint: 0 }));

    let r_b1 = &sh.styles[sh.cell(0, 1).unwrap().style as usize];
    assert_eq!(r_b1.color, Rgb(0xED7D31));
    assert_eq!(r_b1.color_theme, Some(ThemeColor { index: 5, tint: 0 }));
}

fn cf012_databar_negative_and_axis() {
    let mut s = new_sheet();
    // A1: -10, A2: -5, A3: 0, A4: 5, A5: 10
    for v in ["-10", "-5", "0", "5", "10"] {
        s.enter(v, "enter");
    }

    // 1. クイックメニュー「xlsx.cf-bar-solid」（単色データバー）を範囲 A1:A5 に適用
    s.click_cell("A1");
    for _ in 0..4 {
        s.key("shift-down");
    }
    s.menu("xlsx.cf-bar-solid");

    // モデル確認: DataBar が gradient = false で追加されていること
    let model = cf_model(&mut s);
    assert_eq!(model.len(), 1);
    if let ibis_doc_model::CfKind::DataBar { gradient, .. } = &model[0].rules[0].kind {
        assert!(!*gradient, "expected solid fill (gradient == false)");
    } else {
        panic!("expected DataBar rule");
    }

    // オーバーレイ計算の検証
    s.with(|e, _| {
        let (_sheet, overlay) = super::conditional::apply(sheet_of_slide(e.slide()).unwrap()).unwrap();
        assert_eq!(overlay.bars.len(), 5);
        // 軸位置は Automatic で |-10| / (|-10| + 10) = 0.5
        // A1 (-10): 負のバー
        let b1 = &overlay.bars[0];
        assert_eq!(b1.cell, (0, 0));
        assert!(b1.is_negative);
        assert!(!b1.gradient);
        assert_eq!(b1.axis, Some(0.5));
        assert_eq!(b1.to, 0.5);
        assert!((b1.from - 0.025).abs() < 1e-3);
        assert_eq!(b1.color, ibis_doc_model::Rgb(0xD00000));

        // A5 (10): 正のバー
        let b5 = &overlay.bars[4];
        assert_eq!(b5.cell, (4, 0));
        assert!(!b5.is_negative);
        assert!(!b5.gradient);
        assert_eq!(b5.axis, Some(0.5));
        assert_eq!(b5.from, 0.5);
        assert!((b5.to - 0.975).abs() < 1e-3);
        assert_eq!(b5.color, ibis_doc_model::Rgb(0x638EC6));
    });

    // 2. Undo / Redo 検証
    s.key("cmd-z");
    assert!(cf_model(&mut s).is_empty());
    s.key("cmd-shift-z");
    assert_eq!(cf_model(&mut s).len(), 1);

    // 3. 詳細設定ルールの追加（カスタム negative_color, axis_color, axis_position: Midpoint）
    let slide_id = s.with(|e, _| e.slide().id);
    let custom_databar_format = ibis_doc_model::ConditionalFormat {
        source_index: None,
        ranges: vec![ibis_doc_model::CellRange { first_row: 0, last_row: 4, first_column: 0, last_column: 0 }],
        rules: vec![ibis_doc_model::CfRule {
            kind: ibis_doc_model::CfKind::DataBar {
                min: ibis_doc_model::CfValue { kind: "min".into(), value: None, gte: true },
                max: ibis_doc_model::CfValue { kind: "max".into(), value: None, gte: true },
                color: ibis_doc_model::Rgb(0x00FF00),
                gradient: false,
                negative_color: Some(ibis_doc_model::Rgb(0xFF00FF)),
                axis_color: Some(ibis_doc_model::Rgb(0x333333)),
                axis_position: Some(ibis_doc_model::DataBarAxisPosition::Midpoint),
                min_length: None,
                max_length: None,
            },
            priority: 2,
            stop: false,
            style: None,
        }],
    };
    s.update(|e, _, cx| {
        e.document.execute(vec![
            ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::AddConditional { slide: slide_id, format: custom_databar_format }),
        ]).unwrap();
        cx.notify();
    });

    // 保存と開き直し（ラウンドトリップ）
    s.screenshot("cf012_databar_negative_and_axis");
    s.save_as("cf012_databar_negative_and_axis.xlsx");
    let reopened = s.reopen();
    let re_sh = sheet_of_slide(&reopened.slides[1]).unwrap();
    assert_eq!(re_sh.conditional.len(), 2);

    // 1番目のルール（単色）
    if let ibis_doc_model::CfKind::DataBar { gradient, .. } = &re_sh.conditional[0].rules[0].kind {
        assert!(!*gradient);
    } else {
        panic!("expected DataBar");
    }

    // 2番目のルール（カスタム負色・軸色・Midpoint）
    if let ibis_doc_model::CfKind::DataBar {
        gradient,
        color,
        negative_color,
        axis_color,
        axis_position,
        ..
    } = &re_sh.conditional[1].rules[0].kind {
        assert!(!*gradient);
        assert_eq!(*color, ibis_doc_model::Rgb(0x00FF00));
        assert_eq!(*negative_color, Some(ibis_doc_model::Rgb(0xFF00FF)));
        assert_eq!(*axis_color, Some(ibis_doc_model::Rgb(0x333333)));
        assert_eq!(*axis_position, Some(ibis_doc_model::DataBarAxisPosition::Midpoint));
    } else {
        panic!("expected DataBar with custom negative and axis settings");
    }
}

fn cf010_icon_sets_shapes_and_custom() {
    let mut s = new_sheet();
    for v in ["10", "20", "30", "40", "50"] {
        s.enter(v, "enter");
    }

    // 1. クイックメニュー「xlsx.cf-icons」（3つの矢印）を A1:A5 に適用
    s.click_cell("A1");
    for _ in 0..4 {
        s.key("shift-down");
    }
    s.menu("xlsx.cf-icons");

    let model = cf_model(&mut s);
    assert_eq!(model.len(), 1);
    if let ibis_doc_model::CfKind::IconSet { set, reverse, .. } = &model[0].rules[0].kind {
        assert_eq!(set, "3Arrows");
        assert!(!reverse);
    } else {
        panic!("expected IconSet rule");
    }

    // オーバーレイ計算の検証（3Arrows: A1=下矢印(3, red), A3=横矢印(2, yellow), A5=上矢印(1, green)）
    s.with(|e, _| {
        let (_sheet, overlay) = super::conditional::apply(sheet_of_slide(e.slide()).unwrap()).unwrap();
        assert_eq!(overlay.icons.len(), 5);
        let i_a1 = overlay.icons.iter().find(|i| i.0 == (0, 0)).unwrap();
        assert_eq!(i_a1.1, ibis_doc_model::Rgb(0xC00000));
        assert_eq!(i_a1.2, 3);
        let i_a3 = overlay.icons.iter().find(|i| i.0 == (2, 0)).unwrap();
        assert_eq!(i_a3.1, ibis_doc_model::Rgb(0xFFC000));
        assert_eq!(i_a3.2, 2);
        let i_a5 = overlay.icons.iter().find(|i| i.0 == (4, 0)).unwrap();
        assert_eq!(i_a5.1, ibis_doc_model::Rgb(0x00B050));
        assert_eq!(i_a5.2, 1);
    });

    // 2. Undo / Redo
    s.key("cmd-z");
    assert!(cf_model(&mut s).is_empty());
    s.key("cmd-shift-z");
    assert_eq!(cf_model(&mut s).len(), 1);

    // 3. 逆順メニュー「xlsx.cf-icons-reverse」の適用
    s.click_cell("A1");
    for _ in 0..4 {
        s.key("shift-down");
    }
    s.menu("xlsx.cf-icons-reverse");
    let model2 = cf_model(&mut s);
    assert_eq!(model2.len(), 2);
    if let ibis_doc_model::CfKind::IconSet { reverse, .. } = &model2[1].rules[0].kind {
        assert!(reverse);
    } else {
        panic!("expected reversed IconSet");
    }

    // 4. 個別選択（custom = true, スロット1が NoIcon / アイコンなし）ルールの追加
    let slide_id = s.with(|e, _| e.slide().id);
    let custom_icon_format = ibis_doc_model::ConditionalFormat {
        source_index: None,
        ranges: vec![ibis_doc_model::CellRange { first_row: 0, last_row: 4, first_column: 1, last_column: 1 }],
        rules: vec![ibis_doc_model::CfRule {
            kind: ibis_doc_model::CfKind::IconSet {
                set: "3Signs".into(),
                values: vec![
                    ibis_doc_model::CfValue { kind: "percent".into(), value: Some("0".into()), gte: true },
                    ibis_doc_model::CfValue { kind: "percent".into(), value: Some("33".into()), gte: true },
                    ibis_doc_model::CfValue { kind: "percent".into(), value: Some("67".into()), gte: true },
                ],
                reverse: false,
                show_value: true,
                custom: true,
                icons: vec![
                    Some(ibis_doc_model::CfIcon::new("3Signs", 0)),
                    Some(ibis_doc_model::CfIcon::no_icon()),
                    Some(ibis_doc_model::CfIcon::new("3Flags", 2)),
                ],
            },
            priority: 3,
            stop: false,
            style: None,
        }],
    };
    s.update(|e, _, cx| {
        e.document.execute(vec![
            ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::AddConditional { slide: slide_id, format: custom_icon_format }),
        ]).unwrap();
        cx.notify();
    });

    // 5. 保存と開き直し（ラウンドトリップ）
    s.screenshot("cf010_icon_sets_shapes_and_custom");
    s.save_as("cf010_icon_sets_shapes_and_custom.xlsx");
    let reopened = s.reopen();
    let re_sh = sheet_of_slide(&reopened.slides[1]).unwrap();
    assert_eq!(re_sh.conditional.len(), 3);

    // 3番目のルール（カスタム・NoIcon）の検証
    if let ibis_doc_model::CfKind::IconSet { set, custom, icons, .. } = &re_sh.conditional[2].rules[0].kind {
        assert_eq!(set, "3Signs");
        assert!(custom);
        assert_eq!(icons.len(), 3);
        assert_eq!(icons[0], Some(ibis_doc_model::CfIcon::new("3Signs", 0)));
        assert!(icons[1].as_ref().unwrap().is_no_icon());
        assert_eq!(icons[2], Some(ibis_doc_model::CfIcon::new("3Flags", 2)));
    } else {
        panic!("expected custom IconSet");
    }
}

/// XLSX-TABLE-011: 色・アイコンによる抽出（iconFilter）およびアイコンによる並べ替え（SortBy::Icon）
fn table011_icon_filter_and_sort() {
    let mut s = new_sheet();
    // 1. データの入力
    // A1: 得点
    // A2: 10 (slot 0: red)
    // A3: 80 (slot 2: green)
    // A4: 50 (slot 1: yellow)
    // A5: 90 (slot 2: green)
    // A6: 20 (slot 0: red)
    let cells = [
        ("A1", "得点"),
        ("A2", "10"),
        ("A3", "80"),
        ("A4", "50"),
        ("A5", "90"),
        ("A6", "20"),
    ];
    for (addr, val) in cells {
        s.click_cell(addr);
        s.enter(val, "enter");
    }

    // 2. 条件付き書式アイコンセット（3TrafficLights1）を付与
    let slide_id = s.with(|e, _| e.slide().id);
    let icon_format = ibis_doc_model::ConditionalFormat {
        source_index: None,
        ranges: vec![ibis_doc_model::CellRange { first_row: 1, last_row: 5, first_column: 0, last_column: 0 }],
        rules: vec![ibis_doc_model::CfRule {
            kind: ibis_doc_model::CfKind::IconSet {
                set: "3TrafficLights1".into(),
                values: vec![
                    ibis_doc_model::CfValue { kind: "num".into(), value: Some("0".into()), gte: true },
                    ibis_doc_model::CfValue { kind: "num".into(), value: Some("30".into()), gte: true },
                    ibis_doc_model::CfValue { kind: "num".into(), value: Some("70".into()), gte: true },
                ],
                reverse: false,
                show_value: true,
                custom: false,
                icons: Vec::new(),
            },
            priority: 1,
            stop: false,
            style: None,
        }],
    };
    s.update(|e, _, cx| {
        e.document.execute(vec![
            ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::AddConditional { slide: slide_id, format: icon_format }),
        ]).unwrap();
        cx.notify();
    });

    // 3. オートフィルターを設定
    s.click_cell("A1");
    s.menu("xlsx.filter");

    // 4. アイコンフィルターで緑アイコン (icon_id: 2) を抽出
    // A3(80) と A5(90) が残り、A2(10), A4(50), A6(20) が除外される
    s.update(|e, _, cx| {
        e.filter_column(0, Some(ibis_doc_model::ColumnFilter::Icon {
            icon_set: "3TrafficLights1".into(),
            icon_id: 2,
        }), cx);
    });
    s.settle();

    let filtered = s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        sheet.filtered_rows.clone()
    });
    // 行1 (A2=10), 行3 (A4=50), 行5 (A6=20) が非表示
    assert!(filtered.contains(&1));
    assert!(!filtered.contains(&2));
    assert!(filtered.contains(&3));
    assert!(!filtered.contains(&4));
    assert!(filtered.contains(&5));

    // 5. フィルターをクリア
    s.update(|e, _, cx| {
        e.filter_column(0, None, cx);
    });
    s.settle();
    let filtered_cleared = s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        sheet.filtered_rows.clone()
    });
    assert!(filtered_cleared.is_empty());

    // 6. アイコンで並べ替え（SortBy::Icon）
    // 緑アイコン (icon_id: 2) を上に配置: 80, 90 が先頭、残りは 10, 50, 20
    let range = ibis_doc_model::CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 0 };
    s.update(|e, _, cx| {
        e.document.execute(vec![
            ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SortBy {
                slide: slide_id,
                range,
                keys: vec![(0, ibis_doc_model::sheet_command::SortBy::Icon {
                    icon_set: "3TrafficLights1".into(),
                    icon_id: 2,
                    top: true,
                })],
                header: true,
                match_case: false,
            }),
        ]).unwrap();
        cx.notify();
    });
    s.settle();

    assert_eq!(s.value("A2"), ibis_doc_model::CellValue::Number(80.0));
    assert_eq!(s.value("A3"), ibis_doc_model::CellValue::Number(90.0));
    assert_eq!(s.value("A4"), ibis_doc_model::CellValue::Number(10.0));
    assert_eq!(s.value("A5"), ibis_doc_model::CellValue::Number(50.0));
    assert_eq!(s.value("A6"), ibis_doc_model::CellValue::Number(20.0));

    // 元に戻す (Undo)
    s.key("cmd-z");
    assert_eq!(s.value("A2"), ibis_doc_model::CellValue::Number(10.0));
    assert_eq!(s.value("A3"), ibis_doc_model::CellValue::Number(80.0));
    assert_eq!(s.value("A4"), ibis_doc_model::CellValue::Number(50.0));

    // やり直し (Redo)
    s.key("cmd-shift-z");
    assert_eq!(s.value("A2"), ibis_doc_model::CellValue::Number(80.0));
    assert_eq!(s.value("A3"), ibis_doc_model::CellValue::Number(90.0));

    // 7. アイコンフィルターをかけた状態で保存と開き直し
    s.update(|e, _, cx| {
        e.filter_column(0, Some(ibis_doc_model::ColumnFilter::Icon {
            icon_set: "3TrafficLights1".into(),
            icon_id: 2,
        }), cx);
    });
    s.settle();

    s.save_as("table011_icon_filter_and_sort.xlsx");
    let reopened = s.reopen();
    let re_sh = sheet_of_slide(&reopened.slides[1]).unwrap();
    let af = re_sh.auto_filter.as_ref().expect("auto_filter should be saved");
    assert_eq!(af.columns, vec![(0, ibis_doc_model::ColumnFilter::Icon {
        icon_set: "3TrafficLights1".into(),
        icon_id: 2,
    })]);
}

fn custom_views_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.custom_views.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("custom views button not found: {id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}

fn custom_views_field(s: &mut Scenario, id: &str, text: &str) {
    custom_views_click(s, id);
    s.key("cmd-a");
    s.type_text(text);
}

/// XLSX-VIEW-016, XLSX-VIEW-017, XLSX-VIEW-018:
/// ユーザー設定のビュー (Custom Views) の登録・表示・削除・印刷設定および非表示行/列の反映、
/// Undo/Redo、保存・再読み込み、およびテーブルとの排他条件の検証。
fn view016_custom_views_register_show_delete_and_table_exclusion() {
    use ibis_doc_model::{Command, SheetCommand};
    let mut s = new_sheet();

    // 1. Setup initial cells
    s.click_cell("A1");
    s.enter("Name", "tab");
    s.enter("Score", "enter");
    s.click_cell("A2");
    s.enter("Alice", "tab");
    s.enter("100", "enter");
    s.click_cell("A3");
    s.enter("Bob", "tab");
    s.enter("80", "enter");

    // 2. Open Custom Views panel and register "全体表示" (Full View)
    s.menu("xlsx.custom-views");
    assert!(s.with(|e, _| e.wb().panels.custom_views.is_some()));

    custom_views_click(&mut s, "custom-view-add");
    custom_views_field(&mut s, "custom-view-name", "全体表示");
    custom_views_click(&mut s, "custom-view-ok");

    let views_count = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().custom_views.len());
    assert_eq!(views_count, 1);
    custom_views_click(&mut s, "custom-view-close");
    assert!(s.with(|e, _| e.wb().panels.custom_views.is_none()));

    // 3. Modify view settings: zoom to 150%, toggle gridlines off, toggle headings off, hide row 2
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run_sheet(vec![
            Command::Sheet(ibis_doc_model::SheetCommand::SetZoom { slide, zoom: Some(150) }),
            Command::Sheet(ibis_doc_model::SheetCommand::SetGridLines { slide, shown: false }),
            Command::Sheet(ibis_doc_model::SheetCommand::SetHeadings { slide, shown: false }),
            Command::Sheet(ibis_doc_model::SheetCommand::Hide {
                slide,
                axis: ibis_doc_model::sheet_command::Axis::Rows,
                first: 2,
                last: 2,
                hidden: true,
            }),
        ], cx);
    });
    s.settle();

    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.zoom), Some(150));
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.grid_lines), false);
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.headings), false);
    assert!(s.with(|e, _| e.current_sheet().unwrap().1.hidden_rows.contains_key(&2)));

    // 4. Register second Custom View: "拡大非表示"
    s.menu("xlsx.custom-views");
    custom_views_click(&mut s, "custom-view-add");
    custom_views_field(&mut s, "custom-view-name", "拡大非表示");
    custom_views_click(&mut s, "custom-view-ok");

    let views_count2 = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().custom_views.len());
    assert_eq!(views_count2, 2);

    // 5. Show Custom View 1 ("全体表示", item 0)
    custom_views_click(&mut s, "custom-view-item-0");
    custom_views_click(&mut s, "custom-view-show");

    // Verify restored to View 1
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.zoom), None);
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.grid_lines), true);
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.headings), true);
    assert!(!s.with(|e, _| e.current_sheet().unwrap().1.hidden_rows.contains_key(&2)));

    // 6. Show Custom View 2 ("拡大非表示", item 1)
    custom_views_click(&mut s, "custom-view-item-1");
    custom_views_click(&mut s, "custom-view-show");

    // Verify restored to View 2
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.zoom), Some(150));
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.grid_lines), false);
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.headings), false);
    assert!(s.with(|e, _| e.current_sheet().unwrap().1.hidden_rows.contains_key(&2)));

    // 7. Undo / Redo showing views
    custom_views_click(&mut s, "custom-view-close");
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.grid_lines), true);
    assert!(!s.with(|e, _| e.current_sheet().unwrap().1.hidden_rows.contains_key(&2)));

    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.grid_lines), false);
    assert!(s.with(|e, _| e.current_sheet().unwrap().1.hidden_rows.contains_key(&2)));

    // 8. Delete Custom View "全体表示" (item 0)
    s.menu("xlsx.custom-views");
    custom_views_click(&mut s, "custom-view-item-0");
    custom_views_click(&mut s, "custom-view-del");
    assert_eq!(s.with(|e, _| e.presentation().workbook.as_ref().unwrap().custom_views.len()), 1);

    // Undo delete restores it
    custom_views_click(&mut s, "custom-view-close");
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    assert_eq!(s.with(|e, _| e.presentation().workbook.as_ref().unwrap().custom_views.len()), 2);

    // Redo delete removes it again
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    assert_eq!(s.with(|e, _| e.presentation().workbook.as_ref().unwrap().custom_views.len()), 1);

    // 9. Save and reopen OOXML roundtrip
    s.save_as("view016_custom_views.xlsx");
    let reopened = s.reopen();
    let re_wb = reopened.workbook.as_ref().expect("workbook exists");
    assert_eq!(re_wb.custom_views.len(), 1);
    assert_eq!(re_wb.custom_views[0].name, "拡大非表示");
    let re_sh = sheet_of_slide(&reopened.slides[1]).expect("sheet exists");
    assert_eq!(re_sh.custom_views.len(), 1);
    assert_eq!(re_sh.custom_views[0].name, "拡大非表示");
    assert_eq!(re_sh.custom_views[0].zoom, Some(150));
    assert_eq!(re_sh.custom_views[0].grid_lines, false);
    assert_eq!(re_sh.custom_views[0].headings, false);

    // 10. Table exclusion rule (XLSX-VIEW-018):
    // When a table exists, custom views are disabled
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let table = ibis_doc_model::SheetTable {
            name: "Table1".into(),
            range: ibis_doc_model::CellRange {
                first_row: 0,
                last_row: 2,
                first_column: 0,
                last_column: 1,
            },
            header_rows: 1,
            totals_rows: 0,
            columns: vec!["Col1".into(), "Col2".into()],
            style: None,
            filter: None,
            totals: Vec::new(),
        };
        e.run_sheet(vec![Command::Sheet(ibis_doc_model::SheetCommand::SetTable {
            slide,
            index: 0,
            table: Some(table),
            cells: Vec::new(),
        })], cx);
    });
    s.settle();

    assert!(s.with(|e, _| e.presentation().has_tables()));
    assert_eq!(s.menu_state("xlsx.custom-views"), Some((false, false))); // (enabled, checked): enabled is false!

    // Open panel directly and verify exclusion warning and disabled actions
    s.update(|e, window, cx| e.open_custom_views_panel(window, cx));
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.custom_views.as_ref().unwrap().message.contains("テーブルが含まれているため")));
}

fn unhide_window_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.unhide_window.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("unhide window button not found: {id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}

/// XLSX-VIEW-015:
/// ウィンドウの非表示・再表示 (Hide / Unhide Workbook Window):
/// メニューの有効/無効、非表示プレースホルダー、再表示ダイアログパネル、Undo/Redo、
/// およびブックの保護（ウィンドウのロック）による制限の検証。
fn view015_window_hide_unhide_and_protection() {
    use ibis_doc_model::{Command, SheetCommand, WorkbookProtection};
    let mut s = new_sheet();

    // 1. Initial state: not hidden, not locked
    assert!(!s.with(|e, _| e.is_workbook_window_hidden()));
    assert!(!s.with(|e, _| e.is_workbook_windows_locked()));
    assert_eq!(s.menu_state("xlsx.hide-window"), Some((true, false)));
    assert_eq!(s.menu_state("xlsx.unhide-window"), Some((false, false)));

    // 2. Hide window via menu
    s.menu("xlsx.hide-window");
    s.settle();
    assert!(s.with(|e, _| e.is_workbook_window_hidden()));
    assert_eq!(s.menu_state("xlsx.hide-window"), Some((false, false)));
    assert_eq!(s.menu_state("xlsx.unhide-window"), Some((true, false)));

    // 3. Test Undo -> window restored to visible
    s.key("cmd-z");
    s.settle();
    assert!(!s.with(|e, _| e.is_workbook_window_hidden()));
    assert_eq!(s.menu_state("xlsx.hide-window"), Some((true, false)));
    assert_eq!(s.menu_state("xlsx.unhide-window"), Some((false, false)));

    // 4. Test Redo -> window hidden again
    s.key("shift-cmd-z");
    s.settle();
    assert!(s.with(|e, _| e.is_workbook_window_hidden()));
    assert_eq!(s.menu_state("xlsx.hide-window"), Some((false, false)));
    assert_eq!(s.menu_state("xlsx.unhide-window"), Some((true, false)));

    // 5. Open Unhide Window dialog via menu and confirm unhiding
    s.menu("xlsx.unhide-window");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.unhide_window.is_some()));

    unhide_window_click(&mut s, "unhide-window-ok");
    s.settle();
    assert!(!s.with(|e, _| e.is_workbook_window_hidden()));
    assert!(s.with(|e, _| e.wb().panels.unhide_window.is_none()));
    assert_eq!(s.menu_state("xlsx.hide-window"), Some((true, false)));
    assert_eq!(s.menu_state("xlsx.unhide-window"), Some((false, false)));

    // 6. Test Workbook Window Protection lock
    s.update(|e, _, cx| {
        e.run_sheet(vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
            protection: Some(WorkbookProtection {
                lock_structure: false,
                lock_windows: true,
                lock_revision: false,
                hash: None,
                legacy: None,
            }),
        })], cx);
    });
    s.settle();
    assert!(s.with(|e, _| e.is_workbook_windows_locked()));

    // Both hide-window and unhide-window must be disabled when windows are locked
    assert_eq!(s.menu_state("xlsx.hide-window"), Some((false, false)));
    assert_eq!(s.menu_state("xlsx.unhide-window"), Some((false, false)));

    // 7. Unprotect workbook
    s.update(|e, _, cx| {
        e.run_sheet(vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
            protection: None,
        })], cx);
    });
    s.settle();
    assert!(!s.with(|e, _| e.is_workbook_windows_locked()));
    assert_eq!(s.menu_state("xlsx.hide-window"), Some((true, false)));
    assert_eq!(s.menu_state("xlsx.unhide-window"), Some((false, false)));
}

/// XLSX-FN-TEXTBEFORE, XLSX-FN-TEXTAFTER, XLSX-FN-TEXTSPLIT:
/// Verification of Excel-compliant delimiter functions including
/// fractional match_mode/match_end truncation, negative out-of-bound instances,
/// empty text handling, empty delimiter behaviors, fallback string coercion,
/// dynamic array spill, and OOXML save/reopen roundtrip.
fn fn_text_delimiter_functions() {
    let mut s = new_sheet();

    // 1. TEXTBEFORE with fractional match_mode and match_end
    s.click_cell("A1");
    s.enter("=TEXTBEFORE(\"aB|ab\", \"ab\", 1, 0.5, 0, \"missing\")", "enter");
    assert_eq!(s.shown("A1"), "aB|");

    s.click_cell("A2");
    s.enter("=TEXTBEFORE(\"abc\", \"x\", 1, 0, 0.5, \"missing\")", "enter");
    assert_eq!(s.shown("A2"), "missing");

    // 2. TEXTBEFORE with empty delimiter
    s.click_cell("A3");
    s.enter("=TEXTBEFORE(\"abc\", \"\", -2, 0, 0, \"missing\")", "enter");
    assert_eq!(s.shown("A3"), "abc");

    s.click_cell("A4");
    s.enter("=TEXTBEFORE(\"abc\", \"\", 2, 0, 0, \"missing\")", "enter");
    assert_eq!(s.shown("A4"), "");

    // 3. TEXTAFTER with empty delimiter & fractional options
    s.click_cell("A5");
    s.enter("=TEXTAFTER(\"aB|ab\", \"ab\", 1, 0.5, 0, \"missing\")", "enter");
    assert_eq!(s.shown("A5"), "");

    s.click_cell("A6");
    s.enter("=TEXTAFTER(\"abc\", \"\", 2, 0, 0, \"missing\")", "enter");
    assert_eq!(s.shown("A6"), "abc");

    // 4. Fallback converted to text
    s.click_cell("A7");
    s.enter("=TEXTBEFORE(\"abc\", \"x\", 1, 0, 0, TRUE)", "enter");
    assert_eq!(s.shown("A7"), "TRUE");

    // 5. TEXTSPLIT dynamic array
    s.click_cell("B1");
    s.enter("=TEXTSPLIT(\"Alpha,Beta,Gamma\", \",\")", "enter");
    assert_eq!(s.shown("B1"), "Alpha");
    assert_eq!(s.shown("C1"), "Beta");
    assert_eq!(s.shown("D1"), "Gamma");

    // 6. Error cases (#VALUE!)
    s.click_cell("A8");
    s.enter("=TEXTBEFORE(\"\", \"x\", 1, 0, 0, \"missing\")", "enter");
    assert_eq!(s.shown("A8"), "#VALUE!");

    s.click_cell("A9");
    s.enter("=TEXTBEFORE(\"abc\", \"x\", -100, 0, 0, \"missing\")", "enter");
    assert_eq!(s.shown("A9"), "#VALUE!");

    s.click_cell("A10");
    s.enter("=TEXTSPLIT(\"\", \",\")", "enter");
    assert_eq!(s.shown("A10"), "#VALUE!");

    s.click_cell("A11");
    s.enter("=TEXTSPLIT(\"Hello\", \"Hello\", \"Hello\", TRUE)", "enter");
    assert_eq!(s.shown("A11"), "#VALUE!");

    // 7. Save and reopen roundtrip
    s.save_as("fn_text_delimiter_functions.xlsx");
    let reopened = s.reopen();
    let re_sh = sheet_of_slide(&reopened.slides[1]).unwrap();
    assert_eq!(re_sh.cell(0, 0).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("aB|".into())));
    assert_eq!(re_sh.cell(1, 0).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("missing".into())));
    assert_eq!(re_sh.cell(2, 0).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("abc".into())));
    assert_eq!(re_sh.cell(3, 0).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("".into())));
    assert_eq!(re_sh.cell(5, 0).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("abc".into())));
    assert_eq!(re_sh.cell(6, 0).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("TRUE".into())));
    assert_eq!(re_sh.cell(0, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("Alpha".into())));
    assert_eq!(re_sh.cell(0, 2).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("Beta".into())));
    assert_eq!(re_sh.cell(0, 3).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("Gamma".into())));
}


/// XLSX-FN-CHAR / CODE / UNICHAR: 文字コード・文字変換関数（CHAR 255/制御文字, CODE 全角JISコード/半角, UNICHAR/UNICODE, 保存・再読込・Undo/Redo）
fn fn_char_and_code_functions() {
    let mut s = new_sheet();

    // 1. CHAR functions
    s.click_cell("A1");
    s.enter("=CHAR(255)", "enter");
    assert_eq!(s.shown("A1"), "ÿ");

    s.click_cell("A2");
    s.enter("=CHAR(3)", "enter");
    assert_eq!(s.shown("A2"), "\u{3}");

    s.click_cell("A3");
    s.enter("=CHAR(10)", "enter");
    assert_eq!(s.shown("A3"), "\n");

    s.click_cell("A4");
    s.enter("=CHAR(0)", "enter");
    assert_eq!(s.shown("A4"), "#VALUE!");

    s.click_cell("A5");
    s.enter("=CHAR(256)", "enter");
    assert_eq!(s.shown("A5"), "#VALUE!");

    // 2. CODE functions
    s.click_cell("B1");
    s.enter("=CODE(\"日本語テキスト\")", "enter");
    assert_eq!(s.shown("B1"), "18044");

    s.click_cell("B2");
    s.enter("=CODE(\"abc\")", "enter");
    assert_eq!(s.shown("B2"), "97");

    s.click_cell("B3");
    s.enter("=CODE(\"ｱｲｳ ABC\")", "enter");
    assert_eq!(s.shown("B3"), "177");

    s.click_cell("B4");
    s.enter("=CODE(\"\")", "enter");
    assert_eq!(s.shown("B4"), "#VALUE!");

    // 3. UNICHAR and UNICODE functions
    s.click_cell("C1");
    s.enter("=UNICHAR(255)", "enter");
    assert_eq!(s.shown("C1"), "ÿ");

    s.click_cell("C2");
    s.enter("=UNICHAR(1000)", "enter");
    assert_eq!(s.shown("C2"), "Ϩ");

    s.click_cell("C3");
    s.enter("=UNICODE(\"日本語テキスト\")", "enter");
    assert_eq!(s.shown("C3"), "26085");

    // 4. Undo and Redo
    s.click_cell("D1");
    s.enter("=CODE(\"日本語\")", "enter");
    assert_eq!(s.shown("D1"), "18044");
    s.key("cmd-z");
    assert_eq!(s.value("D1"), ibis_doc_model::CellValue::Empty);
    s.key("cmd-shift-z");
    assert_eq!(s.shown("D1"), "18044");
    s.key("cmd-z");

    // 5. Save and reopen roundtrip
    s.save_as("fn_char_and_code_functions.xlsx");
    let reopened = s.reopen();
    let re_sh = sheet_of_slide(&reopened.slides[1]).unwrap();
    assert_eq!(re_sh.cell(0, 0).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("ÿ".into())));
    assert_eq!(re_sh.cell(1, 0).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("\u{3}".into())));
    assert_eq!(re_sh.cell(0, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(18044.0)));
    assert_eq!(re_sh.cell(1, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(97.0)));
    assert_eq!(re_sh.cell(2, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(177.0)));
    assert_eq!(re_sh.cell(0, 2).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("ÿ".into())));
    assert_eq!(re_sh.cell(1, 2).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("Ϩ".into())));
    assert_eq!(re_sh.cell(2, 2).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(26085.0)));
}

/// XLSX-FN-ACOT / ACOTH / TYPE: 三角関数逆数・ハイパーボリック・型判定関数（高精度浮動小数点・暗黙の交差・Undo/Redo・保存ラウンドトリップ）
fn fn_acot_acoth_type_functions() {
    let mut s = new_sheet();

    // Setup base cells for TYPE testing
    s.click_cell("A1");
    s.enter("100", "enter");
    s.click_cell("A2");
    s.enter("200", "enter");

    // 1. ACOT functions (atan(1/x))
    s.click_cell("B1");
    s.enter("=ACOT(10)", "enter");
    assert_eq!(s.shown("B1"), "0.099668652");

    s.click_cell("B2");
    s.enter("=ACOT(100)", "enter");
    assert_eq!(s.shown("B2"), "0.009999667");

    s.click_cell("B3");
    s.enter("=ACOT(0)", "enter");
    assert_eq!(s.shown("B3"), "1.570796327");

    s.click_cell("B4");
    s.enter("=ACOT(-1)", "enter");
    assert_eq!(s.shown("B4"), "2.35619449");

    // 2. ACOTH functions (atanh(1/x))
    s.click_cell("C1");
    s.enter("=ACOTH(100)", "enter");
    assert_eq!(s.shown("C1"), "0.010000333");

    s.click_cell("C2");
    s.enter("=ACOTH(1)", "enter");
    assert_eq!(s.shown("C2"), "#NUM!");

    s.click_cell("C3");
    s.enter("=ACOTH(0.5)", "enter");
    assert_eq!(s.shown("C3"), "#NUM!");

    // 3. TYPE functions
    s.click_cell("D1");
    s.enter("=TYPE(A1)", "enter");
    assert_eq!(s.shown("D1"), "1"); // number

    s.click_cell("D2");
    s.enter("=TYPE(\"hello\")", "enter");
    assert_eq!(s.shown("D2"), "2"); // text

    s.click_cell("D3");
    s.enter("=TYPE(TRUE)", "enter");
    assert_eq!(s.shown("D3"), "4"); // bool

    s.click_cell("D4");
    s.enter("=TYPE(1/0)", "enter");
    assert_eq!(s.shown("D4"), "16"); // error

    s.click_cell("D5");
    s.enter("=TYPE({1, 2; 3, 4})", "enter");
    assert_eq!(s.shown("D5"), "64"); // array constant

    // Out-of-intersection range reference evaluates to #VALUE! -> TYPE is 16
    s.click_cell("D10");
    s.enter("=TYPE(A1:A2)", "enter");
    assert_eq!(s.shown("D10"), "16");

    // 4. Undo and Redo
    s.click_cell("E1");
    s.enter("=ACOT(10)", "enter");
    assert_eq!(s.shown("E1"), "0.099668652");
    s.key("cmd-z");
    assert_eq!(s.value("E1"), ibis_doc_model::CellValue::Empty);
    s.key("cmd-shift-z");
    assert_eq!(s.shown("E1"), "0.099668652");
    s.key("cmd-z");

    // 5. Save and reopen roundtrip
    s.save_as("fn_acot_acoth_type_functions.xlsx");
    let reopened = s.reopen();
    let re_sh = sheet_of_slide(&reopened.slides[1]).unwrap();
    assert_eq!(re_sh.cell(0, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(0.09966865249116204)));
    assert_eq!(re_sh.cell(1, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(0.009999666686665238)));
    assert_eq!(re_sh.cell(0, 2).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(0.010000333353334763)));
    assert_eq!(re_sh.cell(1, 2).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Num)));
    assert_eq!(re_sh.cell(0, 3).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(1.0)));
    assert_eq!(re_sh.cell(4, 3).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(64.0)));
    assert_eq!(re_sh.cell(9, 3).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(16.0)));
}

/// XLSX-FN-N / T: 型変換関数・セル範囲の先頭セル評価・配列定数スカラー評価・Undo/Redo・保存ラウンドトリップ
fn fn_n_and_t_functions() {
    let mut s = new_sheet();

    // 1. データセルの入力
    s.click_cell("A1");
    s.enter("100", "enter");
    s.click_cell("A2");
    s.enter("hello", "enter");
    s.click_cell("A3");
    s.enter("TRUE", "enter");
    s.click_cell("A5");
    s.enter("=1/0", "enter");

    // 2. N 関数のテスト（セル範囲・単一セル・配列定数）
    s.click_cell("B1");
    s.enter("=N(A1:A5)", "enter");
    assert_eq!(s.shown("B1"), "100");

    s.click_cell("B2");
    s.enter("=N(A2:A5)", "enter");
    assert_eq!(s.shown("B2"), "0");

    s.click_cell("B3");
    s.enter("=N(A3)", "enter");
    assert_eq!(s.shown("B3"), "1");

    s.click_cell("B4");
    s.enter("=N(A4)", "enter");
    assert_eq!(s.shown("B4"), "0");

    s.click_cell("B5");
    s.enter("=N(A5)", "enter");
    assert_eq!(s.shown("B5"), "#DIV/0!");

    s.click_cell("B6");
    s.enter("=N({42, 2; 3, 4})", "enter");
    assert_eq!(s.shown("B6"), "42");

    // 3. T 関数のテスト（セル範囲・単一セル・配列定数）
    s.click_cell("C1");
    s.enter("=T(A1:A5)", "enter");
    assert_eq!(s.shown("C1"), "");

    s.click_cell("C2");
    s.enter("=T(A2:A5)", "enter");
    assert_eq!(s.shown("C2"), "hello");

    s.click_cell("C3");
    s.enter("=T(A3)", "enter");
    assert_eq!(s.shown("C3"), "");

    s.click_cell("C4");
    s.enter("=T(A4)", "enter");
    assert_eq!(s.shown("C4"), "");

    s.click_cell("C5");
    s.enter("=T(A5)", "enter");
    assert_eq!(s.shown("C5"), "#DIV/0!");

    s.click_cell("C6");
    s.enter("=T({1, 2; 3, 4})", "enter");
    assert_eq!(s.shown("C6"), "");

    s.click_cell("C7");
    s.enter("=T({\"world\", 2; 3, 4})", "enter");
    assert_eq!(s.shown("C7"), "world");

    // 4. Undo and Redo
    s.click_cell("D1");
    s.enter("=N(A1:A5)", "enter");
    assert_eq!(s.shown("D1"), "100");
    s.key("cmd-z");
    assert_eq!(s.value("D1"), ibis_doc_model::CellValue::Empty);
    s.key("cmd-shift-z");
    assert_eq!(s.shown("D1"), "100");
    s.key("cmd-z");

    // 5. 保存と再読み込みラウンドトリップ
    s.save_as("fn_n_and_t_functions.xlsx");
    let reopened = s.reopen();
    let re_sh = sheet_of_slide(&reopened.slides[1]).unwrap();
    assert_eq!(re_sh.cell(0, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(100.0)));
    assert_eq!(re_sh.cell(1, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(0.0)));
    assert_eq!(re_sh.cell(5, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(42.0)));
    assert_eq!(re_sh.cell(0, 2).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("".into())));
    assert_eq!(re_sh.cell(1, 2).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("hello".into())));
    assert_eq!(re_sh.cell(6, 2).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Text("world".into())));
}

/// XLSX-FN-FACTDOUBLE: FACTDOUBLE 関数のブール値厳格チェック（#VALUE!）・偶数/奇数二重階乗計算・Undo/Redo・保存ラウンドトリップ
fn fn_factdouble_function() {
    let mut s = new_sheet();

    // 1. データセルの入力
    s.click_cell("A1");
    s.enter("6", "enter");
    s.click_cell("A2");
    s.enter("7", "enter");
    s.click_cell("A3");
    s.enter("TRUE", "enter");
    s.click_cell("A4");
    s.enter("-1", "enter");
    s.click_cell("A5");
    s.enter("-2", "enter");

    // 2. FACTDOUBLE 関数のテスト
    s.click_cell("B1");
    s.enter("=FACTDOUBLE(A1)", "enter");
    assert_eq!(s.shown("B1"), "48");

    s.click_cell("B2");
    s.enter("=FACTDOUBLE(A2)", "enter");
    assert_eq!(s.shown("B2"), "105");

    s.click_cell("B3");
    s.enter("=FACTDOUBLE(A3)", "enter");
    assert_eq!(s.shown("B3"), "#VALUE!");

    s.click_cell("B4");
    s.enter("=FACTDOUBLE(A4)", "enter");
    assert_eq!(s.shown("B4"), "1");

    s.click_cell("B5");
    s.enter("=FACTDOUBLE(A5)", "enter");
    assert_eq!(s.shown("B5"), "#NUM!");

    s.click_cell("B6");
    s.enter("=FACTDOUBLE(TRUE)", "enter");
    assert_eq!(s.shown("B6"), "#VALUE!");

    s.click_cell("B7");
    s.enter("=FACTDOUBLE(FALSE)", "enter");
    assert_eq!(s.shown("B7"), "#VALUE!");

    s.click_cell("B8");
    s.enter("=FACTDOUBLE(0)", "enter");
    assert_eq!(s.shown("B8"), "1");

    s.click_cell("B9");
    s.enter("=FACTDOUBLE(\"2\")", "enter");
    assert_eq!(s.shown("B9"), "2");

    // 3. Undo and Redo
    s.click_cell("C1");
    s.enter("=FACTDOUBLE(6)", "enter");
    assert_eq!(s.shown("C1"), "48");
    s.key("cmd-z");
    assert_eq!(s.value("C1"), ibis_doc_model::CellValue::Empty);
    s.key("cmd-shift-z");
    assert_eq!(s.shown("C1"), "48");
    s.key("cmd-z");

    // 4. 保存と再読み込みラウンドトリップ
    s.save_as("fn_factdouble_function.xlsx");
    let reopened = s.reopen();
    let re_sh = sheet_of_slide(&reopened.slides[1]).unwrap();
    assert_eq!(re_sh.cell(0, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(48.0)));
    assert_eq!(re_sh.cell(1, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(105.0)));
    assert_eq!(re_sh.cell(2, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Value)));
    assert_eq!(re_sh.cell(3, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(1.0)));
    assert_eq!(re_sh.cell(4, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Num)));
    assert_eq!(re_sh.cell(5, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Value)));
    assert_eq!(re_sh.cell(7, 1).map(|c| &c.value), Some(&ibis_doc_model::CellValue::Number(1.0)));
}





fn move_chart_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.move_chart.as_ref().expect("move chart panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("move chart button not found: {id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}

/// XLSX-CHEDIT-020:
/// グラフの移動 (Move Chart to Chart Sheet or Existing Sheet):
/// 1. 既存シートへのオブジェクトとしての移動 (ChartMoveTarget::ExistingSheet)
/// 2. 新規シートへの移動 (ChartMoveTarget::NewSheet)
/// 3. Undo / Redo による可逆性
/// 4. ブックの保護（構造のロック）による制限の検証
fn chedit020_move_chart_to_existing_and_new_sheet() {
    use ibis_doc_model::{Command, SheetCommand, WorkbookProtection};

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. Initial state: slide 0 ("月次売上") has the chart
    let chart_idx = s.with(|e, _| {
        e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))).expect("chart exists on 月次売上")
    });
    let chart_pos = s.shape_position(chart_idx);
    s.click(chart_pos);
    s.settle();

    // 2. Open Move Chart panel
    s.menu("xlsx.move-chart");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.move_chart.is_some()));

    // 3. Move to existing sheet ("メモ", sheet item 2)
    move_chart_click(&mut s, "move-chart-mode-existing");
    s.settle();
    move_chart_click(&mut s, "move-chart-sheet-item-2");
    s.settle();
    move_chart_click(&mut s, "move-chart-ok");
    s.settle();

    assert!(s.with(|e, _| e.wb().panels.move_chart.is_none()));
    // Current slide is now "メモ"
    assert_eq!(s.with(|e, _| e.slide().name.clone()), Some("メモ".to_string()));
    let memo_has_chart = s.with(|e, _| e.slide().shapes.iter().any(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))));
    assert!(memo_has_chart, "メモ should now contain the chart");

    // Verify "月次売上" no longer has chart
    s.update(|e, window, cx| {
        let sales_id = e.presentation().slides[0].id;
        e.select_slide(sales_id, window, cx);
    });
    s.settle();
    let sales_has_chart = s.with(|e, _| e.slide().shapes.iter().any(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))));
    assert!(!sales_has_chart, "月次売上 should no longer contain the chart");

    // 4. Test Undo
    s.key("cmd-z");
    s.settle();
    let sales_has_chart_after_undo = s.with(|e, _| e.slide().shapes.iter().any(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))));
    assert!(sales_has_chart_after_undo, "月次売上 should have chart restored on undo");

    // 5. Test Redo
    s.key("cmd-shift-z");
    s.settle();
    // Switch to "メモ"
    s.update(|e, window, cx| {
        let memo_id = e.presentation().slides.iter().find(|sl| sl.name.as_deref() == Some("メモ")).unwrap().id;
        e.select_slide(memo_id, window, cx);
    });
    s.settle();
    let memo_has_chart_redo = s.with(|e, _| e.slide().shapes.iter().any(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))));
    assert!(memo_has_chart_redo, "メモ should have chart restored on redo");

    // 6. Move to a new sheet
    let memo_chart_idx = s.with(|e, _| {
        e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))).expect("chart on メモ")
    });
    let memo_chart_pos = s.shape_position(memo_chart_idx);
    s.click(memo_chart_pos);
    s.settle();

    s.menu("xlsx.move-chart");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.move_chart.is_some()));

    move_chart_click(&mut s, "move-chart-mode-new");
    s.settle();
    move_chart_click(&mut s, "move-chart-ok");
    s.settle();

    assert!(s.with(|e, _| e.wb().panels.move_chart.is_none()));
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 4);
    let new_sheet_name = s.with(|e, _| e.slide().name.clone().unwrap_or_default());
    assert!(new_sheet_name.starts_with("Chart"));
    let new_has_chart = s.with(|e, _| e.slide().shapes.iter().any(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))));
    assert!(new_has_chart, "new sheet should contain the chart");

    // Undo moving to new sheet
    s.key("cmd-z");
    s.settle();
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 3);

    // Redo moving to new sheet
    s.key("cmd-shift-z");
    s.settle();
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 4);
    s.update(|e, window, cx| {
        let new_slide_id = e.presentation().slides[3].id;
        e.select_slide(new_slide_id, window, cx);
    });
    s.settle();

    // 7. Test workbook structure protection:
    // When workbook structure is locked, creating a new sheet via MoveChart is blocked
    s.update(|e, _, cx| {
        e.run_sheet(
            vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
                protection: Some(WorkbookProtection {
                    lock_structure: true,
                    lock_windows: false,
                    ..Default::default()
                }),
            })],
            cx,
        );
    });
    s.settle();

    // Select chart on new sheet
    let chart_idx3 = s.with(|e, _| {
        e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))).expect("chart on new sheet")
    });
    let chart_pos3 = s.shape_position(chart_idx3);
    s.click(chart_pos3);
    s.settle();

    s.menu("xlsx.move-chart");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.move_chart.is_some()));

    // Try to click OK in NewSheet mode while structure is locked
    move_chart_click(&mut s, "move-chart-mode-new");
    s.settle();
    move_chart_click(&mut s, "move-chart-ok");
    s.settle();

    // Panel should still be open with error message
    assert!(s.with(|e, _| e.wb().panels.move_chart.is_some()));
    assert!(s.with(|e, _| e.wb().panels.move_chart.as_ref().unwrap().error.is_some()));

    // Close panel
    move_chart_click(&mut s, "move-chart-cancel");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.move_chart.is_none()));

    // Cleanup protection
    s.update(|e, _, cx| {
        e.run_sheet(
            vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
                protection: None,
            })],
            cx,
        );
    });
    s.settle();
}

fn chart_template_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.chart_template.as_ref().expect("chart template panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("chart template button not found: {id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}

/// XLSX-CHEDIT-021:
/// グラフ テンプレート (.crtx) の保存・読込・適用・編集・DrawingML ラウンドトリップ:
/// 1. 既存グラフのテンプレートとしての保存 (.crtx パッケージ生成、ZIP構造検証)
/// 2. テンプレートの適用 (データ系列を維持しつつスタイル・種別等を適用)
/// 3. Undo / Redo による可逆性
/// 4. テンプレートの削除
fn chedit021_chart_templates() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. Initial state: slide 0 ("月次売上") has the chart
    let chart_idx = s.with(|e, _| {
        e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))).expect("chart exists on 月次売上")
    });
    let chart_pos = s.shape_position(chart_idx);
    s.click(chart_pos);
    s.settle();

    let orig_chart = s.with(|e, _| {
        match &e.slide().shapes[chart_idx].kind {
            ibis_doc_model::ShapeKind::Chart(c) => (**c).clone(),
            _ => panic!("not a chart"),
        }
    });

    // 2. Open Chart Template panel in Save mode
    s.menu("xlsx.chart.save-template");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.chart_template.is_some()));
    assert_eq!(s.with(|e, _| e.wb().panels.chart_template.as_ref().unwrap().mode), crate::xlsx::chart_template::ChartTemplateMode::Save);

    // Set template name
    let tmpl_name = format!("TestTmpl_{}", std::process::id());
    s.update(|e, window, cx| {
        let p = e.wb().panels.chart_template.as_ref().unwrap();
        p.template_name.update(cx, |i, cx| i.set_value(tmpl_name.clone(), window, cx));
    });
    s.settle();

    // Click Save
    chart_template_click(&mut s, "chart-template-save");
    s.settle();

    // Verify .crtx file was created on disk
    let tmpl_dir = crate::xlsx::chart_template::default_chart_templates_dir();
    let tmpl_path = tmpl_dir.join(format!("{tmpl_name}.crtx"));
    assert!(tmpl_path.is_file(), "Saved .crtx file must exist at {tmpl_path:?}");

    // Read and verify .crtx is a valid OPC package
    let crtx_bytes = std::fs::read(&tmpl_path).expect("read saved crtx");
    assert_eq!(&crtx_bytes[..2], b"PK", "crtx must be zip");
    let loaded_chart = ibis_pptx::read_crtx(&crtx_bytes).expect("read_crtx succeeds");
    assert_eq!(loaded_chart.groups[0].kind, orig_chart.groups[0].kind);

    // 3. Mutate target chart: change style
    s.update(|e, _, cx| {
        e.change_sheet_chart(|target| {
            target.style = Some(42);
        }, cx);
    });
    s.settle();

    let mutated_chart = s.with(|e, _| {
        match &e.slide().shapes[chart_idx].kind {
            ibis_doc_model::ShapeKind::Chart(c) => (**c).clone(),
            _ => panic!("not a chart"),
        }
    });
    assert_eq!(mutated_chart.style, Some(42));

    // 4. Open Chart Template panel in Apply mode
    s.menu("xlsx.chart.apply-template");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.chart_template.is_some()));
    assert_eq!(s.with(|e, _| e.wb().panels.chart_template.as_ref().unwrap().mode), crate::xlsx::chart_template::ChartTemplateMode::Apply);

    // Find the template in the list and select it
    let tmpl_idx = s.with(|e, _| {
        e.wb().panels.chart_template.as_ref().unwrap().templates.iter().position(|t| t.name == tmpl_name)
    }).expect("saved template in list");

    s.update(|e, _, cx| {
        e.set_selected_chart_template(tmpl_idx, cx);
    });
    s.settle();

    // Click Apply
    chart_template_click(&mut s, "chart-template-apply");
    s.settle();

    // Panel should be closed now
    assert!(s.with(|e, _| e.wb().panels.chart_template.is_none()));

    // Chart should have restored orig style while preserving series data
    let applied_chart = s.with(|e, _| {
        match &e.slide().shapes[chart_idx].kind {
            ibis_doc_model::ShapeKind::Chart(c) => (**c).clone(),
            _ => panic!("not a chart"),
        }
    });
    assert_eq!(applied_chart.style, orig_chart.style);
    assert_eq!(applied_chart.groups[0].series[0].values, orig_chart.groups[0].series[0].values);

    // 5. Test Undo / Redo
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.settle();
    let undone_chart = s.with(|e, _| {
        match &e.slide().shapes[chart_idx].kind {
            ibis_doc_model::ShapeKind::Chart(c) => (**c).clone(),
            _ => panic!("not a chart"),
        }
    });
    assert_eq!(undone_chart.style, Some(42), "Undo should restore mutated style 42");

    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.settle();
    let redone_chart = s.with(|e, _| {
        match &e.slide().shapes[chart_idx].kind {
            ibis_doc_model::ShapeKind::Chart(c) => (**c).clone(),
            _ => panic!("not a chart"),
        }
    });
    assert_eq!(redone_chart.style, orig_chart.style, "Redo should restore template style");

    // 6. Test Delete Template
    s.menu("xlsx.chart.apply-template");
    s.settle();
    let tmpl_idx_now = s.with(|e, _| {
        e.wb().panels.chart_template.as_ref().unwrap().templates.iter().position(|t| t.name == tmpl_name)
    }).expect("saved template in list before delete");
    s.update(|e, _, cx| {
        e.set_selected_chart_template(tmpl_idx_now, cx);
    });
    s.settle();
    chart_template_click(&mut s, "chart-template-delete");
    s.settle();

    assert!(!tmpl_path.exists(), "Template file should be deleted");

    // Close panel
    chart_template_click(&mut s, "chart-template-cancel");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.chart_template.is_none()));

    // Clean up if still exists
    let _ = std::fs::remove_file(&tmpl_path);
}

fn chtype022_radar_chart_styles_and_rendering() {
    use ibis_doc_model::chart::{ChartType, RadarStyle};
    use ibis_doc_model::ShapeKind;

    // 1. Radar チャートを持つ XLSX フィクスチャを target/scenario-input に作成して開く
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("chtype022-radar.xlsx");

    let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:radarChart><c:radarStyle val="standard"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>S1</c:v></c:tx><c:cat><c:strLit><c:ptCount val="3"/><c:pt idx="0"><c:v>A</c:v></c:pt><c:pt idx="1"><c:v>B</c:v></c:pt><c:pt idx="2"><c:v>C</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="3"/><c:pt idx="0"><c:v>10</c:v></c:pt><c:pt idx="1"><c:v>20</c:v></c:pt><c:pt idx="2"><c:v>30</c:v></c:pt></c:numLit></c:val></c:ser><c:axId val="111"/><c:axId val="222"/></c:radarChart></c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend></c:chart></c:chartSpace>"#;
    let original = std::fs::read(crate::shell::scenario::repo_root().join("test/editing/xlsx-sales.xlsx")).unwrap();
    let bytes = ibis_pptx::test_files::xlsx_with_chart_xml(&original, "xl/charts/chart1.xml", xml);
    std::fs::write(&source, bytes).unwrap();

    let mut s = Scenario::open("target/scenario-input/chtype022-radar.xlsx");

    // 2. グラフを選択して書式パネルを開く
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.settle();
    s.menu("xlsx.chart-format");
    s.settle();
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::Radar { style: Some(RadarStyle::Standard) });

    // 3. 書式パネル経由でレーダー スタイルを「マーカー付き」に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_radar_style(Some(RadarStyle::Marker), cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::Radar { style: Some(RadarStyle::Marker) });

    // 4. 書式パネル経由でレーダー スタイルを「塗りつぶし」に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_radar_style(Some(RadarStyle::Filled), cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::Radar { style: Some(RadarStyle::Filled) });

    // 5. 1手 Undo / Redo
    s.key("cmd-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::Radar { style: Some(RadarStyle::Marker) });

    s.key("cmd-shift-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::Radar { style: Some(RadarStyle::Filled) });

    // 6. ネイティブ描画の検証:
    // chart_element がレーダー グラフを描画できることを確認
    s.with(|e, cx| {
        let (_, c) = e.sheet_chart().unwrap();
        let res = crate::render::shape_view::Resources::new(e.presentation(), cx);
        let frame = ibis_doc_model::EmuRect::new(0, 0, 4_000_000, 3_000_000);
        let _div = crate::render::chart_view::chart_element(c, frame, 1.0, &res);
    });

    // 7. 保存と再読み込み（ラウンドトリップ検証）
    s.save_as("radar-chart-filled-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ShapeKind::Chart(_))).unwrap();
    let ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.groups[0].kind, ChartType::Radar { style: Some(RadarStyle::Filled) });
}

fn chtype011_of_pie_chart_styles_and_rendering() {
    use ibis_doc_model::chart::{ChartType, OfPieKind, OfPieSplit};
    use ibis_doc_model::{Number, ShapeKind};

    // 1. OfPie チャートを持つ XLSX フィクスチャを target/scenario-input に作成して開く
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("chtype011-of-pie.xlsx");

    let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:ofPieChart><c:ofPieType val="pie"/><c:splitType val="pos"/><c:splitPos val="2"/><c:secondPieSize val="75"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>S1</c:v></c:tx><c:cat><c:strLit><c:ptCount val="4"/><c:pt idx="0"><c:v>A</c:v></c:pt><c:pt idx="1"><c:v>B</c:v></c:pt><c:pt idx="2"><c:v>C</c:v></c:pt><c:pt idx="3"><c:v>D</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="4"/><c:pt idx="0"><c:v>40</c:v></c:pt><c:pt idx="1"><c:v>30</c:v></c:pt><c:pt idx="2"><c:v>20</c:v></c:pt><c:pt idx="3"><c:v>10</c:v></c:pt></c:numLit></c:val></c:ser></c:ofPieChart></c:plotArea><c:legend><c:legendPos val="r"/><c:overlay val="0"/></c:legend></c:chart></c:chartSpace>"#;
    let original = std::fs::read(crate::shell::scenario::repo_root().join("test/editing/xlsx-sales.xlsx")).unwrap();
    let bytes = ibis_pptx::test_files::xlsx_with_chart_xml(&original, "xl/charts/chart1.xml", xml);
    std::fs::write(&source, bytes).unwrap();

    let mut s = Scenario::open("target/scenario-input/chtype011-of-pie.xlsx");

    // 2. グラフを選択して書式パネルを開く
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.settle();
    s.menu("xlsx.chart-format");
    s.settle();
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::OfPie { kind: Some(OfPieKind::Pie) });
    let op = c.groups[0].of_pie.as_ref().unwrap();
    assert_eq!(op.split, Some(OfPieSplit::Position));
    assert_eq!(op.split_position, Some(Number(2.0)));
    assert_eq!(op.second_size, Some(75));

    // 3. 書式パネル経由で種類を「補助棒」に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_of_pie_kind(Some(OfPieKind::Bar), cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::OfPie { kind: Some(OfPieKind::Bar) });

    // 4. 書式パネル経由で分割基準を「値」、位置を 15、サイズを 85 に変更
    s.update(|e, _, cx| {
        e.set_sheet_chart_of_pie_split(Some(OfPieSplit::Value), cx);
        e.set_sheet_chart_of_pie_split_position(Some(Number(15.0)), cx);
        e.set_sheet_chart_of_pie_second_size(Some(85), cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::OfPie { kind: Some(OfPieKind::Bar) });
    let op = c.groups[0].of_pie.as_ref().unwrap();
    assert_eq!(op.split, Some(OfPieSplit::Value));
    assert_eq!(op.split_position, Some(Number(15.0)));
    assert_eq!(op.second_size, Some(85));

    // 5. 1手 Undo / Redo
    s.key("cmd-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].of_pie.as_ref().unwrap().second_size, Some(75));

    s.key("cmd-shift-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].of_pie.as_ref().unwrap().second_size, Some(85));

    // 6. ネイティブ描画の検証:
    // chart_element が OfPie グラフ（補助円・補助棒）を描画できることを確認
    s.with(|e, cx| {
        let (_, c) = e.sheet_chart().unwrap();
        let res = crate::render::shape_view::Resources::new(e.presentation(), cx);
        let frame = ibis_doc_model::EmuRect::new(0, 0, 4_000_000, 3_000_000);
        let _div = crate::render::chart_view::chart_element(c, frame, 1.0, &res);
    });

    // 7. 保存と再読み込み（ラウンドトリップ検証）
    s.save_as("of-pie-chart-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ShapeKind::Chart(_))).unwrap();
    let ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.groups[0].kind, ChartType::OfPie { kind: Some(OfPieKind::Bar) });
    let op = c.groups[0].of_pie.as_ref().unwrap();
    assert_eq!(op.split, Some(OfPieSplit::Value));
    assert_eq!(op.split_position, Some(Number(15.0)));
    assert_eq!(op.second_size, Some(85));
}

fn chtype018_stock_chart_hlc_and_ohlc() {
    use ibis_doc_model::chart::ChartType;
    use ibis_doc_model::ShapeKind;

    // 1. Stock (HLC) チャートを持つ XLSX フィクスチャを target/scenario-input に作成して開く
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("chtype018-stock.xlsx");

    let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:stockChart><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>高値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>30</c:v></c:pt><c:pt idx="1"><c:v>35</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="1"/><c:order val="1"/><c:tx><c:v>安値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>20</c:v></c:pt><c:pt idx="1"><c:v>22</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="2"/><c:order val="2"/><c:tx><c:v>終値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>25</c:v></c:pt><c:pt idx="1"><c:v>31</c:v></c:pt></c:numLit></c:val></c:ser><c:hiLowLines/><c:axId val="100000001"/><c:axId val="100000002"/></c:stockChart><c:catAx><c:axId val="100000001"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="100000002"/></c:catAx><c:valAx><c:axId val="100000002"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="100000001"/></c:valAx></c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend></c:chart></c:chartSpace>"#;
    let original = std::fs::read(crate::shell::scenario::repo_root().join("test/editing/xlsx-sales.xlsx")).unwrap();
    let bytes = ibis_pptx::test_files::xlsx_with_chart_xml(&original, "xl/charts/chart1.xml", xml);
    std::fs::write(&source, bytes).unwrap();

    let mut s = Scenario::open("target/scenario-input/chtype018-stock.xlsx");

    // 2. グラフを選択して書式パネルを開く
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.settle();
    s.menu("xlsx.chart-format");
    s.settle();
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let c = chart(&mut s);
    assert_eq!(
        c.groups[0].kind,
        ChartType::Stock {
            up_down_bars: false,
            hi_low_lines: true,
            drop_lines: false,
        }
    );

    // 3. 書式パネル経由で「陽線・陰線」と「降下線」を有効化
    s.update(|e, _, cx| {
        e.set_sheet_chart_stock_up_down_bars(true, cx);
        e.set_sheet_chart_stock_drop_lines(true, cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(
        c.groups[0].kind,
        ChartType::Stock {
            up_down_bars: true,
            hi_low_lines: true,
            drop_lines: true,
        }
    );

    // 4. 1手 Undo / Redo
    s.key("cmd-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(
        c.groups[0].kind,
        ChartType::Stock {
            up_down_bars: true,
            hi_low_lines: true,
            drop_lines: false,
        }
    );

    s.key("cmd-shift-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(
        c.groups[0].kind,
        ChartType::Stock {
            up_down_bars: true,
            hi_low_lines: true,
            drop_lines: true,
        }
    );

    // 5. ネイティブ描画の検証:
    // chart_element が Stock チャート（高値安値線、陽線陰線、降下線）を描画できることを確認
    s.with(|e, cx| {
        let (_, c) = e.sheet_chart().unwrap();
        let res = crate::render::shape_view::Resources::new(e.presentation(), cx);
        let frame = ibis_doc_model::EmuRect::new(0, 0, 4_000_000, 3_000_000);
        let _div = crate::render::chart_view::chart_element(c, frame, 1.0, &res);
    });

    // 6. 保存と再読み込み（ラウンドトリップ検証）
    s.save_as("stock-chart-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ShapeKind::Chart(_))).unwrap();
    let ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(
        c.groups[0].kind,
        ChartType::Stock {
            up_down_bars: true,
            hi_low_lines: true,
            drop_lines: true,
        }
    );
}

fn chtype020_stock_chart_volume_vhlc_and_vohlc() {
    use ibis_doc_model::chart::ChartType;
    use ibis_doc_model::ShapeKind;

    // 1. 出来高付き株価チャート (VHLC) を持つ XLSX フィクスチャを target/scenario-input に作成して開く
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("chtype020-volume-stock.xlsx");

    let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>出来高</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>1000</c:v></c:pt><c:pt idx="1"><c:v>1500</c:v></c:pt></c:numLit></c:val></c:ser><c:axId val="100000001"/><c:axId val="100000002"/></c:barChart><c:stockChart><c:ser><c:idx val="1"/><c:order val="1"/><c:tx><c:v>高値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>30</c:v></c:pt><c:pt idx="1"><c:v>35</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="2"/><c:order val="2"/><c:tx><c:v>安値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>20</c:v></c:pt><c:pt idx="1"><c:v>22</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="3"/><c:order val="3"/><c:tx><c:v>終値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>25</c:v></c:pt><c:pt idx="1"><c:v>31</c:v></c:pt></c:numLit></c:val></c:ser><c:hiLowLines/><c:axId val="100000001"/><c:axId val="100000003"/></c:stockChart><c:catAx><c:axId val="100000001"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="100000002"/></c:catAx><c:valAx><c:axId val="100000002"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="100000001"/></c:valAx><c:valAx><c:axId val="100000003"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="r"/><c:crossAx val="100000001"/><c:crosses val="max"/></c:valAx></c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend></c:chart></c:chartSpace>"#;
    let original = std::fs::read(crate::shell::scenario::repo_root().join("test/editing/xlsx-sales.xlsx")).unwrap();
    let bytes = ibis_pptx::test_files::xlsx_with_chart_xml(&original, "xl/charts/chart1.xml", xml);
    std::fs::write(&source, bytes).unwrap();

    let mut s = Scenario::open("target/scenario-input/chtype020-volume-stock.xlsx");

    // 2. グラフを選択して書式パネルを開く
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.settle();
    s.menu("xlsx.chart-format");
    s.settle();
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let c = chart(&mut s);
    assert_eq!(c.groups.len(), 2);
    assert!(matches!(c.groups[0].kind, ChartType::Bar { horizontal: false, .. }));
    assert_eq!(c.groups[0].series.len(), 1);
    assert_eq!(
        c.groups[1].kind,
        ChartType::Stock {
            up_down_bars: false,
            hi_low_lines: true,
            drop_lines: false,
        }
    );
    assert_eq!(c.groups[1].series.len(), 3);
    assert_eq!(c.axes.len(), 3);

    // 3. 書式パネル経由で「陽線・陰線」と「降下線」を有効化
    s.update(|e, _, cx| {
        e.set_sheet_chart_stock_up_down_bars(true, cx);
        e.set_sheet_chart_stock_drop_lines(true, cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(
        c.groups[1].kind,
        ChartType::Stock {
            up_down_bars: true,
            hi_low_lines: true,
            drop_lines: true,
        }
    );

    // 4. 1手 Undo / Redo
    s.key("cmd-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(
        c.groups[1].kind,
        ChartType::Stock {
            up_down_bars: true,
            hi_low_lines: true,
            drop_lines: false,
        }
    );

    s.key("cmd-shift-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(
        c.groups[1].kind,
        ChartType::Stock {
            up_down_bars: true,
            hi_low_lines: true,
            drop_lines: true,
        }
    );

    // 5. ネイティブ描画の検証:
    // chart_element が複合グループ（出来高 棒グラフ + 株価線・陽線陰線・降下線）を左右2軸で描画できることを確認
    s.with(|e, cx| {
        let (_, c) = e.sheet_chart().unwrap();
        let res = crate::render::shape_view::Resources::new(e.presentation(), cx);
        let frame = ibis_doc_model::EmuRect::new(0, 0, 4_000_000, 3_000_000);
        let _div = crate::render::chart_view::chart_element(c, frame, 1.0, &res);
    });

    // 6. 保存と再読み込み（ラウンドトリップ検証）
    s.save_as("volume-stock-chart-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ShapeKind::Chart(_))).unwrap();
    let ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.groups.len(), 2);
    assert!(matches!(c.groups[0].kind, ChartType::Bar { horizontal: false, .. }));
    assert_eq!(
        c.groups[1].kind,
        ChartType::Stock {
            up_down_bars: true,
            hi_low_lines: true,
            drop_lines: true,
        }
    );
    assert_eq!(c.axes.len(), 3);
}

/// XLSX-CHTYPE-021: 等高線・3D表面グラフ
/// 1. 3D表面グラフ (`surface3DChart`) を持つ XLSX フィクスチャを作成して開く
/// 2. グラフ選択と書式パネル表示
/// 3. 書式パネルからのサブタイプ切替 (3Dワイヤーフレーム、2D等高線、2D等高線ワイヤーフレーム)
/// 4. 1手 Undo / Redo の検証
/// 5. ネイティブ描画 (3Dメッシュ、等高線バンド、ワイヤーフレーム、凡例) の検証
/// 6. 保存と再読み込み (OOXML ラウンドトリップ) の検証
fn chtype021_surface_and_contour_charts() {
    use ibis_doc_model::chart::ChartType;
    use ibis_doc_model::ShapeKind;

    // 1. 3D表面グラフ (surface3DChart, wireframe: 0) を持つ XLSX フィクスチャを target/scenario-input に作成して開く
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("chtype021-surface-scenario.xlsx");

    let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:surface3DChart><c:wireframe val="0"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>系列1</c:v></c:tx><c:cat><c:strLit><c:ptCount val="3"/><c:pt idx="0"><c:v>X1</c:v></c:pt><c:pt idx="1"><c:v>X2</c:v></c:pt><c:pt idx="2"><c:v>X3</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="3"/><c:pt idx="0"><c:v>10</c:v></c:pt><c:pt idx="1"><c:v>20</c:v></c:pt><c:pt idx="2"><c:v>15</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="1"/><c:order val="1"/><c:tx><c:v>系列2</c:v></c:tx><c:cat><c:strLit><c:ptCount val="3"/><c:pt idx="0"><c:v>X1</c:v></c:pt><c:pt idx="1"><c:v>X2</c:v></c:pt><c:pt idx="2"><c:v>X3</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="3"/><c:pt idx="0"><c:v>30</c:v></c:pt><c:pt idx="1"><c:v>25</c:v></c:pt><c:pt idx="2"><c:v>35</c:v></c:pt></c:numLit></c:val></c:ser><c:axId val="111"/><c:axId val="222"/><c:axId val="333"/></c:surface3DChart><c:catAx><c:axId val="111"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="222"/></c:catAx><c:valAx><c:axId val="222"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="111"/></c:valAx><c:serAx><c:axId val="333"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="222"/></c:serAx></c:plotArea><c:legend><c:legendPos val="r"/><c:overlay val="0"/></c:legend></c:chart></c:chartSpace>"#;
    let original = std::fs::read(crate::shell::scenario::repo_root().join("test/editing/xlsx-sales.xlsx")).unwrap();
    let bytes = ibis_pptx::test_files::xlsx_with_chart_xml(&original, "xl/charts/chart1.xml", xml);
    std::fs::write(&source, bytes).unwrap();

    let mut s = Scenario::open("target/scenario-input/chtype021-surface-scenario.xlsx");

    // 2. グラフを選択して書式パネルを開く
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.settle();
    s.menu("xlsx.chart-format");
    s.settle();
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let c = chart(&mut s);
    assert_eq!(c.groups.len(), 1);
    assert_eq!(c.groups[0].kind, ChartType::Surface { is_3d: true, wireframe: false });
    assert_eq!(c.axes.len(), 3);

    // 3. 書式パネル経由で「3Dワイヤーフレーム」に切り替え
    s.update(|e, _, cx| {
        e.set_sheet_chart_surface_type(true, true, cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::Surface { is_3d: true, wireframe: true });

    // 4. 書式パネル経由で「等高線 (2D, 塗り)」に切り替え
    s.update(|e, _, cx| {
        e.set_sheet_chart_surface_type(false, false, cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::Surface { is_3d: false, wireframe: false });

    // 5. 書式パネル経由で「ワイヤーフレーム等高線 (2D, 枠線のみ)」に切り替え
    s.update(|e, _, cx| {
        e.set_sheet_chart_surface_type(false, true, cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::Surface { is_3d: false, wireframe: true });

    // 6. 1手 Undo / Redo
    s.key("cmd-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::Surface { is_3d: false, wireframe: false });

    s.key("cmd-shift-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].kind, ChartType::Surface { is_3d: false, wireframe: true });

    // 7. ネイティブ描画の検証 (3D描画 & 2D等高線描画 & 凡例)
    s.with(|e, cx| {
        let (_, c) = e.sheet_chart().unwrap();
        let res = crate::render::shape_view::Resources::new(e.presentation(), cx);
        let frame = ibis_doc_model::EmuRect::new(0, 0, 4_000_000, 3_000_000);
        let _div = crate::render::chart_view::chart_element(c, frame, 1.0, &res);
    });

    // 8. 保存と再読み込み (ラウンドトリップ検証)
    s.save_as("surface-contour-scenario.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ShapeKind::Chart(_))).unwrap();
    let ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.groups.len(), 1);
    assert_eq!(c.groups[0].kind, ChartType::Surface { is_3d: false, wireframe: true });
}


/// XLSX-CHEDIT-001: データ範囲と系列の変更
/// 1. 書式パネルからの系列追加（列方向の拡張）
/// 2. 系列の並べ替え（上へ移動 / 下へ移動）
/// 3. 系列の削除（単一系列、最後の1系列削除防止エラー）
/// 4. 1手 Undo / Redo
/// 5. 保存と再読み込み（OOXML ラウンドトリップ）
fn chedit001_series_add_delete_and_reorder() {
    use ibis_doc_model::ShapeKind;

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. グラフを選択して書式パネルを開く
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.settle();
    s.menu("xlsx.chart-format");
    s.settle();
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));

    let chart = |s: &mut Scenario| s.with(|e, _| e.sheet_chart().map(|(_, c)| c.clone())).unwrap();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series.len(), 3);
    assert_eq!(c.groups[0].series[0].name, "4月");
    assert_eq!(c.groups[0].series[1].name, "5月");
    assert_eq!(c.groups[0].series[2].name, "6月");
    let range_val = s.with(|e, cx| e.wb().chart_edit.as_ref().unwrap().range.read(cx).value().to_string());
    assert_eq!(range_val, "月次売上!$A$3:$D$8");

    // 2. 系列の追加（add_sheet_chart_series）
    s.update(|e, window, cx| {
        e.add_sheet_chart_series(window, cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series.len(), 4);
    assert_eq!(c.groups[0].series[3].name, "合計");
    let range_val_after_add = s.with(|e, cx| e.wb().chart_edit.as_ref().unwrap().range.read(cx).value().to_string());
    assert_eq!(range_val_after_add, "月次売上!$A$3:$E$8");

    // 3. 系列の並べ替え（move_sheet_chart_series: 系列1「5月」を上へ移動）
    s.update(|e, window, cx| {
        e.move_sheet_chart_series(1, true, window, cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].name, "5月");
    assert_eq!(c.groups[0].series[1].name, "4月");

    // 下へ戻す
    s.update(|e, window, cx| {
        e.move_sheet_chart_series(0, false, window, cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].name, "4月");
    assert_eq!(c.groups[0].series[1].name, "5月");

    // 4. 系列の削除（delete_sheet_chart_series: 追加した系列3を削除）
    s.update(|e, window, cx| {
        e.delete_sheet_chart_series(3, window, cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series.len(), 3);
    let range_val_after_del = s.with(|e, cx| e.wb().chart_edit.as_ref().unwrap().range.read(cx).value().to_string());
    assert_eq!(range_val_after_del, "月次売上!$A$3:$D$8");

    // 5. 1手 Undo / Redo
    s.key("cmd-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series.len(), 4);

    s.key("cmd-shift-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series.len(), 3);

    // 6. 残り1系列時の削除防止エラー検証
    s.update(|e, window, cx| {
        e.delete_sheet_chart_series(2, window, cx);
    });
    s.settle();
    s.update(|e, window, cx| {
        e.delete_sheet_chart_series(1, window, cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series.len(), 1);

    s.update(|e, window, cx| {
        e.delete_sheet_chart_series(0, window, cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series.len(), 1);
    let err = s.with(|e, _| e.wb().chart_edit.as_ref().and_then(|p| p.error.clone()));
    assert_eq!(err, Some(crate::shell::i18n::text("xlsx.chart-series-last-error").into()));

    // Undo 2回で3系列に戻す
    s.key("cmd-z");
    s.settle();
    s.key("cmd-z");
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series.len(), 3);

    // 7. 系列を追加し、並べ替えた状態で保存・再読込
    s.update(|e, window, cx| {
        e.add_sheet_chart_series(window, cx);
    });
    s.settle();
    s.update(|e, window, cx| {
        e.move_sheet_chart_series(1, true, window, cx);
    });
    s.settle();
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series.len(), 4);
    assert_eq!(c.groups[0].series[0].name, "5月");
    assert_eq!(c.groups[0].series[1].name, "4月");

    // 8. 保存と再読み込み（OOXML ラウンドトリップ）
    s.save_as("chedit001-series-modify.xlsx");
    let saved = s.reopen();
    let sh = saved.slides[0].shapes.iter().find(|x| matches!(x.kind, ShapeKind::Chart(_))).unwrap();
    let ShapeKind::Chart(c) = &sh.kind else { unreachable!() };
    assert_eq!(c.groups[0].series.len(), 4);
    assert_eq!(c.groups[0].series[0].name, "5月");
    assert_eq!(c.groups[0].series[1].name, "4月");
    assert_eq!(c.groups[0].series[2].name, "6月");
    assert_eq!(c.groups[0].series[3].name, "合計");
}

fn arrange_windows_click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.wb().panels.arrange_windows.as_ref().expect("the panel").ui.borrow().get(&id).unwrap_or_else(|| panic!("arrange window button not found: {id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}

/// XLSX-SHEET-017:
/// 複数ブックの整列 (Arrange Windows / Arrange All):
/// 並べて表示 (Tiled)・上下に整列 (Horizontal)・左右に整列 (Vertical)・重ねて表示 (Cascade)、
/// 作業中ブック対象オプション、ダイアログ/パネルUI、複数ウィンドウサイズ変更・整列の検証。
fn sheet017_arrange_windows() {
    use crate::xlsx::arrange_windows::ArrangeKind;
    let mut s = new_sheet();

    // 1. Initial menu state
    assert_eq!(s.menu_state("xlsx.arrange-all"), Some((true, false)));

    // 2. Open arrange windows panel
    s.menu("xlsx.arrange-all");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.arrange_windows.is_some()));
    assert_eq!(s.with(|e, _| e.wb().panels.arrange_windows.as_ref().unwrap().kind), ArrangeKind::Tiled);
    assert!(!s.with(|e, _| e.wb().panels.arrange_windows.as_ref().unwrap().active_workbook_only));

    // 3. Radio selection tests
    arrange_windows_click(&mut s, "arrange-horizontal");
    s.settle();
    assert_eq!(s.with(|e, _| e.wb().panels.arrange_windows.as_ref().unwrap().kind), ArrangeKind::Horizontal);

    arrange_windows_click(&mut s, "arrange-vertical");
    s.settle();
    assert_eq!(s.with(|e, _| e.wb().panels.arrange_windows.as_ref().unwrap().kind), ArrangeKind::Vertical);

    arrange_windows_click(&mut s, "arrange-cascade");
    s.settle();
    assert_eq!(s.with(|e, _| e.wb().panels.arrange_windows.as_ref().unwrap().kind), ArrangeKind::Cascade);

    arrange_windows_click(&mut s, "arrange-tiled");
    s.settle();
    assert_eq!(s.with(|e, _| e.wb().panels.arrange_windows.as_ref().unwrap().kind), ArrangeKind::Tiled);

    // 4. Toggle active workbook only option
    arrange_windows_click(&mut s, "arrange-active-only");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.arrange_windows.as_ref().unwrap().active_workbook_only));

    arrange_windows_click(&mut s, "arrange-active-only");
    s.settle();
    assert!(!s.with(|e, _| e.wb().panels.arrange_windows.as_ref().unwrap().active_workbook_only));

    // 5. Cancel closes panel without action
    arrange_windows_click(&mut s, "arrange-cancel");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.arrange_windows.is_none()));

    // 6. Test multi-window arrangement
    let initial_windows = s.with(|_, app| app.windows());
    assert_eq!(initial_windows.len(), 1);

    // Open second window
    s.update(|e, _, cx| e.new_window(cx));
    s.settle();
    let windows = s.with(|_, app| app.windows());
    assert_eq!(windows.len(), 2, "second window opened");

    let (screen_w, screen_h) = s.with(|_, app| {
        app.primary_display()
            .map(|d| {
                let b = d.visible_bounds();
                (f32::from(b.size.width), f32::from(b.size.height))
            })
            .unwrap_or((1280.0, 800.0))
    });

    // Arrange Horizontally: 2 windows, 1 col, 2 rows (height = screen_h / 2, width = screen_w)
    s.menu("xlsx.arrange-all");
    s.settle();
    arrange_windows_click(&mut s, "arrange-horizontal");
    s.settle();
    arrange_windows_click(&mut s, "arrange-ok");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.arrange_windows.is_none()));

    let b0 = s.update_window(windows[0], |w, _| w.bounds());
    let b1 = s.update_window(windows[1], |w, _| w.bounds());
    assert!((f32::from(b0.size.width) - screen_w).abs() < 2.0, "b0 width: {:?}", b0.size.width);
    assert!((f32::from(b0.size.height) - screen_h / 2.0).abs() < 2.0, "b0 height: {:?}", b0.size.height);
    assert!((f32::from(b1.size.width) - screen_w).abs() < 2.0, "b1 width: {:?}", b1.size.width);
    assert!((f32::from(b1.size.height) - screen_h / 2.0).abs() < 2.0, "b1 height: {:?}", b1.size.height);

    // Arrange Vertically: 2 windows, 2 cols, 1 row (width = screen_w / 2, height = screen_h)
    s.menu("xlsx.arrange-all");
    s.settle();
    arrange_windows_click(&mut s, "arrange-vertical");
    s.settle();
    arrange_windows_click(&mut s, "arrange-ok");
    s.settle();

    let b0_v = s.update_window(windows[0], |w, _| w.bounds());
    let b1_v = s.update_window(windows[1], |w, _| w.bounds());
    assert!((f32::from(b0_v.size.width) - screen_w / 2.0).abs() < 2.0, "b0_v width: {:?}", b0_v.size.width);
    assert!((f32::from(b0_v.size.height) - screen_h).abs() < 2.0, "b0_v height: {:?}", b0_v.size.height);
    assert!((f32::from(b1_v.size.width) - screen_w / 2.0).abs() < 2.0, "b1_v width: {:?}", b1_v.size.width);
    assert!((f32::from(b1_v.size.height) - screen_h).abs() < 2.0, "b1_v height: {:?}", b1_v.size.height);

    // Arrange Cascade: 2 windows, 80% screen size
    let cascade_w = (screen_w * 0.8).clamp(640.0, screen_w);
    let cascade_h = (screen_h * 0.8).clamp(480.0, screen_h);
    s.menu("xlsx.arrange-all");
    s.settle();
    arrange_windows_click(&mut s, "arrange-cascade");
    s.settle();
    arrange_windows_click(&mut s, "arrange-ok");
    s.settle();

    let b0_c = s.update_window(windows[0], |w, _| w.bounds());
    let b1_c = s.update_window(windows[1], |w, _| w.bounds());
    assert!((f32::from(b0_c.size.width) - cascade_w).abs() < 2.0, "b0_c width: {:?}", b0_c.size.width);
    assert!((f32::from(b0_c.size.height) - cascade_h).abs() < 2.0, "b0_c height: {:?}", b0_c.size.height);
    assert!((f32::from(b1_c.size.width) - cascade_w).abs() < 2.0, "b1_c width: {:?}", b1_c.size.width);
    assert!((f32::from(b1_c.size.height) - cascade_h).abs() < 2.0, "b1_c height: {:?}", b1_c.size.height);

    // Close second window to restore clean state
    s.close_window(windows[1]);
    s.settle();
    assert_eq!(s.with(|_, app| app.windows().len()), 1, "only original window remains");
}

fn view011_new_window_for_same_workbook() {
    use crate::shell::editor::SlideEditor;
    let mut s = new_sheet();

    // 1. Initial menu state and single-window title
    assert_eq!(s.menu_state("xlsx.new-window"), Some((true, false)));
    assert_eq!(s.with(|_, app| app.windows().len()), 1);
    assert_eq!(s.with(|e, _| e.display_title_formatted()), "xlsx-sales.xlsx");

    // 2. Open new window via menu "xlsx.new-window"
    s.menu("xlsx.new-window");
    s.settle();

    let windows = s.with(|_, app| app.windows());
    assert_eq!(windows.len(), 2, "second window opened for same workbook");

    // Check title bars: Book1:1 and Book1:2 (or xlsx-sales.xlsx:1 and xlsx-sales.xlsx:2)
    assert_eq!(s.with(|e, _| e.display_title_formatted()), "xlsx-sales.xlsx:1");
    let wb_id = s.with(|e, _| e.workbook_id);
    let ed1 = s.with_app(|app| {
        let matching = SlideEditor::find_matching_windows(wb_id, app);
        assert_eq!(matching.len(), 2, "found 2 matching windows");
        matching[1].1.clone()
    });
    let title1 = s.with(|_, app| ed1.read(app).display_title_formatted());
    assert_eq!(title1, "xlsx-sales.xlsx:2");

    // 3. Independent view: zoom on window 1
    s.update_in(windows[1], &ed1, |e, window, cx| {
        e.set_zoom(Some(1.5), window, cx);
    });

    // Verify window 0 zoom is untouched, window 1 zoom is 1.5
    assert_eq!(s.with(|e, _| e.zoom), Some(1.0));
    let zoom1 = s.with(|_, app| ed1.read(app).zoom);
    assert_eq!(zoom1, Some(1.5));

    // 4. Mutual synchronization: edit cell in window 0, check in window 1
    s.click_cell("A1");
    s.enter("SyncTestVal", "enter");
    s.settle();

    assert_eq!(s.shown("A1"), "SyncTestVal");
    let val_in_win1 = s.with(|_, app| {
        let slide = ed1.read(app).slide();
        let sheet = sheet_of_slide(slide).unwrap();
        sheet.cell(0, 0).map(|c| c.value.clone())
    });
    assert_eq!(val_in_win1, Some(CellValue::Text("SyncTestVal".into())));

    // Edit cell in window 1, check in window 0
    s.update_in(windows[1], &ed1, |e, _, cx| {
        let slide = e.current_slide;
        let edits = vec![ibis_doc_model::CellEdit {
            row: 1,
            column: 1,
            input: ibis_doc_model::CellInput::Value(CellValue::Text("FromWin2".into())),
        }];
        e.document.execute(vec![ibis_doc_model::Command::Sheet(
            ibis_doc_model::SheetCommand::SetCells { slide, edits }
        )]).unwrap();
        e.sync_workbook_peers(cx);
        cx.notify();
    });

    assert_eq!(s.shown("B2"), "FromWin2");

    // 5. Close second window: closes without save prompt, renumbers remaining window
    s.update_in(windows[1], &ed1, |e, window, cx| {
        e.close_file(&crate::CloseFile, window, cx);
    });

    assert_eq!(s.with(|_, app| app.windows().len()), 1, "only original window remains");
    assert_eq!(s.with(|e, _| e.display_title_formatted()), "xlsx-sales.xlsx", "title reverted to single window format");

    // 6. Save confirmation on last window:
    // Window 0 still has unsaved edits (can_undo == true)
    assert!(s.with(|e, _| e.document.can_undo()));
    assert!(!s.with(|e, _| e.save_confirm_dialog_open));

    // Closing the file (⌘W) opens save confirmation dialog
    s.update(|e, window, cx| e.close_file(&crate::CloseFile, window, cx));
    s.settle();

    assert!(s.with(|e, _| e.save_confirm_dialog_open), "save confirmation dialog opened on last window");
    assert!(s.with(|e, _| e.has_document), "document remains open while dialog is shown");

    // Cancel: closes dialog, keeps document
    s.update(|e, _, cx| e.close_save_confirm_dialog(cx));
    s.settle();
    assert!(!s.with(|e, _| e.save_confirm_dialog_open));
    assert!(s.with(|e, _| e.has_document));

    // Discard & close: discards changes and closes document
    s.update(|e, window, cx| e.confirm_discard_and_close(window, cx));
    s.settle();
    assert!(!s.with(|e, _| e.save_confirm_dialog_open));
    assert!(!s.with(|e, _| e.has_document), "document closed after discard");
}

fn view013_view_side_by_side_and_sync_scrolling() {
    use crate::shell::editor::SlideEditor;
    let mut s = new_sheet();

    // 1. Initial menu state with 1 window
    assert_eq!(s.menu_state("xlsx.view-side-by-side"), Some((true, false)));
    assert_eq!(s.menu_state("xlsx.sync-scrolling"), Some((false, false)));
    assert_eq!(s.menu_state("xlsx.reset-window-position"), Some((false, false)));

    // 2. Open new window for the same workbook
    s.menu("xlsx.new-window");
    s.settle();

    let windows = s.with(|_, app| app.windows());
    assert_eq!(windows.len(), 2, "2 windows opened");
    let wb_id = s.with(|e, _| e.workbook_id);
    let ed1 = s.with_app(|app| {
        let matching = SlideEditor::find_matching_windows(wb_id, app);
        assert_eq!(matching.len(), 2);
        matching[1].1.clone()
    });

    // 3. Activate View Side by Side via menu
    s.menu("xlsx.view-side-by-side");
    s.settle();

    // Verify both windows are in side-by-side mode with sync scrolling ON
    assert!(s.with(|e, _| e.side_by_side));
    assert!(s.with(|e, _| e.sync_scrolling));
    assert!(s.with(|_, app| ed1.read(app).side_by_side));
    assert!(s.with(|_, app| ed1.read(app).sync_scrolling));

    // Verify menu items updated
    assert_eq!(s.menu_state("xlsx.view-side-by-side"), Some((true, true)));
    assert_eq!(s.menu_state("xlsx.sync-scrolling"), Some((true, true)));
    assert_eq!(s.menu_state("xlsx.reset-window-position"), Some((true, false)));

    // 4. Synchronous scrolling:
    // Scroll window 0 by (0, -120 px)
    s.update(|e, _, cx| {
        e.scroll_workspace_by(point(px(0.), px(-120.)), cx);
    });
    s.settle();

    let offset0 = s.with(|e, _| e.workspace_scroll.offset());
    let offset1 = s.with(|_, app| ed1.read(app).workspace_scroll.offset());
    assert_eq!(offset0.y, px(-120.));
    assert_eq!(offset1.y, px(-120.), "peer window scrolled synchronously");

    // Scroll window 1 by (-50 px, -80 px)
    s.update_in(windows[1], &ed1, |e, _, cx| {
        e.scroll_workspace_by(point(px(-50.), px(-80.)), cx);
    });
    s.settle();

    let offset0 = s.with(|e, _| e.workspace_scroll.offset());
    let offset1 = s.with(|_, app| ed1.read(app).workspace_scroll.offset());
    assert_eq!(offset1.x, px(-50.));
    assert_eq!(offset1.y, px(-200.));
    assert_eq!(offset0.x, px(-50.), "window 0 synchronized horizontal scroll");
    assert_eq!(offset0.y, px(-200.), "window 0 synchronized vertical scroll");

    // 5. Toggle Synchronous Scrolling OFF
    s.menu("xlsx.sync-scrolling");
    s.settle();

    assert_eq!(s.menu_state("xlsx.sync-scrolling"), Some((true, false)));
    assert!(!s.with(|e, _| e.sync_scrolling));
    assert!(!s.with(|_, app| ed1.read(app).sync_scrolling));
    assert!(s.with(|e, _| e.side_by_side), "side by side remains active");

    // Scroll window 0 while sync is off -> independent scrolling
    s.update(|e, _, cx| {
        e.scroll_workspace_by(point(px(0.), px(-60.)), cx);
    });
    s.settle();

    let offset0_after = s.with(|e, _| e.workspace_scroll.offset());
    let offset1_after = s.with(|_, app| ed1.read(app).workspace_scroll.offset());
    assert_eq!(offset0_after.y, px(-260.));
    assert_eq!(offset1_after.y, px(-200.), "peer window did not scroll while sync was off");

    // Toggle Synchronous Scrolling back ON
    s.menu("xlsx.sync-scrolling");
    s.settle();

    assert_eq!(s.menu_state("xlsx.sync-scrolling"), Some((true, true)));
    assert!(s.with(|e, _| e.sync_scrolling));
    assert!(s.with(|_, app| ed1.read(app).sync_scrolling));

    // Scroll window 1 -> syncs again
    s.update_in(windows[1], &ed1, |e, _, cx| {
        e.scroll_workspace_by(point(px(0.), px(-30.)), cx);
    });
    s.settle();

    let offset0_resync = s.with(|e, _| e.workspace_scroll.offset());
    let offset1_resync = s.with(|_, app| ed1.read(app).workspace_scroll.offset());
    assert_eq!(offset1_resync.y, px(-230.));
    assert_eq!(offset0_resync.y, px(-290.), "window 0 synchronized by delta (-30)");

    // 6. Reset Window Position
    s.menu("xlsx.reset-window-position");
    s.settle();

    // 7. Deactivate View Side by Side via menu
    s.menu("xlsx.view-side-by-side");
    s.settle();

    assert!(!s.with(|e, _| e.side_by_side));
    assert!(!s.with(|e, _| e.sync_scrolling));
    assert!(!s.with(|_, app| ed1.read(app).side_by_side));
    assert!(!s.with(|_, app| ed1.read(app).sync_scrolling));
    assert_eq!(s.menu_state("xlsx.view-side-by-side"), Some((true, false)));
    assert_eq!(s.menu_state("xlsx.sync-scrolling"), Some((false, false)));
    assert_eq!(s.menu_state("xlsx.reset-window-position"), Some((false, false)));

    // 8. Re-enable side-by-side, then close peer window -> cleans up side-by-side on window 0
    s.menu("xlsx.view-side-by-side");
    s.settle();
    assert!(s.with(|e, _| e.side_by_side));

    s.update_in(windows[1], &ed1, |e, window, cx| {
        e.close_file(&crate::CloseFile, window, cx);
    });
    s.settle();

    assert_eq!(s.with(|_, app| app.windows().len()), 1);
    assert!(!s.with(|e, _| e.side_by_side), "reverted side-by-side when peer closed");
    assert!(!s.with(|e, _| e.sync_scrolling));
    assert_eq!(s.menu_state("xlsx.sync-scrolling"), Some((false, false)));
}

/// XLSX-FILE-013: 互換性チェック (Compatibility Checker: check_compatibility)
/// ワークブック全体をスキャンし、旧バージョンの Excel (97-2003 / BIFF8) で
/// サポートされていない機能（グリッドサイズ超過、新関数、拡張条件付き書式、
/// テーブル、スパークライン、グラデーション塗りつぶし等）の検出、
/// 重大な機能低下と軽微な精度の低下への分類、該当セルへのジャンプ、再チェック、UI操作を検証する。
fn file013_compatibility_check() {
    let mut s = new_sheet();

    // 1. 初期状態（クリーンな新規ブック）での互換性チェック
    s.menu("xlsx.compatibility-check");
    s.settle();

    assert!(s.with(|e, _| e.wb().panels.compatibility.is_some()), "compatibility panel opened");
    let initial_report = s.with(|e, _| e.wb().panels.compatibility.as_ref().unwrap().report.clone());
    assert!(initial_report.is_clean(), "clean workbook has no compatibility issues");
    assert_eq!(initial_report.significant.len(), 0);
    assert_eq!(initial_report.minor.len(), 0);

    // パネルを閉じる
    s.update(|e, _, cx| e.close_xlsx_compatibility_panel(cx));
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.compatibility.is_none()), "compatibility panel closed");

    // 2. Excel 97-2003 未対応の各種機能をワークブックに追加
    let slide_id = s.with(|e, _| e.slide().id);

    // (A) サポートされていない最新関数 (XLOOKUP) をセル A1 に入力
    s.click_cell("A1");
    s.enter("=XLOOKUP(B1, C1:C5, D1:D5)", "enter");

    // (B) テーブル（Table）を追加 (Minor loss)
    let table = ibis_doc_model::SheetTable {
        name: "SalesTable".into(),
        range: ibis_doc_model::CellRange { first_row: 2, last_row: 5, first_column: 0, last_column: 1 },
        header_rows: 1,
        totals_rows: 0,
        columns: vec!["Col1".into(), "Col2".into()],
        style: None,
        filter: None,
        totals: Vec::new(),
    };

    // (C) 拡張条件付き書式 (DataBar) を追加 (Significant loss)
    let databar_cf = ibis_doc_model::ConditionalFormat {
        source_index: None,
        ranges: vec![ibis_doc_model::CellRange { first_row: 2, last_row: 5, first_column: 2, last_column: 2 }],
        rules: vec![ibis_doc_model::CfRule {
            kind: ibis_doc_model::CfKind::data_bar(
                ibis_doc_model::CfValue { kind: "min".into(), value: None, gte: true },
                ibis_doc_model::CfValue { kind: "max".into(), value: None, gte: true },
                ibis_doc_model::Rgb(0x638EC6),
                true,
            ),
            priority: 1,
            stop: false,
            style: None,
        }],
    };

    // (D) スパークラインを追加 (Minor loss)
    let mut sparkline_group = ibis_doc_model::SparklineGroup::default();
    sparkline_group.sparklines = vec![ibis_doc_model::Sparkline {
        row: 7,
        column: 1,
        sheet: None,
        data: ibis_doc_model::CellRange { first_row: 2, last_row: 5, first_column: 2, last_column: 2 },
    }];

    s.update(|e, _, cx| {
        let slide = e.document.ids().slides.iter_mut().find(|sl| sl.id == slide_id).unwrap();
        let shape = slide.shapes.iter_mut().find_map(|sh| match &mut sh.kind {
            ibis_doc_model::ShapeKind::Sheet(s) => Some(s),
            _ => None,
        }).unwrap();
        let sheet = std::sync::Arc::make_mut(shape);
        sheet.tables.push(table);
        sheet.conditional.push(databar_cf);
        sheet.sparklines.push(sparkline_group);
        sheet.cells.push(ibis_doc_model::SheetCell {
            row: 0,
            column: 256,
            value: ibis_doc_model::CellValue::Text("OutOfRange".into()),
            text: "OutOfRange".into(),
            kind: ibis_doc_model::CellKind::Text,
            formula: None,
            style: 0,
            phonetic: None,
            runs: None,
        });
        cx.notify();
    });
    s.settle();

    // 3. 互換性チェック パネルを開いて再検査
    s.menu("xlsx.compatibility-check");
    s.settle();

    let report = s.with(|e, _| e.wb().panels.compatibility.as_ref().unwrap().report.clone());
    assert!(!report.is_clean(), "workbook now has compatibility issues");

    // 重大な機能低下の検証
    assert!(report.significant.len() >= 3, "expected at least 3 significant issues (bounds, formula, cf)");
    let has_bounds = report.significant.iter().any(|i| i.title.contains("65,536行/256列") && i.cell == Some((0, 256)));
    let has_func = report.significant.iter().any(|i| i.title.contains("XLOOKUP") && i.cell == Some((0, 0)));
    let has_databar = report.significant.iter().any(|i| i.title.contains("データ バー"));
    assert!(has_bounds, "detected out of bounds column IW");
    assert!(has_func, "detected unsupported modern function XLOOKUP at A1");
    assert!(has_databar, "detected extended conditional format DataBar");

    // 軽微な精度の低下の検証
    assert!(report.minor.len() >= 2, "expected at least 2 minor issues (table, sparkline)");
    let has_table = report.minor.iter().any(|i| i.title.contains("SalesTable"));
    let has_sparkline = report.minor.iter().any(|i| i.title.contains("スパークライン"));
    assert!(has_table, "detected table SalesTable conversion to range");
    assert!(has_sparkline, "detected sparkline hidden in older versions");

    // 4. ジャンプ機能（該当セルへのフォーカス）の検証
    // A1 (XLOOKUP) の issue を検索
    let xlookup_idx = report.all_issues().iter().position(|i| i.title.contains("XLOOKUP")).unwrap();
    s.update(|e, _, cx| {
        e.jump_to_xlsx_compatibility_issue(xlookup_idx, cx);
    });
    s.settle();
    assert_eq!(s.selected(), "A1", "jumped to cell A1 with XLOOKUP");

    // 範囲外セル（IW1）の issue を検索
    let bounds_idx = report.all_issues().iter().position(|i| i.title.contains("65,536行/256列")).unwrap();
    s.update(|e, _, cx| {
        e.jump_to_xlsx_compatibility_issue(bounds_idx, cx);
    });
    s.settle();
    let current_cell = s.with(|e, _| e.wb().cells.get(&slide_id).copied());
    assert_eq!(current_cell, Some((256, 0)), "jumped to out of bounds cell IW1");

    // 5. 再チェックボタン（recheck）の検証
    s.update(|e, _, cx| {
        e.recheck_xlsx_compatibility(cx);
    });
    s.settle();
    let msg = s.with(|e, _| e.wb().panels.compatibility.as_ref().unwrap().message.clone());
    assert!(!msg.is_empty(), "recheck message displayed");

    // 6. パネルのクローズ
    s.update(|e, _, cx| {
        e.close_xlsx_compatibility_panel(cx);
    });
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.compatibility.is_none()), "compatibility panel closed cleanly");
}

fn file014_autorecover_settings() {
    let mut s = new_sheet();

    // 1. メニューから「自動回復の設定…」を開く
    s.menu("xlsx.autorecover-settings");
    s.settle();

    assert!(s.with(|e, _| e.wb().panels.autorecover_settings.is_some()), "autorecover settings panel opened");

    // 2. パネル内の入力・トグルの検証
    s.update(|e, _, _| {
        let p = e.wb_mut().panels.autorecover_settings.as_mut().unwrap();
        p.enabled = false;
        p.keep_last_unsaved = true;
        p.disable_for_this_workbook = true;
    });

    // 保存間隔の変更: 15分
    s.update(|e, w, cx| {
        let p = e.wb().panels.autorecover_settings.as_ref().unwrap();
        p.interval_input.update(cx, |inp, cx| {
            inp.set_value("15", w, cx);
        });
    });

    // 設定を保存
    s.update(|e, _, cx| {
        e.save_xlsx_autorecover_settings(cx);
    });
    s.settle();

    let saved_msg = s.with(|e, _| e.wb().panels.autorecover_settings.as_ref().unwrap().message.clone());
    assert!(!saved_msg.is_empty(), "settings saved message displayed");

    let saved_config = super::autorecover::load_autorecover_config();
    assert_eq!(saved_config.interval_minutes, 15);
    assert!(!saved_config.enabled);

    let disable_wb = s.with(|e, _| e.wb().disable_autorecover_for_workbook);
    assert!(disable_wb, "disable for workbook is saved in state");

    // 設定を再度有効化（テスト後の整合性のため）
    s.update(|e, _, cx| {
        let p = e.wb_mut().panels.autorecover_settings.as_mut().unwrap();
        p.enabled = true;
        e.save_xlsx_autorecover_settings(cx);
    });
    s.settle();

    // 3. 「今すぐ自動回復データを保存」の手動実行
    s.update(|e, w, cx| {
        e.save_xlsx_autorecover_now(w, cx);
    });
    s.settle();

    let now_msg = s.with(|e, _| e.wb().panels.autorecover_settings.as_ref().unwrap().message.clone());
    assert!(now_msg.contains("自動回復データを保存しました") || now_msg.contains("AutoRecover"), "manual snapshot message displayed");

    let active_path = s.with(|e, _| e.active_autorecover_file.clone());
    assert!(active_path.is_some(), "active autorecover file path recorded");
    let path = active_path.unwrap();
    assert!(path.exists(), "autorecover snapshot file exists on disk");
    assert!(std::fs::metadata(&path).unwrap().len() > 0, "autorecover file is not empty");

    // 4. パネルのクローズ
    s.update(|e, _, cx| {
        e.close_xlsx_autorecover_settings_panel(cx);
    });
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.autorecover_settings.is_none()), "autorecover settings panel closed");

    // クリーンアップ
    let _ = std::fs::remove_file(&path);
}

fn file015_and_file016_recover_and_save_workbook() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. ワークシートに編集を行う
    s.click_cell("G1");
    s.enter("ImportantRecoverData", "enter");
    s.settle();

    // 2. 自動回復スナップショットを作成
    let snapshot_path = s.update(|e, _, _| {
        e.save_autorecover_snapshot().expect("snapshot saved")
    });
    assert!(snapshot_path.exists(), "snapshot file exists on disk");

    // 3. 未保存のブックの回復パネルを開く
    s.menu("xlsx.recover-unsaved");
    s.settle();

    assert!(s.with(|e, _| e.wb().panels.recover_unsaved.is_some()), "recover unsaved panel opened");
    let entries = s.with(|e, _| e.wb().panels.recover_unsaved.as_ref().unwrap().entries.clone());
    assert!(!entries.is_empty(), "recovered files list contains snapshot");

    let entry_idx = entries.iter().position(|e| e.path == snapshot_path).expect("snapshot in entries");

    // 4. 回復ブックを開く
    s.update(|e, w, cx| {
        e.open_recovered_file_entry(entry_idx, w, cx);
    });
    s.settle();

    // 検証: 回復フラグが立っており、タイトルに[回復済み]が付与され、パネルが閉じている
    assert!(s.with(|e, _| e.is_recovered), "is_recovered is true");
    let title = s.with(|e, _| e.file_name.clone().unwrap_or_default().to_string());
    assert!(title.contains("[回復済み]"), "title includes [回復済み]");
    assert!(s.with(|e, _| e.wb().panels.recover_unsaved.is_none()), "recover panel closed upon opening file");

    // セルG1の内容が復元されていること
    assert_eq!(s.shown("G1"), "ImportantRecoverData", "cell value recovered accurately");

    // 5. 名前を付けて保存（Save As）による回復ファイルの正規保存とバナー解除
    let target_save_dir = std::env::temp_dir().join(format!("ibis_test_recover_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&target_save_dir);
    let target_file = target_save_dir.join("SavedFromRecovery.xlsx");

    s.update(|e, w, _| {
        e.save_to(target_file.clone(), w).expect("save_to succeeded");
    });
    s.settle();

    // 検証: 保存後は is_recovered が false になり、バナーが非表示になること
    assert!(!s.with(|e, _| e.is_recovered), "is_recovered reset to false after save");
    assert!(target_file.exists(), "target saved file exists");

    // クリーンアップ
    let _ = std::fs::remove_file(&target_file);
    let _ = std::fs::remove_dir_all(&target_save_dir);
    let _ = std::fs::remove_file(&snapshot_path);
}

fn file019_backup_and_save_location() {
    use crate::xlsx::backup_save::{load_backup_save_config, save_backup_save_config, BackupFormat, BackupSaveConfig};

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. メニューから「バックアップと保存先の設定…」を開く
    s.menu("xlsx.backup-save-settings");
    s.settle();

    assert!(s.with(|e, _| e.wb().panels.backup_save.is_some()), "backup save settings panel opened");

    // 2. パネル内で設定を変更（常にバックアップ作成をON、形式を.xlkに設定）
    let temp_save_dir = std::env::temp_dir().join(format!("ibis_test_backup_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_save_dir);

    s.update(|e, w, cx| {
        let p = e.wb_mut().panels.backup_save.as_mut().unwrap();
        p.always_create_backup = true;
        p.backup_format = BackupFormat::Xlk;
        p.use_same_folder = true;
        p.default_save_folder_input.update(cx, |input, cx| {
            input.set_value(temp_save_dir.display().to_string(), w, cx);
        });
        e.save_xlsx_backup_save_settings(cx);
    });
    s.settle();

    let saved_msg = s.with(|e, _| e.wb().panels.backup_save.as_ref().unwrap().message.clone());
    assert!(saved_msg.is_some(), "settings saved message shown");

    let saved_config = load_backup_save_config();
    assert!(saved_config.always_create_backup, "always_create_backup persisted as true");
    assert_eq!(saved_config.backup_format, BackupFormat::Xlk, "backup_format persisted as Xlk");

    // 3. ワークブックの初版を保存
    let target_file = temp_save_dir.join("MySalesWorkbook.xlsx");
    s.update(|e, w, _| {
        e.save_to(target_file.clone(), w).expect("initial save succeeded");
    });
    s.settle();
    assert!(target_file.exists(), "target workbook file saved");

    // 4. ワークブックを編集し、上書き保存を実施 -> バックアップが生成されることを検証
    s.click_cell("G2");
    s.enter("UpdatedValueBeforeBackup", "enter");
    s.settle();

    s.update(|e, w, _| {
        e.save_to(target_file.clone(), w).expect("overwrite save succeeded");
    });
    s.settle();

    let backup_file = temp_save_dir.join("MySalesWorkbook のバックアップ.xlk");
    assert!(backup_file.exists(), "backup file (.xlk) was created on overwrite save");

    // 5. バックアップファイルの内容を検証（上書き前の初版データが保持されていること）
    let backup_loaded = crate::read_file(&backup_file).expect("backup file can be read as workbook");
    assert_eq!(ibis_pptx::Kind::of(&backup_loaded.presentation), ibis_pptx::Kind::Workbook, "backup file is valid workbook");

    // 6. 手動バックアップ作成ボタンをテスト
    s.update(|e, _, cx| {
        e.manual_create_backup_now(cx);
    });
    s.settle();
    let manual_msg = s.with(|e, _| e.wb().panels.backup_save.as_ref().unwrap().message.clone());
    assert!(manual_msg.is_some(), "manual backup created message shown");

    // 7. パネルを閉じる
    s.update(|e, _, cx| {
        e.close_xlsx_backup_save_settings_panel(cx);
    });
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.backup_save.is_none()), "backup save settings panel closed");

    // クリーンアップ
    let _ = save_backup_save_config(&BackupSaveConfig::default());
    let _ = std::fs::remove_file(&target_file);
    let _ = std::fs::remove_file(&backup_file);
    let _ = std::fs::remove_dir_all(&temp_save_dir);
}

fn file020_open_and_repair_damaged_workbook() {
    use ibis_pptx::repair::RepairMode;
    use ibis_pptx::ibis_ooxml::zip::{EntryMeta, Writer};

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 2. 壊れたワークブック（制御文字・タグ未完・数式あり）を作成
    let temp_dir = std::env::temp_dir().join(format!("ibis_test_repair_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let damaged_file = temp_dir.join("damaged_sales.xlsx");

    let damaged_sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1" t="inlineStr"><is><t>Salvaged{}Row{}1</t></is></c>
      <c r="B1"><v>999</v></c>
    </row>
    <row r="2">
      <c r="A2"><f>SUM(B1, 10)</f><v>1009</v></c>"#,
        '\x01', '\x08'
    );
    let mut w = Writer::new();
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#;
    let wb = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets>
    <sheet name="Sheet1" sheetId="1" r:id="rId1"/>
  </sheets>
</workbook>"#;
    let wb_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#;
    w.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    w.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    w.add("xl/workbook.xml", EntryMeta::new(), wb.as_bytes()).unwrap();
    w.add("xl/_rels/workbook.xml.rels", EntryMeta::new(), wb_rels.as_bytes()).unwrap();
    w.add("xl/worksheets/sheet1.xml", EntryMeta::new(), damaged_sheet.as_bytes()).unwrap();
    let zip_bytes = w.finish().unwrap();
    std::fs::write(&damaged_file, zip_bytes).unwrap();

    // 通常の open だとエラーになることを確認
    assert!(crate::read_file(&damaged_file).is_err(), "damaged workbook normally fails to parse");

    // 開けなかったブックが記録され、メニュー「破損ブックの修復…」からそのブックの修復パネルが開く
    assert_eq!(s.menu_state("xlsx.open-and-repair").map(|m| m.0), Some(true), "repair menu enabled while a damaged workbook is known");
    s.menu("xlsx.open-and-repair");
    s.settle();
    assert_eq!(s.with(|e, _| e.wb().panels.repair.as_ref().and_then(|p| p.file_path.clone())), Some(damaged_file.clone()), "repair panel opened for the damaged workbook");

    // 3. パネルから「修復 (Repair)」を実行
    s.update(|e, window, cx| {
        let p = e.wb_mut().panels.repair.as_mut().unwrap();
        p.file_path = Some(damaged_file.clone());
        let path_str = damaged_file.to_string_lossy().to_string();
        p.file_path_input.update(cx, |input, cx| input.set_value(path_str, window, cx));
        p.mode = RepairMode::Repair;
        e.execute_xlsx_repair(window, cx);
    });
    s.settle();

    // 4. 修復後の状態を検証
    assert!(s.with(|e, _| e.is_repaired), "is_repaired set to true");
    assert!(s.with(|e, _| e.repair_summary.is_some()), "repair summary exists");
    assert!(s.with(|e, _| e.repair_report.as_ref().unwrap().success), "repair report success is true");
    assert_eq!(s.with(|e, _| e.repair_report.as_ref().unwrap().sheets_recovered), 1, "1 sheet recovered");
    assert!(s.with(|e, _| e.repair_report.as_ref().unwrap().cells_recovered >= 2), "cells recovered");

    // セルデータが正しく読み込まれていること
    assert!(s.formula("A2").is_some(), "formula in A2 preserved in Repair mode");

    // 5. 修復モードを「値に変換 (ExtractValues)」で実行
    let extract_file = temp_dir.join("damaged_extract.xlsx");
    std::fs::copy(&damaged_file, &extract_file).unwrap();

    s.update(|e, window, cx| {
        e.open_xlsx_repair_panel(window, cx);
        let p = e.wb_mut().panels.repair.as_mut().unwrap();
        let path_str = extract_file.to_string_lossy().to_string();
        p.file_path_input.update(cx, |input, cx| input.set_value(path_str, window, cx));
        p.mode = RepairMode::ExtractValues;
        e.execute_xlsx_repair(window, cx);
    });
    s.settle();

    // 数式が消去され、値が維持されていることを検証
    assert_eq!(s.formula("A2"), None, "formula removed in ExtractValues mode");
    assert!(s.with(|e, _| e.is_repaired), "is_repaired is true in ExtractValues mode");

    // 6. 修復バナーの閉じる動作の検証
    s.update(|e, _, cx| {
        e.dismiss_repair_banner(cx);
    });
    s.settle();
    assert!(!s.with(|e, _| e.is_repaired), "banner dismissed by dismiss_repair_banner");

    // 7. 保存時の修復バナー自動非表示（save_to）の検証
    s.update(|e, _, _| {
        e.is_repaired = true;
    });
    let saved_dest = temp_dir.join("repaired_saved.xlsx");
    s.update(|e, window, _| {
        e.save_to(saved_dest.clone(), window).expect("save succeeded");
    });
    s.settle();
    assert!(!s.with(|e, _| e.is_repaired), "is_repaired reset to false after save");
    assert!(saved_dest.exists(), "saved file exists");

    // 8. パネルを閉じる
    s.update(|e, _, cx| {
        e.close_xlsx_repair_panel(cx);
    });
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.repair.is_none()), "repair panel closed");

    // クリーンアップ
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// XLSX-OBJECT-015: ワードアート (WordArt) の挿入・スタイル・文字変形・描画・編集・保存・Undo/Redo 検証。
fn object015_wordart_insert_styles_and_warp() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());
    s.click_cell("C5");

    // 1. 挿入メニューからワードアート（スタイル1）を挿入
    s.menu("xlsx.insert.wordart");
    let wordart_id = s.with(|e, _| {
        e.selected_sheet_shape().expect("wordart shape selected")
    });

    let init_body = s.with(|e, _| {
        e.slide().shape(wordart_id).unwrap().text_body().unwrap().clone()
    });
    assert_eq!(init_body.text(), "ワードアート");
    assert_eq!(init_body.warp, None);
    assert!(init_body.style.bold);
    let r0 = &init_body.paragraphs[0].runs[0];
    assert_eq!(r0.style.font_size, Some(3600));
    assert_eq!(r0.style.color, Some(ibis_doc_model::Rgb(0x000000)));

    // 2. 文字の変形: 上アーチ (textArchUp) を適用
    s.menu("xlsx.text.warp.arch-up");
    assert_eq!(s.with(|e, _| e.selected_sheet_text_warp()), Some("textArchUp".to_string()));

    // 3. 文字の変形: 波 1 (textWave1) に変更
    s.menu("xlsx.text.warp.wave1");
    assert_eq!(s.with(|e, _| e.selected_sheet_text_warp()), Some("textWave1".to_string()));

    // 4. 文字の変形: 円 (textCircle) に変更
    s.menu("xlsx.text.warp.circle");
    assert_eq!(s.with(|e, _| e.selected_sheet_text_warp()), Some("textCircle".to_string()));

    // 5. ワードアートスタイル（スタイル 2: 青、影）を適用
    s.menu("xlsx.text.wordart.style2");
    let styled_body2 = s.with(|e, _| {
        e.slide().shape(wordart_id).unwrap().text_body().unwrap().clone()
    });
    let r2 = &styled_body2.paragraphs[0].runs[0];
    assert_eq!(r2.style.color, Some(ibis_doc_model::Rgb(0x4472C4)));
    assert!(r2.style.shadow.is_some());

    // 6. ワードアートスタイル（スタイル 4: 塗りつぶし白、輪郭黒）を適用
    s.menu("xlsx.text.wordart.style4");
    let styled_body4 = s.with(|e, _| {
        e.slide().shape(wordart_id).unwrap().text_body().unwrap().clone()
    });
    let r4 = &styled_body4.paragraphs[0].runs[0];
    assert_eq!(r4.style.color, Some(ibis_doc_model::Rgb(0xFFFFFF)));
    assert!(r4.style.outline.is_some());

    // 7. 文字の変形を解除 (none)
    s.menu("xlsx.text.warp.none");
    assert_eq!(s.with(|e, _| e.selected_sheet_text_warp()), None);

    // 再度波 1 (textWave1) を設定
    s.menu("xlsx.text.warp.wave1");
    assert_eq!(s.with(|e, _| e.selected_sheet_text_warp()), Some("textWave1".to_string()));

    // 8. 別のワードアートスタイル（スタイル3: オレンジ、輪郭）を新規挿入
    s.click_cell("E10");
    s.menu("xlsx.insert.wordart.style3");
    let wordart3_id = s.with(|e, _| {
        e.selected_sheet_shape().expect("wordart 3 selected")
    });
    assert_ne!(wordart3_id, wordart_id);
    let init_body3 = s.with(|e, _| {
        e.slide().shape(wordart3_id).unwrap().text_body().unwrap().clone()
    });
    let r3 = &init_body3.paragraphs[0].runs[0];
    assert_eq!(r3.style.color, Some(ibis_doc_model::Rgb(0xED7D31)));
    assert!(r3.style.outline.is_some());

    // 9. Undo / Redo 検証
    assert!(s.with(|e, _| e.document.can_undo()));
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    // wordart3 was undone
    s.with(|e, _| {
        assert!(e.slide().shape(wordart3_id).is_none());
        assert!(e.slide().shape(wordart_id).is_some());
    });
    assert!(s.with(|e, _| e.document.can_redo()));
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        assert!(e.slide().shape(wordart3_id).is_some());
    });

    // 10. 全件ロールバック & 保存・再オープン検証
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "all inserted shapes rolled back");
    });
    s.save_as("object015_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

/// XLSX-PIVOT-020: ピボットグラフ (PivotChart) の作成、連動更新、書式パネル表示、OOXML保存・再読込、Undo/Redo 検証。
fn pivot020_pivot_chart_create_and_sync() {
    let mut s = Scenario::open("test/editing/xlsx-pivot-not-refreshed.xlsx");

    // 初期状態: チャート図形は存在しない
    let initial_charts = s.with(|e, _| e.slide().shapes.iter().filter(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))).count());
    assert_eq!(initial_charts, 0, "no chart initially");

    // 1. ピボットテーブル上のセル B4 を選択し、メニュー「xlsx.pivot-chart-insert」からピボットグラフを挿入
    s.click_cell("B4");
    s.menu("xlsx.pivot-chart-insert");

    // ピボットグラフが生成・配置され、選択状態になっていることを検証
    let (chart_id, chart) = s.with(|e, _| {
        e.slide().shapes.iter().find_map(|sh| match &sh.kind {
            ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())),
            _ => None,
        })
    }).expect("pivot chart inserted");

    assert!(chart.is_pivot_chart(), "is_pivot_chart should be true");
    assert_eq!(chart.pivot_source.as_deref(), Some("ピボットテーブル1"));
    assert!(!chart.groups.is_empty(), "chart has groups");
    assert!(!chart.groups[0].series.is_empty(), "chart has series");
    assert_eq!(s.with(|e, _| e.selection), Some(chart_id), "inserted chart is selected");

    // 2. グラフの書式パネルを開き、ピボットグラフ専用UI（連動元表示など）を検証
    s.menu("xlsx.chart-format");
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()), "chart edit panel opened");

    // 3. ピボットテーブルのフィールド構成を変更し、連動ピボットグラフが自動同期されることを検証
    let target = (s.with(|e, _| e.current_slide), 0);
    let series_count_before = chart.groups[0].series.len();
    s.update(move |e, _, cx| {
        // フィールド2 (地域) を行から除外してピボットテーブルを再集計
        e.change_pivot(target, |p| {
            p.place(2, None, 0);
        }, cx);
    });

    // 連動ピボットグラフの系列や内容が自動更新されていることを確認
    let updated_chart = s.with(|e, _| {
        e.slide().shapes.iter().find_map(|sh| match &sh.kind {
            ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()),
            _ => None,
        })
    }).expect("chart still exists after pivot change");
    assert!(updated_chart.is_pivot_chart());
    assert_eq!(updated_chart.pivot_source.as_deref(), Some("ピボットテーブル1"));

    // 4. スクリーンショット保存
    s.screenshot("pivot020_pivot_chart");

    // 5. 元に戻す (Undo) & やり直し (Redo) の検証
    // change_pivot の Undo
    s.key("cmd-z");
    let chart_after_undo1 = s.with(|e, _| {
        e.slide().shapes.iter().find_map(|sh| match &sh.kind {
            ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()),
            _ => None,
        })
    }).expect("chart exists after undo pivot change");
    assert_eq!(chart_after_undo1.groups[0].series.len(), series_count_before);

    // ピボットグラフ挿入の Undo
    s.key("cmd-z");
    let charts_after_undo2 = s.with(|e, _| e.slide().shapes.iter().filter(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))).count());
    assert_eq!(charts_after_undo2, 0, "chart insertion undone");

    // やり直し (Redo)
    s.key("cmd-shift-z");
    let charts_after_redo1 = s.with(|e, _| e.slide().shapes.iter().filter(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))).count());
    assert_eq!(charts_after_redo1, 1, "chart insertion redone");
    let redone_chart = s.with(|e, _| {
        e.slide().shapes.iter().find_map(|sh| match &sh.kind {
            ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()),
            _ => None,
        })
    }).expect("chart exists after redo");
    assert!(redone_chart.is_pivot_chart());
    assert_eq!(redone_chart.pivot_source.as_deref(), Some("ピボットテーブル1"));

    // 6. OOXML 保存・開き直し（ラウンドトリップ検証）
    // 既存チャートを持つ XLSX フィクスチャに c:pivotSource を付与して開き、保存・再読込で pivotSource が保持されることを検証
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let source_path = dir.join("pivot020-pivot-chart-roundtrip.xlsx");

    let pivot_chart_xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:pivotSource><c:name>ピボットテーブル1</c:name><c:fmtId val="0"/></c:pivotSource><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>売上</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>東京</c:v></c:pt><c:pt idx="1"><c:v>大阪</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>100</c:v></c:pt><c:pt idx="1"><c:v>200</c:v></c:pt></c:numLit></c:val></c:ser><c:axId val="100000001"/><c:axId val="100000002"/></c:barChart><c:catAx><c:axId val="100000001"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="100000002"/></c:catAx><c:valAx><c:axId val="100000002"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="100000001"/></c:valAx></c:plotArea><c:legend><c:legendPos val="r"/><c:overlay val="0"/></c:legend></c:chart></c:chartSpace>"#;
    let original = std::fs::read(crate::shell::scenario::repo_root().join("test/editing/xlsx-sales.xlsx")).unwrap();
    let bytes = ibis_pptx::test_files::xlsx_with_chart_xml(&original, "xl/charts/chart1.xml", pivot_chart_xml);
    std::fs::write(&source_path, bytes).unwrap();
    drop(s);
    let mut s2 = Scenario::open("target/scenario-input/pivot020-pivot-chart-roundtrip.xlsx");
    let chart2 = s2.with(|e, _| {
        e.slide().shapes.iter().find_map(|sh| match &sh.kind {
            ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()),
            _ => None,
        })
    }).expect("chart found in roundtrip fixture");
    assert!(chart2.is_pivot_chart());
    assert_eq!(chart2.pivot_source.as_deref(), Some("ピボットテーブル1"));

    s2.save_as("pivot020_roundtrip_saved.xlsx");
    let reopened = s2.reopen();
    let reopened_chart = reopened.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).expect("chart retained in reopened file");
    assert!(reopened_chart.is_pivot_chart());
    assert_eq!(reopened_chart.pivot_source.as_deref(), Some("ピボットテーブル1"));
}

/// XLSX-CELL-006: ふりがな編集パネルによるふりがな設定、PHONETIC 関数との連動、表示/非表示トグル、Undo/Redo、保存・開き直し.
fn cell006_phonetic_edit_and_formula_sync() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. セル A10 に "アイビス"、B10 に "=PHONETIC(A10)" を入力
    s.click_cell("A10");
    s.ime("あいびす", "アイビス");
    s.key("enter");
    assert_eq!(s.value("A10"), CellValue::Text("アイビス".into()));

    s.click_cell("B10");
    s.type_text("=PHONETIC(A10)");
    s.key("enter");

    // phonetic が設定されていない時はセルのテキストをそのまま返す
    assert_eq!(s.shown("B10"), "アイビス");

    // 2. A10 を選択してふりがな編集メニューを開く
    s.click_cell("A10");
    s.menu("xlsx.phonetic-edit");
    assert!(s.with(|e, _| e.wb().panels.phonetic.is_some()));

    // 3. ふりがな "あいびす" を入力して適用
    s.update(|e, window, cx| {
        let p = e.wb().panels.phonetic.as_ref().unwrap();
        let input = p.input.clone();
        input.update(cx, |i, cx| i.set_value("あいびす".to_string(), window, cx));
        e.apply_phonetic(cx);
    });

    // パネルが閉じ、A10 の phonetic が "あいびす" に設定される
    assert!(s.with(|e, _| e.wb().panels.phonetic.is_none()));
    let phonetic_val = s.with(|e, _| {
        sheet_of_slide(e.slide()).and_then(|sh| sh.cell(9, 0)).and_then(|c| c.phonetic.as_deref().map(str::to_string))
    });
    assert_eq!(phonetic_val.as_deref(), Some("あいびす"));

    // 4. PHONETIC 関数が自動連動して再計算される
    assert_eq!(s.shown("B10"), "あいびす");

    // 5. ふりがなの表示/非表示トグル
    assert!(!s.with(|e, _| sheet_of_slide(e.slide()).unwrap().show_phonetic));
    s.menu("xlsx.phonetic-toggle");
    assert!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().show_phonetic));

    // 6. 元に戻す (Undo)
    // Undo 1: 表示トグルが戻る
    s.key("cmd-z");
    assert!(!s.with(|e, _| sheet_of_slide(e.slide()).unwrap().show_phonetic));

    // Undo 2: ふりがな編集が戻り、PHONETIC(A10) も "アイビス" に再計算される
    s.key("cmd-z");
    let undone_phonetic = s.with(|e, _| {
        sheet_of_slide(e.slide()).and_then(|sh| sh.cell(9, 0)).and_then(|c| c.phonetic.as_deref().map(str::to_string))
    });
    assert_eq!(undone_phonetic, None);
    assert_eq!(s.shown("B10"), "アイビス");

    // 7. やり直し (Redo)
    s.key("cmd-shift-z");
    assert_eq!(s.shown("B10"), "あいびす");

    s.key("cmd-shift-z");
    assert!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().show_phonetic));

    // 8. 保存と開き直し (Roundtrip)
    s.save_as("cell006_phonetic_roundtrip.xlsx");
    let reopened = s.reopen();
    let re_sheet = sheet_of_slide(&reopened.slides[0]).unwrap();
    let re_cell = re_sheet.cell(9, 0).unwrap();
    assert_eq!(re_cell.phonetic.as_deref(), Some("あいびす"));
    assert!(re_sheet.show_phonetic);

    drop(s);
}

pub(crate) fn fn_regex_test_extract_replace() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. セル Z1 にテスト用テキストを入力
    s.click_cell("Z1");
    s.enter("Order-2026-Tokyo", "enter");
    assert_eq!(s.shown("Z1"), "Order-2026-Tokyo");

    // 2. REGEXTEST: 数字が含まれるか
    s.click_cell("Z2");
    s.enter(r#"=REGEXTEST(Z1, "\d+")"#, "enter");
    assert_eq!(s.shown("Z2"), "TRUE");

    // 3. REGEXTEST: 小文字英字のみか (一致しない)
    s.click_cell("Z3");
    s.enter(r#"=REGEXTEST(Z1, "^[a-z]+$")"#, "enter");
    assert_eq!(s.shown("Z3"), "FALSE");

    // 4. REGEXEXTRACT: 数字 4 桁を抽出
    s.click_cell("Z4");
    s.enter(r#"=REGEXEXTRACT(Z1, "\d+")"#, "enter");
    assert_eq!(s.shown("Z4"), "2026");

    // 5. REGEXREPLACE: 2 番目に出現する単語 (Tokyo) を置換
    s.click_cell("Z5");
    s.enter(r#"=REGEXREPLACE(Z1, "[A-Za-z]+", "Japan", 2)"#, "enter");
    assert_eq!(s.shown("Z5"), "Order-2026-Japan");

    // 6. REGEXREPLACE: キャプチャグループ $1, $2, $3 による並べ替え
    s.click_cell("Z6");
    s.enter(r#"=REGEXREPLACE(Z1, "([A-Za-z]+)-(\d+)-([A-Za-z]+)", "$3/$2/$1")"#, "enter");
    assert_eq!(s.shown("Z6"), "Tokyo/2026/Order");

    // 7. 保存と開き直し (Roundtrip)
    s.save_as("fn_regex_roundtrip.xlsx");
    let reopened = s.reopen();
    // 再計算後の表示値がすべて保持・復元されていることを検証
    let sheet = sheet_of_slide(&reopened.slides[0]).unwrap();
    // 列 Z はインデックス 25
    assert_eq!(sheet.cell(0, 25).unwrap().value, CellValue::Text("Order-2026-Tokyo".into()));
    assert_eq!(sheet.cell(1, 25).unwrap().value, CellValue::Bool(true));
    assert_eq!(sheet.cell(2, 25).unwrap().value, CellValue::Bool(false));
    assert_eq!(sheet.cell(3, 25).unwrap().value, CellValue::Text("2026".into()));
    assert_eq!(sheet.cell(4, 25).unwrap().value, CellValue::Text("Order-2026-Japan".into()));
    assert_eq!(sheet.cell(5, 25).unwrap().value, CellValue::Text("Tokyo/2026/Order".into()));

    drop(s);
}

pub(crate) fn fn_bahttext() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. セル Z1 に数値 120.75 を入力
    s.click_cell("Z1");
    s.enter("120.75", "enter");
    assert_eq!(s.shown("Z1"), "120.75");

    // 2. セル参照での BAHTTEXT: 120.75
    s.click_cell("Z2");
    s.enter("=BAHTTEXT(Z1)", "enter");
    assert_eq!(s.shown("Z2"), "หนึ่งร้อยยี่สิบบาทเจ็ดสิบห้าสตางค์");

    // 3. ゼロ: 0 -> "ศูนย์บาทถ้วน"
    s.click_cell("Z3");
    s.enter("=BAHTTEXT(0)", "enter");
    assert_eq!(s.shown("Z3"), "ศูนย์บาทถ้วน");

    // 4. 4桁の 1 の位「เอ็ด」: 1001 -> "หนึ่งพันเอ็ดบาทถ้วน"
    s.click_cell("Z4");
    s.enter("=BAHTTEXT(1001)", "enter");
    assert_eq!(s.shown("Z4"), "หนึ่งพันเอ็ดบาทถ้วน");

    // 5. 負数: -25.5 -> "ลบยี่สิบห้าบาทห้าสิบสตางค์"
    s.click_cell("Z5");
    s.enter("=BAHTTEXT(-25.5)", "enter");
    assert_eq!(s.shown("Z5"), "ลบยี่สิบห้าบาทห้าสิบสตางค์");

    // 6. 小数のみ（サタンのみ）: 0.25 -> "ยี่สิบห้าสตางค์"
    s.click_cell("Z6");
    s.enter("=BAHTTEXT(0.25)", "enter");
    assert_eq!(s.shown("Z6"), "ยี่สิบห้าสตางค์");

    // 7. 保存と開き直し (Roundtrip)
    s.save_as("fn_bahttext_roundtrip.xlsx");
    let reopened = s.reopen();
    let sheet = sheet_of_slide(&reopened.slides[0]).unwrap();
    // 列 Z はインデックス 25
    assert_eq!(sheet.cell(0, 25).unwrap().value, CellValue::Number(120.75));
    assert_eq!(sheet.cell(1, 25).unwrap().value, CellValue::Text("หนึ่งร้อยยี่สิบบาทเจ็ดสิบห้าสตางค์".into()));
    assert_eq!(sheet.cell(2, 25).unwrap().value, CellValue::Text("ศูนย์บาทถ้วน".into()));
    assert_eq!(sheet.cell(3, 25).unwrap().value, CellValue::Text("หนึ่งพันเอ็ดบาทถ้วน".into()));
    assert_eq!(sheet.cell(4, 25).unwrap().value, CellValue::Text("ลบยี่สิบห้าบาทห้าสิบสตางค์".into()));
    assert_eq!(sheet.cell(5, 25).unwrap().value, CellValue::Text("ยี่สิบห้าสตางค์".into()));

    drop(s);
}

pub(crate) fn fn_cumipmt_cumprinc() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. CUMIPMT (通常・期末払い due=0)
    s.click_cell("AA1");
    s.enter("=CUMIPMT(5, 10, 2, 2, 10, 0)", "enter");
    assert!(s.shown("AA1").starts_with("-88.000001"));

    // 2. CUMPRINC (通常・期末払い due=0)
    s.click_cell("AA2");
    s.enter("=CUMPRINC(5, 10, 2, 2, 10, 0)", "enter");
    assert!(s.shown("AA2").starts_with("-1.999999"));

    // 3. CUMIPMT (期首払い due=1)
    s.click_cell("AA3");
    s.enter("=CUMIPMT(5, 10, 2, 2, 10, 1)", "enter");
    assert!(s.shown("AA3").starts_with("-14.66666"));

    // 4. CUMIPMT (極大期間 nper=1000 オーバーフローなし・正確な極限評価)
    s.click_cell("AA4");
    s.enter("=CUMIPMT(5, 1000, 2, 2, 10, 0)", "enter");
    assert_eq!(s.shown("AA4"), "-90");

    // 5. 保存と再オープン (Roundtrip) での値および数式復元検証
    s.save_as("fn_cumipmt_roundtrip.xlsx");
    let reopened = s.reopen();
    let sheet = sheet_of_slide(&reopened.slides[0]).unwrap();
    // 列 AA はインデックス 26
    if let CellValue::Number(v) = sheet.cell(0, 26).unwrap().value {
        assert!((v - (-88.00000165381719)).abs() < 1e-12);
    } else {
        panic!("AA1 not a number");
    }
    if let CellValue::Number(v) = sheet.cell(1, 26).unwrap().value {
        assert!((v - (-1.999999834618281)).abs() < 1e-12);
    } else {
        panic!("AA2 not a number");
    }
    if let CellValue::Number(v) = sheet.cell(2, 26).unwrap().value {
        assert!((v - (-14.666666942302866)).abs() < 1e-12);
    } else {
        panic!("AA3 not a number");
    }
    if let CellValue::Number(v) = sheet.cell(3, 26).unwrap().value {
        assert_eq!(v, -90.0);
    } else {
        panic!("AA4 not a number");
    }

    drop(s);
}

pub(crate) fn fn_ppmt() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. PPMT (通常・期末払い due=0)
    s.click_cell("AA1");
    s.enter("=PPMT(2.5, 5, 5, 2)", "enter");
    assert!(s.shown("AA1").starts_with("-1.431296"));

    // 2. PPMT (極大金利 rate=1E15 期末払い due=0 元本返済極限値 -2.0)
    s.click_cell("AA2");
    s.enter("=PPMT(1E+15, 5, 5, 2)", "enter");
    assert_eq!(s.shown("AA2"), "-2");

    // 3. PPMT (期首払い due=1 第1期元利均等返済)
    s.click_cell("AA3");
    s.enter("=PPMT(1, 5, 5, 1, 0.05, 1)", "enter");
    assert!(s.shown("AA3").starts_with("-0.270967"));

    // 4. PPMT (期首払い due=1 第5期・完済時の元本返済 0)
    s.click_cell("AA4");
    s.enter("=PPMT(0.5, 5, 5, 1, -1, 1)", "enter");
    assert_eq!(s.shown("AA4"), "0");

    // 5. 保存と再オープン (Roundtrip) での値および数式復元検証
    s.save_as("fn_ppmt_roundtrip.xlsx");
    let reopened = s.reopen();
    let sheet = sheet_of_slide(&reopened.slides[0]).unwrap();
    // 列 AA はインデックス 26
    if let CellValue::Number(v) = sheet.cell(0, 26).unwrap().value {
        assert!((v - (-1.4312965722801787)).abs() < 1e-12);
    } else {
        panic!("AA1 not a number");
    }
    if let CellValue::Number(v) = sheet.cell(1, 26).unwrap().value {
        assert!((v - (-2.0)).abs() < 1e-12);
    } else {
        panic!("AA2 not a number");
    }
    if let CellValue::Number(v) = sheet.cell(2, 26).unwrap().value {
        assert!((v - (-0.2709677419354839)).abs() < 1e-12);
    } else {
        panic!("AA3 not a number");
    }
    if let CellValue::Number(v) = sheet.cell(3, 26).unwrap().value {
        assert_eq!(v, 0.0);
    } else {
        panic!("AA4 not a number");
    }

    drop(s);
}













/// スペル チェック (ツール > スペル): a misspelling in text cells is listed with
/// suggestions, changed (one, or every one of the same spelling in one step),
/// ignored, undone; formulas' results are not checked; in English no Japanese.
fn spelling_check() {
    use crate::shell::i18n::{self, Language};
    if !crate::spelling::available() {
        return;
    }
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("J2");
    s.enter("We recieve it", "enter");
    s.click_cell("J3");
    s.enter("recieve plan", "enter");
    s.click_cell("J4");
    s.enter("zzqxv", "enter");
    s.click_cell("J5");
    s.enter("=\"recieve\"&\"x\"", "enter");
    let found = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        crate::xlsx::spelling_panel::scan(sheet, None, (0, 0), &Default::default()).iter().filter(|i| i.word == "recieve").count()
    });
    if found != 2 {
        // No English dictionary here.
        return;
    }
    // A single cell selected: the whole sheet from it on.
    s.click_cell("J3");
    s.menu("pptx.spelling.check");
    let (first, count) = s.with(|e, _| {
        let p = e.sheet_state().unwrap().spelling.as_ref().expect("the panel is open");
        (p.issues[0].clone(), p.issues.len())
    });
    assert_eq!((first.row, first.column, first.word.as_str()), (2, 9, "recieve"), "starts at the selected cell");
    assert!(count >= 2, "{count}");
    assert_eq!(s.selected(), "J3");
    let suggestions = s.with(|e, _| e.sheet_state().unwrap().spelling.as_ref().unwrap().suggestions.clone());
    assert!(suggestions.iter().any(|x| x == "receive"), "{suggestions:?}");
    s.screenshot("spelling");
    // 変更: this one only.
    s.update(|e, w, cx| e.sheet_spelling_change("receive", false, w, cx));
    assert_eq!(s.value("J3"), CellValue::Text("receive plan".into()));
    assert_eq!(s.value("J2"), CellValue::Text("We recieve it".into()));
    // すべて変更: the other one (the panel went on to 'zzqxv' after the change).
    s.update(|e, w, cx| e.sheet_spelling_step(true, w, cx));
    assert_eq!(s.selected(), "J2");
    s.update(|e, w, cx| e.sheet_spelling_change("receive", true, w, cx));
    assert_eq!(s.value("J2"), CellValue::Text("We receive it".into()));
    // 無視: the next word is not told again; すべて無視 likewise.
    let word = s.with(|e, _| e.sheet_state().unwrap().spelling.as_ref().unwrap().issues.first().map(|i| i.word.clone()));
    if let Some(word) = word {
        s.update(|e, w, cx| e.sheet_spelling_ignore(true, w, cx));
        assert!(s.with(move |e, _| e.sheet_state().unwrap().spelling.as_ref().unwrap().issues.iter().all(|i| i.word != word)));
    }
    s.update(|e, w, cx| e.close_sheet_spelling(w, cx));
    assert!(s.with(|e, _| e.sheet_state().unwrap().spelling.is_none()));
    // Undone: all in one step.
    s.click_cell("J3");
    s.key("cmd-z");
    assert_eq!(s.value("J2"), CellValue::Text("We recieve it".into()));
    s.key("cmd-z");
    assert_eq!(s.value("J3"), CellValue::Text("recieve plan".into()));
    // A range: only its cells.
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let st = e.wb_mut();
        st.cells.insert(slide, (9, 2));
        st.cell_ends.insert(slide, (9, 2));
    });
    s.menu("pptx.spelling.check");
    assert!(s.with(|e, _| e.sheet_state().unwrap().spelling.as_ref().unwrap().range.is_none()), "one cell: the sheet");
    s.update(|e, w, cx| e.close_sheet_spelling(w, cx));
    // English: the menu and the panel have no Japanese.
    i18n::show(Language::English);
    s.menu("pptx.spelling.check");
    let title = s.update(|e, _, cx| e.sheet_spelling_panel(cx).map(|p| p.title.to_string()));
    assert_eq!(title.as_deref(), Some("Spelling"));
    s.screenshot("spelling-english");
    let label = s.with(|e, _| e.menus().find_mut("pptx.spelling.check").map(|i| i.label.to_string()));
    assert_eq!(label.as_deref(), Some("Spelling…"));
    s.update(|e, w, cx| e.close_sheet_spelling(w, cx));
    i18n::show(Language::Japanese);
    s.update(|_, _, _| {});
}
