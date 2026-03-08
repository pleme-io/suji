//! Scope detection for indent-aware highlighting.
//!
//! Wraps the pure analysis in [`crate::indent`] and exposes a single
//! high-level function that Neovim-facing code can call to get the current
//! scope boundaries for the cursor position.

use crate::indent::{self, IndentStyle, Scope};

/// Detect the scope surrounding `cursor_line` in the given lines.
///
/// Returns `None` when the cursor sits at indent level 0 (no enclosing scope).
#[must_use]
pub fn detect_scope(lines: &[&str], cursor_line: usize, style: IndentStyle) -> Option<Scope> {
    let levels = indent::compute_levels(lines, style);
    indent::find_scope(&levels, cursor_line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indent::IndentStyle;

    #[test]
    fn detect_scope_basic() {
        let lines = ["fn f() {", "    body();", "    more();", "}"];
        let refs: Vec<&str> = lines.iter().copied().collect();
        let scope = detect_scope(&refs, 1, IndentStyle::Spaces(4));
        assert_eq!(
            scope,
            Some(Scope {
                start: 1,
                end: 2,
                level: 1,
            })
        );
    }

    #[test]
    fn detect_scope_no_scope_at_top() {
        let lines = ["top_level();", "another();"];
        let refs: Vec<&str> = lines.iter().copied().collect();
        assert_eq!(detect_scope(&refs, 0, IndentStyle::Spaces(4)), None);
    }

    #[test]
    fn detect_scope_with_blank_line() {
        let lines = [
            "fn f() {",
            "    a();",
            "",
            "    b();",
            "}",
        ];
        let refs: Vec<&str> = lines.iter().copied().collect();
        let scope = detect_scope(&refs, 2, IndentStyle::Spaces(4));
        assert_eq!(
            scope,
            Some(Scope {
                start: 1,
                end: 3,
                level: 1,
            })
        );
    }

    #[test]
    fn detect_scope_tabs() {
        let lines = ["fn f() {", "\tbody();", "\tmore();", "}"];
        let refs: Vec<&str> = lines.iter().copied().collect();
        let scope = detect_scope(&refs, 1, IndentStyle::Tabs);
        assert_eq!(
            scope,
            Some(Scope {
                start: 1,
                end: 2,
                level: 1,
            })
        );
    }
}
