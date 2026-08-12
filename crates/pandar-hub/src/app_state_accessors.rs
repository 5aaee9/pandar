use crate::{
    AppState,
    repositories::{
        AgentRepository, AuditEventRepository, AuthRepository, CommandRepository, JobRepository,
        MaterialRepository, PersonalPresetRepository, PrinterEventTicketRepository,
        PrinterRepository, TenantRepository,
    },
};

impl AppState {
    pub fn tenants(&self) -> &TenantRepository {
        &self.tenants
    }

    pub fn auth(&self) -> &AuthRepository {
        &self.auth
    }

    pub fn audit_events(&self) -> &AuditEventRepository {
        &self.audit_events
    }

    pub fn agents(&self) -> &AgentRepository {
        &self.agents
    }

    pub fn printers(&self) -> &PrinterRepository {
        &self.printers
    }

    pub fn commands(&self) -> &CommandRepository {
        &self.commands
    }

    pub fn jobs(&self) -> &JobRepository {
        &self.jobs
    }

    pub fn materials(&self) -> &MaterialRepository {
        &self.materials
    }

    pub fn personal_presets(&self) -> &PersonalPresetRepository {
        &self.personal_presets
    }

    pub fn printer_event_tickets(&self) -> &PrinterEventTicketRepository {
        &self.printer_event_tickets
    }
}
