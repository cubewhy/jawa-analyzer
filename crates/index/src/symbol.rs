use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use dashmap::{DashMap, DashSet};
use lasso::ThreadedRodeo;
use lru::LruCache;
use parking_lot::Mutex;
use syntax::{ClassStub, Symbol};
use triomphe::Arc;
use vfs::FileId;

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

    /// Generates a unique ID for a mutable local workspace module.
    /// Hashes ONLY the absolute path. We do not hash the modified time because
    /// active files change constantly, and we handle those via the `ParseCache`.
    pub fn from_project_root(path: &Path) -> Self {
        let mut hasher = DefaultHasher::new();
        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        "/\\".hash(&mut hasher);
        abs_path.hash(&mut hasher);

        Self(hasher.finish())
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

    pub fn get_interner(&self) -> &ThreadedRodeo {
        &self.atoms
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
