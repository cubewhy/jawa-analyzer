/// Constructs an `InsnList` using a declarative, Smali-like syntax.
///
/// This macro simplifies the creation of bytecode sequences by abstracting away
/// the verbose struct initialization.
///
/// # Syntax
/// Instructions are semicolon-separated. The general format is:
/// `[prefix] <OPCODE> [operands];`
///
/// ## Supported Instruction Types
///
/// * **Simple (No operands):** `OPCODE;`
///     * Example: `RETURN;`, `NOP;`
/// * **Int (Integer operand):** `int OPCODE <value>;`
///     * Example: `int BIPUSH 10;`
/// * **Var (Local variable index):** `var OPCODE <index>;`
///     * Example: `var ALOAD 0;`
/// * **Type (Type index/reference):** `type OPCODE <index>;`
///     * Example: `type NEW 5;`
/// * **Field (Field access):** `field OPCODE "owner", "name", "descriptor";`
///     * Example: `field GETSTATIC "java/lang/System", "out", "Ljava/io/PrintStream;";`
/// * **Method (Method invocation):** `method OPCODE "owner", "name", "descriptor";`
///     * Example: `method INVOKEVIRTUAL "java/io/PrintStream", "println", "(Ljava/lang/String;)V";`
/// * **Ldc (String constant):** `ldc "string_value";`
///     * Example: `ldc "Hello World";`
/// * **Jump (Offset):** `jump OPCODE <offset>;`
///     * Example: `jump GOTO 10;`
/// * **Iinc (Variable increment):** `iinc OPCODE <var_index>, <increment>;`
///     * Example: `iinc IINC 1, 5;`
///
/// ## Example
///
/// ```rust
///
/// use rust_asm::{opcodes, insn_list, insn::{Insn, LdcValue, MemberRef}};
/// // execute the macro
/// let list = insn_list! {
///     [NOP]
///     [int BIPUSH 42]
///     [var ALOAD 1]
///     [type NEW 5]
///     [field GETSTATIC "java/lang/System", "out", "Ljava/io/PrintStream;"]
///     [method INVOKEVIRTUAL "java/io/PrintStream", "println", "(Ljava/lang/String;)V"]
///     [ldc "Hello Macro"]
///     [jump GOTO -10]
///     [iinc IINC 2, 1]
///     [RETURN]
/// };
///
/// let insns = list.into_insns();
///
/// // Basic assertion on length
/// assert_eq!(insns.len(), 10, "Expected 10 instructions in the list");
///
/// // Detailed assertions per instruction
/// // 1. NOP
/// if let Insn::Simple(node) = &insns[0] {
///     assert_eq!(node.opcode, opcodes::NOP);
/// } else {
///     panic!("Expected Simple Insn at index 0");
/// }
///
/// // 2. BIPUSH 42
/// if let Insn::Int(node) = &insns[1] {
///     assert_eq!(node.insn.opcode, opcodes::BIPUSH);
///     assert_eq!(node.operand, 42);
/// } else {
///     panic!("Expected Int Insn at index 1");
/// }
///
/// // 3. ALOAD 1
/// if let Insn::Var(node) = &insns[2] {
///     assert_eq!(node.insn.opcode, opcodes::ALOAD);
///     assert_eq!(node.var_index, 1);
/// } else {
///     panic!("Expected Var Insn at index 2");
/// }
///
/// // 4. NEW 5
/// if let Insn::Type(node) = &insns[3] {
///     assert_eq!(node.insn.opcode, opcodes::NEW);
///     assert_eq!(node.type_index, 5);
/// } else {
///     panic!("Expected Type Insn at index 3");
/// }
///
/// // 5. GETSTATIC (Field)
/// if let Insn::Field(node) = &insns[4] {
///     assert_eq!(node.insn.opcode, opcodes::GETSTATIC);
///     if let MemberRef::Symbolic {
///         owner,
///         name,
///         descriptor,
///     } = &node.field_ref
///     {
///         assert_eq!(owner, "java/lang/System");
///         assert_eq!(name, "out");
///         assert_eq!(descriptor, "Ljava/io/PrintStream;");
///     } else {
///         panic!("Expected Symbolic reference for Field");
///     }
/// } else {
///     panic!("Expected Field Insn at index 4");
/// }
///
/// // 7. LDC "Hello Macro"
/// if let Insn::Ldc(node) = &insns[6] {
///     assert_eq!(node.insn.opcode, opcodes::LDC);
///     if let LdcValue::String(val) = &node.value {
///         assert_eq!(val, "Hello Macro");
///     } else {
///         panic!("Expected String value for Ldc");
///     }
/// } else {
///     panic!("Expected Ldc Insn at index 6");
/// }
///
/// // 8. GOTO -10
/// if let Insn::Jump(node) = &insns[7] {
///     assert_eq!(node.insn.opcode, opcodes::GOTO);
///     assert_eq!(node.offset, -10);
/// } else {
///     panic!("Expected Jump Insn at index 7");
/// }
///
/// // 9. IINC 2, 1
/// if let Insn::Iinc(node) = &insns[8] {
///     assert_eq!(node.insn.opcode, opcodes::IINC);
///     assert_eq!(node.var_index, 2);
///     assert_eq!(node.increment, 1);
/// } else {
///     panic!("Expected Iinc Insn at index 8");
/// }
/// ```
#[macro_export]
macro_rules! insn_list {
    // Entry Point: Iterative Loop
    // Matches: [ ... ] [ ... ] [ ... ]
    // This parses all instructions "flatly" (O(1) recursion depth), preventing stack overflows.
    ( $( [ $($instruction:tt)+ ] )* ) => {
        {
            let mut list = $crate::insn::InsnList::new();
            $(
                // Dispatch each bracketed group to the handler
                insn_list!(@dispatch list, $($instruction)+);
            )*
            list
        }
    };

    // Simple (e.g., [NOP], [RETURN])
    (@dispatch $list:ident, $opcode:ident) => {
        $list.add($crate::insn::InsnNode { opcode: $crate::opcodes::$opcode });
    };

    // Int (e.g., [int BIPUSH 10])
    (@dispatch $list:ident, int $opcode:ident $operand:expr) => {
        $list.add($crate::insn::IntInsnNode {
            insn: $crate::insn::InsnNode { opcode: $crate::opcodes::$opcode },
            operand: $operand as i32,
        });
    };

    // Var (e.g., [var ALOAD 0])
    (@dispatch $list:ident, var $opcode:ident $index:expr) => {
        $list.add($crate::insn::VarInsnNode {
            insn: $crate::insn::InsnNode { opcode: $crate::opcodes::$opcode },
            var_index: $index as u16,
        });
    };

    // Type (e.g., [type NEW 5])
    (@dispatch $list:ident, type $opcode:ident $index:expr) => {
        $list.add($crate::insn::TypeInsnNode {
            insn: $crate::insn::InsnNode { opcode: $crate::opcodes::$opcode },
            type_index: $index as u16,
        });
    };

    // Field (e.g., [field GETSTATIC "owner", "name", "desc"])
    (@dispatch $list:ident, field $opcode:ident $owner:expr, $name:expr, $desc:expr) => {
        $list.add($crate::insn::FieldInsnNode::new(
            $crate::opcodes::$opcode,
            $owner,
            $name,
            $desc
        ));
    };

    // Method (e.g., [method INVOKEVIRTUAL "owner", "name", "desc"])
    (@dispatch $list:ident, method $opcode:ident $owner:expr, $name:expr, $desc:expr) => {
        $list.add($crate::insn::MethodInsnNode::new(
            $crate::opcodes::$opcode,
            $owner,
            $name,
            $desc
        ));
    };

    // Ldc (e.g., [ldc "hello"])
    (@dispatch $list:ident, ldc $value:expr) => {
        $list.add($crate::insn::LdcInsnNode::string($value));
    };

    // Ldc Index (e.g., [ldc_idx LDC 5])
    (@dispatch $list:ident, ldc_idx $opcode:ident $index:expr) => {
        $list.add($crate::LdcInsnNode::from_index(
            $crate::opcodes::$opcode,
            $index as u16
        ));
    };

    // Jump (e.g., [jump GOTO 10])
    (@dispatch $list:ident, jump $opcode:ident $offset:expr) => {
        $list.add($crate::insn::JumpInsnNode {
            insn: $crate::insn::InsnNode { opcode: $crate::opcodes::$opcode },
            offset: $offset as i32,
        });
    };

    // Iinc (e.g., [iinc IINC 1, 5])
    (@dispatch $list:ident, iinc $opcode:ident $var:expr, $incr:expr) => {
        $list.add($crate::insn::IincInsnNode {
            insn: $crate::insn::InsnNode { opcode: $crate::opcodes::$opcode },
            var_index: $var as u16,
            increment: $incr as i16,
        });
    };
}
