const UNSUPPORTED_FUN_BITS: u64 = (1_u64 << 1)
    | (1_u64 << 6)
    | (1_u64 << 7)
    | (1_u64 << 8)
    | (1_u64 << 9)
    | (1_u64 << 10)
    | (1_u64 << 12)
    | (1_u64 << 13)
    | (1_u64 << 28)
    | (1_u64 << 31)
    | (1_u64 << 40)
    | (1_u64 << 42)
    | (1_u64 << 43)
    | (1_u64 << 44)
    | (1_u64 << 45)
    | (1_u64 << 46)
    | (1_u64 << 48)
    | (1_u64 << 49)
    | (1_u64 << 60)
    | (1_u64 << 62);

const UNSUPPORTED_CFG_BITS: u64 = (1_u64 << 38) | (1_u64 << 39) | (1_u64 << 42);

pub(super) fn studio_fun(value: Option<&str>) -> String {
    masked_hex(value.unwrap_or_default(), UNSUPPORTED_FUN_BITS, false)
}

pub(super) fn studio_cfg(value: Option<&str>) -> Option<String> {
    value.map(|value| masked_hex(value, UNSUPPORTED_CFG_BITS, true))
}

pub(super) fn sdcard_available(aux: Option<&str>) -> bool {
    aux.and_then(|value| u64::from_str_radix(value, 16).ok())
        .is_some_and(|value| ((value >> 12) & 0b11) == 1)
}

fn masked_hex(value: &str, mask: u64, preserve_empty: bool) -> String {
    if preserve_empty && value.is_empty() {
        return String::new();
    }
    let value = u64::from_str_radix(value, 16).unwrap_or_default() & !mask;
    format!("{value:X}")
}
