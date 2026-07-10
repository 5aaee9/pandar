use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct MachineHmsItem {
    pub attr: u32,
    pub code: u32,
}
