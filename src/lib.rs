//! Suji (筋) — visual indent guides and scope highlighting for Neovim
//!
//! Part of the blnvim-ng distribution — a Rust-native Neovim plugin suite.
//! Built with [`nvim-oxi`](https://github.com/noib3/nvim-oxi) for zero-cost
//! Neovim API bindings.

pub mod guides;
pub mod indent;
pub mod scope;

use nvim_oxi as oxi;
use nvim_oxi::api;
use tane::prelude::*;

/// Convert a `tane::Error` into an `oxi::Error` for the plugin entry point.
fn tane_err(e: tane::Error) -> oxi::Error {
    oxi::Error::from(oxi::api::Error::Other(e.to_string()))
}

#[oxi::plugin]
fn suji() -> oxi::Result<()> {
    // Create a namespace for all suji extmarks.
    let ns = Namespace::create("suji").map_err(tane_err)?;
    let ns_id = ns.id();

    // Define highlight groups (soft defaults — users can override).
    Highlight::new(guides::HL_INDENT)
        .fg("#3b3b3b")
        .apply()
        .map_err(tane_err)?;

    Highlight::new(guides::HL_SCOPE)
        .fg("#7aa2f7")
        .apply()
        .map_err(tane_err)?;

    // Register autocommands that trigger a full refresh.
    let events = &[
        "BufEnter",
        "BufWritePost",
        "TextChanged",
        "TextChangedI",
        "CursorMoved",
        "CursorMovedI",
    ];

    Autocmd::on(events)
        .group("suji")
        .desc("Refresh indent guides")
        .register(move |_args| {
            let win = api::get_current_win();
            let mut buf = win.get_buf().map_err(tane::Error::from)?;

            guides::refresh(&mut buf, &win, ns_id)
                .map_err(|e| tane::Error::Custom(e.to_string()))?;

            Ok(false)
        })
        .map_err(tane_err)?;

    Ok(())
}
