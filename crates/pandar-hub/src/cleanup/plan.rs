use super::{CleanupCutoffs, sql::*};

#[derive(Clone, Copy)]
pub(super) enum CleanupCategory {
    Jobs,
    Artifacts,
    Commands,
    MachineEvents,
    AuditEvents,
    PluginLoginTickets,
    TenantTokens,
}

pub(super) struct CleanupPlan {
    cutoffs: CleanupCutoffs,
}

pub(super) struct CleanupSelection<'a> {
    pub(super) table: &'static str,
    pub(super) label: &'static str,
    pub(super) sql: &'static str,
    pub(super) binds: Vec<&'a str>,
}

impl CleanupPlan {
    pub(super) fn new(cutoffs: CleanupCutoffs) -> Self {
        Self { cutoffs }
    }

    pub(super) fn selection(&self, category: CleanupCategory) -> CleanupSelection<'_> {
        match category {
            CleanupCategory::Jobs => CleanupSelection {
                table: "jobs",
                label: "job",
                sql: JOB_SELECTION_SQL,
                binds: vec![&self.cutoffs.jobs],
            },
            CleanupCategory::Artifacts => CleanupSelection {
                table: "job_artifacts",
                label: "artifact",
                sql: ARTIFACT_SELECTION_SQL,
                binds: vec![&self.cutoffs.jobs, &self.cutoffs.jobs],
            },
            CleanupCategory::Commands => CleanupSelection {
                table: "commands",
                label: "command",
                sql: COMMAND_SELECTION_SQL,
                binds: vec![&self.cutoffs.commands, &self.cutoffs.commands],
            },
            CleanupCategory::MachineEvents => CleanupSelection {
                table: "machine_events",
                label: "machine event",
                sql: MACHINE_EVENT_SELECTION_SQL,
                binds: vec![&self.cutoffs.machine_events],
            },
            CleanupCategory::AuditEvents => CleanupSelection {
                table: "audit_events",
                label: "audit event",
                sql: AUDIT_SELECTION_SQL,
                binds: vec![
                    &self.cutoffs.audit,
                    &self.cutoffs.jobs,
                    &self.cutoffs.commands,
                    &self.cutoffs.jobs,
                ],
            },
            CleanupCategory::PluginLoginTickets => CleanupSelection {
                table: "plugin_login_tickets",
                label: "plugin login ticket",
                sql: PLUGIN_TICKET_SELECTION_SQL,
                binds: vec![
                    &self.cutoffs.plugin_tickets,
                    &self.cutoffs.plugin_tickets,
                    &self.cutoffs.plugin_tickets,
                ],
            },
            CleanupCategory::TenantTokens => CleanupSelection {
                table: "tenant_tokens",
                label: "tenant token",
                sql: TENANT_TOKEN_SELECTION_SQL,
                binds: vec![&self.cutoffs.tenant_tokens, &self.cutoffs.tenant_tokens],
            },
        }
    }
}

impl CleanupSelection<'_> {
    pub(super) fn delete_sql(&self) -> String {
        format!("DELETE FROM {} WHERE id IN ({})", self.table, self.sql)
    }
}
