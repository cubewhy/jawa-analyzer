pub mod config;
pub mod flags;
mod global_state;
mod lsp;

pub use lsp::backend::Backend;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = env!("CARGO_PKG_NAME");
