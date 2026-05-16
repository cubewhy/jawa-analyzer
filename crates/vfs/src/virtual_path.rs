mod jar;
mod jimage;

pub use jar::JarHandler;
pub use jimage::JimageHandler;

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
