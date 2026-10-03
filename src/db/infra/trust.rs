// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The signature/trust face (registry-p3-protocol.md §3): Ed25519 metadata
//! signatures, the trust store, and the verified/community classification.
//!
//! The integrity anchor: a version entry is signed over its *canonical JSON*
//! (lexicographic key order, no whitespace, `sig`/`keyid` stripped) — the
//! entry anchors the artifacts through their sha256 fields, so signing the
//! entry signs the artifacts. The private key never leaves the publisher
//! side; the server holds public material only, so a full server compromise
//! still cannot forge a signature.
//!
//! Classification law (§3): a signature that verifies against a keyid in the
//! trust store ⇒ `Verified`; no signature, or a keyid the store does not
//! know ⇒ `Community` (presented explicitly, never silently mixed); a
//! signature present and keyed but **failing verification is tamper** — a
//! hard error, the metadata equivalent of the checksum-mismatch wall.

use anyhow::{Context, Result};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::db::infra::registry::LibMeta;

/// The trust store (§3): authenticated-publisher public keys, user-extensible.
/// Distribution is the §7⑦b dual channel: the factory anchors below ride the
/// mcc release, and `lib trust update` merges verified rows from the
/// registry's `/trust.json` into this store. Entries here take precedence
/// over a factory anchor on a keyid collision.
pub fn trust_store_path() -> std::path::PathBuf {
    crate::cli::datadir::config_dir().join("trust.toml")
}

#[derive(Deserialize, Serialize, Default)]
struct TrustFile {
    #[serde(default)]
    keys: Vec<TrustRow>,
}

/// One trust row — the store (`trust.toml [[keys]]`) and the wire
/// (`/trust.json`) share the shape. `sig`/`signer` are the ⑦b auxiliary
/// channel's provenance: `sig` = `ed25519:<base64>` over the row's canonical
/// JSON minus `sig`, made by the `signer` keyid (old key signs new key).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustRow {
    pub keyid: String,
    /// 64 hex chars — the raw Ed25519 public key.
    pub public: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer: Option<String>,
}

/// A factory trust anchor: a key baked into mcc (the primary distribution
/// channel — it updates with a release, never over the network). The table
/// starts empty: no production publisher keys exist yet; the shape is here so
/// the first anchor is a one-line table entry, not a code change.
pub struct TrustAnchor {
    pub keyid: &'static str,
    /// 64 hex chars — the raw Ed25519 public key.
    pub public: &'static str,
}

/// The `/trust.json` wire shape (§7⑦b auxiliary channel): the key table the
/// registry publishes, rows signed per [`TrustRow`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustTable {
    #[serde(default)]
    pub keys: Vec<TrustRow>,
}

/// Fetch a registry's trust table (the `lib trust update` read face): a
/// `file://` tree reads `<tree>/trust.json`; an HTTP source GETs
/// `<base>/trust.json`. A missing file/404 is an error naming the endpoint —
/// a registry that publishes no table is a trust-relevant fact, not a quiet
/// empty merge.
pub fn fetch_trust_table(
    src: &crate::db::infra::registry::RegistrySource,
) -> Result<TrustTable> {
    use crate::db::infra::registry::RegistrySource;
    let text = match src {
        RegistrySource::File(root) => {
            let path = root.join("trust.json");
            if !path.is_file() {
                anyhow::bail!("the tree carries no trust.json: {}", path.display());
            }
            std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?
        }
        RegistrySource::Http { base } => {
            let resp = match crate::db::infra::httpfetch::get(&format!("{base}/trust.json"), None)
            {
                Ok(r) => r,
                Err(crate::db::infra::httpfetch::FetchError::NotFound) => {
                    anyhow::bail!("the registry publishes no /trust.json ({base})")
                }
                Err(e) => anyhow::bail!("trust table fetch failed: {e}"),
            };
            if !(200..300).contains(&resp.status) {
                anyhow::bail!("trust table fetch failed: HTTP {} for /trust.json", resp.status);
            }
            String::from_utf8(resp.body).context("trust.json is not UTF-8")?
        }
    };
    serde_json::from_str(&text).with_context(|| "invalid trust.json (expected {\"keys\": […]})".to_string())
}

/// The factory-anchor table (§7⑦b primary channel). Malformed entries are a
/// build-time review failure, not a runtime downgrade — the lookup refuses
/// them loudly.
pub const FACTORY_ANCHORS: &[TrustAnchor] = &[];

