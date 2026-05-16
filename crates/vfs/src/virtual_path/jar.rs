use crate::VfsPath;
use crate::virtual_path::VirtualPathHandler;
use moka::sync::Cache;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use url::Url;
use zip::ZipArchive;

use parking_lot::Mutex;

#[cfg(target_os = "windows")]
fn open_shared_jar(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x00000001;
    const FILE_SHARE_WRITE: u32 = 0x00000002;
    const FILE_SHARE_DELETE: u32 = 0x00000004;

    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(path)
}

#[cfg(not(target_os = "windows"))]
fn open_shared_jar(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

pub struct JarManager {
    cache: Cache<PathBuf, Arc<Mutex<ZipArchive<File>>>>,
}

impl Default for JarManager {
    fn default() -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(64)
                .time_to_idle(Duration::from_secs(2))
                .build(),
        }
    }
}

impl JarManager {
    pub fn get_archive(&self, path: &Path) -> std::io::Result<Arc<Mutex<ZipArchive<File>>>> {
        let path_buf = path.to_path_buf();

        self.cache
            .try_get_with(path_buf, || -> std::io::Result<_> {
                let file = open_shared_jar(path)?;
                let archive = ZipArchive::new(file)
                    .map_err(|e| std::io::Error::other(format!("Invalid ZIP/JAR: {:?}", e)))?;

                Ok(Arc::new(Mutex::new(archive)))
            })
            .map_err(|e| std::io::Error::other(format!("Jar cache fetch error: {}", e)))
    }
}

pub struct JarHandler {
    manager: JarManager,
}

impl JarHandler {
    pub fn new() -> Self {
        Self {
            manager: JarManager::default(),
        }
    }
}

impl Default for JarHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualPathHandler for JarHandler {
    fn can_handle(&self, protocol: &str) -> bool {
        protocol == "jar"
    }

    fn fetch_bytes(&self, url: &Url) -> std::io::Result<Vec<u8>> {
        let path = url.path();
        let (jar_path_str, entry_path) = path.split_once('!').ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid JAR URI missing '!': {}", path),
            )
        })?;

        let jar_path = Path::new(jar_path_str);

        let archive_arc = self.manager.get_archive(jar_path)?;

        let mut archive = archive_arc.lock();

        let clean_entry_path = entry_path.strip_prefix('/').unwrap_or(entry_path);

        let mut file = archive.by_name(clean_entry_path).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "Entry {} not found in jar {:?}: {:?}",
                    clean_entry_path, jar_path, e
                ),
            )
        })?;

        let mut bytes = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut bytes)?;

        Ok(bytes)
    }

    fn list_files(&self, root_url: &Url) -> std::io::Result<Vec<VfsPath>> {
        let path = root_url.path();
        let (jar_path_str, entry_path) = path.split_once('!').ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid JAR URI missing '!': {}", path),
            )
        })?;

        let jar_path = Path::new(jar_path_str);
        let archive_arc = self.manager.get_archive(jar_path)?;
        let archive = archive_arc.lock();

        let clean_entry_path = entry_path.strip_prefix('/').unwrap_or(entry_path);

        let exact_dir_prefix = if clean_entry_path.is_empty() {
            String::new()
        } else if clean_entry_path.ends_with('/') {
            clean_entry_path.to_string()
        } else {
            format!("{}/", clean_entry_path)
        };

        let paths: std::io::Result<Vec<VfsPath>> = archive
            .file_names()
            .filter_map(|name| {
                let is_match = if clean_entry_path.is_empty() {
                    true
                } else {
                    name == clean_entry_path || name.starts_with(&exact_dir_prefix)
                };

                if is_match {
                    let url_str = format!("jar://{}!/{}", jar_path_str, name);

                    match Url::parse(&url_str) {
                        Ok(url) => Some(Ok(VfsPath::from(&url))),
                        Err(e) => Some(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Failed to parse generated URL '{}': {}", url_str, e),
                        ))),
                    }
                } else {
                    None
                }
            })
            .collect();

        paths
    }
}
