use crate::insn::Handle;
use std::collections::HashMap;

/// A builder for the constant pool of a class.
///
/// This struct manages the deduplication of constant pool entries, ensuring that
/// strings, classes, and member references are stored efficiently.
#[derive(Debug, Default)]
pub struct ConstantPoolBuilder {
    cp: Vec<CpInfo>,
    utf8: HashMap<String, u16>,
    class: HashMap<String, u16>,
    module: HashMap<String, u16>,
    package: HashMap<String, u16>,
    string: HashMap<String, u16>,
    name_and_type: HashMap<(String, String), u16>,
    field_ref: HashMap<(String, String, String), u16>,
    method_ref: HashMap<(String, String, String), u16>,
    interface_method_ref: HashMap<(String, String, String), u16>,
    method_type: HashMap<String, u16>,
    method_handle: HashMap<(u8, String, String, String, bool), u16>,
    invoke_dynamic: HashMap<(u16, String, String), u16>,
}

impl ConstantPoolBuilder {
    /// Creates a new, empty `ConstantPoolBuilder`.
    ///
    /// The constant pool starts with a dummy entry at index 0, as per JVM spec.
    pub fn new() -> Self {
        Self {
            cp: vec![CpInfo::Unusable],
            ..Default::default()
        }
    }

    /// Creates a `ConstantPoolBuilder` pre-populated with an existing pool.
    ///
    /// This preserves existing indices and initializes deduplication maps
    /// based on the pool contents.
    pub fn from_pool(pool: Vec<CpInfo>) -> Self {
        let cp = if pool.is_empty() {
            vec![CpInfo::Unusable]
        } else {
            pool
        };
        let mut builder = Self {
            cp,
            ..Default::default()
        };

        fn cp_utf8(cp: &[CpInfo], index: u16) -> Option<&str> {
            match cp.get(index as usize) {
                Some(CpInfo::Utf8(value)) => Some(value.as_str()),
                _ => None,
            }
        }

        fn cp_class_name(cp: &[CpInfo], index: u16) -> Option<&str> {
            match cp.get(index as usize) {
                Some(CpInfo::Class { name_index }) => cp_utf8(cp, *name_index),
                _ => None,
            }
        }

        fn cp_module_name(cp: &[CpInfo], index: u16) -> Option<&str> {
            match cp.get(index as usize) {
                Some(CpInfo::Module { name_index }) => cp_utf8(cp, *name_index),
                _ => None,
            }
        }

        fn cp_package_name(cp: &[CpInfo], index: u16) -> Option<&str> {
            match cp.get(index as usize) {
                Some(CpInfo::Package { name_index }) => cp_utf8(cp, *name_index),
                _ => None,
            }
        }

        fn cp_name_and_type(cp: &[CpInfo], index: u16) -> Option<(&str, &str)> {
            match cp.get(index as usize) {
                Some(CpInfo::NameAndType {
                    name_index,
                    descriptor_index,
                }) => {
                    let name = cp_utf8(cp, *name_index)?;
                    let desc = cp_utf8(cp, *descriptor_index)?;
                    Some((name, desc))
                }
                _ => None,
            }
        }

        fn cp_member_ref(cp: &[CpInfo], index: u16) -> Option<(String, String, String, bool)> {
            match cp.get(index as usize) {
                Some(CpInfo::Fieldref {
                    class_index,
                    name_and_type_index,
                }) => {
                    let owner = cp_class_name(cp, *class_index)?.to_string();
                    let (name, desc) = cp_name_and_type(cp, *name_and_type_index)?;
                    Some((owner, name.to_string(), desc.to_string(), false))
                }
                Some(CpInfo::Methodref {
                    class_index,
                    name_and_type_index,
                }) => {
                    let owner = cp_class_name(cp, *class_index)?.to_string();
                    let (name, desc) = cp_name_and_type(cp, *name_and_type_index)?;
                    Some((owner, name.to_string(), desc.to_string(), false))
                }
                Some(CpInfo::InterfaceMethodref {
                    class_index,
                    name_and_type_index,
                }) => {
                    let owner = cp_class_name(cp, *class_index)?.to_string();
                    let (name, desc) = cp_name_and_type(cp, *name_and_type_index)?;
                    Some((owner, name.to_string(), desc.to_string(), true))
                }
                _ => None,
            }
        }

        for (index, entry) in builder.cp.iter().enumerate() {
            let index = index as u16;
            match entry {
                CpInfo::Utf8(value) => {
                    builder.utf8.entry(value.clone()).or_insert(index);
                }
                CpInfo::Class { name_index } => {
                    if let Some(name) = cp_utf8(&builder.cp, *name_index) {
                        builder.class.entry(name.to_string()).or_insert(index);
                    }
                }
                CpInfo::Module { .. } => {
                    if let Some(name) = cp_module_name(&builder.cp, index) {
                        builder.module.entry(name.to_string()).or_insert(index);
                    }
                }
                CpInfo::Package { .. } => {
                    if let Some(name) = cp_package_name(&builder.cp, index) {
                        builder.package.entry(name.to_string()).or_insert(index);
                    }
                }
                CpInfo::String { string_index } => {
                    if let Some(value) = cp_utf8(&builder.cp, *string_index) {
                        builder.string.entry(value.to_string()).or_insert(index);
                    }
                }
                CpInfo::NameAndType {
                    name_index,
                    descriptor_index,
                } => {
                    if let (Some(name), Some(desc)) = (
                        cp_utf8(&builder.cp, *name_index),
                        cp_utf8(&builder.cp, *descriptor_index),
                    ) {
                        builder
                            .name_and_type
                            .entry((name.to_string(), desc.to_string()))
                            .or_insert(index);
                    }
                }
                CpInfo::Fieldref {
                    class_index,
                    name_and_type_index,
                } => {
                    if let (Some(owner), Some((name, desc))) = (
                        cp_class_name(&builder.cp, *class_index),
                        cp_name_and_type(&builder.cp, *name_and_type_index),
                    ) {
                        builder
                            .field_ref
                            .entry((owner.to_string(), name.to_string(), desc.to_string()))
                            .or_insert(index);
                    }
                }
                CpInfo::Methodref {
                    class_index,
                    name_and_type_index,
                } => {
                    if let (Some(owner), Some((name, desc))) = (
                        cp_class_name(&builder.cp, *class_index),
                        cp_name_and_type(&builder.cp, *name_and_type_index),
                    ) {
                        builder
                            .method_ref
                            .entry((owner.to_string(), name.to_string(), desc.to_string()))
                            .or_insert(index);
                    }
                }
                CpInfo::InterfaceMethodref {
                    class_index,
                    name_and_type_index,
                } => {
                    if let (Some(owner), Some((name, desc))) = (
                        cp_class_name(&builder.cp, *class_index),
                        cp_name_and_type(&builder.cp, *name_and_type_index),
                    ) {
                        builder
                            .interface_method_ref
                            .entry((owner.to_string(), name.to_string(), desc.to_string()))
                            .or_insert(index);
                    }
                }
                CpInfo::MethodType { descriptor_index } => {
                    if let Some(desc) = cp_utf8(&builder.cp, *descriptor_index) {
                        builder.method_type.entry(desc.to_string()).or_insert(index);
                    }
                }
                CpInfo::MethodHandle {
                    reference_kind,
                    reference_index,
                } => {
                    if let Some((owner, name, desc, is_interface)) =
                        cp_member_ref(&builder.cp, *reference_index)
                    {
                        builder
                            .method_handle
                            .entry((*reference_kind, owner, name, desc, is_interface))
                            .or_insert(index);
                    }
                }
                CpInfo::InvokeDynamic {
                    bootstrap_method_attr_index,
                    name_and_type_index,
                } => {
                    if let Some((name, desc)) = cp_name_and_type(&builder.cp, *name_and_type_index)
                    {
                        builder
                            .invoke_dynamic
                            .entry((
                                *bootstrap_method_attr_index,
                                name.to_string(),
                                desc.to_string(),
                            ))
                            .or_insert(index);
                    }
                }
                _ => {}
            }
        }

        builder
    }

