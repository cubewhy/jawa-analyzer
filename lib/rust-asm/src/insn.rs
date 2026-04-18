use crate::opcodes;
use crate::types::Type;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone)]
pub struct InsnNode {
    pub opcode: u8,
}

impl From<u8> for InsnNode {
    fn from(value: u8) -> Self {
        Self { opcode: value }
    }
}

#[derive(Debug, Clone)]
pub struct IntInsnNode {
    pub insn: InsnNode,
    pub operand: i32,
}

#[derive(Debug, Clone)]
pub struct VarInsnNode {
    pub insn: InsnNode,
    pub var_index: u16,
}

#[derive(Debug, Clone)]
pub struct TypeInsnNode {
    pub insn: InsnNode,
    pub type_index: u16,
}

#[derive(Debug, Clone)]
pub struct FieldInsnNode {
    pub insn: InsnNode,
    pub field_ref: MemberRef,
}

#[derive(Debug, Clone)]
pub struct MethodInsnNode {
    pub insn: InsnNode,
    pub method_ref: MemberRef,
}

#[derive(Debug, Clone)]
pub struct InvokeInterfaceInsnNode {
    pub insn: InsnNode,
    pub method_index: u16,
    pub count: u8,
}

#[derive(Debug, Clone)]
pub struct InvokeDynamicInsnNode {
    pub insn: InsnNode,
    pub method_index: u16,
    pub name: Option<String>,
    pub descriptor: Option<String>,
    pub bootstrap_method: Option<Handle>,
    pub bootstrap_args: Vec<BootstrapArgument>,
}

#[derive(Debug, Clone)]
pub struct JumpInsnNode {
    pub insn: InsnNode,
    pub offset: i32,
}

#[derive(Debug, Clone)]
pub struct JumpLabelInsnNode {
    pub insn: InsnNode,
    pub target: LabelNode,
}

#[derive(Debug, Clone)]
pub struct LdcInsnNode {
    pub insn: InsnNode,
    pub value: LdcValue,
}

#[derive(Debug, Clone)]
pub struct IincInsnNode {
    pub insn: InsnNode,
    pub var_index: u16,
    pub increment: i16,
}

#[derive(Debug, Clone)]
pub struct TableSwitchInsnNode {
    pub insn: InsnNode,
    pub default_offset: i32,
    pub low: i32,
    pub high: i32,
    pub offsets: Vec<i32>,
}

#[derive(Debug, Clone)]
pub struct LookupSwitchInsnNode {
    pub insn: InsnNode,
    pub default_offset: i32,
    pub pairs: Vec<(i32, i32)>,
}

#[derive(Debug, Clone)]
pub struct TableSwitchLabelInsnNode {
    pub insn: InsnNode,
    pub default_target: LabelNode,
    pub low: i32,
    pub high: i32,
    pub targets: Vec<LabelNode>,
}

#[derive(Debug, Clone)]
pub struct LookupSwitchLabelInsnNode {
    pub insn: InsnNode,
    pub default_target: LabelNode,
    pub pairs: Vec<(i32, LabelNode)>,
}

#[derive(Debug, Clone)]
pub struct MultiANewArrayInsnNode {
    pub insn: InsnNode,
    pub type_index: u16,
    pub dimensions: u8,
}

static NEXT_LABEL_ID: AtomicUsize = AtomicUsize::new(0);

