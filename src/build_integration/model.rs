use std::path::PathBuf;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::index::{ClasspathId, ModuleId};

use super::detection::DetectedBuildToolKind;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelFreshness {
    Fresh,
    Stale,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelFidelity {
    Full,
    Partial,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceModelProvenance {
    pub tool: DetectedBuildToolKind,
    pub tool_version: Option<String>,
    pub imported_at: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaToolchainInfo {
    pub language_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRoot {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceRootId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceRootKind {
    Sources,
    Tests,
    Resources,
    Generated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSourceRoot {
    pub id: SourceRootId,
    pub path: PathBuf,
    pub kind: WorkspaceRootKind,
    pub classpath: ClasspathId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceModule {
    pub id: ModuleId,
    pub name: String,
    pub directory: PathBuf,
    pub roots: Vec<WorkspaceSourceRoot>,
    pub compile_classpath: Vec<PathBuf>,
    pub test_classpath: Vec<PathBuf>,
    pub dependency_modules: Vec<ModuleId>,
    pub java: JavaToolchainInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceModelSnapshot {
    pub generation: u64,
    pub root: WorkspaceRoot,
    pub name: String,
    pub modules: Vec<WorkspaceModule>,
    pub provenance: WorkspaceModelProvenance,
    pub freshness: ModelFreshness,
    pub fidelity: ModelFidelity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaPackageInference {
    pub source_root_id: SourceRootId,
    pub source_root_path: PathBuf,
    pub relative_dir: PathBuf,
    pub package: String,
}

impl WorkspaceModelSnapshot {
    pub fn module_for_path(&self, path: &std::path::Path) -> Option<&WorkspaceModule> {
        self.modules.iter().find(|module| {
            module.directory == path
                || path.starts_with(&module.directory)
                || module.roots.iter().any(|root| path.starts_with(&root.path))
        })
    }

    pub fn source_root_for_path(
        &self,
        path: &std::path::Path,
        preferred_root: Option<SourceRootId>,
    ) -> Option<&WorkspaceSourceRoot> {
        if let Some(preferred_root) = preferred_root
            && let Some(root) = self
                .modules
                .iter()
                .flat_map(|module| module.roots.iter())
                .find(|root| root.id == preferred_root && path.starts_with(&root.path))
        {
            return Some(root);
        }

        self.modules
            .iter()
            .flat_map(|module| module.roots.iter())
            .filter(|root| path.starts_with(&root.path))
            .max_by_key(|root| root.path.components().count())
    }

    pub fn infer_java_package_for_file(
        &self,
        path: &std::path::Path,
        preferred_root: Option<SourceRootId>,
    ) -> Option<JavaPackageInference> {
        let source_root = self.source_root_for_path(path, preferred_root)?;
        let parent_dir = path.parent()?;
        let relative_dir = parent_dir
            .strip_prefix(&source_root.path)
            .ok()?
            .to_path_buf();
        let package = package_name_from_relative_dir(&relative_dir)?;
        Some(JavaPackageInference {
            source_root_id: source_root.id,
            source_root_path: source_root.path.clone(),
            relative_dir,
            package,
        })
    }

    pub fn scope_for_path(
        &self,
        path: &std::path::Path,
    ) -> Option<(ModuleId, ClasspathId, Option<SourceRootId>)> {
        self.modules.iter().find_map(|module| {
            if let Some(root) = module
                .roots
                .iter()
                .filter(|root| path.starts_with(&root.path))
                .max_by_key(|root| root.path.components().count())
            {
                return Some((module.id, root.classpath, Some(root.id)));
            }

            if module.directory == path || path.starts_with(&module.directory) {
                return Some((module.id, ClasspathId::Main, None));
            }

            None
        })
    }
}

fn package_name_from_relative_dir(relative_dir: &std::path::Path) -> Option<String> {
    let components = relative_dir
        .components()
        .map(|component| component.as_os_str().to_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;

    Some(components.join("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_test_roots_to_test_classpath() {
        let snapshot = WorkspaceModelSnapshot {
            generation: 1,
            root: WorkspaceRoot {
                path: PathBuf::from("/workspace"),
            },
            name: "demo".into(),
            modules: vec![WorkspaceModule {
                id: ModuleId(1),
                name: "app".into(),
                directory: PathBuf::from("/workspace/app"),
                roots: vec![
                    WorkspaceSourceRoot {
                        id: SourceRootId(1),
                        path: PathBuf::from("/workspace/app/src/main/java"),
                        kind: WorkspaceRootKind::Sources,
                        classpath: ClasspathId::Main,
                    },
                    WorkspaceSourceRoot {
                        id: SourceRootId(2),
                        path: PathBuf::from("/workspace/app/src/test/java"),
                        kind: WorkspaceRootKind::Tests,
                        classpath: ClasspathId::Test,
                    },
                ],
                compile_classpath: vec![],
                test_classpath: vec![],
                dependency_modules: vec![],
                java: JavaToolchainInfo {
                    language_version: None,
                },
            }],
            provenance: WorkspaceModelProvenance {
                tool: DetectedBuildToolKind::Gradle,
                tool_version: Some("8.10".into()),
                imported_at: SystemTime::UNIX_EPOCH,
            },
            freshness: ModelFreshness::Fresh,
            fidelity: ModelFidelity::Full,
        };

        assert_eq!(
            snapshot.scope_for_path(std::path::Path::new("/workspace/app/src/test/java/A.java")),
            Some((ModuleId(1), ClasspathId::Test, Some(SourceRootId(2))))
        );
    }

    #[test]
    fn infers_java_package_from_preferred_source_root() {
        let snapshot = WorkspaceModelSnapshot {
            generation: 1,
            root: WorkspaceRoot {
                path: PathBuf::from("/workspace"),
            },
            name: "demo".into(),
            modules: vec![WorkspaceModule {
                id: ModuleId(1),
                name: "app".into(),
                directory: PathBuf::from("/workspace/app"),
                roots: vec![
                    WorkspaceSourceRoot {
                        id: SourceRootId(1),
                        path: PathBuf::from("/workspace/app/src/main/java"),
                        kind: WorkspaceRootKind::Sources,
                        classpath: ClasspathId::Main,
                    },
                    WorkspaceSourceRoot {
                        id: SourceRootId(2),
                        path: PathBuf::from("/workspace/app/src/test/java"),
                        kind: WorkspaceRootKind::Tests,
                        classpath: ClasspathId::Test,
                    },
                ],
                compile_classpath: vec![],
                test_classpath: vec![],
                dependency_modules: vec![],
                java: JavaToolchainInfo {
                    language_version: None,
                },
            }],
            provenance: WorkspaceModelProvenance {
                tool: DetectedBuildToolKind::Gradle,
                tool_version: Some("8.10".into()),
                imported_at: SystemTime::UNIX_EPOCH,
            },
            freshness: ModelFreshness::Fresh,
            fidelity: ModelFidelity::Full,
        };

        let inference = snapshot
            .infer_java_package_for_file(
                std::path::Path::new("/workspace/app/src/main/java/org/example/foo/Bar.java"),
                Some(SourceRootId(1)),
            )
            .expect("package inference");

        assert_eq!(inference.source_root_id, SourceRootId(1));
        assert_eq!(inference.relative_dir, PathBuf::from("org/example/foo"));
        assert_eq!(inference.package, "org.example.foo");
    }

    #[test]
    fn infers_empty_package_for_file_directly_under_source_root() {
        let snapshot = WorkspaceModelSnapshot {
            generation: 1,
            root: WorkspaceRoot {
                path: PathBuf::from("/workspace"),
            },
            name: "demo".into(),
            modules: vec![WorkspaceModule {
                id: ModuleId(1),
                name: "app".into(),
                directory: PathBuf::from("/workspace/app"),
                roots: vec![WorkspaceSourceRoot {
                    id: SourceRootId(1),
                    path: PathBuf::from("/workspace/app/src/main/java"),
                    kind: WorkspaceRootKind::Sources,
                    classpath: ClasspathId::Main,
                }],
                compile_classpath: vec![],
                test_classpath: vec![],
                dependency_modules: vec![],
                java: JavaToolchainInfo {
                    language_version: None,
                },
            }],
            provenance: WorkspaceModelProvenance {
                tool: DetectedBuildToolKind::Gradle,
                tool_version: Some("8.10".into()),
                imported_at: SystemTime::UNIX_EPOCH,
            },
            freshness: ModelFreshness::Fresh,
            fidelity: ModelFidelity::Full,
        };

        let inference = snapshot
            .infer_java_package_for_file(
                std::path::Path::new("/workspace/app/src/main/java/App.java"),
                Some(SourceRootId(1)),
            )
            .expect("package inference");

        assert!(inference.relative_dir.as_os_str().is_empty());
        assert!(inference.package.is_empty());
    }
}
