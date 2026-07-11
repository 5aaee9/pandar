use serde::Deserialize;

use super::operation::Axis;

#[derive(Clone, Copy, Deserialize)]
pub(super) enum StudioAxis {
    X,
    Y,
    Z,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(try_from = "i8")]
pub(super) enum StudioDirection {
    Negative,
    Positive,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(try_from = "u8")]
pub(super) enum StudioMoveMode {
    OneMillimeter,
    TenMillimeters,
}

impl StudioAxis {
    pub(super) const fn operation_axis(self) -> Axis {
        match self {
            Self::X => Axis::X,
            Self::Y => Axis::Y,
            Self::Z => Axis::Z,
        }
    }
}

impl StudioDirection {
    pub(super) const fn sign(self) -> f64 {
        match self {
            Self::Negative => -1.0,
            Self::Positive => 1.0,
        }
    }
}

impl StudioMoveMode {
    pub(super) const fn distance_mm(self) -> f64 {
        match self {
            Self::OneMillimeter => 1.0,
            Self::TenMillimeters => 10.0,
        }
    }
}

impl TryFrom<i8> for StudioDirection {
    type Error = &'static str;

    fn try_from(value: i8) -> Result<Self, Self::Error> {
        match value {
            -1 => Ok(Self::Negative),
            1 => Ok(Self::Positive),
            _ => Err("Studio xyz_ctrl dir must be -1 or 1"),
        }
    }
}

impl TryFrom<u8> for StudioMoveMode {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::OneMillimeter),
            1 => Ok(Self::TenMillimeters),
            _ => Err("Studio xyz_ctrl mode must be 0 or 1"),
        }
    }
}
