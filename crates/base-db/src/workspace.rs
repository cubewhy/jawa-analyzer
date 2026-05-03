#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyScope {
    /// Gradle: api, Maven: compile
    Api,
    /// Gradle: implementation
    Implementation,
    /// Gradle: compileOnly, Maven: provided
    CompileOnly,
    /// Gradle: testImplementation, Maven: test
    Test,
    /// Gradle: runtimeOnly, Maven: runtime
    RuntimeOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Dependency {
    pub target: WorkspaceModule,
    pub scope: DependencyScope,
    // TODO: jpms module name
}

#[salsa::input]
#[derive(Debug)]
pub struct WorkspaceModule {
    #[returns(ref)]
    pub name: String,

    #[returns(ref)]
    pub dependencies: Vec<Dependency>,

    pub jdk_version: u8,
}
