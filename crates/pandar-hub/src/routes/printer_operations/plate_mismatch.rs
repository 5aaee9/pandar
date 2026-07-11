use crate::repositories::PrintErrorAction;

pub const BUILD_PLATE_MISMATCH: u32 = 83_918_929;
const SUPPORTED_FAMILIES: [&str; 6] = ["093", "094", "20P", "22E", "239", "31B"];

pub fn supports(serial: &str, action: PrintErrorAction) -> bool {
    let family = serial.get(..3).map(str::to_ascii_uppercase);
    family
        .as_deref()
        .is_some_and(|value| SUPPORTED_FAMILIES.contains(&value))
        && matches!(
            action,
            PrintErrorAction::Resume | PrintErrorAction::Ignore | PrintErrorAction::Stop
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_plate_mismatch_catalog_is_closed_to_the_six_studio_families() {
        for family in ["093", "094", "20P", "22E", "239", "31B", "20p"] {
            for action in [
                PrintErrorAction::Resume,
                PrintErrorAction::Ignore,
                PrintErrorAction::Stop,
            ] {
                assert!(supports(&format!("{family}123456789"), action));
            }
        }
        for serial in ["26A123456789", "XYZ123456789", "20"] {
            for action in [
                PrintErrorAction::Resume,
                PrintErrorAction::Ignore,
                PrintErrorAction::Stop,
            ] {
                assert!(!supports(serial, action));
            }
        }
    }
}
