use super::*;
use axum::{
    body::Body,
    http::{Method, Request, header::AUTHORIZATION},
};
use http_body_util::BodyExt;
use std::io::{Cursor, Write};
use tower::ServiceExt;
use zip::{ZipWriter, write::SimpleFileOptions};

pub(super) async fn multipart_request_as(
    app: Router,
    method: Method,
    uri: &str,
    body: MultipartTestBody,
    token: &str,
) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={}", body.boundary),
                )
                .body(Body::from(body.body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&body).unwrap();

    (status, body)
}

pub(super) struct MultipartTestBody {
    boundary: String,
    body: Vec<u8>,
}

pub(super) fn multipart_print_body(
    printer_id: Option<&str>,
    file: Option<(&str, &str, &[u8])>,
    plate_id: i64,
) -> MultipartTestBody {
    multipart_print_body_with_mappings(printer_id, file, plate_id, None, None)
}

pub(super) fn multipart_print_body_with_mappings(
    printer_id: Option<&str>,
    file: Option<(&str, &str, &[u8])>,
    plate_id: i64,
    ams_mapping: Option<Value>,
    ams_mapping2: Option<Value>,
) -> MultipartTestBody {
    let boundary = format!("pandar-test-{}", uuid::Uuid::new_v4().simple());
    let mut body = Vec::new();
    let mut push_text = |name: &str, value: &str| {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    };

    if let Some(printer_id) = printer_id {
        push_text("printer_id", printer_id);
    }
    push_text("filename", "plate file.3mf");
    push_text("content_type", "model/3mf");
    push_text("plate_id", &plate_id.to_string());
    push_text("use_ams", "true");
    push_text("flow_cali", "false");
    push_text("timelapse", "true");
    if let Some(ams_mapping) = ams_mapping {
        push_text("ams_mapping", &ams_mapping.to_string());
    }
    if let Some(ams_mapping2) = ams_mapping2 {
        push_text("ams_mapping2", &ams_mapping2.to_string());
    }
    if let Some((filename, content_type, bytes)) = file {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    MultipartTestBody { boundary, body }
}

pub(super) fn multipart_print_body_file_first(
    file: (&str, &str, &[u8]),
    fields: &[(&str, &str)],
) -> MultipartTestBody {
    let boundary = format!("pandar-test-{}", uuid::Uuid::new_v4().simple());
    let mut body = Vec::new();
    let (filename, content_type, bytes) = file;
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");

    for (name, value) in fields {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    MultipartTestBody { boundary, body }
}

pub(super) fn multipart_print_body_with_fields(
    file: Option<(&str, &str, &[u8])>,
    fields: &[(&str, &str)],
) -> MultipartTestBody {
    let boundary = format!("pandar-test-{}", uuid::Uuid::new_v4().simple());
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    if let Some((filename, content_type, bytes)) = file {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    MultipartTestBody { boundary, body }
}

pub(super) fn slicer_metadata_fixture() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut bytes);
        let options = SimpleFileOptions::default();
        zip.start_file("Metadata/plate_1.gcode", options).unwrap();
        zip.write_all(b"").unwrap();
        zip.start_file("Metadata/slice_info.config", options)
            .unwrap();
        zip.write_all(
            br##"
            <config>
              <plate index="1" prediction="120" weight="4.5">
                <object name="calibration cube"/>
                <filament id="1" type="PLA" color="#00ff00" used_g="4.5" used_m="1.2"/>
              </plate>
            </config>
            "##,
        )
        .unwrap();
        zip.finish().unwrap();
    }
    bytes.into_inner()
}
