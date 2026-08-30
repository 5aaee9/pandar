use super::PrinterOperationKind;
use crate::repositories::{RepositoryError, RepositoryResult};

pub fn validate_printer_operation(operation: &PrinterOperationKind) -> RepositoryResult<()> {
    operation
        .validate()
        .map_err(|_| RepositoryError::InvalidPrinterControl)
}
