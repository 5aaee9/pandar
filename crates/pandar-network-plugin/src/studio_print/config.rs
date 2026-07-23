use std::{fs::File, io::Read, path::Path};

use anyhow::{Context, anyhow, bail};
use quick_xml::{Reader, XmlVersion, events::Event};
use zip::ZipArchive;

const MAX_SLICE_INFO_BYTES: u64 = 1024 * 1024;

pub(super) fn diagnostic_category(error: &anyhow::Error) -> &'static str {
    if error.downcast_ref::<std::io::Error>().is_some() {
        "io"
    } else if error.downcast_ref::<zip::result::ZipError>().is_some() {
        "archive"
    } else if error.downcast_ref::<quick_xml::Error>().is_some()
        || error
            .downcast_ref::<quick_xml::events::attributes::AttrError>()
            .is_some()
    {
        "xml"
    } else if error.downcast_ref::<std::num::ParseIntError>().is_some() {
        "plate_index"
    } else {
        "contract"
    }
}

pub(super) fn plate_index(path: &Path) -> anyhow::Result<u32> {
    let file = File::open(path).context("open Studio config 3MF")?;
    let mut archive = ZipArchive::new(file).context("decode Studio config 3MF")?;
    let mut entry = archive
        .by_name("Metadata/slice_info.config")
        .context("locate Studio slice_info.config")?;
    if entry.size() > MAX_SLICE_INFO_BYTES {
        bail!("Studio slice_info.config exceeds the metadata size limit");
    }
    let mut contents = String::new();
    entry
        .read_to_string(&mut contents)
        .context("read Studio slice_info.config")?;
    parse_plate_index(&contents)
}

fn parse_plate_index(contents: &str) -> anyhow::Result<u32> {
    let mut reader = Reader::from_str(contents);
    let mut inside_plate = false;
    let mut plate_index = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) if event.name().as_ref() == b"plate" => {
                inside_plate = true;
            }
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if inside_plate && event.name().as_ref() == b"metadata" =>
            {
                let mut key = None;
                let mut value = None;
                for attribute in event.attributes() {
                    let attribute = attribute.context("decode Studio config XML attribute")?;
                    let decoded = attribute
                        .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
                        .context("decode Studio config XML value")?
                        .into_owned();
                    match attribute.key.as_ref() {
                        b"key" => key = Some(decoded),
                        b"value" => value = Some(decoded),
                        _ => {}
                    }
                }
                if key.as_deref() == Some("index") {
                    let value = value
                        .ok_or_else(|| anyhow!("Studio config plate index has no value"))?
                        .parse::<u32>()
                        .context("parse Studio config plate index")?;
                    if value == 0 || plate_index.replace(value).is_some() {
                        bail!("Studio config must contain one positive plate index");
                    }
                }
            }
            Ok(Event::End(event)) if event.name().as_ref() == b"plate" => inside_plate = false,
            Ok(Event::Eof) => break,
            Err(error) => return Err(error).context("parse Studio slice_info.config"),
            _ => {}
        }
    }
    plate_index.ok_or_else(|| anyhow!("Studio config has no plate index"))
}
