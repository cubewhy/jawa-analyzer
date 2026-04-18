use std::fmt;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    Void,
    Boolean,
    Char,
    Byte,
    Short,
    Int,
    Float,
    Long,
    Double,
    /// Array type, with the element type (which may itself be an array).
    Array(Box<Type>),
    /// Object type, storing the internal name (e.g. `"java/lang/Object"`).
    Object(String),
    /// Method type, storing argument types and return type.
    Method {
        argument_types: Vec<Type>,
        return_type: Box<Type>,
    },
}

impl Type {
    pub const VOID_TYPE: Type = Type::Void;
    pub const BOOLEAN_TYPE: Type = Type::Boolean;
    pub const CHAR_TYPE: Type = Type::Char;
    pub const BYTE_TYPE: Type = Type::Byte;
    pub const SHORT_TYPE: Type = Type::Short;
    pub const INT_TYPE: Type = Type::Int;
    pub const FLOAT_TYPE: Type = Type::Float;
    pub const LONG_TYPE: Type = Type::Long;
    pub const DOUBLE_TYPE: Type = Type::Double;

    /// Returns the `Type` corresponding to the given field or method descriptor.
    ///
    /// # Panics
    /// Panics if the descriptor is invalid.
    pub fn get_type(descriptor: &str) -> Self {
        let bytes = descriptor.as_bytes();
        let mut pos = 0;
        Self::parse(bytes, &mut pos)
    }

    /// Returns the `Type` corresponding to the given internal name.
    /// If the name starts with `'['`, it is treated as an array descriptor.
    pub fn get_object_type(internal_name: &str) -> Self {
        if internal_name.starts_with('[') {
            Self::get_type(internal_name)
        } else {
            Type::Object(internal_name.to_string())
        }
    }

    /// Returns the method `Type` corresponding to the given method descriptor.
    ///
    /// # Panics
    /// Panics if the descriptor is not a valid method descriptor.
    pub fn get_method_type(descriptor: &str) -> Self {
        let ty = Self::get_type(descriptor);
        match ty {
            Type::Method { .. } => ty,
            _ => panic!("Not a method descriptor: {}", descriptor),
        }
    }

    /// Creates a method type from its return type and argument types.
    pub fn get_method_type_from_parts(return_type: Type, argument_types: Vec<Type>) -> Self {
        Type::Method {
            argument_types,
            return_type: Box::new(return_type),
        }
    }

    /// Returns the sort of this type (as an integer, compatible with ASM constants).
    pub fn get_sort(&self) -> u8 {
        match self {
            Type::Void => 0,
            Type::Boolean => 1,
            Type::Char => 2,
            Type::Byte => 3,
            Type::Short => 4,
            Type::Int => 5,
            Type::Float => 6,
            Type::Long => 7,
            Type::Double => 8,
            Type::Array(_) => 9,
            Type::Object(_) => 10,
            Type::Method { .. } => 11,
        }
    }

    /// Returns the descriptor of this type.
    pub fn get_descriptor(&self) -> String {
        match self {
            Type::Void => "V".to_string(),
            Type::Boolean => "Z".to_string(),
            Type::Char => "C".to_string(),
            Type::Byte => "B".to_string(),
            Type::Short => "S".to_string(),
            Type::Int => "I".to_string(),
            Type::Float => "F".to_string(),
            Type::Long => "J".to_string(),
            Type::Double => "D".to_string(),
            Type::Array(elem) => format!("[{}", elem.get_descriptor()),
            Type::Object(name) => format!("L{};", name),
            Type::Method {
                argument_types,
                return_type,
            } => {
                let mut desc = String::from("(");
                for arg in argument_types {
                    desc.push_str(&arg.get_descriptor());
                }
                desc.push(')');
                desc.push_str(&return_type.get_descriptor());
                desc
            }
        }
    }

