//! Pure Rust indent analysis.
//!
//! Calculates indent levels for lines, detects indent width (tabs vs spaces),
//! and provides scope boundary detection. This module is entirely decoupled
//! from Neovim APIs so that every function can be unit-tested in isolation.

/// Describes how a buffer is indented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentStyle {
    /// Hard tabs — each `\t` counts as one indent level.
    Tabs,
    /// Spaces — every `width` consecutive leading spaces counts as one level.
    Spaces(usize),
}

impl IndentStyle {
    /// Convenience: the column width consumed by one indent level.
    #[must_use]
    pub const fn width(self) -> usize {
        match self {
            Self::Tabs => 1, // one tab character = one level
            Self::Spaces(w) => w,
        }
    }
}

/// Returns the number of leading whitespace *columns* in `line`.
///
/// Tabs are counted as `tab_width` columns; spaces as 1.
#[must_use]
pub fn leading_whitespace(line: &str, tab_width: usize) -> usize {
    let mut cols = 0;
    for ch in line.chars() {
        match ch {
            '\t' => cols += tab_width,
            ' ' => cols += 1,
            _ => break,
        }
    }
    cols
}

/// Returns the raw number of leading whitespace *characters* in `line`
/// (i.e. the byte-count of the leading whitespace prefix, assuming ASCII
/// whitespace).
#[must_use]
pub fn leading_ws_chars(line: &str) -> usize {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count()
}

/// Calculates the indent level of a single line given an [`IndentStyle`].
///
/// For spaces the level is `leading_columns / width` (integer division).
/// For tabs the level equals the number of leading tab characters.
#[must_use]
pub fn indent_level(line: &str, style: IndentStyle) -> usize {
    match style {
        IndentStyle::Tabs => line.chars().take_while(|c| *c == '\t').count(),
        IndentStyle::Spaces(w) => {
            if w == 0 {
                return 0;
            }
            leading_whitespace(line, 1) / w
        }
    }
}

/// Returns `true` when a line consists solely of whitespace (or is empty).
#[must_use]
pub fn is_blank(line: &str) -> bool {
    line.chars().all(|c| c == ' ' || c == '\t')
}

/// Computes indent levels for every line in a set of lines.
///
/// Blank lines inherit the *minimum* level of their surrounding non-blank
/// neighbours so that indent guides are drawn continuously through empty
/// lines inside a block.
#[must_use]
pub fn compute_levels(lines: &[&str], style: IndentStyle) -> Vec<usize> {
    let n = lines.len();
    if n == 0 {
        return Vec::new();
    }

    // First pass: raw levels; blank lines get `usize::MAX` as sentinel.
    let mut levels: Vec<usize> = lines
        .iter()
        .map(|line| {
            if is_blank(line) {
                usize::MAX
            } else {
                indent_level(line, style)
            }
        })
        .collect();

    // Second pass: propagate from non-blank neighbours.
    // Forward pass: blank lines inherit the level of the previous non-blank line.
    let mut prev = 0usize;
    let mut forward = vec![0usize; n];
    for (i, lvl) in levels.iter().enumerate() {
        if *lvl == usize::MAX {
            forward[i] = prev;
        } else {
            prev = *lvl;
            forward[i] = *lvl;
        }
    }

    // Backward pass.
    let mut next = 0usize;
    let mut backward = vec![0usize; n];
    for i in (0..n).rev() {
        if levels[i] == usize::MAX {
            backward[i] = next;
        } else {
            next = levels[i];
            backward[i] = levels[i];
        }
    }

    // Merge: blank lines take the minimum of forward/backward so they belong
    // to the shallower enclosing scope.
    for i in 0..n {
        if levels[i] == usize::MAX {
            levels[i] = forward[i].min(backward[i]);
        }
    }

    levels
}

