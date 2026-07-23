#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeTarget {
    LinuxAmd64,
    WindowsAmd64,
}

impl NativeTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::LinuxAmd64 => "linux-amd64",
            Self::WindowsAmd64 => "windows-amd64",
        }
    }
}

pub(crate) fn validate_current_host(label: &str) -> Result<NativeTarget, String> {
    validate_host(label, std::env::consts::OS, std::env::consts::ARCH)
}

fn validate_host(label: &str, os: &str, arch: &str) -> Result<NativeTarget, String> {
    let target = match label {
        "linux-amd64" => NativeTarget::LinuxAmd64,
        "windows-amd64" => NativeTarget::WindowsAmd64,
        _ => return Err(format!("unsupported native release label {label}")),
    };
    let expected = match target {
        NativeTarget::LinuxAmd64 => ("linux", "x86_64"),
        NativeTarget::WindowsAmd64 => ("windows", "x86_64"),
    };
    if (os, arch) != expected {
        return Err(format!(
            "release label {label} requires native host {}/{}, got {os}/{arch}",
            expected.0, expected.1
        ));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::{NativeTarget, validate_host};

    #[test]
    fn windows_amd64_requires_real_windows_x86_64_host() {
        assert!(validate_host("windows-amd64", "linux", "x86_64").is_err());
        assert!(validate_host("windows-amd64", "windows", "aarch64").is_err());
        assert_eq!(
            validate_host("windows-amd64", "windows", "x86_64").unwrap(),
            NativeTarget::WindowsAmd64
        );
    }

    #[test]
    fn linux_amd64_requires_real_linux_x86_64_host() {
        assert!(validate_host("linux-amd64", "windows", "x86_64").is_err());
        assert!(validate_host("linux-amd64", "linux", "aarch64").is_err());
        assert_eq!(
            validate_host("linux-amd64", "linux", "x86_64").unwrap(),
            NativeTarget::LinuxAmd64
        );
    }
}
