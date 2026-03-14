use rust_asm::constant_pool::CpInfo;

use crate::index::{AnnotationValue, intern_str};

pub fn parse_const_value(tag: u8, idx: u16, cp: &[CpInfo]) -> AnnotationValue {
    match tag {
        b'B' => cp_int(cp, idx)
            .map(|v| AnnotationValue::Byte(v as i8))
            .unwrap_or(AnnotationValue::Unknown),
        b'C' => cp_int(cp, idx)
            .map(|v| AnnotationValue::Char(v as u16))
            .unwrap_or(AnnotationValue::Unknown),
        b'D' => cp_double(cp, idx)
            .map(AnnotationValue::Double)
            .unwrap_or(AnnotationValue::Unknown),
        b'F' => cp_float(cp, idx)
            .map(AnnotationValue::Float)
            .unwrap_or(AnnotationValue::Unknown),
        b'I' => cp_int(cp, idx)
            .map(AnnotationValue::Int)
            .unwrap_or(AnnotationValue::Unknown),
        b'J' => cp_long(cp, idx)
            .map(AnnotationValue::Long)
            .unwrap_or(AnnotationValue::Unknown),
        b'S' => cp_int(cp, idx)
            .map(|v| AnnotationValue::Short(v as i16))
            .unwrap_or(AnnotationValue::Unknown),
        b'Z' => cp_int(cp, idx)
            .map(|v| AnnotationValue::Boolean(v != 0))
            .unwrap_or(AnnotationValue::Unknown),
        b's' => cp_utf8(cp, idx)
            .map(|s| AnnotationValue::String(intern_str(s)))
            .unwrap_or(AnnotationValue::Unknown),
        _ => AnnotationValue::Unknown,
    }
}

pub fn cp_utf8(cp: &[CpInfo], idx: u16) -> Option<&str> {
    match cp.get(idx as usize)? {
        CpInfo::Utf8(s) => Some(s.as_str()),
        _ => None,
    }
}

pub fn cp_int(cp: &[CpInfo], idx: u16) -> Option<i32> {
    match cp.get(idx as usize)? {
        CpInfo::Integer(v) => Some(*v),
        _ => None,
    }
}

pub fn cp_long(cp: &[CpInfo], idx: u16) -> Option<i64> {
    match cp.get(idx as usize)? {
        CpInfo::Long(v) => Some(*v),
        _ => None,
    }
}

pub fn cp_float(cp: &[CpInfo], idx: u16) -> Option<f32> {
    match cp.get(idx as usize)? {
        CpInfo::Float(v) => Some(*v),
        _ => None,
    }
}

pub fn cp_double(cp: &[CpInfo], idx: u16) -> Option<f64> {
    match cp.get(idx as usize)? {
        CpInfo::Double(v) => Some(*v),
        _ => None,
    }
}

pub fn cp_utf8_desc_to_internal(cp: &[CpInfo], idx: u16) -> Option<String> {
    let s = match cp.get(idx as usize)? {
        CpInfo::Utf8(u) => u.as_str(),
        _ => return None,
    };
    // Expect "Lpkg/Name;" or "[L..;"
    let s = s.trim();
    let s = s.strip_prefix('L')?.strip_suffix(';')?;
    Some(s.to_string())
}
