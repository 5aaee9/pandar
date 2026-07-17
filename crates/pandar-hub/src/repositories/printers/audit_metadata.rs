use serde::Serialize;

#[derive(Serialize)]
pub(super) struct PrinterDeleteAuditMetadata<'a> {
    pub(super) printer_name: &'a str,
    pub(super) serial_number: &'a str,
    pub(super) agent_id: String,
    pub(super) previous_status: &'a str,
}

#[derive(Serialize)]
pub(super) struct PrinterUpdateAuditMetadata<'a> {
    pub(super) previous_name: &'a str,
    pub(super) previous_host: &'a Option<String>,
    pub(super) printer_name: &'a str,
    pub(super) printer_host: &'a Option<String>,
    pub(super) serial_number: &'a str,
    pub(super) agent_id: String,
}
