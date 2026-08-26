use serde::Deserialize;

use crate::{
    PluginHttpResult,
    firmware::{StudioFirmwareParse, parse_studio_firmware},
    gcode::{StudioOperationParse, operation_json_from_gcode},
    read_utf8, result, stable_error_body,
    studio_status::{StudioStatusRequest, parse_status_request},
};

const H2C_AUTO_NOZZLE_MAPPING_COMMAND: &str = "get_auto_nozzle_mapping";

const UNSUPPORTED: i32 = 0;
const FIRMWARE: i32 = 1;
const STATUS_GET_VERSION: i32 = 2;
const STATUS_PUSH_ALL: i32 = 3;
const OPERATION: i32 = 4;
const H2C_AUTO_NOZZLE_MAPPING: i32 = 5;
const VALID: i32 = 0;
const INVALID: i32 = 1;
const ABI_SUCCESS: i32 = 0;
const ABI_INVALID_RESULT: i32 = -19;

#[repr(C)]
pub struct PluginStudioMessageResult {
    pub kind: i32,
    pub outcome: i32,
    pub abi_status: i32,
    pub body_ptr: *mut u8,
    pub body_len: usize,
    pub body_cap: usize,
}

/// Stable parser statuses: 0 is an operation with HTTP 200; 1 is unsupported with HTTP 400;
/// 2 is an invalid native candidate with HTTP 400. Both error statuses use the same body.
#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_operation_json_from_gcode(
    message_ptr: *const u8,
    message_len: usize,
) -> PluginHttpResult {
    read_utf8(message_ptr, message_len)
        .map_or(StudioOperationParse::Unsupported, |message| {
            operation_json_from_gcode(&message)
        })
        .into_http_result()
}

/// Stable status-request kinds: 0 is not a status request, 1 is `info.get_version`,
/// and 2 is `pushing.pushall`. The body is the typed request sequence string.
#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_classify_status_request(
    message_ptr: *const u8,
    message_len: usize,
) -> PluginHttpResult {
    let (kind, sequence_id) = read_utf8(message_ptr, message_len)
        .map_or((0, String::new()), |message| {
            crate::studio_status::classify_status_request(&message)
        });
    result(kind, 200, sequence_id)
}

impl PluginStudioMessageResult {
    fn new(kind: i32, outcome: i32, abi_status: i32, body: impl Into<String>) -> Self {
        let mut body = body.into().into_bytes();
        let result = Self {
            kind,
            outcome,
            abi_status,
            body_ptr: body.as_mut_ptr(),
            body_len: body.len(),
            body_cap: body.capacity(),
        };
        std::mem::forget(body);
        result
    }

