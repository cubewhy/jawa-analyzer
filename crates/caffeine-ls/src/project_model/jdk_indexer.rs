use std::{io::Read, path::PathBuf};

use base_db::workspace::{Library, LibraryOrigin};
use hir::{ClassOrModuleData, HirDatabase, bytecode::parse_cafebabe};
use jimage_rs::JImage;
use rayon::iter::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::global_state::{BackgroundTaskEvent, ProgressEvent, ProgressState};

pub struct JdkIndexer {
    jdk_home: PathBuf,
    task_sender: crossbeam_channel::Sender<BackgroundTaskEvent>,
}

enum JdkLayout {
    Modular(PathBuf),
    Legacy(PathBuf),
}

impl JdkIndexer {
    pub fn new(
        jdk_home: PathBuf,
        task_sender: crossbeam_channel::Sender<BackgroundTaskEvent>,
    ) -> Self {
        Self {
            jdk_home,
            task_sender,
        }
    }

    pub fn run_index(
        &self,
        db: &mut dyn HirDatabase,
    ) -> anyhow::Result<(Library, FxHashMap<SmolStr, ClassOrModuleData>)> {
        let modules_path = self.jdk_home.join("lib").join("modules");

        let (archive_path, parsed_classes) = if modules_path.exists() {
            let classes = self.index_jimage(&modules_path, db)?;
            (modules_path, classes)
        } else {
            let rt_jar = self.jdk_home.join("lib").join("rt.jar");
            let classes = self.index_rt_jar(db, &rt_jar)?;
            (rt_jar, classes)
        };

        // TODO: probe jdk version
        let jdk_lib_id = Library::new(db, LibraryOrigin::Jdk { version: 17 }, archive_path);

        Ok((jdk_lib_id, parsed_classes))
    }

    fn index_rt_jar(
        &self,
        db: &mut dyn HirDatabase,
        path: &PathBuf,
    ) -> anyhow::Result<FxHashMap<SmolStr, ClassOrModuleData>> {
        let file = std::fs::File::open(path)?;

        let mut results = FxHashMap::default();
        let mut archive = zip::ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            if file.name().ends_with(".class") {
                let mut buf = Vec::new();
                file.read_to_end(&mut buf)?;
                let Ok(class_or_module) = parse_cafebabe(&buf) else {
                    tracing::error!("Failed to parse {}", file.name());
                    continue;
                };
                results.insert(class_or_module.fqn(), class_or_module);
            }
        }

        Ok(results)
    }

    fn index_jimage(
        &self,
        path: &PathBuf,
        db: &mut dyn HirDatabase,
    ) -> anyhow::Result<FxHashMap<SmolStr, ClassOrModuleData>> {
        let jimage = JImage::open(&path)
            .map_err(|e| anyhow::anyhow!("Failed to open jimage at {:?}: {:?}", path, e))?;

        let names = jimage
            .resource_names()
            .map_err(|e| anyhow::anyhow!("Failed to list resource names: {:?}", e))?;

        let total = names.len() as u32;
        self.send_progress("JDK Indexing", "Scanning modules...", 0, total);

        let results: FxHashMap<_, _> = names
            .into_par_iter()
            .enumerate()
            .filter_map(|(idx, res_name)| {
                if idx % 200 == 0 {
                    self.send_progress("JDK Indexing", "Processing classes...", idx as u32, total);
                }

                let (module, path) = res_name.get_full_name();

                if !path.ends_with(".class") {
                    return None;
                }

                let lookup_key = format!("/{}/{}", module, path);

                match jimage.find_resource(&lookup_key) {
                    Ok(Some(bytes)) => {
                        let parsed_data = parse_cafebabe(&bytes)
                            .inspect_err(|err| {
                                tracing::error!("Failed to parse class {}: {:?}", lookup_key, err);
                            })
                            .ok()?;

                        let fqn = parsed_data.fqn();

                        Some((fqn, parsed_data))
                    }
                    _ => None,
                }
            })
            .collect();

        self.send_progress_end("JDK Indexing", "Completed");

        Ok(results)
    }

    fn send_progress(&self, title: &str, message: &str, current: u32, total: u32) {
        let percentage = (current as f32 / total as f32 * 100.0) as u32;
        let _ = self
            .task_sender
            .send(BackgroundTaskEvent::Progress(ProgressEvent {
                token: "jdk-indexing".to_string(),
                title: title.to_string(),
                message: Some(format!("{} ({}%)", message, percentage)),
                percentage: Some(percentage),
                state: ProgressState::Report,
            }));
    }

    fn send_progress_end(&self, title: &str, message: &str) {
        let _ = self
            .task_sender
            .send(BackgroundTaskEvent::Progress(ProgressEvent {
                token: "jdk-indexing".to_string(),
                title: title.to_string(),
                message: Some(message.to_string()),
                percentage: Some(100),
                state: ProgressState::End,
            }));
    }
}
