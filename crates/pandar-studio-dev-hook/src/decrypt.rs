use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::NoPadding};
use anyhow::{Context, bail};
use std::{fs, path::Path};

const HEADER_END_MARKER: &[u8] = b"END_HEADER";
const DEFAULT_CN_KEY: &[u8; 32] = b"OruMpXAHc7K8cgqLbJnRbAPOcQmFnH3J";
const DEFAULT_CN_IV: &[u8; 16] = b"Ln2XZ0u6SLGfhftc";

type Aes256CbcDecryptor = cbc::Decryptor<aes::Aes256>;

pub fn decrypt_bambu_studio_local_key_log(
    log_file: &Path,
    output_file: &Path,
) -> anyhow::Result<()> {
    let input = fs::read(log_file)
        .with_context(|| format!("read Bambu Studio encrypted log {}", log_file.display()))?;
    let ciphertext_offset = ciphertext_offset(&input)?;
    let mut ciphertext = input[ciphertext_offset..].to_vec();
    let trailing = ciphertext.len() % 16;
    if trailing != 0 {
        ciphertext.truncate(ciphertext.len() - trailing);
    }
    if ciphertext.is_empty() {
        bail!("Bambu Studio encrypted log has no aligned ciphertext");
    }

    let plaintext = Aes256CbcDecryptor::new(DEFAULT_CN_KEY.into(), DEFAULT_CN_IV.into())
        .decrypt_padded_mut::<NoPadding>(&mut ciphertext)
        .map_err(|_| anyhow::anyhow!("decrypt Bambu Studio log with local CN key"))?;
    let decoded = plaintext
        .iter()
        .copied()
        .filter(|byte| *byte != 0)
        .collect::<Vec<_>>();
    fs::write(output_file, decoded)
        .with_context(|| format!("write decrypted Bambu Studio log {}", output_file.display()))?;
    Ok(())
}

fn ciphertext_offset(input: &[u8]) -> anyhow::Result<usize> {
    let Some(marker_start) = input
        .windows(HEADER_END_MARKER.len())
        .position(|window| window == HEADER_END_MARKER)
    else {
        bail!("Bambu Studio encrypted log is missing END_HEADER marker");
    };

    let mut offset = marker_start + HEADER_END_MARKER.len();
    while offset < input.len() && matches!(input[offset], b'\r' | b'\n') {
        offset += 1;
    }
    Ok(offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{BlockEncryptMut, KeyIvInit};

    type Aes256CbcEncryptor = cbc::Encryptor<aes::Aes256>;

    #[test]
    fn decrypts_local_key_log_after_header_newline() {
        let temp = tempfile::tempdir().expect("tempdir");
        let log_file = temp.path().join("studio_enc_cn.log.0");
        let output_file = temp.path().join("studio.log");
        let mut block = [0_u8; 16];
        block[..12].copy_from_slice(b"hello studio");
        let mut ciphertext = block.to_vec();
        let encrypted = Aes256CbcEncryptor::new(DEFAULT_CN_KEY.into(), DEFAULT_CN_IV.into())
            .encrypt_padded_mut::<NoPadding>(&mut ciphertext, 16)
            .expect("encrypt fixture");

        let mut input = b"BEGIN_HEADER\n{}\nEND_HEADER\n".to_vec();
        input.extend_from_slice(encrypted);
        fs::write(&log_file, input).expect("write encrypted log");

        decrypt_bambu_studio_local_key_log(&log_file, &output_file).expect("decrypt log");

        assert_eq!(
            fs::read_to_string(output_file).expect("read output"),
            "hello studio"
        );
    }
}
