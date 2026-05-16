use jimage_rs::JImage;
use moka::sync::Cache;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

use crate::virtual_path::VirtualPathHandler;

pub struct JimageManager {
    cache: Cache<String, Arc<JImage>>,
}

impl Default for JimageManager {
    fn default() -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(10)
                .time_to_idle(Duration::from_secs(3))
                .build(),
        }
    }
}

impl JimageManager {
    pub fn get_jimage(&self, path: &str) -> std::io::Result<Arc<JImage>> {
        self.cache
            .try_get_with(path.to_string(), || {
                JImage::open(path)
                    .map(Arc::new)
                    .map_err(|e| std::io::Error::other(format!("JImage open error: {:?}", e)))
            })
            .map_err(|e| std::io::Error::other(format!("Cache fetch error: {}", e)))
    }
}

#[derive(Default)]
pub struct JimageHandler {
    manager: JimageManager,
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

        let jimage = self.manager.get_jimage(img_path)?;

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
}
