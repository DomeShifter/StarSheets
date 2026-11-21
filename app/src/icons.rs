//! Toolbar icons: Material Symbols SVGs (Apache-2.0, see app/icons/README.md), embedded at build time.

use egui::{Image, ImageSource, vec2};

const ICONS: &[(&str, &[u8])] = &[
    ("add", include_bytes!("../icons/add.svg")),
    ("add_comment", include_bytes!("../icons/add_comment.svg")),
    ("arrow_drop_down", include_bytes!("../icons/arrow_drop_down.svg")),
    ("border_all", include_bytes!("../icons/border_all.svg")),
    ("border_bottom", include_bytes!("../icons/border_bottom.svg")),
    ("border_clear", include_bytes!("../icons/border_clear.svg")),
    ("border_horizontal", include_bytes!("../icons/border_horizontal.svg")),
    ("border_inner", include_bytes!("../icons/border_inner.svg")),
    ("border_left", include_bytes!("../icons/border_left.svg")),
    ("border_outer", include_bytes!("../icons/border_outer.svg")),
    ("border_right", include_bytes!("../icons/border_right.svg")),
    ("border_top", include_bytes!("../icons/border_top.svg")),
    ("border_vertical", include_bytes!("../icons/border_vertical.svg")),
    ("cell_merge", include_bytes!("../icons/cell_merge.svg")),
    ("content_copy", include_bytes!("../icons/content_copy.svg")),
    ("decimal_decrease", include_bytes!("../icons/decimal_decrease.svg")),
    ("decimal_increase", include_bytes!("../icons/decimal_increase.svg")),
    ("edit", include_bytes!("../icons/edit.svg")),
    ("filter_alt", include_bytes!("../icons/filter_alt.svg")),
    ("format_align_center", include_bytes!("../icons/format_align_center.svg")),
    ("format_align_left", include_bytes!("../icons/format_align_left.svg")),
    ("format_align_right", include_bytes!("../icons/format_align_right.svg")),
    ("format_bold", include_bytes!("../icons/format_bold.svg")),
    ("format_clear", include_bytes!("../icons/format_clear.svg")),
    ("format_color_fill", include_bytes!("../icons/format_color_fill.svg")),
    ("format_color_text", include_bytes!("../icons/format_color_text.svg")),
    ("format_italic", include_bytes!("../icons/format_italic.svg")),
    ("format_paint", include_bytes!("../icons/format_paint.svg")),
    ("functions", include_bytes!("../icons/functions.svg")),
    ("history", include_bytes!("../icons/history.svg")),
    ("insert_chart", include_bytes!("../icons/insert_chart.svg")),
    ("keyboard_arrow_down", include_bytes!("../icons/keyboard_arrow_down.svg")),
    ("keyboard_arrow_up", include_bytes!("../icons/keyboard_arrow_up.svg")),
    ("link", include_bytes!("../icons/link.svg")),
    ("link_off", include_bytes!("../icons/link_off.svg")),
    ("menu", include_bytes!("../icons/menu.svg")),
    ("percent", include_bytes!("../icons/percent.svg")),
    ("print", include_bytes!("../icons/print.svg")),
    ("redo", include_bytes!("../icons/redo.svg")),
    ("remove", include_bytes!("../icons/remove.svg")),
    ("search", include_bytes!("../icons/search.svg")),
    ("strikethrough_s", include_bytes!("../icons/strikethrough_s.svg")),
    ("text_rotation_angleup", include_bytes!("../icons/text_rotation_angleup.svg")),
    ("undo", include_bytes!("../icons/undo.svg")),
    ("vertical_align_bottom", include_bytes!("../icons/vertical_align_bottom.svg")),
    ("vertical_align_center", include_bytes!("../icons/vertical_align_center.svg")),
    ("vertical_align_top", include_bytes!("../icons/vertical_align_top.svg")),
    ("wrap_text", include_bytes!("../icons/wrap_text.svg")),
    ("zoom_in", include_bytes!("../icons/zoom_in.svg")),
];

/// The icon called `name`, 18 px, or None if it isn't embedded (the test below checks the names
/// the toolbar uses).
pub fn icon(name: &str) -> Option<Image<'static>> {
    let (n, bytes) = ICONS.iter().find(|(n, _)| *n == name)?;
    Some(Image::new(ImageSource::Bytes { uri: format!("bytes://starsheets/{n}.svg").into(), bytes: (*bytes).into() }).fit_to_exact_size(vec2(18.0, 18.0)))
}

#[cfg(test)]
pub fn has(name: &str) -> bool {
    ICONS.iter().any(|(n, _)| *n == name)
}
