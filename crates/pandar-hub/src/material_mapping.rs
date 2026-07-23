pub(crate) use pandar_core::{
    StudioAmsMappingEntry as AmsMapping2Entry, StudioAmsMappingInfo as AmsMappingInfoEntry,
};

pub(crate) type AmsMapping = Vec<i32>;
pub(crate) type AmsMapping2 = Vec<AmsMapping2Entry>;
pub(crate) type AmsMappingInfo = Vec<AmsMappingInfoEntry>;

pub(crate) fn validate_mapping_len(len: usize) -> bool {
    len <= 32
}
