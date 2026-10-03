// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc lib trust` (registry-design.md §7⑦b, the auxiliary distribution
//! channel): pull the registry's key table (`/trust.json`) and merge the rows
//! whose signature verifies against an already-trusted key — a factory anchor
//! or a row accepted earlier in the same file. Old key signs new key; the
//! bootstrap is the factory channel (anchors ride the mcc release).
//!
//! Every row gets a status line — accepted, already trusted, or refused with
//! the reason — and a refusal fails the command. Nothing is silently
//! skipped: a trust decision that evaporates into a log line is the failure
//! mode this face exists to prevent.

use anyhow::{bail, Result};
use ed25519_dalek::VerifyingKey;
use std::collections::BTreeMap;

use mcc::{RegistrySource, TrustRow};
use RowReject::{AlreadyTrusted, Refused};

pub fn run(action: &mcc::cli::TrustAction) -> Result<()> {
    match action {
        mcc::cli::TrustAction::Update { registry } => cmd_trust_update(registry.as_deref()),
    }
}

fn cmd_trust_update(override_url: Option<&str>) -> Result<()> {
    let url = override_url
        .map(|s| s.to_string())
        .or_else(|| {
            mcc::cli::config::get_registry_url(crate::cmds::lib::client_project_root().as_deref())
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "lib trust update: no registry configured — pass --registry or set \
                 [registry] url in mcc.yaml"
            )
        })?;
    let src = RegistrySource::from_url(&url).map_err(|e| anyhow::anyhow!("lib trust update: {e}"))?;
    let table = mcc::fetch_trust_table(&src).map_err(|e| anyhow::anyhow!("lib trust update: {e}"))?;
    if table.keys.is_empty() {
        bail!("lib trust update: trust.json carries no rows");
    }

    // The trusted set starts at factory anchors ∪ store and grows within the
    // file — a row signed by a key accepted earlier in this same table
    // verifies (the old-key-signs-new-key chain).
    let mut trusted = mcc::trust_keys()?;
    let mut accepted: Vec<TrustRow> = Vec::new();
    let mut already: usize = 0;
    let mut refused: Vec<String> = Vec::new();
    for row in &table.keys {
        match verdict(row, &mut trusted) {
            Ok(note) => {
                eprintln!("  accepted   {} — {note}", row.keyid);
                accepted.push(row.clone());
            }
            Err(AlreadyTrusted) => {
                eprintln!("  already    {} — the key is trusted", row.keyid);
                already += 1;
            }
            Err(Refused(reason)) => {
                eprintln!("  refused    {} — {reason}", row.keyid);
                refused.push(row.keyid.clone());
            }
        }
    }

    let landed = mcc::trust_store_merge(&accepted)?;
    if !accepted.is_empty() {
        eprintln!(
            "✓ stored {landed}/{} accepted row(s) → {}",
            accepted.len(),
            mcc::trust_store_path().display()
        );
    } else if already > 0 && refused.is_empty() {
        eprintln!("= all {already} row(s) already trusted — nothing to store");
    }
    if !refused.is_empty() {
        bail!(
            "lib trust update: {} row(s) refused: {}",
            refused.len(),
            refused.join(", ")
        );
    }
    Ok(())
}

/// The outcome of one row's admission decision. `Err(AlreadyTrusted)` is the
/// benign no-op (same keyid, same key material already trusted); `Err(Refused)`
/// names the reason.
enum RowReject {
    AlreadyTrusted,
    Refused(String),
}

fn verdict(row: &TrustRow, trusted: &mut BTreeMap<String, VerifyingKey>) -> Result<String, RowReject> {
    let refuse = |r: String| Refused(r);
    let key = match verify_row_shape(row) {
        Ok(k) => k,
        Err(e) => return Err(refuse(e.to_string())),
    };
    let derived = mcc::keyid_of(&key);
    let kid = row.keyid.to_ascii_lowercase();
    if derived != kid {
        return Err(refuse(format!("keyid does not match the public key (derived {derived})")));
    }
    if let Some(existing) = trusted.get(&kid) {
        if existing.to_bytes() == key.to_bytes() {
            return Err(AlreadyTrusted);
        }
        return Err(refuse(
            "keyid collision: a different public key already holds this keyid".into(),
        ));
    }
    let Some(signer) = row.signer.as_deref() else {
        return Err(refuse("row is unsigned — no signer, no anchor chain".into()));
    };
    let Some(signer_key) = trusted.get(&signer.to_ascii_lowercase()) else {
        return Err(refuse(format!(
            "signer {signer} is not a trusted key (no chain from an anchor)"
        )));
    };
    if let Err(e) = mcc::verify_trust_row(row, signer_key) {
        return Err(refuse(e.to_string()));
    }
    trusted.insert(kid, key);
    Ok(format!("signed by {signer}"))
}

/// The row's key material: 64 hex chars, a valid Ed25519 point.
fn verify_row_shape(row: &TrustRow) -> Result<VerifyingKey> {
    let s = row.public.trim();
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("public key must be 64 hex chars (32 bytes)");
    }
    let mut raw = [0u8; 32];
    for (i, b) in raw.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .map_err(|e| anyhow::anyhow!("bad public key hex: {e}"))?;
    }
    VerifyingKey::from_bytes(&raw).map_err(|e| anyhow::anyhow!("bad public key: {e}"))
}
