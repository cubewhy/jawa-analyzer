use jimage_rs::JImage;
use moka::sync::Cache;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

use crate::VfsPath;
use crate::virtual_path::VirtualPathHandler;

pub struct JimageHandler {
    cache: Cache<String, Arc<JImage>>,
}

impl Default for JimageHandler {
    fn default() -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(10)
                .time_to_idle(Duration::from_secs(3))
                .build(),
        }
    }
}

impl JimageHandler {
    fn get_jimage(&self, path: &str) -> std::io::Result<Arc<JImage>> {
        self.cache
            .try_get_with(path.to_string(), || {
                JImage::open(path)
                    .map(Arc::new)
                    .map_err(|e| std::io::Error::other(format!("JImage open error: {:?}", e)))
            })
            .map_err(|e| std::io::Error::other(format!("Cache fetch error: {}", e)))
    }
}

impl VirtualPathHandler for JimageHandler {
    fn can_handle(&self, protocol: &str) -> bool {
        protocol == "jrt"
    }

    fn fetch_bytes(&self, url: &Url) -> std::io::Result<Vec<u8>> {
        let path = url.path();
        let (img_path, resource_path) = path.split_once('!').ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid JRT URI missing '!': {}", path),
            )
        })?;

        let jimage = self.get_jimage(img_path)?;

        let resource_path = if !resource_path.starts_with('/') {
            format!("/{}", resource_path)
        } else {
            resource_path.to_string()
        };

        match jimage.find_resource(&resource_path) {
            Ok(Some(data)) => Ok(data.into_owned()),
            Ok(None) => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "Resource {} not found in jimage {}",
                    resource_path, img_path
                ),
            )),
            Err(e) => Err(std::io::Error::other(format!(
                "JImage find_resource error: {:?}",
                e
            ))),
        }
    }

    fn list_files(&self, root_url: &Url) -> std::io::Result<Vec<VfsPath>> {
        let path = root_url.path();
        let (img_path, resource_path) = path.split_once('!').ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid JRT URI missing '!': {}", path),
            )
        })?;

        let jimage = self.get_jimage(img_path)?;

        let resource_path = if !resource_path.starts_with('/') {
            format!("/{}", resource_path)
        } else {
            resource_path.to_string()
        };

        let exact_dir_prefix = if resource_path == "/" {
            "/".to_string()
        } else if resource_path.ends_with('/') {
            resource_path.clone()
        } else {
            format!("{}/", resource_path)
        };

        let paths: std::io::Result<Vec<VfsPath>> = jimage
            .resource_names_iter()
            .map(|item| item.map_err(|e| std::io::Error::other(format!("JImage error: {:?}", e))))
            .filter_map(|item| match item {
                Ok(resource) => {
                    let full_name = resource.get_full_name();
                    let res_path = full_name.1;

                    if res_path == resource_path || res_path.starts_with(&exact_dir_prefix) {
                        let url_str = format!("jrt://{}!{}", img_path, res_path);

                        match Url::parse(&url_str) {
                            Ok(url) => Some(Ok(VfsPath::from(&url))),
                            Err(e) => Some(Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("Failed to parse URL '{}': {}", url_str, e),
                            ))),
                        }
                    } else {
                        None
                    }
                }
                Err(e) => Some(Err(e)),
            })
            .collect();

        paths
    }
}
