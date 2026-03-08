//! Suji (筋) — visual indent guides and scope highlighting for Neovim
//!
//! Part of the blnvim-ng distribution — a Rust-native Neovim plugin suite.
//! Built with [`nvim-oxi`](https://github.com/noib3/nvim-oxi) for zero-cost
//! Neovim API bindings.

use nvim_oxi as oxi;

#[oxi::plugin]
fn suji() -> oxi::Result<()> {
    Ok(())
}
