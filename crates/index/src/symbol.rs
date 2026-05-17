use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use dashmap::{DashMap, DashSet};
use lasso::{Spur, ThreadedRodeo};
use lru::LruCache;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use triomphe::Arc;
use vfs::FileId;

pub type Symbol = Spur;

#[derive(Clone, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum TypeRef {
    Primitive(PrimitiveType),
    Reference {
        /// The dot fqn of the reference type
        name: Symbol,
        generic_args: Vec<TypeRef>,
    },
    Wildcard {
        bound: Option<Box<TypeBound>>,
    },
    TypeVariable(Symbol),
    Array(Box<TypeRef>),
    Error,
}

#[derive(Clone, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum TypeBound {
    Upper(TypeRef), // extends
    Lower(TypeRef), // super
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug, Hash)]
pub enum AnnotationValue {
    String(Symbol),
    Primitive(PrimitiveValue),
    Class(TypeRef),
    Enum {
        class_type: TypeRef,
        entry_name: Symbol,
    },
    Annotation(AnnotationSig),
    Array(Vec<AnnotationValue>),
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Hash, Debug)]
pub struct AnnotationSig {
    pub annotation_type: TypeRef,
    pub arguments: Vec<(Symbol, AnnotationValue)>,
}

#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Debug, Hash)]
pub enum PrimitiveValue {
    Int(i32),
    Long(i64),
    Float(u32),
    Double(u64),
    Boolean(bool),
    Byte(i8),
    Char(u16),
    Short(i16),
    Void,
}

impl PrimitiveValue {
    #[inline]
    pub fn float(val: f32) -> Self {
        Self::Float(val.to_bits())
    }

    #[inline]
    pub fn double(val: f64) -> Self {
        Self::Double(val.to_bits())
    }

    #[inline]
    pub fn get_float(&self) -> Option<f32> {
        if let Self::Float(bits) = self {
            Some(f32::from_bits(*bits))
        } else {
            None
        }
    }

