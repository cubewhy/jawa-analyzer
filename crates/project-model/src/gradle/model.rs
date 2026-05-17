use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct GradleWorkspace {
    pub workspace_name: String,
    pub projects: Vec<GradleProject>,
}

#[derive(Debug, Deserialize)]
pub struct GradleProject {
    pub path: String, // e.g., ":", ":core", ":app"
    pub name: String, // e.g., "core"
    pub project_dir: PathBuf,
    pub source_roots: Vec<PathBuf>,
    pub test_roots: Vec<PathBuf>,
    pub resource_roots: Vec<PathBuf>,
    pub generated_roots: Vec<PathBuf>,
    pub compile_classpath: Vec<PathBuf>,
    pub test_classpath: Vec<PathBuf>,
    pub module_dependencies: Vec<String>, // List of project paths e.g., [":core"]
    pub java_language_version: Option<String>,
}
