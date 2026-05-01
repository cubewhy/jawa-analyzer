pub mod config;
pub mod flags;
mod global_state;
mod lsp;

pub use lsp::backend::Backend;
pub use lsp::worker::Worker;

pub use global_state::GlobalState;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = env!("CARGO_PKG_NAME");
