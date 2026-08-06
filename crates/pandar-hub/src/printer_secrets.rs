use std::{fmt, sync::Arc};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, Generate, KeyInit, Payload},
};
use anyhow::{Context, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use zeroize::Zeroizing;

use crate::{
    db::{ConnectionDialectExt, Database},
    entities::printers,
};

const ACCESS_CODE_KEY_ENV: &str = "PANDAR_PRINTER_ACCESS_CODE_KEY";
const ACCESS_CODE_ENVELOPE_PREFIX: &str = "v1:";
const AES_256_KEY_SIZE: usize = 32;
const AES_GCM_NONCE_SIZE: usize = 12;

#[derive(Clone)]
pub(crate) struct PrinterAccessCodeCipher {
    cipher: Arc<Aes256Gcm>,
}

impl fmt::Debug for PrinterAccessCodeCipher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PrinterAccessCodeCipher([REDACTED])")
    }
}

impl PrinterAccessCodeCipher {
    fn from_key_bytes(key: &[u8]) -> anyhow::Result<Self> {
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|_| anyhow::anyhow!("printer access-code key must decode to 32 bytes"))?;
        Ok(Self {
            cipher: Arc::new(cipher),
        })
    }

    fn from_config_value(value: Option<&str>) -> anyhow::Result<Self> {
        let value = value
            .filter(|value| !value.trim().is_empty())
            .with_context(|| format!("{ACCESS_CODE_KEY_ENV} is required"))?;
        let key = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(value.trim())
                .with_context(|| format!("decode {ACCESS_CODE_KEY_ENV} as unpadded base64url"))?,
        );
        if key.len() != AES_256_KEY_SIZE {
            bail!("{ACCESS_CODE_KEY_ENV} must decode to exactly 32 bytes");
        }
        Self::from_key_bytes(&key)
    }

    #[cfg(not(test))]
    fn from_env() -> anyhow::Result<Self> {
        Self::from_config_value(std::env::var(ACCESS_CODE_KEY_ENV).ok().as_deref())
    }

    pub(crate) fn encrypt(
        &self,
        tenant_id: &str,
        serial_number: &str,
        access_code: &str,
    ) -> anyhow::Result<String> {
        let nonce = Nonce::generate();
        let aad = access_code_aad(tenant_id, serial_number);
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: access_code.as_bytes(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| anyhow::anyhow!("encrypt printer access code"))?;
        let mut envelope = Vec::with_capacity(nonce.len() + ciphertext.len());
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&ciphertext);
        Ok(format!(
            "{ACCESS_CODE_ENVELOPE_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(envelope)
        ))
    }

    pub(crate) fn decrypt(
        &self,
        tenant_id: &str,
        serial_number: &str,
        envelope: &str,
    ) -> anyhow::Result<String> {
        let encoded = envelope
            .strip_prefix(ACCESS_CODE_ENVELOPE_PREFIX)
            .context("unsupported printer access-code envelope version")?;
        let envelope = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(encoded)
                .context("decode printer access-code envelope")?,
        );
        if envelope.len() <= AES_GCM_NONCE_SIZE {
            bail!("printer access-code envelope is truncated");
        }
        let (nonce, ciphertext) = envelope.split_at(AES_GCM_NONCE_SIZE);
        let nonce = <&Nonce<_>>::try_from(nonce).expect("nonce split at nonce size");
        let aad = access_code_aad(tenant_id, serial_number);
        let plaintext = self
            .cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| anyhow::anyhow!("authenticate printer access-code envelope"))?;
        String::from_utf8(plaintext).context("decode printer access code as UTF-8")
    }
}

fn access_code_aad(tenant_id: &str, serial_number: &str) -> String {
    format!("pandar:printer-access-code:v1\0{tenant_id}\0{serial_number}")
}

pub(crate) fn configured_printer_access_code_cipher() -> anyhow::Result<PrinterAccessCodeCipher> {
    #[cfg(test)]
    {
        PrinterAccessCodeCipher::from_key_bytes(&[0x42; AES_256_KEY_SIZE])
    }
    #[cfg(not(test))]
    {
        PrinterAccessCodeCipher::from_env()
    }
}

pub(crate) async fn migrate_printer_access_codes(
    database: &Database,
    cipher: &PrinterAccessCodeCipher,
) -> anyhow::Result<()> {
    let transaction = database
        .begin_write_transaction()
        .await
        .context("begin printer access-code encryption migration")?;
    let query = printers::Entity::find();
    let models = transaction
        .lock_for_update(query)
        .all(&transaction)
        .await
        .context("load printer access codes for encryption migration")?;
    let mut migrated = 0_u64;

    for model in models {
        match (&model.access_code, &model.access_code_encrypted) {
            (Some(_), Some(_)) => {
                bail!(
                    "printer {} has both plaintext and encrypted access codes",
                    model.id
                );
            }
            (Some(access_code), None) => {
                let envelope = cipher
                    .encrypt(&model.tenant_id, &model.serial_number, access_code)
                    .with_context(|| format!("encrypt access code for printer {}", model.id))?;
                let mut active: printers::ActiveModel = model.into();
                active.access_code = Set(None);
                active.access_code_encrypted = Set(Some(envelope));
                active
                    .update(&transaction)
                    .await
                    .context("persist encrypted printer access code")?;
                migrated += 1;
            }
            (None, Some(envelope)) => {
                cipher
                    .decrypt(&model.tenant_id, &model.serial_number, envelope)
                    .with_context(|| {
                        format!("validate encrypted access code for printer {}", model.id)
                    })?;
            }
            (None, None) => {}
        }
    }

    transaction
        .commit()
        .await
        .context("commit printer access-code encryption migration")?;
    if migrated > 0 {
        tracing::info!(migrated, "encrypted legacy printer access codes");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cipher(seed: u8) -> PrinterAccessCodeCipher {
        PrinterAccessCodeCipher::from_key_bytes(&[seed; AES_256_KEY_SIZE]).unwrap()
    }

    #[test]
    fn access_code_envelopes_are_versioned_randomized_and_authenticated() {
        let access_code_cipher = cipher(1);
        let first = access_code_cipher
            .encrypt("tenant", "serial", "12345678")
            .unwrap();
        let second = access_code_cipher
            .encrypt("tenant", "serial", "12345678")
            .unwrap();

        assert!(first.starts_with(ACCESS_CODE_ENVELOPE_PREFIX));
        assert_ne!(first, second);
        assert_eq!(
            access_code_cipher
                .decrypt("tenant", "serial", &first)
                .unwrap(),
            "12345678"
        );
        assert!(
            access_code_cipher
                .decrypt("other-tenant", "serial", &first)
                .is_err()
        );
        assert!(cipher(2).decrypt("tenant", "serial", &first).is_err());
    }

    #[test]
    fn access_code_key_configuration_requires_32_base64url_bytes() {
        assert!(PrinterAccessCodeCipher::from_config_value(None).is_err());
        assert!(PrinterAccessCodeCipher::from_config_value(Some("not-base64")).is_err());
        let short = URL_SAFE_NO_PAD.encode([0_u8; 31]);
        assert!(PrinterAccessCodeCipher::from_config_value(Some(&short)).is_err());
        let valid = URL_SAFE_NO_PAD.encode([0_u8; AES_256_KEY_SIZE]);
        PrinterAccessCodeCipher::from_config_value(Some(&valid)).unwrap();
    }

    #[test]
    fn access_code_envelopes_reject_unknown_versions() {
        let error = cipher(1)
            .decrypt("tenant", "serial", "v2:anything")
            .unwrap_err();
        assert!(format!("{error:#}").contains("unsupported"));
    }
}
