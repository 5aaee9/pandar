use std::{
    ffi::{OsStr, OsString},
    fs,
    fs::File,
    io::{Read, Seek},
    path::Path,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::archive::sha256_hex;

#[derive(Debug)]
pub(crate) struct ProbeReport {
    pub plugin_sha256: String,
}

pub(crate) fn run_packaged_cli(cli: &Path, timeout: Duration) -> Result<(), String> {
    let cli = fs::canonicalize(cli).map_err(|error| format!("resolve staged CLI: {error}"))?;
    let output = run_native(&cli, &[OsString::from("--help")], timeout, "packaged CLI")?;
    require_success(
        output,
        timeout,
        "packaged CLI --help",
        &path_redactions(&cli),
    )
}

pub(crate) fn run_abi_probe(
    probe: &Path,
    probe_args: &[String],
    plugin: &Path,
    timeout: Duration,
) -> Result<ProbeReport, String> {
    if probe_args
        .iter()
        .any(|arg| arg == "--plugin" || arg.starts_with("--plugin="))
    {
        return Err("ABI probe --plugin is reserved for the staged artifact".to_owned());
    }
    let probe = fs::canonicalize(probe)
        .map_err(|error| format!("resolve native ABI probe executable: {error}"))?;
    let plugin = fs::canonicalize(plugin)
        .map_err(|error| format!("resolve staged plugin artifact: {error}"))?;
    let before = sha256_hex(&plugin)?;
    let mut args = probe_args.iter().map(OsString::from).collect::<Vec<_>>();
    args.push(OsString::from("--plugin"));
    args.push(plugin.as_os_str().to_owned());

    let output = run_native(&probe, &args, timeout, "native packaged-plugin ABI probe")?;
    let after = sha256_hex(&plugin)
        .map_err(|error| format!("ABI probe did not preserve staged plugin: {error}"))?;
    if after != before {
        return Err("ABI probe mutated the staged plugin artifact".to_owned());
    }
    let mut redactions = path_redactions(&plugin);
    redactions.extend(path_redactions(&probe));
    redactions.extend(probe_args.iter().filter(|arg| !arg.is_empty()).cloned());
    require_success(
        output,
        timeout,
        "native packaged-plugin ABI probe",
        &redactions,
    )?;
    Ok(ProbeReport {
        plugin_sha256: before,
    })
}

enum Completion {
    Exited(ExitStatus),
    TimedOut,
}

struct NativeOutput {
    completion: Completion,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_native(
    program: &Path,
    args: &[OsString],
    timeout: Duration,
    context: &str,
) -> Result<NativeOutput, String> {
    let mut stdout = tempfile::tempfile()
        .map_err(|error| format!("create {context} stdout capture: {error}"))?;
    let mut stderr = tempfile::tempfile()
        .map_err(|error| format!("create {context} stderr capture: {error}"))?;
    let mut child = Command::new(program)
        .args(args.iter().map(OsStr::new))
        .stdin(Stdio::null())
        .stdout(
            stdout
                .try_clone()
                .map_err(|error| format!("clone {context} stdout capture: {error}"))?,
        )
        .stderr(
            stderr
                .try_clone()
                .map_err(|error| format!("clone {context} stderr capture: {error}"))?,
        )
        .spawn()
        .map_err(|error| format!("start {context}: {error}"))?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("poll {context}: {error}"))?
        {
            return finish_output(
                Completion::Exited(status),
                &mut stdout,
                &mut stderr,
                context,
            );
        }
        if Instant::now() >= deadline {
            if let Some(status) = child
                .try_wait()
                .map_err(|error| format!("poll {context} at timeout: {error}"))?
            {
                return finish_output(
                    Completion::Exited(status),
                    &mut stdout,
                    &mut stderr,
                    context,
                );
            }
            child
                .kill()
                .map_err(|error| format!("terminate timed-out {context}: {error}"))?;
            child
                .wait()
                .map_err(|error| format!("reap timed-out {context}: {error}"))?;
            return finish_output(Completion::TimedOut, &mut stdout, &mut stderr, context);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn read_bounded(stream: &mut File) -> Result<Vec<u8>, String> {
    const LIMIT: usize = 64 * 1024;
    stream
        .rewind()
        .map_err(|error| format!("rewind process diagnostics: {error}"))?;
    let mut captured = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = stream
            .read(&mut buffer)
            .map_err(|error| format!("read process diagnostics: {error}"))?;
        if count == 0 {
            break;
        }
        captured.extend_from_slice(&buffer[..count.min(LIMIT - captured.len())]);
        if captured.len() == LIMIT {
            break;
        }
    }
    Ok(captured)
}

fn finish_output(
    completion: Completion,
    stdout: &mut File,
    stderr: &mut File,
    context: &str,
) -> Result<NativeOutput, String> {
    Ok(NativeOutput {
        completion,
        stdout: read_bounded(stdout)
            .map_err(|error| format!("capture {context} stdout: {error}"))?,
        stderr: read_bounded(stderr)
            .map_err(|error| format!("capture {context} stderr: {error}"))?,
    })
}

fn require_success(
    output: NativeOutput,
    timeout: Duration,
    context: &str,
    redactions: &[String],
) -> Result<(), String> {
    let diagnostic = redacted_diagnostic(&output, redactions);
    let suffix = if diagnostic.is_empty() {
        String::new()
    } else {
        format!(": {diagnostic}")
    };
    match output.completion {
        Completion::Exited(status) if status.success() => Ok(()),
        Completion::Exited(status) => Err(format!("{context} exited with {status}{suffix}")),
        Completion::TimedOut => Err(format!(
            "{context} timed out after {} ms{suffix}",
            timeout.as_millis()
        )),
    }
}

fn redacted_diagnostic(output: &NativeOutput, redactions: &[String]) -> String {
    let mut diagnostic = format!(
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for value in redactions {
        diagnostic = diagnostic.replace(value, "<redacted>");
    }
    diagnostic
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn path_redactions(path: &Path) -> Vec<String> {
    let mut values = vec![path.display().to_string()];
    if let Some(parent) = path.parent() {
        values.push(parent.display().to_string());
    }
    values
        .iter()
        .filter_map(|value| value.strip_prefix(r"\\?\").map(str::to_owned))
        .collect::<Vec<_>>()
        .into_iter()
        .chain(values)
        .filter(|value| !value.is_empty())
        .collect()
}

#[cfg(test)]
mod tests;
