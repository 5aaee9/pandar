use std::{collections::BTreeMap, fs, io, io::Cursor, path::PathBuf, time::Duration};

use anyhow::{Context, bail};
use futures_util::TryStreamExt;
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use zip::ZipArchive;

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/5aaee9/pandar/releases/latest";
const MAX_BUNDLE_BYTES: usize = 128 * 1024 * 1024;
const MAX_CHECKSUM_BYTES: usize = 4096;
const MAX_EXTRACTED_BYTES: u64 = 256 * 1024 * 1024;

const HOOK_DLL: &str = "pandar_studio_hook.dll";
const PLUGIN_DLL: &str = "pandar_network_plugin.dll";
const SOURCE_DLL: &str = "pandar_bambu_source.dll";

#[derive(Debug)]
pub(crate) struct StudioHookRelease {
    _temp: TempDir,
    pub(crate) hook_file: PathBuf,
    pub(crate) plugin_file: PathBuf,
    pub(crate) source_file: PathBuf,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

pub(crate) async fn download_latest_studio_hook_release(
    profile: &pandar_studio_profile::StudioProfile,
) -> anyhow::Result<StudioHookRelease> {
    let client = Client::builder()
        .user_agent("pandar-studio-hook")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()
        .context("build GitHub Release HTTP client")?;
    download_studio_hook_release(&client, LATEST_RELEASE_URL, profile).await
}

async fn download_studio_hook_release(
    client: &Client,
    release_url: &str,
    profile: &pandar_studio_profile::StudioProfile,
) -> anyhow::Result<StudioHookRelease> {
    let bundle_name = profile.hook_bundle_name();
    let checksum_name = format!("{bundle_name}.sha256");
    let release = client
        .get(release_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("request latest Pandar GitHub Release")?
        .error_for_status()
        .context("latest Pandar GitHub Release request failed")?
        .json::<GitHubRelease>()
        .await
        .context("decode latest Pandar GitHub Release")?;

    let assets = release
        .assets
        .into_iter()
        .map(|asset| (asset.name, asset.browser_download_url))
        .collect::<BTreeMap<_, _>>();
    let bundle_url = assets
        .get(&bundle_name)
        .with_context(|| format!("latest Pandar GitHub Release has no {bundle_name}"))?;
    let checksum_url = assets
        .get(&checksum_name)
        .with_context(|| format!("latest Pandar GitHub Release has no {checksum_name}"))?;

    let checksum = download_bounded(client, checksum_url, MAX_CHECKSUM_BYTES)
        .await
        .context("download Studio hook bundle checksum")?;
    let expected = parse_checksum(&checksum, &bundle_name)?;
    let bundle = download_bounded(client, bundle_url, MAX_BUNDLE_BYTES)
        .await
        .context("download Studio hook bundle")?;
    verify_checksum(&expected, &bundle)?;
    extract_bundle(bundle)
}

async fn download_bounded(client: &Client, url: &str, limit: usize) -> anyhow::Result<Vec<u8>> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("request GitHub Release asset {url}"))?
        .error_for_status()
        .with_context(|| format!("GitHub Release asset request failed for {url}"))?;
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        bail!("GitHub Release asset exceeds {limit} bytes");
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream
        .try_next()
        .await
        .context("read GitHub Release asset body")?
    {
        if body.len() + chunk.len() > limit {
            bail!("GitHub Release asset exceeds {limit} bytes");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn parse_checksum(body: &[u8], bundle_name: &str) -> anyhow::Result<String> {
    let text = std::str::from_utf8(body).context("checksum asset is not UTF-8")?;
    let mut fields = text.split_whitespace();
    let checksum = fields.next().context("checksum asset is empty")?;
    let filename = fields.next().context("checksum asset has no filename")?;
    if fields.next().is_some() || filename.trim_start_matches('*') != bundle_name {
        bail!("checksum asset must contain only the checksum for {bundle_name}");
    }
    if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("checksum asset does not contain a SHA-256 digest");
    }
    Ok(checksum.to_ascii_lowercase())
}

fn verify_checksum(expected: &str, bundle: &[u8]) -> anyhow::Result<()> {
    let actual = hex_sha256(bundle);
    if actual != expected {
        bail!("Studio hook bundle checksum mismatch: expected {expected}, got {actual}");
    }
    Ok(())
}

fn extract_bundle(bundle: Vec<u8>) -> anyhow::Result<StudioHookRelease> {
    let temp = tempfile::tempdir().context("create Studio hook release staging directory")?;
    let mut archive =
        ZipArchive::new(Cursor::new(bundle)).context("open Studio hook release ZIP")?;
    let expected = [HOOK_DLL, PLUGIN_DLL, SOURCE_DLL];
    if archive.len() != expected.len() {
        bail!("Studio hook release ZIP must contain exactly three files");
    }

    let mut extracted = BTreeMap::new();
    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("read Studio hook release ZIP entry {index}"))?;
        let name = entry.name().to_owned();
        if !expected.contains(&name.as_str()) || !entry.is_file() {
            bail!("unexpected Studio hook release ZIP entry {name}");
        }
        extracted_bytes = extracted_bytes
            .checked_add(entry.size())
            .context("Studio hook release ZIP extracted size overflow")?;
        if extracted_bytes > MAX_EXTRACTED_BYTES {
            bail!("Studio hook release ZIP exceeds {MAX_EXTRACTED_BYTES} extracted bytes");
        }
        let path = temp.path().join(&name);
        let mut output = fs::File::create(&path)
            .with_context(|| format!("create staged Studio hook file {}", path.display()))?;
        io::copy(&mut entry, &mut output)
            .with_context(|| format!("extract staged Studio hook file {name}"))?;
        extracted.insert(name, path);
    }

