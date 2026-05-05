use anyhow::Context;
use rust_asm::{class_reader::ClassReader, constants::ACC_MODULE};

use crate::ClassOrModuleData;

pub fn parse_cafebabe(bytes: &[u8]) -> anyhow::Result<ClassOrModuleData> {
    let node = ClassReader::new(bytes)
        .to_class_node()
        .context("Failed to parse class")?;

    let model = if node.access_flags & ACC_MODULE != 0 {
        // module class
        let module = todo!();
        ClassOrModuleData::Module(module)
    } else {
        let class = todo!();
        ClassOrModuleData::Class(class)
    };

    Ok(model)
}
