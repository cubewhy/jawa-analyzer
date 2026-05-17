use crate::gradle::script::LEGACY_GRADLE_INIT_SCRIPT;
use crate::{Dependency, DependencyKind, DependencyScope, ProjectData, ProjectId, WorkspaceGraph};

use super::model::GradleWorkspace;
use super::script::GRADLE_INIT_SCRIPT;
use index::symbol::LibraryId;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use tempfile::NamedTempFile;
use triomphe::Arc;
use vfs::AbsPathBuf;

/// Parses a version string like "7.4.2" or "4.10.3" into (major, minor) integers.
fn parse_gradle_version(version_str: &str) -> (u32, u32) {
    let mut parts = version_str.split('.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor)
}

fn probe_version_from_wrapper(workspace_root: &Path) -> Option<(u32, u32)> {
    let props_path = workspace_root.join("gradle/wrapper/gradle-wrapper.properties");
    if !props_path.exists() {
        return None;
    }

    let content = fs::read_to_string(props_path).ok()?;
    for line in content.lines() {
        // Look for a line like: distributionUrl=https\://services.gradle.org/distributions/gradle-7.4-bin.zip
        if line.contains("distributionUrl")
            && let Some(idx) = line.find("gradle-")
        {
            let version_part = &line[idx + 7..];
            if let Some(end_idx) = version_part.find("-") {
                let version_str = &version_part[..end_idx];
                return Some(parse_gradle_version(version_str));
            }
        }
    }
    None
}

fn probe_version_from_cli(gradle_cmd: &str, workspace_root: &Path) -> Option<(u32, u32)> {
    let output = Command::new(gradle_cmd)
        .current_dir(workspace_root)
        .arg("--version")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // Output format usually contains a dedicated line: "Gradle 7.4"
        if let Some(version_str) = line.strip_prefix("Gradle ") {
            return Some(parse_gradle_version(version_str));
        }
    }
    None
}

pub fn import_gradle_workspace(
    workspace_root: &Path,
    java_exec: &Path,
) -> anyhow::Result<GradleWorkspace> {
    // TODO: IntelliJ doesn't use the wrapper script directly, use the gradle-wrapper.jar directly
    // as a fallback
    let gradlew_path = if cfg!(windows) {
        workspace_root.join("gradlew.bat")
    } else {
        workspace_root.join("gradlew")
    };

    let gradle_cmd = if gradlew_path.exists() {
        gradlew_path.to_string_lossy().into_owned()
    } else {
        // no wrapper found :(
        // Fallback to global gradle
        "gradle".to_string()
    };

    // probe gradle version
    let (major_version, minor_version) = probe_version_from_wrapper(workspace_root)
        .or_else(|| probe_version_from_cli(&gradle_cmd, workspace_root))
        // Default fallback to a modern runtime setup if detection fails completely
        .unwrap_or((7, 0));

    tracing::info!(
        "Detected Gradle version {}.{}",
        major_version,
        minor_version
    );

    let selected_script = if major_version < 5 {
        tracing::debug!("Using legacy Gradle configuration script");
        LEGACY_GRADLE_INIT_SCRIPT
    } else {
        tracing::debug!("Using modern Gradle configuration script");
        GRADLE_INIT_SCRIPT
    };

    let mut init_script = NamedTempFile::new()?;
    init_script.write_all(selected_script.as_bytes())?;
    init_script.flush()?;

    // e.g.: ./gradlew --init-script /tmp/init.gradle exportWorkspaceModel
    let output = Command::new(&gradle_cmd)
        .env("JAVA_HOME", java_exec)
        .current_dir(workspace_root)
        .arg("--init-script")
        .arg(init_script.path())
        .arg("exportWorkspaceModel")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Gradle execution failed:\n{}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let begin_marker = "WORKSPACE_MODEL_BEGIN";
    let end_marker = "WORKSPACE_MODEL_END";

    let json_start = stdout
        .find(begin_marker)
        .map(|idx| idx + begin_marker.len());
    let json_end = stdout.find(end_marker);

    match (json_start, json_end) {
        (Some(start), Some(end)) if start < end => {
            let json_str = stdout[start..end].trim();

            let workspace: GradleWorkspace = serde_json::from_str(json_str)?;
            Ok(workspace)
        }
        _ => {
            tracing::error!("Raw Gradle Output:\n{}", stdout);
            anyhow::bail!("Failed to locate structural JSON markers in Gradle output.");
        }
    }
}