    let take = |name: &str| {
        extracted
            .get(name)
            .cloned()
            .with_context(|| format!("Studio hook release ZIP is missing {name}"))
    };
    Ok(StudioHookRelease {
        hook_file: take(HOOK_DLL)?,
        plugin_file: take(PLUGIN_DLL)?,
        source_file: take(SOURCE_DLL)?,
        _temp: temp,
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    fn bundle(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut output);
            for (name, body) in entries {
                zip.start_file(*name, SimpleFileOptions::default()).unwrap();
                zip.write_all(body).unwrap();
            }
            zip.finish().unwrap();
        }
        output.into_inner()
    }

    #[test]
    fn checksum_requires_exact_bundle_name() {
        let bundle_name = pandar_studio_profile::catalog()
            .default()
            .hook_bundle_name();
        let digest = "a".repeat(64);
        assert_eq!(
            parse_checksum(
                format!("{digest}  {bundle_name}\n").as_bytes(),
                &bundle_name,
            )
            .unwrap(),
            digest
        );
        assert!(parse_checksum(format!("{digest}  other.zip\n").as_bytes(), &bundle_name).is_err());
    }

    #[test]
    fn bundle_checksum_mismatch_is_rejected() {
        assert!(verify_checksum(&"0".repeat(64), b"bundle").is_err());
    }

    #[test]
    fn release_bundle_requires_exact_three_files() {
        let release = extract_bundle(bundle(&[
            (HOOK_DLL, b"hook"),
            (PLUGIN_DLL, b"plugin"),
            (SOURCE_DLL, b"source"),
        ]))
        .unwrap();
        assert_eq!(fs::read(release.hook_file).unwrap(), b"hook");
        assert_eq!(fs::read(release.plugin_file).unwrap(), b"plugin");
        assert_eq!(fs::read(release.source_file).unwrap(), b"source");

        assert!(extract_bundle(bundle(&[(HOOK_DLL, b"hook")])).is_err());
        assert!(
            extract_bundle(bundle(&[
                (HOOK_DLL, b"hook"),
                (PLUGIN_DLL, b"plugin"),
                (SOURCE_DLL, b"source"),
                ("extra.dll", b"extra"),
            ]))
            .is_err()
        );
    }

    #[tokio::test]
    async fn downloads_and_verifies_github_release_assets() {
        let profile = pandar_studio_profile::catalog().default();
        let bundle_name = profile.hook_bundle_name();
        let checksum_name = format!("{bundle_name}.sha256");
        let bundle = bundle(&[
            (HOOK_DLL, b"hook"),
            (PLUGIN_DLL, b"plugin"),
            (SOURCE_DLL, b"source"),
        ]);
        let checksum = format!("{}  {bundle_name}\n", hex_sha256(&bundle));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let release_body = format!(
            r#"{{"assets":[{{"name":"{bundle_name}","browser_download_url":"{base_url}/bundle"}},{{"name":"{checksum_name}","browser_download_url":"{base_url}/checksum"}}]}}"#
        );
        let server = thread::spawn(move || {
            for stream in listener.incoming().take(3) {
                let mut stream = stream.unwrap();
                let mut request = [0_u8; 2048];
                let length = stream.read(&mut request).unwrap();
                let request = std::str::from_utf8(&request[..length]).unwrap();
                let path = request.split_whitespace().nth(1).unwrap();
                let (status, body) = match path {
                    "/release" => ("200 OK", release_body.as_bytes()),
                    "/checksum" => ("200 OK", checksum.as_bytes()),
                    "/bundle" => ("200 OK", bundle.as_slice()),
                    _ => ("404 Not Found", &b"missing"[..]),
                };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
            }
        });

        let client = Client::builder().build().unwrap();
        let release =
            download_studio_hook_release(&client, &format!("{base_url}/release"), profile)
                .await
                .unwrap();
        assert_eq!(fs::read(release.hook_file).unwrap(), b"hook");
        assert_eq!(fs::read(release.plugin_file).unwrap(), b"plugin");
        assert_eq!(fs::read(release.source_file).unwrap(), b"source");
        server.join().unwrap();
    }
}
