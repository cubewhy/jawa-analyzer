use rustc_hash::FxHashMap;
use std::path::Path;

/// Lombok configuration loaded from lombok.config files
#[derive(Debug, Clone, Default)]
pub struct LombokConfig {
    /// Configuration key-value pairs
    settings: FxHashMap<String, String>,
}

impl LombokConfig {
    /// Create a new empty configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Load configuration from a directory hierarchy
    /// Walks up from the given path to find lombok.config files
    pub fn load_from_directory(start_path: &Path) -> Self {
        let mut config = Self::new();
        let mut current = start_path.to_path_buf();

        // Walk up the directory tree
        loop {
            let config_file = current.join("lombok.config");
            if config_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&config_file) {
                    config.merge_from_content(&content);
                }
            }

            // Check for stop file
            if config.has_stop_directive() {
                break;
            }

            // Move to parent directory
            if !current.pop() {
                break;
            }
        }

        config
    }

    /// Merge configuration from file content
    pub fn merge_from_content(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }

            // Parse key = value or key : value
            if let Some((key, value)) = Self::parse_config_line(line) {
                self.settings.insert(key.to_string(), value.to_string());
            }
        }
    }

    /// Parse a single configuration line
    fn parse_config_line(line: &str) -> Option<(&str, &str)> {
        // Support both = and : as separators
        if let Some(pos) = line.find('=') {
            let key = line[..pos].trim();
            let value = line[pos + 1..].trim();
            return Some((key, value));
        }
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim();
            let value = line[pos + 1..].trim();
            return Some((key, value));
        }
        None
    }

    /// Check if configuration has a stop directive
    fn has_stop_directive(&self) -> bool {
        self.settings
            .get("config.stopBubbling")
            .map(|v| v == "true")
            .unwrap_or(false)
    }

    /// Get a configuration value
    pub fn get(&self, key: &str) -> Option<&str> {
        self.settings.get(key).map(|s| s.as_str())
    }

    /// Get a boolean configuration value
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| match v {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
    }

    /// Get accessors.chain setting (default: false)
    pub fn accessors_chain(&self) -> bool {
        self.get_bool(super::types::config_keys::ACCESSORS_CHAIN)
            .unwrap_or(false)
    }

    /// Get accessors.fluent setting (default: false)
    pub fn accessors_fluent(&self) -> bool {
        self.get_bool(super::types::config_keys::ACCESSORS_FLUENT)
            .unwrap_or(false)
    }

    /// Get accessors.prefix setting (default: empty)
    pub fn accessors_prefix(&self) -> Vec<String> {
        self.get(super::types::config_keys::ACCESSORS_PREFIX)
            .map(|s| {
                s.split(';')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get log field name (default: "log")
    pub fn log_field_name(&self) -> &str {
        self.get(super::types::config_keys::LOG_FIELD_NAME)
            .unwrap_or("log")
    }

    /// Get log field is static (default: true)
    pub fn log_field_is_static(&self) -> bool {
        self.get_bool(super::types::config_keys::LOG_FIELD_IS_STATIC)
            .unwrap_or(true)
    }

    /// Get copyable annotations patterns
    pub fn copyable_annotations(&self) -> Vec<String> {
        self.get(super::types::config_keys::COPYABLE_ANNOTATIONS)
            .map(|s| {
                s.split(';')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get toString.includeFieldNames (default: true)
    pub fn to_string_include_field_names(&self) -> bool {
        self.get_bool(super::types::config_keys::TO_STRING_INCLUDE_FIELD_NAMES)
            .unwrap_or(true)
    }

    /// Get toString.doNotUseGetters (default: false)
    pub fn to_string_do_not_use_getters(&self) -> bool {
        self.get_bool(super::types::config_keys::TO_STRING_DO_NOT_USE_GETTERS)
            .unwrap_or(false)
    }

    /// Get equalsAndHashCode.doNotUseGetters (default: false)
    pub fn equals_and_hash_code_do_not_use_getters(&self) -> bool {
        self.get_bool(super::types::config_keys::EQUALS_AND_HASH_CODE_DO_NOT_USE_GETTERS)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_line() {
        assert_eq!(
            LombokConfig::parse_config_line("lombok.accessors.chain = true"),
            Some(("lombok.accessors.chain", "true"))
        );
        assert_eq!(
            LombokConfig::parse_config_line("lombok.log.fieldName: logger"),
            Some(("lombok.log.fieldName", "logger"))
        );
        assert_eq!(LombokConfig::parse_config_line("# comment"), None);
        assert_eq!(LombokConfig::parse_config_line(""), None);
    }

    #[test]
    fn test_merge_from_content() {
        let mut config = LombokConfig::new();
        config.merge_from_content(
            r#"
            # This is a comment
            lombok.accessors.chain = true
            lombok.log.fieldName : logger
            
            // Another comment
            lombok.accessors.fluent = false
            "#,
        );

        assert_eq!(config.get("lombok.accessors.chain"), Some("true"));
        assert_eq!(config.get("lombok.log.fieldName"), Some("logger"));
        assert_eq!(config.get("lombok.accessors.fluent"), Some("false"));
    }

    #[test]
    fn test_accessors_prefix() {
        let mut config = LombokConfig::new();
        config.merge_from_content("lombok.accessors.prefix = m_;f_;_");

        let prefixes = config.accessors_prefix();
        assert_eq!(prefixes, vec!["m_", "f_", "_"]);
    }

    #[test]
    fn test_defaults() {
        let config = LombokConfig::new();
        assert!(!config.accessors_chain());
        assert!(!config.accessors_fluent());
        assert_eq!(config.log_field_name(), "log");
        assert!(config.log_field_is_static());
        assert!(config.to_string_include_field_names());
        assert!(!config.to_string_do_not_use_getters());
    }
}