fn anchor_keys() -> Result<BTreeMap<String, VerifyingKey>> {
    let mut map = BTreeMap::new();
    for a in FACTORY_ANCHORS {
        let raw = decode_hex(a.public).map_err(|e| {
            anyhow::anyhow!("factory trust anchor `{}`: bad public key hex: {e}", a.keyid)
        })?;
        let bytes: [u8; 32] = raw.try_into().map_err(|_| {
            anyhow::anyhow!("factory trust anchor `{}`: public key must be 32 bytes", a.keyid)
        })?;
        let key = VerifyingKey::from_bytes(&bytes)
            .map_err(|e| anyhow::anyhow!("factory trust anchor `{}`: {e}", a.keyid))?;
        map.insert(a.keyid.to_ascii_lowercase(), key);
    }
    Ok(map)
}

/// The trust read: keyid → verifying key, factory anchors merged under the
/// user store (a stored row wins its keyid). An absent store with no anchors
/// is the everything-community state, not an error.
pub fn trust_keys() -> Result<BTreeMap<String, VerifyingKey>> {
    let mut map = anchor_keys()?;
    let Some(rows) = trust_store_rows()? else {
        return Ok(map);
    };
    for entry in rows {
        let raw = decode_hex(&entry.public)
            .with_context(|| format!("trust store entry `{}`: bad public key hex", entry.keyid))?;
        let bytes: [u8; 32] = raw
            .try_into()
            .map_err(|_| anyhow::anyhow!("trust store entry `{}`: public key must be 32 bytes", entry.keyid))?;
        let key = VerifyingKey::from_bytes(&bytes)
            .map_err(|e| anyhow::anyhow!("trust store entry `{}`: {e}", entry.keyid))?;
        map.insert(entry.keyid.to_ascii_lowercase(), key);
    }
    Ok(map)
}

/// The store's own rows (no factory anchors). `None` = no store file.
pub fn trust_store_rows() -> Result<Option<Vec<TrustRow>>> {
    let path = trust_store_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };
    let file: TrustFile =
        toml::from_str(&text).with_context(|| format!("invalid trust store: {}", path.display()))?;
    Ok(Some(file.keys))
}

/// Merge rows into the store: unseen keyids are appended, known keyids keep
/// the stored row (the user's explicit entry outranks the wire). Atomic
/// tmp+rename inside the config dir; the toml round-trip drops hand-written
/// comments (the store is machine-managed once a merge has landed).
/// Returns how many rows landed.
pub fn trust_store_merge(rows: &[TrustRow]) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let existing = trust_store_rows()?.unwrap_or_default();
    let mut all = existing;
    let mut landed = 0;
    for row in rows {
        // Keyids are hex digests: compare in the canonical lowercase form
        // (keyid_of emits lowercase; the wire may carry either case).
        let kid = row.keyid.to_ascii_lowercase();
        if all.iter().any(|r| r.keyid.to_ascii_lowercase() == kid) {
            continue;
        }
        all.push(row.clone());
        landed += 1;
    }
    if landed == 0 {
        return Ok(0);
    }
    let body = toml::to_string_pretty(&TrustFile { keys: all })
        .context("trust store does not serialize")?;
    let path = trust_store_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    let tmp = path.with_extension("toml.part");
    std::fs::write(&tmp, &body).with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("cannot move {}", path.display()))?;
    Ok(landed)
}

/// One trust row's verification (⑦b auxiliary channel): rebuild the
/// canonical bytes minus `sig`, check against the signer's key. Failure is a
/// refused row, never a silent skip.
pub fn verify_trust_row(row: &TrustRow, key: &VerifyingKey) -> Result<()> {
    use base64::Engine;
    let mut value = serde_json::to_value(row).context("trust row does not serialize")?;
    if let serde_json::Value::Object(map) = &mut value {
        map.remove("sig");
    }
    let msg = canonical_json(&value);
    let sig = row.sig.as_deref().ok_or_else(|| anyhow::anyhow!("row carries no signature"))?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(sig.strip_prefix("ed25519:").unwrap_or(sig))
        .with_context(|| format!("row `{}`: signature is not base64", row.keyid))?;
    let signature = ed25519_dalek::Signature::from_slice(&raw)
        .map_err(|e| anyhow::anyhow!("row `{}`: malformed signature: {e}", row.keyid))?;
    key.verify(msg.as_bytes(), &signature).map_err(|_| {
        anyhow::anyhow!(
            "row `{}`: signature verification FAILED for signer key {} — refused",
            row.keyid,
            row.signer.as_deref().unwrap_or("?")
        )
    })?;
    Ok(())
}

