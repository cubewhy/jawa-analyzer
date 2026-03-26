use std::sync::Arc;

/// Structured internal type names, using JVM-internal base names plus generic args and array dims.
/// - Objects: base_internal = "java/lang/String"
/// - Primitive types: base_internal = "int"
/// - Arrays: array_dims > 0
/// - With generics: args = [TypeName], rendered as "java/util/List<Ljava/lang/String;>"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeNameKind {
    Internal,
    Primitive,
    TypeVar,
    SourceLike,
    Intersection,
    Wildcard,
    WildcardExtends,
    WildcardSuper,
    Null,
    Unknown,
    Capture,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeName {
    pub base_internal: Arc<str>,
    pub kind: TypeNameKind,
    pub args: Vec<TypeName>,
    pub array_dims: usize,
}

impl TypeName {
    const INTERSECTION_INTERNAL: &'static str = "&";

    pub fn new(base_internal: impl Into<Arc<str>>) -> Self {
        let raw: Arc<str> = base_internal.into();
        let mut base = raw.as_ref().trim();
        let mut dims = 0usize;
        while let Some(stripped) = base.strip_suffix("[]") {
            dims += 1;
            base = stripped.trim_end();
        }
        Self::with_kind(base, Self::classify_legacy_base(base)).with_array_dims(dims)
    }

    pub fn with_args(base_internal: impl Into<Arc<str>>, args: Vec<TypeName>) -> Self {
        let raw: Arc<str> = base_internal.into();
        let kind = Self::classify_legacy_base(raw.as_ref());
        Self::with_args_and_kind(raw, kind, args)
    }

    pub fn with_kind(base_internal: impl Into<Arc<str>>, kind: TypeNameKind) -> Self {
        TypeName {
            base_internal: base_internal.into(),
            kind,
            args: Vec::new(),
            array_dims: 0,
        }
    }

    pub fn with_args_and_kind(
        base_internal: impl Into<Arc<str>>,
        kind: TypeNameKind,
        args: Vec<TypeName>,
    ) -> Self {
        TypeName {
            base_internal: base_internal.into(),
            kind,
            args,
            array_dims: 0,
        }
    }

    pub fn internal(base_internal: impl Into<Arc<str>>) -> Self {
        Self::with_kind(base_internal, TypeNameKind::Internal)
    }

    pub fn primitive(base_internal: impl Into<Arc<str>>) -> Self {
        Self::with_kind(base_internal, TypeNameKind::Primitive)
    }

    pub fn type_var(base_internal: impl Into<Arc<str>>) -> Self {
        Self::with_kind(base_internal, TypeNameKind::TypeVar)
    }

    pub fn source_like(base_internal: impl Into<Arc<str>>) -> Self {
        Self::with_kind(base_internal, TypeNameKind::SourceLike)
    }

    pub fn unknown() -> Self {
        Self::with_kind("unknown", TypeNameKind::Unknown)
    }

    pub fn null() -> Self {
        Self::with_kind("null", TypeNameKind::Null)
    }

    pub fn wildcard() -> Self {
        Self::with_kind("*", TypeNameKind::Wildcard)
    }

    pub fn internal_with_args(base_internal: impl Into<Arc<str>>, args: Vec<TypeName>) -> Self {
        Self::with_args_and_kind(base_internal, TypeNameKind::Internal, args)
    }

    pub fn wildcard_extends(inner: TypeName) -> Self {
        Self::with_args_and_kind("+", TypeNameKind::WildcardExtends, vec![inner])
    }

    pub fn wildcard_super(inner: TypeName) -> Self {
        Self::with_args_and_kind("-", TypeNameKind::WildcardSuper, vec![inner])
    }

    pub fn capture() -> Self {
        Self::with_kind("capture", TypeNameKind::Capture)
    }

