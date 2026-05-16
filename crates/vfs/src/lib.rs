use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        mpsc::Sender,
    },
    time::Duration,
};

use notify_debouncer_mini::{
    DebouncedEvent, new_debouncer,
    notify::{self, RecursiveMode},
};
use rustc_hash::FxHashMap;

use crate::virtual_path::VirtualPathHandler;

pub mod virtual_path;

#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct FileId(pub u32);

#[derive(Default)]
pub struct Vfs {
    next_id: AtomicU32,

    uri_to_id: FxHashMap<String, FileId>,
    id_to_uri: FxHashMap<FileId, String>,

    overlays: FxHashMap<FileId, Arc<str>>,

    handlers: Vec<Box<dyn VirtualPathHandler>>,
    pending_events: Vec<VfsEvent>,
}

impl Vfs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_handler(&mut self, handler: impl VirtualPathHandler + 'static) {
        self.handlers.push(Box::new(handler));
    }

    pub fn alloc_file_id(&mut self, uri: &str) -> FileId {
        if let Some(&id) = self.uri_to_id.get(uri) {
            return id;
        }

        let id = FileId(self.next_id.fetch_add(1, Ordering::SeqCst));

        self.uri_to_id.insert(uri.to_string(), id);
        self.id_to_uri.insert(id, uri.to_string());

        id
    }

    pub fn file_path(&self, id: FileId) -> Option<&str> {
        self.id_to_uri.get(&id).map(|x| x.as_ref())
    }

    pub fn set_overlay(&mut self, id: FileId, text: String) {
        self.overlays.insert(id, Arc::from(text));
        self.pending_events.push(VfsEvent::Modify { id });
    }

    pub fn clear_overlay(&mut self, id: FileId) {
        if self.overlays.remove(&id).is_some() {
            self.pending_events.push(VfsEvent::Modify { id });
        }
    }

    pub fn take_changes(&mut self) -> Vec<VfsEvent> {
        std::mem::take(&mut self.pending_events)
    }

    pub fn fetch_content(&self, id: FileId) -> std::io::Result<Vec<u8>> {
        if let Some(text) = self.overlays.get(&id) {
            return Ok(text.as_bytes().to_vec());
        }

        let uri = self
            .id_to_uri
            .get(&id)
            .ok_or_else(|| std::io::Error::other("FileId not found"))?;

        let scheme = uri.split("://").next().unwrap_or("file");

        for handler in &self.handlers {
            if handler.can_handle(scheme) {
                return handler.fetch_bytes(uri);
            }
        }

        Err(std::io::Error::other(format!(
            "No handler registered for scheme: {}",
            scheme
        )))
    }

    pub fn handle_watcher_message(&mut self, msg: WatcherMessage) {
        match msg {
            WatcherMessage::FileSystemChanged(events) => {
                for event in events {
                    let path = event.path;

                    let uri = format!("file://{}", path.to_string_lossy().replace('\\', "/"));

                    if !path.exists() {
                        if let Some(&id) = self.uri_to_id.get(&uri) {
                            self.uri_to_id.remove(&uri);
                            self.id_to_uri.remove(&id);
                            self.overlays.remove(&id);

                            self.pending_events.push(VfsEvent::Delete { id });
                        }
                    } else {
                        if let Some(&id) = self.uri_to_id.get(&uri) {
                            if !self.overlays.contains_key(&id) {
                                self.pending_events.push(VfsEvent::Modify { id });
                            }
                        } else {
                            let id = self.alloc_file_id(&uri);
                            self.pending_events.push(VfsEvent::Create { id, uri });
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VfsEvent {
    Create { id: FileId, uri: String },
    Modify { id: FileId },
    Delete { id: FileId },
}

pub enum WatcherMessage {
    FileSystemChanged(Vec<DebouncedEvent>),
}

pub struct VfsWatcher {
    _debouncer: notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>,
}

impl VfsWatcher {
    pub fn new(watch_root: PathBuf, sender: Sender<WatcherMessage>) -> std::io::Result<Self> {
        let mut debouncer = new_debouncer(Duration::from_millis(100), move |res| match res {
            Ok(events) => {
                let _ = sender.send(WatcherMessage::FileSystemChanged(events));
            }
            Err(e) => eprintln!("VFS Notify error: {:?}", e),
        })
        .map_err(|e| std::io::Error::other(format!("Failed to create watcher: {:?}", e)))?;

        debouncer
            .watcher()
            .watch(&watch_root, RecursiveMode::Recursive)
            .map_err(|e| std::io::Error::other(format!("Failed to watch path: {:?}", e)))?;

        Ok(Self {
            _debouncer: debouncer,
        })
    }
}
