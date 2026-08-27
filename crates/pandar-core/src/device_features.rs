use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BambuDeviceFeatures(u64);

impl BambuDeviceFeatures {
    pub fn from_hex(value: &str) -> Result<Self, BambuDeviceFeaturesParseError> {
        let value = value.trim_ascii();
        if value.is_empty() {
            return Err(BambuDeviceFeaturesParseError::Empty);
        }
        if value.len() > 16 {
            return Err(BambuDeviceFeaturesParseError::TooLong);
        }
        if !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(BambuDeviceFeaturesParseError::InvalidHexadecimal);
        }

        u64::from_str_radix(value, 16)
            .map(Self)
            .map_err(|_| BambuDeviceFeaturesParseError::InvalidHexadecimal)
    }

    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub fn to_hex(self) -> String {
        format!("{:X}", self.0)
    }

    pub const fn contains(self, feature: BambuDeviceFeature) -> bool {
        self.0 & (1_u64 << feature.bit()) != 0
    }
}

impl fmt::Display for BambuDeviceFeatures {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl FromStr for BambuDeviceFeatures {
    type Err = BambuDeviceFeaturesParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_hex(value)
    }
}

impl Serialize for BambuDeviceFeatures {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for BambuDeviceFeatures {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_hex(&value).map_err(D::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum BambuDeviceFeaturesParseError {
    #[error("device feature bitmap is empty")]
    Empty,
    #[error("device feature bitmap exceeds 16 hexadecimal digits")]
    TooLong,
    #[error("device feature bitmap contains non-hexadecimal characters")]
    InvalidHexadecimal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum BambuDeviceFeature {
    MqttHoming = 32,
    MqttAxisControl = 38,
}

impl BambuDeviceFeature {
    pub const fn bit(self) -> u32 {
        self as u32
    }
}

/// Device capability a printer operation requires before it may dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiredDeviceFeature {
    BambuMqttHoming,
    BambuMqttAxisControl,
}

impl RequiredDeviceFeature {
    pub const fn bambu_feature(self) -> BambuDeviceFeature {
        match self {
            Self::BambuMqttHoming => BambuDeviceFeature::MqttHoming,
            Self::BambuMqttAxisControl => BambuDeviceFeature::MqttAxisControl,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BambuMqttHoming => "bambu_mqtt_homing",
            Self::BambuMqttAxisControl => "bambu_mqtt_axis_control",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BambuDeviceFeature, BambuDeviceFeatures};

    #[test]
    fn parses_formats_and_queries_bambu_fun_bits() {
        let features = BambuDeviceFeatures::from_hex("  4100000000  ").unwrap();
        assert_eq!(features.to_hex(), "4100000000");
        assert!(features.contains(BambuDeviceFeature::MqttHoming));
        assert!(features.contains(BambuDeviceFeature::MqttAxisControl));
    }

    #[test]
    fn preserves_unnamed_and_high_bits() {
        let features = BambuDeviceFeatures::from_hex("8000004100000020").unwrap();
        assert_eq!(features.bits(), 0x8000_0041_0000_0020);
        assert_eq!(features.to_hex(), "8000004100000020");
    }

    #[test]
    fn canonicalizes_zero_and_rejects_non_grammar_inputs() {
        assert_eq!(BambuDeviceFeatures::from_hex("0000").unwrap().to_hex(), "0");
        for value in [
            "",
            " ",
            "-1",
            "+1",
            "0x1",
            "1_0",
            "GG",
            "10000000000000000",
            "\u{00A0}1\u{00A0}",
        ] {
            assert!(BambuDeviceFeatures::from_hex(value).is_err(), "{value}");
        }
    }
}