    /// Consumes the builder and returns the raw vector of `CpInfo` entries.
    pub fn into_pool(self) -> Vec<CpInfo> {
        self.cp
    }

    /// Adds a UTF-8 string to the constant pool if it doesn't exist.
    ///
    /// Returns the index of the entry.
    pub fn utf8(&mut self, value: &str) -> u16 {
        if let Some(index) = self.utf8.get(value) {
            return *index;
        }
        let index = self.push(CpInfo::Utf8(value.to_string()));
        self.utf8.insert(value.to_string(), index);
        index
    }

    /// Adds a Class constant to the pool.
    ///
    /// This will recursively add the UTF-8 name of the class.
    pub fn class(&mut self, name: &str) -> u16 {
        if let Some(index) = self.class.get(name) {
            return *index;
        }
        let name_index = self.utf8(name);
        let index = self.push(CpInfo::Class { name_index });
        self.class.insert(name.to_string(), index);
        index
    }

    /// Adds a Module constant to the pool.
    ///
    /// The name is the module name string stored in `CONSTANT_Utf8_info`.
    pub fn module(&mut self, name: &str) -> u16 {
        if let Some(index) = self.module.get(name) {
            return *index;
        }
        let name_index = self.utf8(name);
        let index = self.push(CpInfo::Module { name_index });
        self.module.insert(name.to_string(), index);
        index
    }

