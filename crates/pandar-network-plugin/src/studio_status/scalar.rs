use serde::{Serialize, Serializer};

use super::input::Scalar;

pub(super) fn text(value: &Option<Scalar>) -> String {
    value.as_ref().map(Scalar::text).unwrap_or_default()
}

pub(super) fn text_if_present(value: &Option<Scalar>) -> Option<String> {
    let value = text(value);
    (!value.is_empty()).then_some(value)
}

pub(super) fn scalar_if_present(value: &Option<Scalar>) -> Option<StudioScalar> {
    text_if_present(value).map(StudioScalar::from_text)
}

#[derive(Clone)]
pub(super) struct JsonNumber(serde_json::Number);

impl JsonNumber {
    pub(super) fn new(value: &str) -> Self {
        serde_json::from_str::<serde_json::Number>(value)
            .map(Self)
            .unwrap_or_else(|_| Self(serde_json::Number::from(0)))
    }
}

impl Serialize for JsonNumber {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum StudioScalar {
    Number(JsonNumber),
    String(String),
}

impl StudioScalar {
    fn from_text(value: String) -> Self {
        if serde_json::from_str::<serde_json::Number>(&value).is_ok() {
            Self::Number(JsonNumber::new(&value))
        } else {
            Self::String(value)
        }
    }
}

pub(super) fn json_number_or_zero(value: String) -> String {
    if value.is_empty() {
        return "0".to_string();
    }
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut out = String::new();
    for c in value.chars() {
        if c.is_ascii_digit() {
            seen_digit = true;
            out.push(c);
        } else if c == '.' && !seen_dot {
            seen_dot = true;
            out.push(c);
        } else if (c == '-' || c == '+') && out.is_empty() {
            out.push(c);
        }
    }
    if !seen_digit || out == "-" || out == "+" {
        "0".to_string()
    } else {
        out
    }
}

fn json_temperature_bits(value: &str) -> u32 {
    let parsed = json_number_or_zero(value.to_string())
        .parse::<f64>()
        .unwrap_or(0.0);
    if parsed <= 0.0 {
        0
    } else if parsed >= 65535.0 {
        65535
    } else {
        (parsed + 0.5) as u32
    }
}

pub(super) fn packed_temperature(current: &str, target: &str) -> u32 {
    json_temperature_bits(current) | (json_temperature_bits(target) << 16)
}

pub(super) fn parse_u64_or_zero(value: &str) -> u64 {
    value.parse().unwrap_or(0)
}

pub(super) fn hex_string(value: u64) -> String {
    format!("{value:x}")
}
