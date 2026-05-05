use std::path::PathBuf;

use indexmap::IndexMap;
use smol_str::SmolStr;

#[salsa::input]
pub struct Library {
    pub kind: LibraryOrigin,

    #[returns(ref)]
    pub archive_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LibraryOrigin {
    Jdk { version: u8 },
    Jar { maven_coordinate: Option<SmolStr> },
    Unknown,
}

#[salsa::input]
pub struct Module {
    /// The name of the module (e.g., "core", "web-api")
    #[returns(ref)]
    pub name: SmolStr,

    /// The Maven/Gradle coordinate if this module is published
    /// e.g., "com.mycompany:core:1.0.0"
    #[returns(ref)]
    pub coordinate: Option<SmolStr>,

    /// The root directory of this module in your Virtual File System.
    /// This is crucial so you know where "src/main/java" starts.
    #[returns(ref)]
    pub root_path: vfs::VfsPath,
}

#[salsa::input]
pub struct WorkspaceGraph {
    /// Maps a local workspace Module to the external Libraries (JARs) it can see.
    #[returns(ref)]
    pub external_dependencies: IndexMap<Module, Vec<Library>>,

    /// Maps a local workspace Module to other workspace Modules it depends on.
    #[returns(ref)]
    pub internal_dependencies: IndexMap<Module, Vec<Module>>,

    /// Maps a physical source file back to the Module it belongs to.
    #[returns(ref)]
    pub file_to_module: IndexMap<vfs::FileId, Module>,
}
