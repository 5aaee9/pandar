use std::cmp::Ordering;

use anyhow::Context;
use serde::Deserialize;
use serde_json::{Value, value::RawValue};

const I64_ABSOLUTE_BOUND: &str = "9223372036854775808";

#[derive(Deserialize)]
struct RawReport<'a> {
    #[serde(default, borrow)]
    print: Option<RawPrint<'a>>,
}

#[derive(Deserialize)]
struct RawPrint<'a> {
    #[serde(default, borrow, deserialize_with = "deserialize_raw_job_id")]
    job_id: RawJobId<'a>,
}

#[derive(Default)]
enum RawJobId<'a> {
    #[default]
    Absent,
    Present(&'a RawValue),
}

pub(crate) fn decode_mqtt_report_payload(payload: &[u8]) -> anyhow::Result<Value> {
    let raw_job_id = serde_json::from_slice::<RawReport<'_>>(payload)
        .ok()
        .and_then(|report| report.print)
        .and_then(|print| match print.job_id {
            RawJobId::Present(value) if is_json_number(value.get()) => Some(value),
            RawJobId::Absent | RawJobId::Present(_) => None,
        });

    match raw_job_id {
        Some(raw_job_id) => decode_with_normalized_job_id(payload, raw_job_id),
        None => serde_json::from_slice(payload).context("decode MQTT report payload as JSON"),
    }
}

fn decode_with_normalized_job_id(payload: &[u8], raw_job_id: &RawValue) -> anyhow::Result<Value> {
    let raw_job_id = raw_job_id.get().as_bytes();
    let start = raw_job_id.as_ptr() as usize - payload.as_ptr() as usize;
    let end = start + raw_job_id.len();
    let normalized = truncate_decimal_to_i64(
        std::str::from_utf8(raw_job_id).expect("RawValue borrowed valid JSON text"),
    )
    .unwrap_or_default();
    let normalized = serde_json::to_vec(&normalized).expect("string serialization cannot fail");
    let mut patched = Vec::with_capacity(payload.len() - raw_job_id.len() + normalized.len());
    patched.extend_from_slice(&payload[..start]);
    patched.extend_from_slice(&normalized);
    patched.extend_from_slice(&payload[end..]);

    serde_json::from_slice(&patched).context("decode MQTT report payload as JSON")
}

fn is_json_number(raw: &str) -> bool {
    matches!(raw.as_bytes().first(), Some(b'-' | b'0'..=b'9'))
}

fn deserialize_raw_job_id<'de, D>(deserializer: D) -> Result<RawJobId<'de>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <&RawValue>::deserialize(deserializer).map(RawJobId::Present)
}

fn truncate_decimal_to_i64(raw: &str) -> Option<String> {
    let (negative, unsigned) = match raw.strip_prefix('-') {
        Some(unsigned) => (true, unsigned),
        None => (false, raw),
    };
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(index) => (&unsigned[..index], parse_exponent(&unsigned[index + 1..])),
        None => (unsigned, 0),
    };
    let fraction_length = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let digits = mantissa
        .bytes()
        .filter(|byte| *byte != b'.')
        .skip_while(|byte| *byte == b'0')
        .collect::<Vec<_>>();
    if digits.is_empty() {
        return Some("0".to_owned());
    }

    let fraction_length = i64::try_from(fraction_length).unwrap_or(i64::MAX);
    let shift = exponent.saturating_sub(fraction_length);
    let (integer, has_fraction) = integer_and_fraction(&digits, shift)?;

    match integer.len().cmp(&I64_ABSOLUTE_BOUND.len()) {
        Ordering::Greater => return None,
        Ordering::Equal => match integer.as_str().cmp(I64_ABSOLUTE_BOUND) {
            Ordering::Greater => return None,
            Ordering::Equal if !negative || has_fraction => return None,
            Ordering::Less | Ordering::Equal => {}
        },
        Ordering::Less => {}
    }

    if integer == "0" || !negative {
        Some(integer)
    } else {
        Some(format!("-{integer}"))
    }
}

fn integer_and_fraction(digits: &[u8], shift: i64) -> Option<(String, bool)> {
    if shift >= 0 {
        let shift = usize::try_from(shift).ok()?;
        let integer_length = digits.len().checked_add(shift)?;
        if integer_length > I64_ABSOLUTE_BOUND.len() {
            return None;
        }
        let mut integer = String::with_capacity(integer_length);
        integer.extend(digits.iter().map(|byte| char::from(*byte)));
        integer.extend(std::iter::repeat_n('0', shift));
        return Some((integer, false));
    }

    let truncated_digits = shift.unsigned_abs();
    if truncated_digits >= digits.len() as u64 {
        return Some(("0".to_owned(), digits.iter().any(|digit| *digit != b'0')));
    }
    let split = digits.len() - truncated_digits as usize;
    let integer =
        String::from_utf8(digits[..split].to_vec()).expect("JSON number digits are UTF-8");
    let has_fraction = digits[split..].iter().any(|digit| *digit != b'0');
    Some((integer, has_fraction))
}

fn parse_exponent(raw: &str) -> i64 {
    let (negative, digits) = match raw.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, raw.strip_prefix('+').unwrap_or(raw)),
    };
    let magnitude = digits.bytes().fold(0_i64, |value, digit| {
        value
            .saturating_mul(10)
            .saturating_add(i64::from(digit - b'0'))
    });
    if negative { -magnitude } else { magnitude }
}