    #[inline]
    pub fn get_double(&self) -> Option<f64> {
        if let Self::Double(bits) = self {
            Some(f64::from_bits(*bits))
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum PrimitiveType {
    Int,
    Long,
    Float,
    Double,
    Boolean,
    Byte,
    Char,
    Short,
    Void,
}

#[derive(Clone, PartialEq, Eq, Debug, Hash, Deserialize, Serialize)]
pub struct TypeParameter {
    pub name: Symbol,
    pub bounds: Vec<TypeRef>,
    pub annotations: Vec<AnnotationSig>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub struct RecordComponentData {
    pub name: Symbol,
    pub component_type: TypeRef,
    pub annotations: Vec<AnnotationSig>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub struct ClassStub {
    pub name: Symbol,
    /// JVM Access Flags
    pub flags: u16,
    pub super_class: Option<TypeRef>,
    pub interfaces: Vec<TypeRef>,
    pub type_params: Vec<TypeParameter>,

    pub permitted_subclasses: Vec<TypeRef>,
    pub record_components: Vec<RecordComponentData>,

    pub methods: Vec<MethodStub>,
    pub fields: Vec<FieldStub>,
    pub annotations: Vec<AnnotationSig>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub enum ClassKind {
    Class,
    Interface,
    Enum,
    Record,
    Annotation,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub struct ParamData {
    pub flags: u16,
    pub name: Option<Symbol>,
    pub param_type: TypeRef,
    pub annotations: Vec<AnnotationSig>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub struct MethodStub {
    pub flags: u16,
    pub name: Symbol,
    pub return_type: TypeRef,
    pub type_params: Vec<TypeParameter>,
    pub throws_list: Vec<TypeRef>,
    pub params: Vec<ParamData>,
    pub annotations: Vec<AnnotationSig>,

    /// The default value of an annotation entry
    pub default_value: Option<AnnotationValue>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub struct FieldStub {
    pub flags: u16,
    pub field_type: TypeRef,
    pub annotations: Vec<AnnotationSig>,
    pub constant_value: Option<AnnotationValue>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub struct ModuleStub {
    pub name: Symbol,
    pub flags: u16,
    pub version: Option<Symbol>,

    pub requires: Vec<ModuleRequires>,
    pub exports: Vec<ModuleExports>,
    pub opens: Vec<ModuleOpens>,
    pub uses: Vec<TypeRef>,
    pub provides: Vec<ModuleProvides>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub enum ClassOrModuleStub {
    Class(ClassStub),
    Module(ModuleStub),
}

impl ClassOrModuleStub {
    pub fn fqn(&self) -> Symbol {
        match self {
            Self::Class(class_data) => class_data.name,
            Self::Module(module_data) => module_data.name,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub struct ModuleRequires {
    pub module_name: Symbol,
    pub flags: u16,
    pub compiled_version: Option<Symbol>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub struct ModuleExports {
    pub package_name: Symbol,
    pub flags: u16,
    pub to_modules: Vec<Symbol>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub struct ModuleOpens {
    pub package_name: Symbol,
    pub flags: u16,
    pub to_modules: Vec<Symbol>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub struct ModuleProvides {
    pub service_interface: TypeRef,
    pub with_implementations: Vec<TypeRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LibraryId(pub u64);

impl LibraryId {
    /// Generate a unique ID for a JAR file based on its path and metadata
    pub fn from_jar_path(path: &Path) -> std::io::Result<Self> {
        let metadata = fs::metadata(path)?;
        let modified = metadata.modified()?;

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);

        format!("{:?}", modified).hash(&mut hasher);

        Ok(Self(hasher.finish()))
    }
}

// The ScopedSymbol is our universal key for both Memory and Disk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopedSymbol {
    pub lib_id: LibraryId,
    pub symbol: Symbol,
}

pub struct GlobalSymbolIndex {
    /// A fast, concurrent set to check if a class exists before hitting the disk.
    known_classes: DashSet<ScopedSymbol>,

    /// Maps a file to the classes it provides.
    workspace_file_classes: DashMap<FileId, Vec<ScopedSymbol>>,
    workspace_stubs: DashMap<ScopedSymbol, Arc<ClassStub>>,

    library_ledger: DashMap<LibraryId, Vec<Symbol>>,

    pub atoms: ThreadedRodeo,

    /// In-memory LRU cache for parsed stubs.
    /// Wrapped in a Mutex because `get` operations mutate the LRU order.
    stub_cache: Mutex<LruCache<ScopedSymbol, Arc<ClassStub>>>,

    /// The base directory where serialized stubs will be stored.
    cache_dir: PathBuf,
}

impl GlobalSymbolIndex {
    /// Creates a new index with a specific LRU capacity and cache directory.
    pub fn new(cache_dir: impl AsRef<Path>, lru_capacity: usize) -> Self {
        fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");

        Self {
            known_classes: DashSet::new(),
            workspace_file_classes: DashMap::new(),
            workspace_stubs: DashMap::new(),
            library_ledger: DashMap::new(),
            atoms: ThreadedRodeo::default(),
            stub_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(lru_capacity).expect("Capacity must be > 0"),
            )),
            cache_dir: cache_dir.as_ref().to_path_buf(),
        }
    }

    #[inline]
    pub fn intern(&self, s: &str) -> Symbol {
        self.atoms.get_or_intern(s)
    }

    #[inline]
    pub fn resolve(&self, sym: Symbol) -> &str {
        self.atoms.resolve(&sym)
    }

    /// Translates the scope into an isolated disk path:
    /// e.g., "cache_dir/libraries/1458392058/java/lang/String.bin"
    fn get_disk_path(&self, scoped_sym: ScopedSymbol) -> PathBuf {
        let fqn_str = self.atoms.resolve(&scoped_sym.symbol);
        let relative_path = PathBuf::from(fqn_str.replace('.', "/"));

        self.cache_dir
            .join("libraries")
            .join(scoped_sym.lib_id.0.to_string())
            .join(relative_path)
            .with_extension("bin")
    }

    pub fn insert_library_stubs(&self, lib_id: LibraryId, stubs: Vec<ClassStub>) {
        let mut cache = self.stub_cache.lock();
        let mut ledger_symbols = Vec::with_capacity(stubs.len());

        for stub in stubs {
            let scoped_sym = ScopedSymbol {
                lib_id,
                symbol: stub.name,
            };

            ledger_symbols.push(stub.name);

            self.known_classes.insert(scoped_sym);

            let path = self.get_disk_path(scoped_sym);
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(bytes) = postcard::to_allocvec(&stub) {
                let _ = fs::write(&path, bytes);
            }

            cache.put(scoped_sym, Arc::new(stub));
        }

        self.library_ledger.insert(lib_id, ledger_symbols);
    }

    pub fn update_workspace_file(
        &self,
        workspace_id: LibraryId,
        file_id: FileId,
        stubs: Vec<ClassStub>,
    ) {
        // lookup and remove old file
        if let Some((_, old_scoped_syms)) = self.workspace_file_classes.remove(&file_id) {
            let mut cache = self.stub_cache.lock();
            for old_sym in old_scoped_syms {
                self.known_classes.remove(&old_sym);
                cache.pop(&old_sym);
            }
        }

        // insert the new stub
        let mut new_syms = Vec::with_capacity(stubs.len());

        for stub in stubs {
            let scoped_sym = ScopedSymbol {
                lib_id: workspace_id,
                symbol: stub.name,
            };
            new_syms.push(scoped_sym);

            self.known_classes.insert(scoped_sym);
            self.workspace_stubs.insert(scoped_sym, Arc::new(stub));
        }

        self.workspace_file_classes.insert(file_id, new_syms);
    }

    pub fn remove_library(&self, lib_id: LibraryId) {
        if let Some((_, symbols)) = self.library_ledger.remove(&lib_id) {
            let mut cache = self.stub_cache.lock();
            for sym in symbols {
                let scoped_sym = ScopedSymbol {
                    lib_id,
                    symbol: sym,
                };
                self.known_classes.remove(&scoped_sym);
                cache.pop(&scoped_sym);
            }
        }

        self.workspace_stubs.retain(|k, _| k.lib_id != lib_id);
        self.workspace_file_classes.retain(|_, scoped_syms| {
            // Drop files where the first symbol's lib_id matches the target
            scoped_syms.first().is_none_or(|sym| sym.lib_id != lib_id)
        });

        let lib_dir = self.cache_dir.join("libraries").join(lib_id.0.to_string());

        let _ = fs::remove_dir_all(lib_dir);
    }

    /// Removes all traces of a workspace file from the index.
    pub fn remove_file(&self, file_id: FileId) {
        if let Some((_, old_scoped_syms)) = self.workspace_file_classes.remove(&file_id) {
            // Lock the LRU cache once for the whole loop
            let mut cache = self.stub_cache.lock();

            for old_sym in old_scoped_syms {
                self.known_classes.remove(&old_sym);
                self.workspace_stubs.remove(&old_sym);

                cache.pop(&old_sym);
            }
        }
    }

    pub fn resolve_class(&self, lib_id: LibraryId, fqn: &str) -> Option<Arc<ClassStub>> {
        let sym = self.atoms.get(fqn)?;

        // Combine the library ID and the resolved symbol into our composite key
        let scoped_sym = ScopedSymbol {
            lib_id,
            symbol: sym,
        };

        // Quick memory check: Does this class even exist in this specific library?
        if !self.known_classes.contains(&scoped_sym) {
            return None;
        }

        // check the LRU cache
        {
            let mut cache = self.stub_cache.lock();
            if let Some(stub) = cache.get(&scoped_sym) {
                return Some(stub.clone()); // Cache hit
            }
        }

        // check the workspace stubs
        if let Some(stub) = self.workspace_stubs.get(&scoped_sym) {
            return Some(stub.clone());
        }

        // Cache miss: Load stub from disk using the scoped_sym
        let path = self.get_disk_path(scoped_sym);
        let bytes = fs::read(&path).ok()?;
        let stub: ClassStub = postcard::from_bytes(&bytes).ok()?;
        let arc_stub = Arc::new(stub);

        // Put the newly loaded stub back into the LRU cache
        {
            let mut cache = self.stub_cache.lock();
            cache.put(scoped_sym, arc_stub.clone());
        }

        Some(arc_stub)
    }
}
