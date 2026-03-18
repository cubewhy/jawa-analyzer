use std::sync::Arc;

/// Lombok access level for generated members
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessLevel {
    Public,
    Protected,
    Package,
    Private,
    Module,
    None,
}

impl AccessLevel {
    /// Convert Lombok AccessLevel to JVM access flags
    pub fn to_access_flags(self) -> u16 {
        use rust_asm::constants::*;
        match self {
            AccessLevel::Public => ACC_PUBLIC,
            AccessLevel::Protected => ACC_PROTECTED,
            AccessLevel::Package => 0, // package-private = no flags
            AccessLevel::Private => ACC_PRIVATE,
            AccessLevel::Module => 0, // treat as package for now
            AccessLevel::None => 0,
        }
    }

    /// Parse AccessLevel from annotation value
    pub fn from_annotation_value(value: &crate::index::AnnotationValue) -> Option<Self> {
        use crate::index::AnnotationValue;
        match value {
            AnnotationValue::Enum { const_name, .. } => match const_name.as_ref() {
                "PUBLIC" => Some(AccessLevel::Public),
                "PROTECTED" => Some(AccessLevel::Protected),
                "PACKAGE" => Some(AccessLevel::Package),
                "PRIVATE" => Some(AccessLevel::Private),
                "MODULE" => Some(AccessLevel::Module),
                "NONE" => Some(AccessLevel::None),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Type of Lombok constructor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LombokConstructorType {
    NoArgs,
    RequiredArgs,
    AllArgs,
}

/// Type of Lombok builder method
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LombokBuilderMethod {
    Builder,
    BuilderMethod,
    BuilderSetter { field_name: Arc<str> },
}

/// Lombok annotation names (internal JVM format)
pub mod annotations {
    pub const GETTER: &str = "lombok/Getter";
    pub const SETTER: &str = "lombok/Setter";
    pub const TO_STRING: &str = "lombok/ToString";
    pub const EQUALS_AND_HASH_CODE: &str = "lombok/EqualsAndHashCode";
    pub const NO_ARGS_CONSTRUCTOR: &str = "lombok/NoArgsConstructor";
    pub const REQUIRED_ARGS_CONSTRUCTOR: &str = "lombok/RequiredArgsConstructor";
    pub const ALL_ARGS_CONSTRUCTOR: &str = "lombok/AllArgsConstructor";
    pub const DATA: &str = "lombok/Data";
    pub const VALUE: &str = "lombok/Value";
    pub const BUILDER: &str = "lombok/Builder";
    pub const SINGULAR: &str = "lombok/Singular";
    pub const WITH: &str = "lombok/With";
    pub const WITHER: &str = "lombok/experimental/Wither"; // deprecated, use @With
    pub const NON_NULL: &str = "lombok/NonNull";
    pub const CLEANUP: &str = "lombok/Cleanup";
    pub const SNEAKY_THROWS: &str = "lombok/SneakyThrows";
    pub const SYNCHRONIZED: &str = "lombok/Synchronized";
    pub const LOCKED: &str = "lombok/Locked";
    pub const DELEGATE: &str = "lombok/experimental/Delegate";
    pub const GETTER_LAZY: &str = "lombok/Getter"; // with lazy=true parameter

    // Log annotations
    pub const SLF4J: &str = "lombok/extern/slf4j/Slf4j";
    pub const LOG: &str = "lombok/extern/java/Log";
    pub const LOG4J: &str = "lombok/extern/log4j/Log4j";
    pub const LOG4J2: &str = "lombok/extern/log4j/Log4j2";
    pub const COMMONS_LOG: &str = "lombok/extern/apachecommons/CommonsLog";
    pub const JBOSS_LOG: &str = "lombok/extern/jbosslog/JBossLog";
    pub const FLOGGER: &str = "lombok/extern/flogger/Flogger";
    pub const CUSTOM_LOG: &str = "lombok/CustomLog";
    pub const XSLF4J: &str = "lombok/extern/slf4j/XSlf4j";
}

/// Configuration keys for lombok.config
pub mod config_keys {
    pub const ACCESSORS_CHAIN: &str = "lombok.accessors.chain";
    pub const ACCESSORS_FLUENT: &str = "lombok.accessors.fluent";
    pub const ACCESSORS_PREFIX: &str = "lombok.accessors.prefix";
    pub const GETTER_FLAG_USAGE: &str = "lombok.getter.flagUsage";
    pub const SETTER_FLAG_USAGE: &str = "lombok.setter.flagUsage";
    pub const LOG_FIELD_NAME: &str = "lombok.log.fieldName";
    pub const LOG_FIELD_IS_STATIC: &str = "lombok.log.fieldIsStatic";
    pub const COPYABLE_ANNOTATIONS: &str = "lombok.copyableAnnotations";
    pub const TO_STRING_INCLUDE_FIELD_NAMES: &str = "lombok.toString.includeFieldNames";
    pub const TO_STRING_DO_NOT_USE_GETTERS: &str = "lombok.toString.doNotUseGetters";
    pub const EQUALS_AND_HASH_CODE_DO_NOT_USE_GETTERS: &str =
        "lombok.equalsAndHashCode.doNotUseGetters";
    pub const FIELD_DEFAULTS_DEFAULT_PRIVATE: &str = "lombok.fieldDefaults.defaultPrivate";
    pub const FIELD_DEFAULTS_DEFAULT_FINAL: &str = "lombok.fieldDefaults.defaultFinal";
}
