use std::sync::Arc;

pub struct PathResolver {
    handlers: Vec<Arc<dyn VirtualPathHandler>>,
}

impl PathResolver {
    pub fn new(handlers: Vec<Arc<dyn VirtualPathHandler>>) -> Self {
        Self { handlers }
    }

    pub fn resolve(&self, path: &vfs::VfsPath) -> std::io::Result<Vec<u8>> {
        // resolve from filesystem
        if let Some(abs_path) = path.as_path() {
            return std::fs::read(abs_path);
        }

        let path_str = path.to_string();

        if let Some((protocol, remainder)) = path_str.trim_start_matches("/").split_once("://") {
            for handler in &self.handlers {
                if handler.can_handle(protocol) {
                    return handler.fetch_bytes(remainder);
                }
            }
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("No handler found for path: {}", path),
        ))
    }
}

pub trait VirtualPathHandler: Send + Sync {
    /// Determine if this handler can parse a certain protocol.
    fn can_handle(&self, protocol: &str) -> bool;

    /// Get bytes.
    ///
    /// The input is the path after the protocol has been stripped.
    /// For example:
    ///   Raw uri: `protocol:///a.txt`
    ///   Path without protocol: `/a.txt`
    fn fetch_bytes(&self, path: &str) -> std::io::Result<Vec<u8>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    struct MemoryHandler {
        protocol: String,
        content: Vec<u8>,
    }

    impl VirtualPathHandler for MemoryHandler {
        fn can_handle(&self, protocol: &str) -> bool {
            self.protocol == protocol
        }

        fn fetch_bytes(&self, path: &str) -> std::io::Result<Vec<u8>> {
            if path == "/success" {
                Ok(self.content.clone())
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Not in memory",
                ))
            }
        }
    }

    #[test]
    fn test_resolve_physical_path() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "physical content").unwrap();

        let resolver = PathResolver::new(vec![]);
        let vfs_path = vfs::VfsPath::new_real_path(file_path.to_str().unwrap().to_string());

        let result = resolver.resolve(&vfs_path).unwrap();
        assert_eq!(result, b"physical content");
    }

    #[test]
    fn test_resolve_virtual_protocol() {
        let mock_data = b"virtual content".to_vec();
        let handler = Arc::new(MemoryHandler {
            protocol: "mem".to_string(),
            content: mock_data.clone(),
        });

        let resolver = PathResolver::new(vec![handler]);

        let vfs_path = vfs::VfsPath::new_virtual_path("/mem:///success".to_string());

        let result = resolver.resolve(&vfs_path).unwrap();
        assert_eq!(result, mock_data);
    }

    #[test]
    fn test_protocol_not_found() {
        let resolver = PathResolver::new(vec![]);
        let vfs_path = vfs::VfsPath::new_virtual_path("/unknown://foo".to_string());

        let result = resolver.resolve(&vfs_path);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn test_handler_internal_error() {
        let handler = Arc::new(MemoryHandler {
            protocol: "mem".to_string(),
            content: vec![],
        });
        let resolver = PathResolver::new(vec![handler]);

        let vfs_path = vfs::VfsPath::new_virtual_path("/mem:///not_exist".to_string());
        let result = resolver.resolve(&vfs_path);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().get_ref().unwrap().to_string(),
            "Not in memory"
        );
    }

    #[test]
    fn test_malformed_uri() {
        let resolver = PathResolver::new(vec![]);
        let vfs_path = vfs::VfsPath::new_virtual_path("/not_a_protocol_path".to_string());

        let result = resolver.resolve(&vfs_path);
        assert!(result.is_err());
    }
}