/// Detect the dominant indent style by scanning the lines.
///
/// Heuristic: count lines that start with tabs vs lines that start with
/// spaces (ignoring blank lines).  When spaces win, the indent width is the
/// GCD of all leading-space counts (clamped to `[2, 8]`).
///
/// Falls back to `IndentStyle::Spaces(shiftwidth)` when the buffer is empty
/// or ambiguous.
#[must_use]
pub fn detect_style(lines: &[&str], fallback_width: usize) -> IndentStyle {
    let mut tab_lines = 0u32;
    let mut space_lines = 0u32;
    let mut space_gcd: usize = 0;

    for line in lines {
        // Skip blank lines — they carry no indent information.
        if is_blank(line) {
            continue;
        }
        let mut chars = line.chars();
        match chars.next() {
            Some('\t') => tab_lines += 1,
            Some(' ') => {
                let count = 1 + chars.take_while(|c| *c == ' ').count();
                space_lines += 1;
                space_gcd = gcd(space_gcd, count);
            }
            _ => {}
        }
    }

    if tab_lines == 0 && space_lines == 0 {
        return IndentStyle::Spaces(fallback_width);
    }

    if tab_lines >= space_lines {
        IndentStyle::Tabs
    } else {
        let width = space_gcd.clamp(2, 8);
        IndentStyle::Spaces(width)
    }
}

/// Greatest common divisor (Euclidean algorithm).
const fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// A contiguous scope defined by a range of lines that share a common indent
/// level greater than or equal to a threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope {
    /// First line of the scope (0-indexed, inclusive).
    pub start: usize,
    /// Last line of the scope (0-indexed, inclusive).
    pub end: usize,
    /// The indent level that defines this scope.
    pub level: usize,
}

