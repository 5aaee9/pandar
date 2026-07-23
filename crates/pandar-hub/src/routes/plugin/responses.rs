pub(crate) fn redact_artifact_error(message: &str) -> String {
    message
        .lines()
        .map(|line| {
            if line.contains("artifact directory ")
                || line.contains("artifact file ")
                || line.contains("artifact storage path ")
            {
                line.split_once("artifact")
                    .map(|(prefix, suffix)| {
                        format!("{prefix}artifact{}", redact_artifact_path(suffix))
                    })
                    .unwrap_or_else(|| line.to_owned())
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_artifact_path(suffix: &str) -> String {
    for marker in [" directory ", " file ", " storage path "] {
        if let Some((prefix, _)) = suffix.split_once(marker) {
            return format!("{prefix}{marker}[redacted]");
        }
    }
    suffix.to_owned()
}