    pub fn intersection(bounds: Vec<TypeName>) -> Self {
        let mut flattened = Vec::new();
        for bound in bounds {
            if bound.is_intersection() {
                flattened.extend(bound.args);
            } else {
                flattened.push(bound);
            }
        }
        match flattened.len() {
            0 => TypeName::with_kind(Self::INTERSECTION_INTERNAL, TypeNameKind::Intersection),
            1 => flattened
                .into_iter()
                .next()
                .unwrap_or_else(TypeName::unknown),
            _ => TypeName::with_args_and_kind(
                Self::INTERSECTION_INTERNAL,
                TypeNameKind::Intersection,
                flattened,
            ),
        }
    }

    pub fn with_array_dims(mut self, dims: usize) -> Self {
        self.array_dims = dims;
        self
    }

    pub fn is_array(&self) -> bool {
        self.array_dims > 0
    }

    pub fn is_primitive(&self) -> bool {
        self.kind == TypeNameKind::Primitive
    }

    pub fn is_intersection(&self) -> bool {
        self.kind == TypeNameKind::Intersection
    }

    pub fn is_internal(&self) -> bool {
        self.kind == TypeNameKind::Internal
    }

    pub fn is_type_var(&self) -> bool {
        self.kind == TypeNameKind::TypeVar
    }

    pub fn is_source_like(&self) -> bool {
        self.kind == TypeNameKind::SourceLike
    }

    pub fn is_unknown(&self) -> bool {
        self.kind == TypeNameKind::Unknown
    }

    pub fn is_null(&self) -> bool {
        self.kind == TypeNameKind::Null
    }

    pub fn is_wildcard(&self) -> bool {
        self.kind == TypeNameKind::Wildcard
    }

    pub fn is_wildcard_extends(&self) -> bool {
        self.kind == TypeNameKind::WildcardExtends
    }

    pub fn is_wildcard_super(&self) -> bool {
        self.kind == TypeNameKind::WildcardSuper
    }

    pub fn is_capture(&self) -> bool {
        self.kind == TypeNameKind::Capture
    }

    pub fn is_wildcard_like(&self) -> bool {
        matches!(
            self.kind,
            TypeNameKind::Wildcard
                | TypeNameKind::WildcardExtends
                | TypeNameKind::WildcardSuper
                | TypeNameKind::Capture
        )
    }

    pub fn is_exact_class(&self) -> bool {
        if self.is_intersection() {
            return self.primary_bound().is_some_and(TypeName::is_exact_class);
        }
        self.kind == TypeNameKind::Internal
    }

    pub fn has_generics(&self) -> bool {
        !self.args.is_empty()
    }

    /// Erase generic parameters and array dims for index lookup.
    pub fn erased_internal(&self) -> &str {
        if self.is_intersection() {
            if let Some(primary) = self.primary_bound() {
                return primary.erased_internal();
            }
        }
        &self.base_internal
    }

    /// Erase generics but keep array dims, e.g. "java/lang/String[]".
    pub fn erased_internal_with_arrays(&self) -> String {
        let mut s = if self.is_intersection() {
            self.primary_bound()
                .map(TypeName::erased_internal_with_arrays)
                .unwrap_or_else(|| self.base_internal.to_string())
        } else {
            self.base_internal.to_string()
        };
        if self.array_dims > 0 {
            s.push_str(&"[]".repeat(self.array_dims));
        }
        s
    }

    pub fn contains_slash(&self) -> bool {
        if self.is_intersection() {
            return self.args.iter().any(TypeName::contains_slash);
        }
        self.base_internal.contains('/')
    }

    pub fn wildcard_upper_bound(&self) -> Option<&TypeName> {
        if self.is_wildcard_extends() {
            return self.args.first();
        }
        None
    }

    pub fn primary_bound(&self) -> Option<&TypeName> {
        self.is_intersection().then(|| self.args.first()).flatten()
    }

    pub fn bounds_for_lookup(&self) -> Vec<&TypeName> {
        if self.is_intersection() {
            return self.args.iter().collect();
        }
        vec![self]
    }

    /// "java/lang/String[][]" → Some("java/lang/String[]")
    pub fn element_type(&self) -> Option<TypeName> {
        if self.array_dims == 0 {
            return None;
        }
        let mut next = self.clone();
        next.array_dims -= 1;
        Some(next)
    }