/// Find the innermost scope surrounding `cursor_line`.
///
/// A "scope" is a maximal contiguous block of lines whose indent level is
/// **strictly greater than** the indent level of the boundary lines above
/// and below.
///
/// `levels` must have been produced by [`compute_levels`].
#[must_use]
pub fn find_scope(levels: &[usize], cursor_line: usize) -> Option<Scope> {
    if levels.is_empty() || cursor_line >= levels.len() {
        return None;
    }

    let cursor_level = levels[cursor_line];
    if cursor_level == 0 {
        return None;
    }

    // Walk upward to find a line with a strictly lower level.
    let mut start = cursor_line;
    while start > 0 && levels[start - 1] >= cursor_level {
        start -= 1;
    }

    // Walk downward.
    let mut end = cursor_line;
    while end + 1 < levels.len() && levels[end + 1] >= cursor_level {
        end += 1;
    }

    Some(Scope {
        start,
        end,
        level: cursor_level,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── leading_whitespace ────────────────────────────────────────────

    #[test]
    fn leading_whitespace_empty() {
        assert_eq!(leading_whitespace("", 4), 0);
    }

    #[test]
    fn leading_whitespace_no_indent() {
        assert_eq!(leading_whitespace("hello", 4), 0);
    }

    #[test]
    fn leading_whitespace_spaces() {
        assert_eq!(leading_whitespace("    hello", 4), 4);
    }

    #[test]
    fn leading_whitespace_tabs() {
        assert_eq!(leading_whitespace("\t\thello", 4), 8);
    }

    #[test]
    fn leading_whitespace_mixed() {
        assert_eq!(leading_whitespace("\t  hello", 4), 6);
    }

    // ── leading_ws_chars ──────────────────────────────────────────────

    #[test]
    fn ws_chars_empty() {
        assert_eq!(leading_ws_chars(""), 0);
    }

    #[test]
    fn ws_chars_tabs_and_spaces() {
        assert_eq!(leading_ws_chars("\t  x"), 3);
    }

    // ── indent_level ──────────────────────────────────────────────────

    #[test]
    fn indent_level_spaces_basic() {
        let style = IndentStyle::Spaces(4);
        assert_eq!(indent_level("", style), 0);
        assert_eq!(indent_level("x", style), 0);
        assert_eq!(indent_level("    x", style), 1);
        assert_eq!(indent_level("        x", style), 2);
        assert_eq!(indent_level("      x", style), 1); // 6/4 = 1
    }

    #[test]
    fn indent_level_spaces_two() {
        let style = IndentStyle::Spaces(2);
        assert_eq!(indent_level("  x", style), 1);
        assert_eq!(indent_level("    x", style), 2);
        assert_eq!(indent_level("   x", style), 1); // 3/2 = 1
    }

    #[test]
    fn indent_level_tabs() {
        let style = IndentStyle::Tabs;
        assert_eq!(indent_level("", style), 0);
        assert_eq!(indent_level("x", style), 0);
        assert_eq!(indent_level("\tx", style), 1);
        assert_eq!(indent_level("\t\tx", style), 2);
    }

    #[test]
    fn indent_level_zero_width_spaces() {
        // Edge case: width = 0 should not panic.
        assert_eq!(indent_level("    x", IndentStyle::Spaces(0)), 0);
    }

    // ── is_blank ──────────────────────────────────────────────────────

    #[test]
    fn blank_lines() {
        assert!(is_blank(""));
        assert!(is_blank("   "));
        assert!(is_blank("\t\t"));
        assert!(is_blank(" \t "));
        assert!(!is_blank("  x"));
    }

    // ── compute_levels ────────────────────────────────────────────────

    #[test]
    fn compute_levels_empty() {
        assert_eq!(compute_levels(&[], IndentStyle::Spaces(4)), Vec::<usize>::new());
    }

    #[test]
    fn compute_levels_simple() {
        let lines = ["fn main() {", "    let x = 1;", "    let y = 2;", "}"];
        let refs: Vec<&str> = lines.iter().copied().collect();
        let levels = compute_levels(&refs, IndentStyle::Spaces(4));
        assert_eq!(levels, vec![0, 1, 1, 0]);
    }

    #[test]
    fn compute_levels_blank_inherits() {
        let lines = [
            "fn main() {",
            "    let x = 1;",
            "",
            "    let y = 2;",
            "}",
        ];
        let refs: Vec<&str> = lines.iter().copied().collect();
        let levels = compute_levels(&refs, IndentStyle::Spaces(4));
        // The blank line between two level-1 lines should be level 1.
        assert_eq!(levels, vec![0, 1, 1, 1, 0]);
    }

    #[test]
    fn compute_levels_blank_at_boundary() {
        let lines = [
            "fn main() {",
            "    if true {",
            "        deep();",
            "",
            "    }",
            "}",
        ];
        let refs: Vec<&str> = lines.iter().copied().collect();
        let levels = compute_levels(&refs, IndentStyle::Spaces(4));
        // Blank line at index 3: forward=2, backward=1 → min=1.
        assert_eq!(levels, vec![0, 1, 2, 1, 1, 0]);
    }

    #[test]
    fn compute_levels_all_blank() {
        let lines = ["", "  ", "\t"];
        let refs: Vec<&str> = lines.iter().copied().collect();
        let levels = compute_levels(&refs, IndentStyle::Spaces(4));
        assert_eq!(levels, vec![0, 0, 0]);
    }

    #[test]
    fn compute_levels_tabs() {
        let lines = ["fn f() {", "\tlet x = 1;", "\t\tdeep();", "\t}", ""];
        let refs: Vec<&str> = lines.iter().copied().collect();
        let levels = compute_levels(&refs, IndentStyle::Tabs);
        assert_eq!(levels, vec![0, 1, 2, 1, 0]);
    }

    // ── detect_style ──────────────────────────────────────────────────

    #[test]
    fn detect_empty_fallback() {
        assert_eq!(detect_style(&[], 4), IndentStyle::Spaces(4));
    }

    #[test]
    fn detect_tabs() {
        let lines = ["\tlet x;", "\t\ty();", "end"];
        let refs: Vec<&str> = lines.iter().copied().collect();
        assert_eq!(detect_style(&refs, 4), IndentStyle::Tabs);
    }

    #[test]
    fn detect_spaces_four() {
        let lines = ["    let x;", "        y();", "end"];
        let refs: Vec<&str> = lines.iter().copied().collect();
        assert_eq!(detect_style(&refs, 4), IndentStyle::Spaces(4));
    }

    #[test]
    fn detect_spaces_two() {
        let lines = ["  let x;", "    y();", "end"];
        let refs: Vec<&str> = lines.iter().copied().collect();
        assert_eq!(detect_style(&refs, 4), IndentStyle::Spaces(2));
    }

    #[test]
    fn detect_mixed_tabs_win() {
        let lines = ["\tx", "\ty", "  z"];
        let refs: Vec<&str> = lines.iter().copied().collect();
        assert_eq!(detect_style(&refs, 4), IndentStyle::Tabs);
    }

    #[test]
    fn detect_all_blank_lines() {
        let lines = ["", "   ", "\t"];
        let refs: Vec<&str> = lines.iter().copied().collect();
        assert_eq!(detect_style(&refs, 2), IndentStyle::Spaces(2));
    }

    // ── find_scope ────────────────────────────────────────────────────

    #[test]
    fn scope_none_at_level_zero() {
        let levels = vec![0, 1, 1, 0];
        assert_eq!(find_scope(&levels, 0), None);
        assert_eq!(find_scope(&levels, 3), None);
    }

    #[test]
    fn scope_simple() {
        let levels = vec![0, 1, 1, 0];
        assert_eq!(
            find_scope(&levels, 1),
            Some(Scope {
                start: 1,
                end: 2,
                level: 1,
            })
        );
    }

    #[test]
    fn scope_nested() {
        // 0: fn main() {
        // 1:     if true {
        // 2:         deep();
        // 3:     }
        // 4: }
        let levels = vec![0, 1, 2, 1, 0];
        assert_eq!(
            find_scope(&levels, 2),
            Some(Scope {
                start: 2,
                end: 2,
                level: 2,
            })
        );
        assert_eq!(
            find_scope(&levels, 1),
            Some(Scope {
                start: 1,
                end: 3,
                level: 1,
            })
        );
    }

    #[test]
    fn scope_out_of_bounds() {
        let levels = vec![0, 1, 0];
        assert_eq!(find_scope(&levels, 99), None);
    }

    #[test]
    fn scope_empty_levels() {
        assert_eq!(find_scope(&[], 0), None);
    }

    #[test]
    fn scope_continuous_block() {
        let levels = vec![0, 2, 2, 2, 0];
        assert_eq!(
            find_scope(&levels, 2),
            Some(Scope {
                start: 1,
                end: 3,
                level: 2,
            })
        );
    }

    #[test]
    fn scope_at_boundary_between_blocks() {
        let levels = vec![0, 1, 0, 1, 0];
        assert_eq!(
            find_scope(&levels, 1),
            Some(Scope {
                start: 1,
                end: 1,
                level: 1,
            })
        );
        assert_eq!(
            find_scope(&levels, 3),
            Some(Scope {
                start: 3,
                end: 3,
                level: 1,
            })
        );
    }

    // ── gcd ───────────────────────────────────────────────────────────

    #[test]
    fn gcd_basic() {
        assert_eq!(gcd(0, 0), 0);
        assert_eq!(gcd(4, 0), 4);
        assert_eq!(gcd(0, 4), 4);
        assert_eq!(gcd(4, 8), 4);
        assert_eq!(gcd(6, 4), 2);
        assert_eq!(gcd(12, 8), 4);
    }

    // ── IndentStyle::width ────────────────────────────────────────────

    #[test]
    fn style_width() {
        assert_eq!(IndentStyle::Tabs.width(), 1);
        assert_eq!(IndentStyle::Spaces(4).width(), 4);
        assert_eq!(IndentStyle::Spaces(2).width(), 2);
    }
}
