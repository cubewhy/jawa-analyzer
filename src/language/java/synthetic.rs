mod common;
mod rules;

pub use common::{
    SyntheticDefinition, SyntheticDefinitionKind, SyntheticInput, SyntheticMemberRule,
    SyntheticMemberSet, SyntheticOrigin, extract_type_members_with_synthetics,
    resolve_synthetic_definition, synthesize_for_type,
};
pub use rules::enum_rule::enum_constant_names;
pub use rules::record_rule::{RecordComponent, record_components};