    /// Adds a Package constant to the pool.
    ///
    /// The name uses JVM package format such as `java/lang`.
    pub fn package(&mut self, name: &str) -> u16 {
        if let Some(index) = self.package.get(name) {
            return *index;
        }
        let name_index = self.utf8(name);
        let index = self.push(CpInfo::Package { name_index });
        self.package.insert(name.to_string(), index);
        index
    }

    /// Adds a String constant to the pool.
    ///
    /// This is for string literals (e.g., `ldc "foo"`).
    pub fn string(&mut self, value: &str) -> u16 {
        if let Some(index) = self.string.get(value) {
            return *index;
        }
        let string_index = self.utf8(value);
        let index = self.push(CpInfo::String { string_index });
        self.string.insert(value.to_string(), index);
        index
    }

    pub fn integer(&mut self, value: i32) -> u16 {
        self.push(CpInfo::Integer(value))
    }

    pub fn float(&mut self, value: f32) -> u16 {
        self.push(CpInfo::Float(value))
    }

    pub fn long(&mut self, value: i64) -> u16 {
        let index = self.push(CpInfo::Long(value));
        // Long takes two entries
        self.cp.push(CpInfo::Unusable);
        index
    }

    pub fn double(&mut self, value: f64) -> u16 {
        let index = self.push(CpInfo::Double(value));
        // Double takes two entries
        self.cp.push(CpInfo::Unusable);
        index
    }

    /// Adds a NameAndType constant to the pool.
    ///
    /// Used for field and method descriptors.
    pub fn name_and_type(&mut self, name: &str, descriptor: &str) -> u16 {
        let key = (name.to_string(), descriptor.to_string());
        if let Some(index) = self.name_and_type.get(&key) {
            return *index;
        }
        let name_index = self.utf8(name);
        let descriptor_index = self.utf8(descriptor);
        let index = self.push(CpInfo::NameAndType {
            name_index,
            descriptor_index,
        });
        self.name_and_type.insert(key, index);
        index
    }

    /// Adds a Fieldref constant to the pool.
    pub fn field_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> u16 {
        let key = (owner.to_string(), name.to_string(), descriptor.to_string());
        if let Some(index) = self.field_ref.get(&key) {
            return *index;
        }
        let class_index = self.class(owner);
        let name_and_type_index = self.name_and_type(name, descriptor);
        let index = self.push(CpInfo::Fieldref {
            class_index,
            name_and_type_index,
        });
        self.field_ref.insert(key, index);
        index
    }

    /// Adds a Methodref constant to the pool.
    pub fn method_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> u16 {
        let key = (owner.to_string(), name.to_string(), descriptor.to_string());
        if let Some(index) = self.method_ref.get(&key) {
            return *index;
        }
        let class_index = self.class(owner);
        let name_and_type_index = self.name_and_type(name, descriptor);
        let index = self.push(CpInfo::Methodref {
            class_index,
            name_and_type_index,
        });
        self.method_ref.insert(key, index);
        index
    }

    pub fn interface_method_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> u16 {
        let key = (owner.to_string(), name.to_string(), descriptor.to_string());
        if let Some(index) = self.interface_method_ref.get(&key) {
            return *index;
        }
        let class_index = self.class(owner);
        let name_and_type_index = self.name_and_type(name, descriptor);
        let index = self.push(CpInfo::InterfaceMethodref {
            class_index,
            name_and_type_index,
        });
        self.interface_method_ref.insert(key, index);
        index
    }