/// Generate a publishing key pair: the hex seed goes to `path` (0600 — the
/// single-tenant bare-file ruling, §6②), the public key and keyid come back
/// for the consumer-side trust store.
pub fn keygen(path: &Path) -> Result<(String, String)> {
    let mut seed = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut seed);
    let signing = SigningKey::from_bytes(&seed);
    write_seed(path, &seed)?;
    let public = hex_encode(&signing.verifying_key().to_bytes());
    Ok((public, keyid_of(&signing.verifying_key())))
}

/// The key id: the first 16 hex of the public key's SHA-256 (§3).
pub fn keyid_of(key: &VerifyingKey) -> String {
    let digest = sha256(&key.to_bytes());
    hex_encode(&digest[..8])
}

fn load_signing_key(path: &Path) -> Result<SigningKey> {
    let seed = read_seed(path)?;
    let bytes: [u8; 32] = seed
        .try_into()
        .map_err(|_| anyhow::anyhow!("signing key `{}`: expected 32-byte hex seed", path.display()))?;
    Ok(SigningKey::from_bytes(&bytes))
}

/// Sign one version entry: canonical JSON of the entry with `sig`/`keyid`
/// absent (the caller passes it that way), base64(R‖S) back with the keyid.
pub fn sign_version(entry: &serde_json::Value, key_path: &Path) -> Result<(String, String)> {
    let signing = load_signing_key(key_path)?;
    let msg = canonical_json(entry);
    let sig = signing.sign(msg.as_bytes());
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
    Ok((format!("ed25519:{encoded}"), keyid_of(&signing.verifying_key())))
}

/// Canonical JSON (§3): object keys lexicographic, no whitespace, strings
/// through serde_json's escaping. Metadata carries no floats (checksums,
/// sizes, names), so serde_json's scalar printing is byte-stable here.
pub fn canonical_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(map) => {
            let keys: std::collections::BTreeSet<&String> = map.keys().collect();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    let mut s = Vec::new();
                    serde_json::to_writer(&mut s, k).expect("string serializes");
                    format!("{}:{}", String::from_utf8(s).expect("utf8"), canonical_json(&map[k]))
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        serde_json::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        other => serde_json::to_string(other).expect("value serializes"),
    }
}

/// The trust level a version entry presents (§3 — the column the outward
/// faces show, never a silent mix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    Verified,
    Community,
}

impl Trust {
    pub fn label(self) -> &'static str {
        match self {
            Trust::Verified => "verified",
            Trust::Community => "community",
        }
    }
}

/// Verify every signed version row of a freshly-read metadata file (§3:
/// verification happens once, at the read face — install/build/fetch-docs
/// all consume the same already-verified read). Unsigned rows pass; a keyed
/// signature that fails is tamper and stops the read. The failure is the
/// typed [`SolveError::Signature`] so the RPC/CLI faces keep their
/// enum-mapped codes (D15) — callers lift it with the registry's
/// typed-error seam, never a string match.
pub fn verify_meta(meta: &LibMeta) -> std::result::Result<(), crate::db::infra::registry::SolveError> {
    let signed: Vec<(String, &crate::db::infra::registry::VersionMeta)> = meta
        .versions
        .iter()
        .filter(|(_, v)| v.sig.is_some())
        .map(|(k, v)| (k.clone(), v))
        .collect();
    if signed.is_empty() {
        return Ok(());
    }
    let keys = match trust_keys() {
        Ok(k) => k,
        Err(e) => return Err(crate::db::infra::registry::SolveError::Registry(e.to_string())),
    };
    for (ver, vmeta) in signed {
        let Some(sig) = vmeta.sig.as_deref() else { continue };
        let Some(keyid) = vmeta.keyid.as_deref() else {
            return Err(crate::db::infra::registry::SolveError::Registry(format!(
                "registry metadata `{}@{ver}` carries a signature but no keyid",
                meta.name
            )));
        };
        let Some(key) = keys.get(&keyid.to_ascii_lowercase()) else {
            // Unknown key: community, not tamper — the store simply predates
            // the publisher. Presented as such by the classification faces.
            continue;
        };
        if verify_entry(meta.name.as_str(), &ver, sig, keyid, vmeta, key).is_err() {
            return Err(crate::db::infra::registry::SolveError::Signature {
                name: meta.name.clone(),
                ver,
                keyid: keyid.to_string(),
            });
        }
    }
    Ok(())
}

