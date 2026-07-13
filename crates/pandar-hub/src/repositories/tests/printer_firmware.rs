mod cas;
mod postgres;
mod schema;

use pandar_core::{
    AgentId, PrinterFirmwareModule, PrinterFirmwareState, PrinterUpgradeState, TenantId,
};

use super::*;
use crate::{
    db::Database,
    repositories::{PrinterFirmwareUpdateOutcome, test_helpers::insert_printer_fixture},
};

struct FirmwareFixture {
    database: Database,
    agents: AgentRepository,
    printers: PrinterRepository,
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: String,
    serial: String,
}

impl FirmwareFixture {
    async fn new(database: Database, slug: &str) -> Self {
        let tenants = TenantRepository::new(database.clone());
        let agents = AgentRepository::new(database.clone());
        let printers = PrinterRepository::new(database.clone());
        let tenant = tenants.create(slug, "Firmware Tenant").await.unwrap();
        let agent = agents.create(tenant.id, "firmware-agent").await.unwrap();
        let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
        let serial = format!("serial-{printer_id}");
        Self {
            database,
            agents,
            printers,
            tenant_id: tenant.id,
            agent_id: agent.id,
            printer_id,
            serial,
        }
    }

    async fn claim(&self, session_id: &str) {
        self.agents
            .claim_online_session(
                self.tenant_id,
                self.agent_id,
                session_id,
                "test",
                "2026-07-12T00:00:00Z",
            )
            .await
            .unwrap();
    }

    async fn firmware(&self) -> PrinterFirmwareState {
        self.printers
            .get_with_live_status_for_tenant(self.tenant_id, &self.printer_id)
            .await
            .unwrap()
            .unwrap()
            .firmware
    }
}

fn firmware_module(name: &str, version: &str) -> PrinterFirmwareModule {
    PrinterFirmwareModule {
        name: name.to_owned(),
        software_version: Some(version.to_owned()),
        software_new_version: None,
        new_version: None,
        visible: None,
        product_name: None,
        serial_number: None,
        hardware_version: None,
        firmware_flag: None,
    }
}

fn upgrade_state(status: &str) -> PrinterUpgradeState {
    PrinterUpgradeState {
        status: Some(status.to_owned()),
        progress: Some("25".to_owned()),
        message: None,
        module: Some("ota".to_owned()),
        error_code: None,
        new_version_state: None,
        consistency_request: None,
        force_upgrade: None,
        display_state: None,
        ota_new_version_number: None,
        ams_new_version_number: None,
        ahb_new_version_number: None,
        new_versions: None,
        ams_firmware: None,
    }
}
