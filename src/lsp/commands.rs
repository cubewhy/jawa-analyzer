pub const COMMAND_SHOW_MEMORY_STATUS: &str = "java-analyzer.server.showMemoryStatus";
pub const COMMAND_CLEAR_CACHES: &str = "java-analyzer.server.clearCaches";

pub fn server_commands() -> Vec<String> {
    vec![
        COMMAND_SHOW_MEMORY_STATUS.to_string(),
        COMMAND_CLEAR_CACHES.to_string(),
    ]
}
