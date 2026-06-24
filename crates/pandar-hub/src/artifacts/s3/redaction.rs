pub(super) fn redacted_s3_error<E>(operation: &'static str) -> impl FnOnce(E) -> anyhow::Error
where
    E: std::fmt::Debug + std::fmt::Display,
{
    move |err| {
        anyhow::anyhow!("S3 {operation} request failed")
            .context(redact_s3_error_evidence(&format!("{err}")))
            .context(redact_s3_error_evidence(&format!("{err:?}")))
    }
}

fn redact_s3_error_evidence(input: &str) -> String {
    redact_after_labels(
        input,
        &[
            "AWSAccessKeyId",
            "secret_access_key",
            "access_key_id",
            "access key",
            "access_key",
            "secret",
        ],
    )
}

fn redact_after_labels(input: &str, labels: &[&str]) -> String {
    let mut output = input.to_string();
    for label in labels {
        output = redact_after_label(&output, label);
    }
    output
}

fn redact_after_label(input: &str, label: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let lower = input.to_ascii_lowercase();
    let label_lower = label.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(offset) = lower[cursor..].find(&label_lower) {
        let start = cursor + offset;
        let value_start = start + label.len();
        if !is_label_boundary(input, start, value_start) {
            output.push_str(&input[cursor..start + 1]);
            cursor = start + 1;
            continue;
        }
        output.push_str(&input[cursor..value_start]);

        let mut token_start = value_start;
        let xml_closing_tag = xml_closing_tag(input, start);
        while token_start < input.len()
            && matches!(
                input.as_bytes()[token_start],
                b' ' | b'=' | b':' | b'"' | b'\'' | b'\\' | b'>'
            )
        {
            output.push(input.as_bytes()[token_start] as char);
            token_start += 1;
        }

        let mut token_end = token_start;
        while token_end < input.len() && !token_ended(input, token_end, xml_closing_tag.as_deref())
        {
            token_end += 1;
        }

        if token_end > token_start {
            output.push_str("[redacted]");
        }
        cursor = token_end;
    }
    output.push_str(&input[cursor..]);
    output
}

fn is_label_boundary(input: &str, start: usize, end: usize) -> bool {
    let before = start
        .checked_sub(1)
        .and_then(|index| input.as_bytes().get(index))
        .is_none_or(|byte| !is_identifier_byte(*byte));
    let after = input
        .as_bytes()
        .get(end)
        .is_none_or(|byte| !is_identifier_byte(*byte));
    before && after
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn token_ended(input: &str, index: usize, xml_closing_tag: Option<&str>) -> bool {
    matches!(
        input.as_bytes()[index],
        b' ' | b',' | b';' | b')' | b']' | b'}' | b'"' | b'\'' | b'\\'
    ) || xml_closing_tag.is_some_and(|tag| input[index..].starts_with(tag))
}

fn xml_closing_tag(input: &str, label_start: usize) -> Option<String> {
    let tag_end = input[label_start..].find('>')? + label_start;
    let tag_start = input[..tag_end].rfind('<')? + 1;
    let tag_name = &input[tag_start..tag_end];
    if tag_name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        Some(format!("</{tag_name}>"))
    } else {
        None
    }
}