fn next_label_id() -> usize {
    NEXT_LABEL_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Label {
    pub id: usize,
}

impl Default for Label {
    fn default() -> Self {
        Self::new()
    }
}

impl Label {
    pub fn new() -> Self {
        Self {
            id: next_label_id(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LabelNode {
    pub id: usize,
}

impl Default for LabelNode {
    fn default() -> Self {
        Self::new()
    }
}

impl LabelNode {
    pub fn new() -> Self {
        Self {
            id: next_label_id(),
        }
    }

    pub fn from_label(label: Label) -> Self {
        Self { id: label.id }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LineNumberInsnNode {
    pub line: u16,
    pub start: LabelNode,
}
impl LineNumberInsnNode {
    pub(crate) fn new(line: u16, start: LabelNode) -> Self {
        Self { line, start }
    }
}

#[derive(Debug, Clone)]
pub struct TryCatchBlockNode {
    pub start: LabelNode,
    pub end: LabelNode,
    pub handler: LabelNode,
    pub catch_type: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AbstractInsnNode {
    Label(LabelNode),
    LineNumber(LineNumberInsnNode),
    Insn(Insn),
    JumpLabel(JumpLabelInsnNode),
    TableSwitchLabel(TableSwitchLabelInsnNode),
    LookupSwitchLabel(LookupSwitchLabelInsnNode),
}

#[derive(Debug, Clone)]
pub enum Insn {
    Simple(InsnNode),
    Int(IntInsnNode),
    Var(VarInsnNode),
    Type(TypeInsnNode),
    Field(FieldInsnNode),
    Method(MethodInsnNode),
    InvokeInterface(InvokeInterfaceInsnNode),
    InvokeDynamic(InvokeDynamicInsnNode),
    Jump(JumpInsnNode),
    Ldc(LdcInsnNode),
    Iinc(IincInsnNode),
    TableSwitch(TableSwitchInsnNode),
    LookupSwitch(LookupSwitchInsnNode),
    MultiANewArray(MultiANewArrayInsnNode),
}

#[derive(Debug, Clone)]
pub enum MemberRef {
    Index(u16),
    Symbolic {
        owner: String,
        name: String,
        descriptor: String,
    },
}

#[derive(Debug, Clone)]
pub enum LdcValue {
    Index(u16),
    String(String),
    Type(Type),
    Int(i32),
    Float(f32),
    Long(i64),
    Double(f64),
}

#[derive(Debug, Clone)]
pub struct Handle {
    pub reference_kind: u8,
    pub owner: String,
    pub name: String,
    pub descriptor: String,
    pub is_interface: bool,
}

#[derive(Debug, Clone)]
pub enum BootstrapArgument {
    Integer(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    String(String),
    Class(String),
    MethodType(String),
    Handle(Handle),
}

#[derive(Debug, Clone, Default)]
pub struct InsnList {
    insns: Vec<Insn>,
}

impl InsnList {
    pub fn new() -> Self {
        Self { insns: Vec::new() }
    }

    pub fn add<T: Into<Insn>>(&mut self, insn: T) -> &mut Self {
        self.insns.push(insn.into());
        self
    }

    pub fn insns(&self) -> &[Insn] {
        &self.insns
    }

    pub fn into_insns(self) -> Vec<Insn> {
        self.insns
    }
}

#[derive(Debug, Clone, Default)]
pub struct NodeList {
    nodes: Vec<AbstractInsnNode>,
}

impl NodeList {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn add<T: Into<AbstractInsnNode>>(&mut self, node: T) -> &mut Self {
        self.nodes.push(node.into());
        self
    }

    pub fn add_node(&mut self, node: AbstractInsnNode) -> &mut Self {
        self.nodes.push(node);
        self
    }

    pub fn nodes(&self) -> &[AbstractInsnNode] {
        &self.nodes
    }

    pub fn into_nodes(self) -> Vec<AbstractInsnNode> {
        self.nodes
    }
}

impl From<LabelNode> for AbstractInsnNode {
    fn from(value: LabelNode) -> Self {
        AbstractInsnNode::Label(value)
    }
}

impl From<LineNumberInsnNode> for AbstractInsnNode {
    fn from(value: LineNumberInsnNode) -> Self {
        AbstractInsnNode::LineNumber(value)
    }
}

impl From<Insn> for AbstractInsnNode {
    fn from(value: Insn) -> Self {
        AbstractInsnNode::Insn(value)
    }
}

impl From<JumpLabelInsnNode> for AbstractInsnNode {
    fn from(value: JumpLabelInsnNode) -> Self {
        AbstractInsnNode::JumpLabel(value)
    }
}

impl From<TableSwitchLabelInsnNode> for AbstractInsnNode {
    fn from(value: TableSwitchLabelInsnNode) -> Self {
        AbstractInsnNode::TableSwitchLabel(value)
    }
}

impl From<LookupSwitchLabelInsnNode> for AbstractInsnNode {
    fn from(value: LookupSwitchLabelInsnNode) -> Self {
        AbstractInsnNode::LookupSwitchLabel(value)
    }
}

impl FieldInsnNode {
    pub fn new(opcode: u8, owner: &str, name: &str, descriptor: &str) -> Self {
        Self {
            insn: opcode.into(),
            field_ref: MemberRef::Symbolic {
                owner: owner.to_string(),
                name: name.to_string(),
                descriptor: descriptor.to_string(),
            },
        }
    }

    pub fn from_index(opcode: u8, index: u16) -> Self {
        Self {
            insn: opcode.into(),
            field_ref: MemberRef::Index(index),
        }
    }
}

impl MethodInsnNode {
    pub fn new(opcode: u8, owner: &str, name: &str, descriptor: &str) -> Self {
        Self {
            insn: opcode.into(),
            method_ref: MemberRef::Symbolic {
                owner: owner.to_string(),
                name: name.to_string(),
                descriptor: descriptor.to_string(),
            },
        }
    }

    pub fn from_index(opcode: u8, index: u16) -> Self {
        Self {
            insn: opcode.into(),
            method_ref: MemberRef::Index(index),
        }
    }
}

impl InvokeDynamicInsnNode {
    pub fn new(
        name: &str,
        descriptor: &str,
        bootstrap_method: Handle,
        bootstrap_args: &[BootstrapArgument],
    ) -> Self {
        Self {
            insn: opcodes::INVOKEDYNAMIC.into(),
            method_index: 0,
            name: Some(name.to_string()),
            descriptor: Some(descriptor.to_string()),
            bootstrap_method: Some(bootstrap_method),
            bootstrap_args: bootstrap_args.to_vec(),
        }
    }

    pub fn from_index(index: u16) -> Self {
        Self {
            insn: opcodes::INVOKEDYNAMIC.into(),
            method_index: index,
            name: None,
            descriptor: None,
            bootstrap_method: None,
            bootstrap_args: Vec::new(),
        }
    }
}

impl LdcInsnNode {
    pub fn from_index(opcode: u8, index: u16) -> Self {
        Self {
            insn: opcode.into(),
            value: LdcValue::Index(index),
        }
    }

    pub fn int(value: i32) -> Self {
        Self {
            insn: opcodes::LDC.into(),
            value: LdcValue::Int(value),
        }
    }

    pub fn long(value: i64) -> Self {
        Self {
            insn: opcodes::LDC2_W.into(),
            value: LdcValue::Long(value),
        }
    }

    pub fn float(value: f32) -> Self {
        Self {
            insn: opcodes::LDC.into(),
            value: LdcValue::Float(value),
        }
    }

    pub fn double(value: f64) -> Self {
        Self {
            insn: opcodes::LDC2_W.into(),
            value: LdcValue::Double(value),
        }
    }

    pub fn short(value: i16) -> Self {
        Self::int(value as i32)
    }

    pub fn char(value: u16) -> Self {
        Self::int(value as i32)
    }

    pub fn byte(value: i8) -> Self {
        Self::int(value as i32)
    }

    pub fn boolean(value: bool) -> Self {
        Self::int(if value { 1 } else { 0 })
    }

    pub fn string(value: &str) -> Self {
        Self {
            insn: opcodes::LDC.into(),
            value: LdcValue::String(value.to_string()),
        }
    }

    pub fn typed(value: Type) -> Self {
        Self {
            insn: opcodes::LDC.into(),
            value: LdcValue::Type(value),
        }
    }
}

impl From<InsnNode> for Insn {
    fn from(value: InsnNode) -> Self {
        Insn::Simple(value)
    }
}

impl From<IntInsnNode> for Insn {
    fn from(value: IntInsnNode) -> Self {
        Insn::Int(value)
    }
}

impl From<VarInsnNode> for Insn {
    fn from(value: VarInsnNode) -> Self {
        Insn::Var(value)
    }
}

impl From<TypeInsnNode> for Insn {
    fn from(value: TypeInsnNode) -> Self {
        Insn::Type(value)
    }
}

impl From<FieldInsnNode> for Insn {
    fn from(value: FieldInsnNode) -> Self {
        Insn::Field(value)
    }
}

impl From<MethodInsnNode> for Insn {
    fn from(value: MethodInsnNode) -> Self {
        Insn::Method(value)
    }
}

impl From<InvokeInterfaceInsnNode> for Insn {
    fn from(value: InvokeInterfaceInsnNode) -> Self {
        Insn::InvokeInterface(value)
    }
}

impl From<InvokeDynamicInsnNode> for Insn {
    fn from(value: InvokeDynamicInsnNode) -> Self {
        Insn::InvokeDynamic(value)
    }
}

impl From<JumpInsnNode> for Insn {
    fn from(value: JumpInsnNode) -> Self {
        Insn::Jump(value)
    }
}

impl From<LdcInsnNode> for Insn {
    fn from(value: LdcInsnNode) -> Self {
        Insn::Ldc(value)
    }
}

impl From<IincInsnNode> for Insn {
    fn from(value: IincInsnNode) -> Self {
        Insn::Iinc(value)
    }
}

impl From<TableSwitchInsnNode> for Insn {
    fn from(value: TableSwitchInsnNode) -> Self {
        Insn::TableSwitch(value)
    }
}

impl From<LookupSwitchInsnNode> for Insn {
    fn from(value: LookupSwitchInsnNode) -> Self {
        Insn::LookupSwitch(value)
    }
}

impl From<MultiANewArrayInsnNode> for Insn {
    fn from(value: MultiANewArrayInsnNode) -> Self {
        Insn::MultiANewArray(value)
    }
}
