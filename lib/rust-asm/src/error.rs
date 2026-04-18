#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("class read error")]
    ClassRead(#[source] ClassReadError),
    #[error("class write error")]
    ClassWriter(#[source] ClassWriteError),
}

#[derive(thiserror::Error, Debug)]
pub enum ClassReadError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("invalid magic 0x{0:08x}")]
    InvalidMagic(u32),
    #[error("invalid class major version {0}")]
    InvalidClassVersion(u16),
    #[error("invalid constant pool tag {0}")]
    InvalidConstantPoolTag(u8),
    #[error("invalid constant pool index {0}")]
    InvalidIndex(u16),
    #[error("invalid attribute {0}")]
    InvalidAttribute(String),
    #[error("invalid opcode 0x{opcode:02x} at {offset}")]
    InvalidOpcode {
        /// The opcode that caused the error.
        opcode: u8,
        /// Offset into the byte sequence where the error occurred.
        offset: usize,
    },
    #[error("modified utf8 error: {0}")]
    Utf8Error(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ClassWriteError {
    #[error("missing constant pool")]
    MissingConstantPool,
    #[error("invalid constant pool")]
    InvalidConstantPool,
    #[error("invalid opcode 0x{opcode:02X} at offset {offset}")]
    InvalidOpcode { opcode: u8, offset: usize },
    #[error("frame computation error: {0}")]
    FrameComputation(String),
}
