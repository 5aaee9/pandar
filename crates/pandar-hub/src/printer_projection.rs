use pandar_core::{AgentId, TenantId};

use crate::{AppState, cluster::HubControlMessage, metrics::ControlPlaneMetric};

impl AppState {
    pub async fn publish_printer_projection_change(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        serial_number: &str,
    ) {
        if let Err(err) = self
            .control_plane()
            .publish(HubControlMessage::PrinterProjectionChange {
                tenant_id: tenant_id.to_string(),
                printer_id: printer_id.to_owned(),
                serial_number: serial_number.to_owned(),
            })
            .await
        {
            self.metrics()
                .record_control_plane(ControlPlaneMetric::PublishFailed);
            self.printer_events().invalidate_epoch();
            tracing::error!(
                error = %format!("{err:#}"),
                "failed to publish printer projection change control message"
            );
        } else {
            self.metrics()
                .record_control_plane(ControlPlaneMetric::PublishOk);
        }
    }

    pub async fn publish_printer_projection_change_for_serial(
        &self,
        tenant_id: TenantId,
        serial_number: &str,
    ) {
        let printer = match self
            .printers()
            .get_by_serial_for_tenant(tenant_id, serial_number)
            .await
        {
            Ok(Some(printer)) => printer,
            Ok(None) => return,
            Err(err) => {
                self.printer_events().invalidate_epoch();
                tracing::error!(
                    error = %format!("{err:#}"),
                    "failed to resolve printer serial for projection change"
                );
                return;
            }
        };
        self.publish_printer_projection_change(tenant_id, &printer.id, &printer.serial_number)
            .await;
    }

    pub async fn publish_agent_printers_projection_changes(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) {
        let Some(printers) = self
            .agent_printers_for_projection(tenant_id, agent_id)
            .await
        else {
            return;
        };
        for printer in printers {
            self.publish_printer_projection_change(tenant_id, &printer.id, &printer.serial_number)
                .await;
        }
    }

    pub async fn publish_agent_printers_local_projection_changes(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) {
        let Some(printers) = self
            .agent_printers_for_projection(tenant_id, agent_id)
            .await
        else {
            return;
        };
        for printer in printers {
            self.printer_events()
                .publish_local_projection_change(
                    tenant_id,
                    crate::printer_events::PrinterProjectionChange {
                        printer_id: printer.id,
                        serial_number: printer.serial_number,
                    },
                )
                .await;
        }
    }

    async fn agent_printers_for_projection(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> Option<Vec<pandar_core::Printer>> {
        match self.printers().list_for_tenant(tenant_id).await {
            Ok(printers) => Some(
                printers
                    .into_iter()
                    .filter(|printer| printer.agent_id == agent_id)
                    .collect(),
            ),
            Err(err) => {
                self.printer_events().invalidate_epoch();
                tracing::error!(
                    error = %format!("{err:#}"),
                    "failed to list tenant printers for projection changes"
                );
                None
            }
        }
    }
}
