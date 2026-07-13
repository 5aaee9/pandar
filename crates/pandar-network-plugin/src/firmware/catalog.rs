use pandar_core::{FirmwareCatalogEntry, FirmwareCatalogTarget};
use serde::Serialize;

#[derive(Serialize)]
struct CatalogEnvelope<'a> {
    devices: [CatalogDevice<'a>; 1],
}

#[derive(Serialize)]
struct CatalogDevice<'a> {
    dev_id: &'a str,
    firmware: Vec<CatalogItem<'a>>,
    ams: Vec<AmsCatalog<'a>>,
}

#[derive(Serialize)]
struct AmsCatalog<'a> {
    firmware: Vec<CatalogItem<'a>>,
}

#[derive(Serialize)]
struct CatalogItem<'a> {
    version: &'a str,
    url: &'a str,
    description: &'a str,
}

pub fn firmware_catalog_json(dev_id: &str, entries: &[FirmwareCatalogEntry]) -> String {
    let mut firmware = Vec::new();
    let mut ams_firmware = Vec::new();
    for entry in entries.iter().filter(|entry| !entry.url.is_empty()) {
        let item = CatalogItem {
            version: &entry.version,
            url: &entry.url,
            description: &entry.description,
        };
        match entry.target {
            FirmwareCatalogTarget::Printer => firmware.push(item),
            FirmwareCatalogTarget::Ams => ams_firmware.push(item),
        }
    }
    let ams = (!ams_firmware.is_empty())
        .then_some(AmsCatalog {
            firmware: ams_firmware,
        })
        .into_iter()
        .collect();
    serde_json::to_string(&CatalogEnvelope {
        devices: [CatalogDevice {
            dev_id,
            firmware,
            ams,
        }],
    })
    .expect("typed Studio firmware catalog is serializable")
}
