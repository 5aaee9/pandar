use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AmsUnitKind {
    Ams,
    AmsLite,
    #[serde(rename = "ams_2_pro")]
    Ams2Pro,
    AmsHt,
    AmsLiteMixed,
    #[default]
    #[serde(other)]
    Unknown,
}

impl AmsUnitKind {
    pub fn from_studio_info(info: &str) -> Option<Self> {
        let info = info
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X");
        let type_id = u64::from_str_radix(info, 16).ok()? & 0xF;
        match type_id {
            1 => Some(Self::Ams),
            2 => Some(Self::AmsLite),
            3 => Some(Self::Ams2Pro),
            4 => Some(Self::AmsHt),
            5 => Some(Self::AmsLiteMixed),
            _ => None,
        }
    }

    pub fn from_unit_id(unit_id: &str) -> Self {
        match unit_id.parse::<u32>() {
            Ok(0..=63) => Self::Ams,
            Ok(128..=135) => Self::AmsHt,
            _ => Self::Unknown,
        }
    }

    pub const fn studio_type_id(self) -> Option<u8> {
        match self {
            Self::Ams => Some(1),
            Self::AmsLite => Some(2),
            Self::Ams2Pro => Some(3),
            Self::AmsHt => Some(4),
            Self::AmsLiteMixed => Some(5),
            Self::Unknown => None,
        }
    }

    pub const fn uses_four_slot_exist_bits(self) -> bool {
        matches!(
            self,
            Self::Ams | Self::AmsLite | Self::Ams2Pro | Self::AmsLiteMixed
        )
    }

    pub fn global_tray_id(self, unit_id: u64, tray_id: u64) -> Option<u64> {
        match self {
            Self::AmsLiteMixed => (tray_id < 4).then(|| 24 + tray_id),
            Self::Ams | Self::AmsLite | Self::Ams2Pro => {
                (unit_id < 64 && tray_id < 4).then(|| unit_id * 4 + tray_id)
            }
            Self::AmsHt | Self::Unknown => None,
        }
    }

    pub fn studio_global_tray_id(self, unit_id: u64, tray_id: u64) -> Option<u64> {
        self.global_tray_id(unit_id, tray_id)
    }

    pub fn studio_exist_bit(self, unit_id: u64) -> Option<u64> {
        match self {
            Self::AmsLiteMixed => Some(12),
            Self::Ams | Self::AmsLite | Self::Ams2Pro => (unit_id < 64).then_some(unit_id),
            Self::AmsHt | Self::Unknown => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn studio_info_type_nibble_maps_known_ams_models() {
        assert_eq!(
            AmsUnitKind::from_studio_info("10001101"),
            Some(AmsUnitKind::Ams)
        );
        assert_eq!(
            AmsUnitKind::from_studio_info("00000002"),
            Some(AmsUnitKind::AmsLite)
        );
        assert_eq!(
            AmsUnitKind::from_studio_info("3"),
            Some(AmsUnitKind::Ams2Pro)
        );
        assert_eq!(
            AmsUnitKind::from_studio_info("0x4"),
            Some(AmsUnitKind::AmsHt)
        );
        assert_eq!(
            AmsUnitKind::from_studio_info("00000005"),
            Some(AmsUnitKind::AmsLiteMixed)
        );
        assert_eq!(AmsUnitKind::from_studio_info("0"), None);
    }

    #[test]
    fn mixed_ams_lite_uses_reserved_tray_and_existence_ranges() {
        assert_eq!(AmsUnitKind::AmsLiteMixed.global_tray_id(0, 0), Some(24));
        assert_eq!(AmsUnitKind::AmsLiteMixed.global_tray_id(7, 3), Some(27));
        assert_eq!(AmsUnitKind::AmsLiteMixed.global_tray_id(0, 4), None);
        assert_eq!(AmsUnitKind::AmsLiteMixed.studio_exist_bit(0), Some(12));
    }
}
