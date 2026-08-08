use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterCoolingSystem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<PrinterCoolingMode>,
    pub fans: Vec<PrinterCoolingFan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterCoolingMode {
    Cooling,
    Heating,
    Exhaust,
    FullCooling,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterCoolingFan {
    pub kind: PrinterCoolingFanKind,
    pub speed_percent: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterCoolingFanKind {
    Hotend,
    PartCooling,
    Auxiliary,
    Chamber,
    HotendSecond,
    Controller,
    InnerLoop,
    AuxiliarySecond,
}
