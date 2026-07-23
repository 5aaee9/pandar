use crate::machine::BambuPrinterEndpoint;

#[derive(Debug, Clone, Default)]
pub struct PrinterEndpointSecrets {
    access_codes: Vec<String>,
    hosts: Vec<String>,
}

impl PrinterEndpointSecrets {
    pub fn from_endpoints<'a>(
        endpoints: impl IntoIterator<Item = &'a BambuPrinterEndpoint>,
    ) -> Self {
        let mut secrets = Self::default();
        for endpoint in endpoints {
            secrets.record(endpoint);
        }
        secrets
    }

    pub fn record(&mut self, endpoint: &BambuPrinterEndpoint) {
        record_distinct(&mut self.access_codes, &endpoint.access_code);
        record_distinct(&mut self.hosts, &endpoint.host);
    }

    pub fn redact(&self, message: &str) -> String {
        let redacted = self.hosts.iter().fold(message.to_owned(), |message, host| {
            message.replace(host, "[REDACTED_PRINTER_HOST]")
        });
        redact_known_access_codes(&redacted, self.access_codes.clone())
    }
}

fn record_distinct(values: &mut Vec<String>, value: &str) {
    if value.is_empty() {
        return;
    }
    values.retain(|existing| existing != value);
    values.push(value.to_owned());
}

pub fn redact_access_code(message: &str, access_code: &str) -> String {
    if access_code.is_empty() {
        return message.to_owned();
    }
    message.replace(access_code, "[REDACTED_ACCESS_CODE]")
}

pub fn redact_known_access_codes(
    message: &str,
    access_codes: impl IntoIterator<Item = String>,
) -> String {
    access_codes
        .into_iter()
        .fold(message.to_owned(), |redacted, access_code| {
            redact_access_code(&redacted, &access_code)
        })
}