    /// Returns the Java class name corresponding to this type (e.g. "int", "java.lang.Object[]").
    pub fn get_class_name(&self) -> String {
        match self {
            Type::Void => "void".to_string(),
            Type::Boolean => "boolean".to_string(),
            Type::Char => "char".to_string(),
            Type::Byte => "byte".to_string(),
            Type::Short => "short".to_string(),
            Type::Int => "int".to_string(),
            Type::Float => "float".to_string(),
            Type::Long => "long".to_string(),
            Type::Double => "double".to_string(),
            Type::Array(elem) => format!("{}[]", elem.get_class_name()),
            Type::Object(name) => name.replace('/', "."),
            Type::Method { .. } => panic!("get_class_name() called on a method type"),
        }
    }

    /// Returns the internal name of this type.
    /// For object types, this is the internal name (e.g. `"java/lang/Object"`).
    /// For array types, this is the descriptor itself (e.g. `"[I"`).
    /// For other types, returns `None`.
    pub fn internal_name(&self) -> Option<String> {
        match self {
            Type::Object(name) => Some(name.clone()),
            Type::Array(_) => Some(self.get_descriptor()), // array internal name = descriptor
            _ => None,
        }
    }

    /// If this is an array type, returns the number of dimensions.
    /// Otherwise returns 0.
    pub fn get_dimensions(&self) -> usize {
        match self {
            Type::Array(elem) => 1 + elem.get_dimensions(),
            _ => 0,
        }
    }

    /// If this is an array type, returns the element type (which may itself be an array).
    /// Otherwise returns `None`.
    pub fn get_element_type(&self) -> Option<&Type> {
        match self {
            Type::Array(elem) => Some(elem),
            _ => None,
        }
    }

    /// If this is a method type, returns the argument types.
    pub fn get_argument_types(&self) -> Option<&[Type]> {
        match self {
            Type::Method { argument_types, .. } => Some(argument_types),
            _ => None,
        }
    }

    /// If this is a method type, returns the return type.
    pub fn get_return_type(&self) -> Option<&Type> {
        match self {
            Type::Method { return_type, .. } => Some(return_type),
            _ => None,
        }
    }

    /// Returns the size of values of this type (1 for most, 2 for long/double, 0 for void).
    pub fn get_size(&self) -> usize {
        match self {
            Type::Void => 0,
            Type::Long | Type::Double => 2,
            _ => 1,
        }
    }

    /// Returns the number of arguments of this method type.
    /// Panics if called on a non-method type.
    pub fn get_argument_count(&self) -> usize {
        match self {
            Type::Method { argument_types, .. } => argument_types.len(),
            _ => panic!("get_argument_count() called on a non-method type"),
        }
    }

    /// Parses a type from a byte slice starting at position `pos`.
    /// Returns the type and advances `pos` to the next position after the type.
    fn parse(bytes: &[u8], pos: &mut usize) -> Self {
        let c = bytes[*pos] as char;
        match c {
            'V' => {
                *pos += 1;
                Type::Void
            }
            'Z' => {
                *pos += 1;
                Type::Boolean
            }
            'C' => {
                *pos += 1;
                Type::Char
            }
            'B' => {
                *pos += 1;
                Type::Byte
            }
            'S' => {
                *pos += 1;
                Type::Short
            }
            'I' => {
                *pos += 1;
                Type::Int
            }
            'F' => {
                *pos += 1;
                Type::Float
            }
            'J' => {
                *pos += 1;
                Type::Long
            }
            'D' => {
                *pos += 1;
                Type::Double
            }
            'L' => {
                *pos += 1; // skip 'L'
                let start = *pos;
                while bytes[*pos] != b';' {
                    *pos += 1;
                }
                let name = std::str::from_utf8(&bytes[start..*pos])
                    .expect("Invalid UTF-8 in internal name")
                    .to_string();
                *pos += 1; // skip ';'
                Type::Object(name)
            }
            '[' => {
                *pos += 1; // skip '['
                let elem = Box::new(Self::parse(bytes, pos));
                Type::Array(elem)
            }
            '(' => {
                *pos += 1; // skip '('
                let mut args = Vec::new();
                while bytes[*pos] != b')' {
                    args.push(Self::parse(bytes, pos));
                }
                *pos += 1; // skip ')'
                let ret = Box::new(Self::parse(bytes, pos));
                Type::Method {
                    argument_types: args,
                    return_type: ret,
                }
            }
            _ => panic!("Invalid descriptor character: {}", c),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get_descriptor())
    }
}
