//! Render indent guides as Neovim extmarks with virtual text.
//!
//! Each indent level on each line gets a thin vertical `│` character placed
//! via overlay virtual text.  The current scope's guides use a distinct
//! highlight group so users can visually track the active block.

use nvim_oxi::api::{self, Buffer, Window};
use nvim_oxi::api::opts::{OptionOpts, SetExtmarkOpts};
use nvim_oxi::api::types::ExtmarkVirtTextPosition;

use crate::indent::{self, IndentStyle, Scope};

/// The character used for indent guide lines.
const GUIDE_CHAR: &str = "\u{2502}"; // │

/// Highlight group names.
pub const HL_INDENT: &str = "SujiIndent";
pub const HL_SCOPE: &str = "SujiScope";

/// Read the buffer's `shiftwidth` option (falls back to `tabstop` when
/// shiftwidth is 0, matching Neovim's behaviour).
fn buf_shiftwidth(buf: &Buffer) -> nvim_oxi::Result<usize> {
    let opts = OptionOpts::builder().buffer(buf.clone()).build();
    let sw: i64 = api::get_option_value("shiftwidth", &opts)?;
    if sw > 0 {
        return Ok(sw as usize);
    }
    let ts: i64 = api::get_option_value("tabstop", &opts)?;
    Ok(ts.max(1) as usize)
}

/// Read the buffer's `expandtab` option.
fn buf_expandtab(buf: &Buffer) -> nvim_oxi::Result<bool> {
    let opts = OptionOpts::builder().buffer(buf.clone()).build();
    let et: bool = api::get_option_value("expandtab", &opts)?;
    Ok(et)
}

/// Determine the indent style for a buffer.
fn buf_indent_style(buf: &Buffer, lines: &[&str]) -> nvim_oxi::Result<IndentStyle> {
    let sw = buf_shiftwidth(buf)?;
    let et = buf_expandtab(buf)?;

    // If the buffer has content, try to auto-detect.
    if !lines.is_empty() {
        let detected = indent::detect_style(lines, sw);
        return Ok(detected);
    }

    // Empty buffer: trust the vim options.
    if et {
        Ok(IndentStyle::Spaces(sw))
    } else {
        Ok(IndentStyle::Tabs)
    }
}

/// Clear all suji extmarks in the buffer.
fn clear(buf: &mut Buffer, ns_id: u32) -> nvim_oxi::Result<()> {
    buf.clear_namespace(ns_id, ..)?;
    Ok(())
}

/// Place a single guide extmark on `line` at column `col`.
fn place_guide(
    buf: &mut Buffer,
    ns_id: u32,
    line: usize,
    col: usize,
    hl_group: &str,
) -> nvim_oxi::Result<()> {
    let opts = SetExtmarkOpts::builder()
        .virt_text([(GUIDE_CHAR, hl_group)])
        .virt_text_pos(ExtmarkVirtTextPosition::Overlay)
        .priority(1)
        .build();
    buf.set_extmark(ns_id, line, col, &opts)?;
    Ok(())
}

/// Compute the column offset for a given indent level.
///
/// For spaces: `level * width`.
/// For tabs: `level` (each tab is one character).
const fn guide_col(level: usize, style: IndentStyle) -> usize {
    match style {
        IndentStyle::Tabs => level,
        IndentStyle::Spaces(w) => level * w,
    }
}

/// Full refresh: clear existing guides and redraw for the entire buffer.
///
/// `ns_id` — the namespace created for suji.
/// `scope` — optional scope to highlight differently.
pub fn render(
    buf: &mut Buffer,
    ns_id: u32,
    levels: &[usize],
    style: IndentStyle,
    scope: Option<Scope>,
) -> nvim_oxi::Result<()> {
    clear(buf, ns_id)?;

    for (line_idx, &level) in levels.iter().enumerate() {
        for guide_level in 0..level {
            let col = guide_col(guide_level, style);
            let in_scope = scope.is_some_and(|s| {
                line_idx >= s.start
                    && line_idx <= s.end
                    && guide_level == s.level - 1
            });
            let hl = if in_scope { HL_SCOPE } else { HL_INDENT };
            place_guide(buf, ns_id, line_idx, col, hl)?;
        }
    }

    Ok(())
}

/// Convenience: read buffer lines, analyse, and render guides.
///
/// Called from autocommand callbacks.
pub fn refresh(buf: &mut Buffer, win: &Window, ns_id: u32) -> nvim_oxi::Result<()> {
    let line_count = buf.line_count()?;
    if line_count == 0 {
        clear(buf, ns_id)?;
        return Ok(());
    }

    // Collect lines as owned strings, then borrow.
    let oxi_lines: Vec<nvim_oxi::String> = buf.get_lines(0..line_count, false)?.collect();
    let owned: Vec<String> = oxi_lines
        .iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    let lines: Vec<&str> = owned.iter().map(String::as_str).collect();

    let style = buf_indent_style(buf, &lines)?;
    let levels = indent::compute_levels(&lines, style);

    // Cursor line for scope detection (get_cursor returns 1-indexed row).
    let (cursor_row, _) = win.get_cursor()?;
    let cursor_line = cursor_row.saturating_sub(1);
    let scope = indent::find_scope(&levels, cursor_line);

    render(buf, ns_id, &levels, style, scope)?;

    Ok(())
}
