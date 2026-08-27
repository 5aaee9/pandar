use serde::{Deserialize, Serialize};

/// Recovery action a caller may request for an in-flight print error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintErrorAction {
    Resume,
    Ignore,
    Stop,
}

/// Bambu Studio's build-plate print-error recovery catalog: which serial
/// families accept which print-error/action pairs for native recovery.
pub mod plate_mismatch {
    use super::PrintErrorAction;

    pub const BUILD_PLATE_MISMATCH: u32 = 83_918_929;
    pub const BUILD_PLATE_NOT_DETECTED: u32 = 83_918_945;
    pub const BUILD_PLATE_MARKER_NOT_DETECTED: u32 = 83_918_946;
    pub const BUILD_PLATE_OFFSET: u32 = 83_918_988;
    pub const BUILD_PLATE_COLLISION_RISK: u32 = 83_919_003;
    pub const VISUAL_ENCODER_BOARD_NOT_DETECTED: u32 = 83_919_008;
    const SUPPORTED_FAMILIES: [&str; 6] = ["093", "094", "20P", "22E", "239", "31B"];

    pub fn supports(serial: &str, print_error: u32, action: PrintErrorAction) -> bool {
        let Some(family) = serial.get(..3).map(str::to_ascii_uppercase) else {
            return false;
        };
        if !SUPPORTED_FAMILIES.contains(&family.as_str()) {
            return false;
        }
        match print_error {
            BUILD_PLATE_MISMATCH => matches!(
                action,
                PrintErrorAction::Resume | PrintErrorAction::Ignore | PrintErrorAction::Stop
            ),
            BUILD_PLATE_NOT_DETECTED
            | BUILD_PLATE_MARKER_NOT_DETECTED
            | BUILD_PLATE_OFFSET
            | BUILD_PLATE_COLLISION_RISK => {
                matches!(action, PrintErrorAction::Resume | PrintErrorAction::Ignore)
            }
            VISUAL_ENCODER_BOARD_NOT_DETECTED => {
                family != "22E"
                    && matches!(action, PrintErrorAction::Resume | PrintErrorAction::Ignore)
            }
            _ => false,
        }
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
                    assert!(supports(
                        &format!("{family}123456789"),
                        BUILD_PLATE_MISMATCH,
                        action
                    ));
                }
            }
            for serial in ["26A123456789", "XYZ123456789", "20"] {
                for action in [
                    PrintErrorAction::Resume,
                    PrintErrorAction::Ignore,
                    PrintErrorAction::Stop,
                ] {
                    assert!(!supports(serial, BUILD_PLATE_MISMATCH, action));
                }
            }
        }

        #[test]
        fn build_plate_marker_catalog_matches_studio_actions() {
            for family in SUPPORTED_FAMILIES {
                for action in [PrintErrorAction::Ignore, PrintErrorAction::Resume] {
                    assert!(supports(
                        &format!("{family}123456789"),
                        BUILD_PLATE_MARKER_NOT_DETECTED,
                        action
                    ));
                }
                assert!(!supports(
                    &format!("{family}123456789"),
                    BUILD_PLATE_MARKER_NOT_DETECTED,
                    PrintErrorAction::Stop
                ));
            }
            assert!(!supports(
                "20P123456789",
                BUILD_PLATE_MARKER_NOT_DETECTED + 1,
                PrintErrorAction::Resume
            ));
        }

        #[test]
        fn additional_plate_recovery_catalog_matches_studio_runtime_actions() {
            for error in [
                BUILD_PLATE_NOT_DETECTED,
                BUILD_PLATE_OFFSET,
                BUILD_PLATE_COLLISION_RISK,
            ] {
                for family in SUPPORTED_FAMILIES {
                    let serial = format!("{family}123456789");
                    assert!(supports(&serial, error, PrintErrorAction::Ignore));
                    assert!(supports(&serial, error, PrintErrorAction::Resume));
                    assert!(!supports(&serial, error, PrintErrorAction::Stop));
                }
            }

            for family in ["093", "094", "20P", "239", "31B"] {
                let serial = format!("{family}123456789");
                assert!(supports(
                    &serial,
                    VISUAL_ENCODER_BOARD_NOT_DETECTED,
                    PrintErrorAction::Resume
                ));
                assert!(supports(
                    &serial,
                    VISUAL_ENCODER_BOARD_NOT_DETECTED,
                    PrintErrorAction::Ignore
                ));
            }
            assert!(!supports(
                "22E123456789",
                VISUAL_ENCODER_BOARD_NOT_DETECTED,
                PrintErrorAction::Resume
            ));
        }
    }
}