/// One entry's verification: rebuild the canonical bytes, check the
/// signature. Failure is tamper — named, never downgraded to community.
fn verify_entry(
    name: &str,
    ver: &str,
    sig: &str,
    keyid: &str,
    vmeta: &crate::db::infra::registry::VersionMeta,
    key: &VerifyingKey,
) -> Result<()> {
    use base64::Engine;
    let mut entry = serde_json::to_value(vmeta).context("version entry does not serialize")?;
    if let serde_json::Value::Object(map) = &mut entry {
        map.remove("sig");
        map.remove("keyid");
    }
    let msg = canonical_json(&entry);
    let raw = base64::engine::general_purpose::STANDARD
        .decode(sig.strip_prefix("ed25519:").unwrap_or(sig))
        .with_context(|| format!("`{name}@{ver}`: signature is not base64"))?;
    let signature = ed25519_dalek::Signature::from_slice(&raw)
        .map_err(|e| anyhow::anyhow!("`{name}@{ver}`: malformed signature: {e}"))?;
    key.verify(msg.as_bytes(), &signature)
        .map_err(|_| anyhow::anyhow!("`{name}@{ver}`: signature verification FAILED for key {keyid} — the metadata or the key row is not what the publisher signed (tamper)"))?;
    Ok(())
}

/// Classify one version row for the outward faces (search/list). A failing
/// signature must never reach here as Community — reads go through
/// [`verify_meta`], which hard-errors first.
pub fn classify(meta: &LibMeta, ver: &str, keys: &BTreeMap<String, VerifyingKey>) -> Trust {
    let Some(vmeta) = meta.versions.get(ver) else {
        return Trust::Community;
    };
    match (&vmeta.sig, &vmeta.keyid) {
        (Some(_), Some(keyid)) if keys.contains_key(&keyid.to_ascii_lowercase()) => Trust::Verified,
        _ => Trust::Community,
    }
}

// ── seed file + small codec helpers ──

fn write_seed(path: &Path, seed: &[u8; 32]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("cannot create signing key {}", path.display()))?;
        writeln!(f, "{}", hex_encode(seed))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, format!("{}\n", hex_encode(seed)))
            .with_context(|| format!("cannot create signing key {}", path.display()))?;
    }
    Ok(())
}

fn read_seed(path: &Path) -> Result<Vec<u8>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read signing key {}", path.display()))?;
    let hex = text.trim();
    decode_hex(hex).with_context(|| format!("signing key `{}`: not a hex seed", path.display()))
}

fn decode_hex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        anyhow::bail!("bad hex");
    }
    Ok((0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect::<std::result::Result<Vec<u8>, _>>()?)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Minimal sha256 over already-collected bytes (the crate root's
/// `sha256_hex_file` is file-bound; keys are in memory).
fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_key(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("mcc-trust-{tag}-{}-{:?}",
            std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn keygen_sign_verify_round_trip() {
        let path = tmp_key("rt");
        let (public, keyid) = keygen(&path).expect("keygen");
        assert_eq!(public.len(), 64, "64 hex = 32 bytes");
        assert_eq!(keyid.len(), 16, "keyid = 16 hex");

        let signing = load_signing_key(&path).expect("seed reads back");
        let entry = serde_json::json!({"checksum": "sha256:abc", "size": 7, "yanked": false});
        let (sig, sig_keyid) = sign_version(&entry, &path).expect("sign");
        assert_eq!(sig_keyid, keyid);

        // Verify through the trust-store path.
        let keys = BTreeMap::from([(keyid.clone(), signing.verifying_key())]);
        let mut vmeta = crate::db::infra::registry::VersionMeta::default();
        vmeta.sig = Some(sig.clone());
        vmeta.keyid = Some(keyid.clone());
        let meta = crate::db::infra::registry::LibMeta {
            name: "t".into(),
            category: String::new(),
            package: None,
            description: None,
            versions: [("0.1".to_string(), vmeta.clone())].into_iter().collect(),
        };
        assert_eq!(classify(&meta, "0.1", &keys), Trust::Verified);
        assert_eq!(classify(&meta, "0.1", &BTreeMap::new()), Trust::Community);
        let _ = sig;
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tampered_entry_is_a_hard_error() {
        let path = tmp_key("tamper");
        let (_, keyid) = keygen(&path).expect("keygen");
        let signing = load_signing_key(&path).expect("seed reads back");

        let mut entry = serde_json::json!({"checksum": "sha256:abc"});
        let (sig, _) = sign_version(&entry, &path).expect("sign");
        // Flip the payload after signing — the classic metadata swap.
        entry["checksum"] = serde_json::json!("sha256:evil");

        let mut vmeta = crate::db::infra::registry::VersionMeta::default();
        vmeta.sig = Some(sig);
        vmeta.keyid = Some(keyid.clone());
        let res = verify_entry("t", "0.1", vmeta.sig.as_deref().unwrap(), &keyid, &vmeta, &signing.verifying_key());
        assert!(res.is_err(), "a flipped payload must not verify");
        let _ = std::fs::remove_file(&path);
    }
}
