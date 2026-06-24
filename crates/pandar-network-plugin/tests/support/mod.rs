use std::{
    io::Read,
    net::TcpStream,
    time::{Duration, Instant},
};

pub fn read_http_request_with_timeout(
    stream: &mut TcpStream,
    streaming_timeout: Option<Duration>,
) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    let headers_end = loop {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0, "client closed before sending request");
        request.extend_from_slice(&buffer[..read]);
        if let Some(pos) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let text = String::from_utf8_lossy(&request).to_string();
    let content_length = text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().unwrap())
    });
    if let Some(content_length) = content_length {
        while request.len() - headers_end < content_length {
            let read = stream.read(&mut buffer).unwrap();
            assert_ne!(read, 0, "client closed before sending full request body");
            request.extend_from_slice(&buffer[..read]);
        }
    } else {
        read_streaming_body(
            stream,
            &mut request,
            &mut buffer,
            headers_end,
            &text,
            streaming_timeout,
        );
    }
    String::from_utf8_lossy(&request).to_string()
}

pub fn request_body(request: &str) -> &str {
    request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("")
}

pub fn assert_multipart_print_request(request: &str) {
    assert!(
        request.contains("content-type: multipart/form-data; boundary="),
        "request did not use multipart/form-data: {request}"
    );
    for field in [
        "printer_id",
        "filename",
        "content_type",
        "plate_id",
        "use_ams",
        "flow_cali",
        "timelapse",
        "ams_mapping",
        "ams_mapping2",
        "file",
    ] {
        assert!(
            request.contains(&format!(r#"name="{field}""#)),
            "request did not contain multipart field {field}: {request}"
        );
    }
    let retired_json_artifact_field = ["artifact", "base64"].join("_");
    assert!(
        !request.contains(&retired_json_artifact_field),
        "request still contains retired JSON artifact field: {request}"
    );
}

pub fn assert_multipart_file_part(request: &str, filename: &str, bytes: &[u8]) {
    let body = request_body(request);
    assert!(
        body.contains(&format!(r#"name="file"; filename="{filename}""#)),
        "bad print file part: {body}"
    );
    assert!(
        body.as_bytes()
            .windows(bytes.len())
            .any(|window| window == bytes),
        "bad artifact bytes: {body}"
    );
}

fn read_streaming_body(
    stream: &mut TcpStream,
    request: &mut Vec<u8>,
    buffer: &mut [u8; 1024],
    headers_end: usize,
    headers: &str,
    streaming_timeout: Option<Duration>,
) {
    let transfer_encoding_chunked = headers.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"))
    });
    let multipart_end = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.eq_ignore_ascii_case("content-type") {
            return None;
        }
        let boundary = value
            .split(';')
            .find_map(|part| part.trim().strip_prefix("boundary="))?;
        Some(format!("--{}--", boundary.trim_matches('"')).into_bytes())
    });
    if !transfer_encoding_chunked && multipart_end.is_none() {
        return;
    }
    let Some(timeout) = streaming_timeout else {
        return;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let deadline = Instant::now() + timeout;
    loop {
        if multipart_end
            .as_ref()
            .is_some_and(|end| request[headers_end..].windows(end.len()).any(|w| w == end))
        {
            break;
        }
        if transfer_encoding_chunked && request[headers_end..].ends_with(b"\r\n0\r\n\r\n") {
            break;
        }
        match stream.read(buffer) {
            Ok(0) => break,
            Ok(read) => request.extend_from_slice(&buffer[..read]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && Instant::now() < deadline =>
            {
                continue;
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(error) => panic!("failed reading request body: {error}"),
        }
    }
}