    pub fn method_type(&mut self, descriptor: &str) -> u16 {
        if let Some(index) = self.method_type.get(descriptor) {
            return *index;
        }
        let descriptor_index = self.utf8(descriptor);
        let index = self.push(CpInfo::MethodType { descriptor_index });
        self.method_type.insert(descriptor.to_string(), index);
        index
    }

    pub fn method_handle(&mut self, handle: &Handle) -> u16 {
        let key = (
            handle.reference_kind,
            handle.owner.clone(),
            handle.name.clone(),
            handle.descriptor.clone(),
            handle.is_interface,
        );
        if let Some(index) = self.method_handle.get(&key) {
            return *index;
        }
        let reference_index = match handle.reference_kind {
            1..=4 => self.field_ref(&handle.owner, &handle.name, &handle.descriptor),
            9 => self.interface_method_ref(&handle.owner, &handle.name, &handle.descriptor),
            _ => self.method_ref(&handle.owner, &handle.name, &handle.descriptor),
        };
        let index = self.push(CpInfo::MethodHandle {
            reference_kind: handle.reference_kind,
            reference_index,
        });
        self.method_handle.insert(key, index);
        index
    }

    pub fn invoke_dynamic(&mut self, bsm_index: u16, name: &str, descriptor: &str) -> u16 {
        let key = (bsm_index, name.to_string(), descriptor.to_string());
        if let Some(index) = self.invoke_dynamic.get(&key) {
            return *index;
        }
        let name_and_type_index = self.name_and_type(name, descriptor);
        let index = self.push(CpInfo::InvokeDynamic {
            bootstrap_method_attr_index: bsm_index,
            name_and_type_index,
        });
        self.invoke_dynamic.insert(key, index);
        index
    }

    fn push(&mut self, entry: CpInfo) -> u16 {
        self.cp.push(entry);
        (self.cp.len() - 1) as u16
    }
}
#[derive(Debug, Clone)]
pub enum CpInfo {
    Unusable,
    Utf8(String),
    Integer(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    Class {
        name_index: u16,
    },
    String {
        string_index: u16,
    },
    Fieldref {
        class_index: u16,
        name_and_type_index: u16,
    },
    Methodref {
        class_index: u16,
        name_and_type_index: u16,
    },
    InterfaceMethodref {
        class_index: u16,
        name_and_type_index: u16,
    },
    NameAndType {
        name_index: u16,
        descriptor_index: u16,
    },
    MethodHandle {
        reference_kind: u8,
        reference_index: u16,
    },
    MethodType {
        descriptor_index: u16,
    },
    Dynamic {
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
    InvokeDynamic {
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
    Module {
        name_index: u16,
    },
    Package {
        name_index: u16,
    },
}

impl CpInfo {
    pub fn as_int(&self) -> Option<i32> {
        match self {
            Self::Integer(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::Float(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_long(&self) -> Option<i64> {
        match self {
            Self::Long(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_double(&self) -> Option<f64> {
        match self {
            Self::Double(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_utf8(&self) -> Option<&str> {
        match self {
            Self::Utf8(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

pub trait ConstantPoolExt {
    fn resolve_utf8(&self, index: u16) -> Option<&str>;
    fn get_int(&self, index: u16) -> Option<i32>;
    fn get_long(&self, index: u16) -> Option<i64>;
    fn get_float(&self, index: u16) -> Option<f32>;
    fn get_double(&self, index: u16) -> Option<f64>;
}

impl ConstantPoolExt for [CpInfo] {
    fn resolve_utf8(&self, index: u16) -> Option<&str> {
        self.get(index as usize).and_then(|entry| entry.as_utf8())
    }

    fn get_int(&self, index: u16) -> Option<i32> {
        self.get(index as usize).and_then(|entry| entry.as_int())
    }

    fn get_long(&self, index: u16) -> Option<i64> {
        self.get(index as usize).and_then(|entry| entry.as_long())
    }

    fn get_float(&self, index: u16) -> Option<f32> {
        self.get(index as usize).and_then(|entry| entry.as_float())
    }

    fn get_double(&self, index: u16) -> Option<f64> {
        self.get(index as usize).and_then(|entry| entry.as_double())
    }
}