    fn invalid(kind: i32) -> Self {
        Self::new(
            kind,
            INVALID,
            ABI_INVALID_RESULT,
            stable_error_body("unsupported_printer_operation"),
        )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_dispatch_studio_message(
    message_ptr: *const u8,
    message_len: usize,
) -> PluginStudioMessageResult {
    let Some(message) = read_utf8(message_ptr, message_len) else {
        return PluginStudioMessageResult::invalid(UNSUPPORTED);
    };

    match parse_studio_firmware(&message) {
        StudioFirmwareParse::Firmware(_) => {
            return PluginStudioMessageResult::new(FIRMWARE, VALID, ABI_SUCCESS, String::new());
        }
        StudioFirmwareParse::InvalidFirmware => {
            return PluginStudioMessageResult::invalid(FIRMWARE);
        }
        StudioFirmwareParse::NotFirmware => {}
    }

    match classify_h2c_auto_nozzle_mapping(&message) {
        H2cAutoNozzleMappingClassification::Valid(request) => {
            return PluginStudioMessageResult::new(
                H2C_AUTO_NOZZLE_MAPPING,
                VALID,
                ABI_SUCCESS,
                serde_json::to_string(&request)
                    .expect("H2C auto nozzle mapping request is serializable"),
            );
        }
        H2cAutoNozzleMappingClassification::Invalid => {
            return PluginStudioMessageResult::invalid(H2C_AUTO_NOZZLE_MAPPING);
        }
        H2cAutoNozzleMappingClassification::NotH2c => {}
    }

    if let Some(request) = parse_status_request(&message) {
        return match request {
            StudioStatusRequest::GetVersion { sequence_id } => {
                PluginStudioMessageResult::new(STATUS_GET_VERSION, VALID, ABI_SUCCESS, sequence_id)
            }
            StudioStatusRequest::PushAll { sequence_id } => {
                PluginStudioMessageResult::new(STATUS_PUSH_ALL, VALID, ABI_SUCCESS, sequence_id)
            }
        };
    }

    match operation_json_from_gcode(&message) {
        StudioOperationParse::Operation(operation) => PluginStudioMessageResult::new(
            OPERATION,
            VALID,
            ABI_SUCCESS,
            serde_json::to_string(&operation).expect("printer operation is serializable"),
        ),
        StudioOperationParse::Unsupported | StudioOperationParse::InvalidNativeCandidate => {
            PluginStudioMessageResult::invalid(UNSUPPORTED)
        }
    }
}

enum H2cAutoNozzleMappingClassification {
    Valid(pandar_core::H2cAutoNozzleMappingRequest),
    Invalid,
    NotH2c,
}

/// Classifies a message by its typed `print.command`, never by substring
/// matching, so payloads that merely mention the command (for example a gcode
/// line printed with `M117`) are not misrouted into the H2C parser.
fn classify_h2c_auto_nozzle_mapping(message: &str) -> H2cAutoNozzleMappingClassification {
    match serde_json::from_str::<pandar_core::H2cAutoNozzleMappingEnvelope>(message) {
        Ok(envelope) if envelope.print.command == H2C_AUTO_NOZZLE_MAPPING_COMMAND => {
            if envelope.print.is_valid() {
                H2cAutoNozzleMappingClassification::Valid(envelope.print)
            } else {
                H2cAutoNozzleMappingClassification::Invalid
            }
        }
        _ => {
            // The full envelope requires typed payload fields; probe the print
            // command so malformed mapping requests still report the H2C kind.
            let intended = serde_json::from_str::<H2cCommandProbe>(message)
                .ok()
                .and_then(|probe| probe.print)
                .is_some_and(|command| command == H2C_AUTO_NOZZLE_MAPPING_COMMAND);
            if intended {
                H2cAutoNozzleMappingClassification::Invalid
            } else {
                H2cAutoNozzleMappingClassification::NotH2c
            }
        }
    }
}

#[derive(Deserialize)]
struct H2cCommandProbe {
    #[serde(default)]
    print: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatch(message: &str) -> PluginStudioMessageResult {
        pandar_plugin_dispatch_studio_message(message.as_ptr(), message.len())
    }

    fn free_body(outcome: PluginStudioMessageResult) {
        crate::pandar_plugin_free_with_capacity(
            outcome.body_ptr.cast(),
            outcome.body_len,
            outcome.body_cap,
        );
    }

    #[test]
    fn gcode_line_mentioning_the_h2c_command_is_an_operation() {
        let message = r#"{"print":{"command":"gcode_line","param":"M117 get_auto_nozzle_mapping","sequence_id":"42"}}"#;
        let outcome = dispatch(message);
        assert_eq!(outcome.kind, OPERATION);
        assert_eq!(outcome.outcome, VALID);
        free_body(outcome);
    }

    #[test]
    fn valid_h2c_mapping_request_is_dispatched_to_the_h2c_parser() {
        let message = r#"{"print":{"command":"get_auto_nozzle_mapping","sequence_id":"42","version":1,"group_info":[{"id":0,"ext":1,"dia":0.4,"vol":"E3D High Flow"}]}}"#;
        let outcome = dispatch(message);
        assert_eq!(outcome.kind, H2C_AUTO_NOZZLE_MAPPING);
        assert_eq!(outcome.outcome, VALID);
        free_body(outcome);
    }

    #[test]
    fn malformed_h2c_mapping_request_reports_the_h2c_kind() {
        let message = r#"{"print":{"command":"get_auto_nozzle_mapping","sequence_id":"abc"}}"#;
        let outcome = dispatch(message);
        assert_eq!(outcome.kind, H2C_AUTO_NOZZLE_MAPPING);
        assert_eq!(outcome.outcome, INVALID);
        free_body(outcome);
    }

    #[test]
    fn status_request_still_dispatches_before_operations() {
        let message = r#"{"info":{"command":"get_version","sequence_id":"7"}}"#;
        let outcome = dispatch(message);
        assert_eq!(outcome.kind, STATUS_GET_VERSION);
        assert_eq!(outcome.outcome, VALID);
        free_body(outcome);
    }
}
