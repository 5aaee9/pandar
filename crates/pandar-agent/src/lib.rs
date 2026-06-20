use clap::Parser;

#[derive(Debug, Clone, Parser, PartialEq, Eq)]
#[command(
    name = "pandar-agent",
    about = "Connects local Bambu printers to pandar-hub"
)]
pub struct AgentConfig {
    #[arg(
        long,
        env = "PANDAR_HUB_GRPC_URL",
        default_value = "http://127.0.0.1:50051"
    )]
    pub hub_grpc_url: String,
    #[arg(long, env = "PANDAR_AGENT_NAME", default_value = "local-agent")]
    pub agent_name: String,
}

pub trait BambuMachineGateway {
    fn label(&self) -> &str;
}

pub struct ReferenceBackedGateway {
    label: String,
}

impl ReferenceBackedGateway {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl BambuMachineGateway for ReferenceBackedGateway {
    fn label(&self) -> &str {
        &self.label
    }
}

pub fn startup_summary(config: &AgentConfig) -> String {
    format!(
        "agent {} will connect to {}",
        config.agent_name, config.hub_grpc_url
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_cli_config() {
        let config = AgentConfig::parse_from([
            "pandar-agent",
            "--hub-grpc-url",
            "http://hub.internal:50051",
            "--agent-name",
            "garage",
        ]);

        assert_eq!(config.hub_grpc_url, "http://hub.internal:50051");
        assert_eq!(config.agent_name, "garage");
    }

    #[test]
    fn startup_summary_names_hub_and_agent() {
        let config = AgentConfig {
            hub_grpc_url: "http://hub.internal:50051".to_owned(),
            agent_name: "garage".to_owned(),
        };

        assert_eq!(
            startup_summary(&config),
            "agent garage will connect to http://hub.internal:50051"
        );
    }
}
