pub use pandar_core::{
    PrintErrorAction, PrinterAxis, PrinterAxisMovement, PrinterOperation as PrinterOperationKind,
};
use serde::{Deserialize, Serialize};

mod audit;
mod validate;

pub use audit::operation_audit_metadata;
pub use validate::validate_printer_operation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrinterOperationPayload {
    pub printer_id: String,
    pub serial_number: String,
    pub operation: PrinterOperationKind,
}