pub fn build_graph_from_json(workspace: GradleWorkspace) -> WorkspaceGraph {
    let mut graph = WorkspaceGraph::default();

    // Maps Gradle paths (e.g., ":core") to internal ProjectIds
    let mut path_to_project_id = FxHashMap::default();
    // Maps external JAR file paths to unique LibraryIds
    let mut jar_to_library_id = FxHashMap::default();

    // Pre-allocate ProjectIds and LibraryIds for everything
    for (next_project_id, project) in workspace.projects.iter().enumerate() {
        let project_id = ProjectId(next_project_id.try_into().unwrap());
        path_to_project_id.insert(project.path.clone(), project_id);

        // Collect source/test root prefixes for the VFS pass later
        let all_roots = project
            .source_roots
            .iter()
            .chain(project.test_roots.iter())
            .chain(project.resource_roots.iter())
            .chain(project.generated_roots.iter());

        for root in all_roots {
            if let Ok(abs_path) = AbsPathBuf::try_from(root.clone()) {
                graph.root_to_project.insert(abs_path, project_id);
            }
        }

        // Allocate unique LibraryIds for external JAR dependencies
        let all_jars = project
            .compile_classpath
            .iter()
            .chain(project.test_classpath.iter());
        for jar in all_jars {
            if jar.extension().is_some_and(|ext| ext == "jar") {
                jar_to_library_id
                    .entry(jar.clone())
                    .or_insert_with(|| LibraryId::from_jar_path(jar));
            }
        }
    }

    // Build the explicit ProjectData and map its internal/external dependencies
    for project in workspace.projects {
        let project_id = *path_to_project_id.get(&project.path).unwrap();
        let mut dependencies = Vec::new();

        // Map Internal Module Dependencies
        for subproject_path in &project.module_dependencies {
            if let Some(&target_id) = path_to_project_id.get(subproject_path) {
                dependencies.push(Dependency {
                    kind: DependencyKind::Internal(target_id),
                    scope: DependencyScope::Compile, // Module dependencies are compile-scoped
                });
            }
        }

        // Map External Compile Classpath JARs
        for jar in &project.compile_classpath {
            if let Some(Ok(lib_id)) = jar_to_library_id.get(jar) {
                dependencies.push(Dependency {
                    kind: DependencyKind::External(*lib_id),
                    scope: DependencyScope::Compile,
                });
            }
        }

        // Map External Test Classpath JARs (Only if they aren't already in Compile)
        for jar in &project.test_classpath {
            if let Some(Ok(lib_id)) = jar_to_library_id.get(jar) {
                let already_in_compile = dependencies.iter().any(|d| {
                    matches!(d.kind, DependencyKind::External(id) if id == *lib_id)
                        && d.scope == DependencyScope::Compile
                });

                if !already_in_compile {
                    dependencies.push(Dependency {
                        kind: DependencyKind::External(*lib_id),
                        scope: DependencyScope::Test,
                    });
                }
            }
        }

        let abs_root_path =
            AbsPathBuf::try_from(project.project_dir).unwrap_or_else(AbsPathBuf::assert_utf8);

        // Create a unique LibraryId for this project's own compiled output symbol-set
        let project_library_id = LibraryId::from_project_root(abs_root_path.as_std_path());

        let project_data = ProjectData {
            id: project_id,
            name: SmolStr::from(project.name),
            root_path: abs_root_path,
            library_id: project_library_id,
            dependencies,
        };

        graph.projects.insert(project_id, Arc::new(project_data));
    }

    graph
}