    /// "java/lang/String" → "java/lang/String[]"
    pub fn wrap_array(&self) -> TypeName {
        let mut next = self.clone();
        next.array_dims += 1;
        next
    }

    /// Internal style with generics, e.g. "java/util/List<Ljava/lang/String;>".
    pub fn to_internal_with_generics(&self) -> String {
        if self.is_intersection() {
            let mut rendered = self
                .args
                .iter()
                .map(TypeName::to_internal_with_generics)
                .collect::<Vec<_>>()
                .join(" & ");
            if self.array_dims > 0 {
                rendered.push_str(&"[]".repeat(self.array_dims));
            }
            return rendered;
        }

        let base = self.base_internal.as_ref();
        let mut s =
            if self.args.is_empty() || self.is_wildcard_extends() || self.is_wildcard_super() {
                base.to_string()
            } else {
                let arg_sigs: Vec<String> =
                    self.args.iter().map(|a| a.to_jvm_signature()).collect();
                format!("{}<{}>", base, arg_sigs.join(""))
            };
        if self.array_dims > 0 {
            s.push_str(&"[]".repeat(self.array_dims));
        }
        s
    }

    /// Internal style with generics for substitution/rendering paths.
    /// Unlike `to_internal_with_generics`, this keeps non-slash class-like names
    /// as object signatures in generic arguments (e.g. `LBox;`), while retaining
    /// likely type variables (e.g. `TR;`).
    pub fn to_internal_with_generics_for_substitution(&self) -> String {
        if self.is_intersection() {
            let mut rendered = self
                .args
                .iter()
                .map(TypeName::to_internal_with_generics_for_substitution)
                .collect::<Vec<_>>()
                .join(" & ");
            if self.array_dims > 0 {
                rendered.push_str(&"[]".repeat(self.array_dims));
            }
            return rendered;
        }

        fn to_jvm_sig_for_substitution(ty: &TypeName) -> String {
            let mut sig = match ty.kind {
                TypeNameKind::Primitive => match ty.base_internal.as_ref() {
                    "byte" => "B".to_string(),
                    "char" => "C".to_string(),
                    "double" => "D".to_string(),
                    "float" => "F".to_string(),
                    "int" => "I".to_string(),
                    "long" => "J".to_string(),
                    "short" => "S".to_string(),
                    "boolean" => "Z".to_string(),
                    "void" => "V".to_string(),
                    other => other.to_string(),
                },
                TypeNameKind::Wildcard => "*".to_string(),
                TypeNameKind::WildcardExtends | TypeNameKind::WildcardSuper => {
                    if let Some(inner) = ty.args.first() {
                        format!("{}{}", ty.base_internal, to_jvm_sig_for_substitution(inner))
                    } else {
                        ty.base_internal.to_string()
                    }
                }
                TypeNameKind::TypeVar => format!("T{};", ty.base_internal),
                TypeNameKind::Internal
                | TypeNameKind::SourceLike
                | TypeNameKind::Capture
                | TypeNameKind::Null
                | TypeNameKind::Unknown
                | TypeNameKind::Intersection => {
                    let base = ty.base_internal.as_ref();
                    if ty.args.is_empty() {
                        format!("L{};", base)
                    } else {
                        let arg_sigs: Vec<String> =
                            ty.args.iter().map(to_jvm_sig_for_substitution).collect();
                        format!("L{}<{}>;", base, arg_sigs.join(""))
                    }
                }
            };

            if ty.array_dims > 0 {
                sig = format!("{}{}", "[".repeat(ty.array_dims), sig);
            }
            sig
        }

        let base = self.base_internal.as_ref();
        let mut s = if self.args.is_empty() {
            base.to_string()
        } else {
            let arg_sigs: Vec<String> = self.args.iter().map(to_jvm_sig_for_substitution).collect();
            format!("{}<{}>", base, arg_sigs.join(""))
        };
        if self.array_dims > 0 {
            s.push_str(&"[]".repeat(self.array_dims));
        }
        s
    }

    /// JVM signature, e.g. "Ljava/util/List<Ljava/lang/String;>;" or "[I".
    pub fn to_jvm_signature(&self) -> String {
        if self.is_intersection() {
            if let Some(primary) = self.primary_bound() {
                let mut sig = primary.to_jvm_signature();
                if self.array_dims > 0 {
                    sig = format!("{}{}", "[".repeat(self.array_dims), sig);
                }
                return sig;
            }
            return "Ljava/lang/Object;".to_string();
        }

        let mut sig = match self.kind {
            TypeNameKind::Primitive => match self.base_internal.as_ref() {
                "byte" => "B".to_string(),
                "char" => "C".to_string(),
                "double" => "D".to_string(),
                "float" => "F".to_string(),
                "int" => "I".to_string(),
                "long" => "J".to_string(),
                "short" => "S".to_string(),
                "boolean" => "Z".to_string(),
                "void" => "V".to_string(),
                other => other.to_string(),
            },
            TypeNameKind::Wildcard => "*".to_string(),
            TypeNameKind::WildcardExtends | TypeNameKind::WildcardSuper => {
                if let Some(inner) = self.args.first() {
                    format!("{}{}", self.base_internal, inner.to_jvm_signature())
                } else {
                    self.base_internal.to_string()
                }
            }
            TypeNameKind::Internal | TypeNameKind::SourceLike | TypeNameKind::Capture => {
                let base = self.base_internal.as_ref();
                if self.args.is_empty() {
                    format!("L{};", base)
                } else {
                    let arg_sigs: Vec<String> =
                        self.args.iter().map(|a| a.to_jvm_signature()).collect();
                    format!("L{}<{}>;", base, arg_sigs.join(""))
                }
            }
            TypeNameKind::TypeVar | TypeNameKind::Null | TypeNameKind::Unknown => {
                format!("T{};", self.base_internal)
            }
            TypeNameKind::Intersection => "Ljava/lang/Object;".to_string(),
        };

        if self.array_dims > 0 {
            sig = format!("{}{}", "[".repeat(self.array_dims), sig);
        }
        sig
    }

    fn classify_legacy_base(base: &str) -> TypeNameKind {
        match base {
            "byte" | "char" | "double" | "float" | "int" | "long" | "short" | "boolean"
            | "void" => TypeNameKind::Primitive,
            Self::INTERSECTION_INTERNAL => TypeNameKind::Intersection,
            "*" | "?" => TypeNameKind::Wildcard,
            "+" => TypeNameKind::WildcardExtends,
            "-" => TypeNameKind::WildcardSuper,
            "null" => TypeNameKind::Null,
            "capture" => TypeNameKind::Capture,
            _ if base.contains('/') => TypeNameKind::Internal,
            _ => TypeNameKind::SourceLike,
        }
    }
}

impl From<&str> for TypeName {
    fn from(s: &str) -> Self {
        TypeName::new(s)
    }
}

impl From<String> for TypeName {
    fn from(s: String) -> Self {
        TypeName::new(s)
    }
}

impl From<Arc<str>> for TypeName {
    fn from(arc: Arc<str>) -> Self {
        TypeName::new(arc)
    }
}

impl std::fmt::Display for TypeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_internal_with_generics())
    }
}

#[cfg(test)]
mod tests {
    use super::TypeName;

    #[test]
    fn test_erased_internal_keeps_base_and_preserves_array_via_array_helper() {
        let ty = TypeName::new("int[]");
        assert_eq!(ty.erased_internal(), "int");
        assert_eq!(ty.erased_internal_with_arrays(), "int[]");
    }

    #[test]
    fn test_to_internal_with_generics_preserves_array_dims() {
        let ty = TypeName::new("java/lang/String[][]");
        assert_eq!(ty.to_internal_with_generics(), "java/lang/String[][]");
    }

    #[test]
    fn test_intersection_erases_to_primary_bound() {
        let ty = TypeName::intersection(vec![
            TypeName::new("java/io/Closeable"),
            TypeName::new("java/lang/Runnable"),
        ]);
        assert!(ty.is_intersection());
        assert_eq!(ty.erased_internal(), "java/io/Closeable");
        assert_eq!(
            ty.to_internal_with_generics(),
            "java/io/Closeable & java/lang/Runnable"
        );
    }
}
