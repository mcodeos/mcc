// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The package-registry substrate (registry-design.md §3, the P2 local
//! closure): the `file://` source face, the metadata cache, and the
//! `mcode.lock` file. The server side is dumb storage — the protocol is a
//! static file tree (`/lib/<name>.json` per name, `/dl/<category>/<name>/<ver>/`
//! for the artifacts), all version selection happens client-side (§3.1), and
//! the solver consumes exactly these faces so a later `Http` source drops in
//! without touching a call site.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ── The source face ──

/// Which registry endpoint a solve reads. `File` is the P2 local tree; the
/// P3 `Http` face reads the same five-endpoint shape over reqwest (§1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrySource {
    File(PathBuf),
    /// The static-server face: `base` is the registry URL, trailing slash
    /// stripped — every endpoint hangs off it (`/lib/…`, `/dl/…`,
    /// `/search.json`).
    Http { base: String },
}

/// Download tier of an artifact (§1.5: same container shape, dual products —
/// static-server friendly, no range dependency).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Thin,
    Full,
}

impl Tier {
    pub fn suffix(self) -> &'static str {
        match self {
            Tier::Thin => ".thin.mcl",
            Tier::Full => ".mcl",
        }
    }

    /// The metadata checksum field the tier's integrity anchors (§1.2: thin
    /// anchors `thin_checksum`, full anchors `checksum`).
    pub fn checksum_field(self) -> &'static str {
        match self {
            Tier::Thin => "thin_checksum",
            Tier::Full => "checksum",
        }
    }
}

impl RegistrySource {
    /// Parse the configured registry URL. `file:///abs` maps to `/abs`; a
    /// bare absolute path is accepted as-is; `http(s)://host[/…]` is the P3
    /// face. Anything else is refused with a names-the-scheme error (no
    /// silent fallback).
    pub fn from_url(url: &str) -> Result<Self> {
        let t = url.trim();
        if let Some(rest) = t.strip_prefix("file://") {
            return Ok(RegistrySource::File(PathBuf::from(rest)));
        }
        if t.starts_with("http://") || t.starts_with("https://") {
            return Ok(RegistrySource::Http {
                base: t.trim_end_matches('/').to_string(),
            });
        }
        let p = PathBuf::from(t);
        if p.is_absolute() {
            Ok(RegistrySource::File(p))
        } else {
            anyhow::bail!(
                "registry.url `{t}` is neither a file:// URL nor an absolute path (config key: [registry] url / [config.registry])"
            )
        }
    }

    /// The metadata-cache shard for this source (§2.2): `local/` for a file
    /// tree, the URL authority (`host[:port]`) for an HTTP endpoint — two
    /// registries never share a cache row.
    pub fn cache_shard(&self) -> String {
        match self {
            RegistrySource::File(_) => "local".to_string(),
            RegistrySource::Http { base } => {
                // base is scheme://[userinfo@]host[:port][/…] (the shape is
                // frozen at from_url) — the authority segment is the shard
                // key. A protocol-shape parse, not a general URL parser.
                let rest = base.split_once("://").map(|(_, r)| r).unwrap_or(base);
                let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
                authority.to_string()
            }
        }
    }

    /// `GET /lib/<name>.json` — per-name metadata: the file being there is
    /// existence (200), its absence in a reachable tree is a 404 (`None`).
    /// An unreachable tree/endpoint is an error; the metadata cache is the
    /// offline truth the caller falls back to (`meta_json_cached`).
    ///
    /// Both faces verify signed rows through the trust face before the
    /// metadata escapes (§3: verification happens once, at the read).
    pub fn meta_json(&self, name: &str) -> Result<Option<LibMeta>> {
        match self {
            RegistrySource::File(root) => {
                if !root.is_dir() {
                    anyhow::bail!("registry root is not reachable: {}", root.display());
                }
                let path = root.join("lib").join(format!("{name}.json"));
                if !path.is_file() {
                    return Ok(None);
                }
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("failed to read registry metadata: {}", path.display()))?;
                let meta: LibMeta = serde_json::from_str(&text)
                    .with_context(|| format!("invalid registry metadata JSON: {}", path.display()))?;
                crate::db::infra::trust::verify_meta(&meta)?;
                Ok(Some(meta))
            }
            RegistrySource::Http { base } => {
                let url = format!("{base}/lib/{name}.json");
                let shard = self.cache_shard();
                let etag = cached_etag(&shard, name);
                let fetched = match crate::db::infra::httpfetch::get(&url, etag.as_deref()) {
                    Ok(r) => Some(r),
                    Err(crate::db::infra::httpfetch::FetchError::NotFound) => return Ok(None),
                    Err(e) => {
                        return Err(anyhow::anyhow!("registry metadata fetch failed: {e}"));
                    }
                };
                let Some(resp) = fetched else { unreachable!("matched above") };
                if resp.status == 304 {
                    // Not modified: the cache row is the truth. A cache row
                    // missing behind a live etag is a corrupt cache — fall
                    // through to an unconditional refetch.
                    if let Some(meta) = cached_meta(&shard, name) {
                        return Ok(Some(meta));
                    }
                    let resp = crate::db::infra::httpfetch::get(&url, None).map_err(|e| {
                        anyhow::anyhow!("registry metadata fetch failed: {e}")
                    })?;
                    return self.http_meta_store(name, &shard, resp.status, resp.etag, &resp.body);
                }
                self.http_meta_store(name, &shard, resp.status, resp.etag, &resp.body)
            }
        }
    }

    /// Parse + verify + park a 200 metadata body (the HTTP face's shared
    /// tail). Anything outside 2xx/304 is an error naming the status.
    fn http_meta_store(
        &self,
        name: &str,
        shard: &str,
        status: u16,
        etag: Option<String>,
        body: &[u8],
    ) -> Result<Option<LibMeta>> {
        if !(200..300).contains(&status) {
            anyhow::bail!("registry metadata fetch failed: HTTP {status} for /lib/{name}.json");
        }
        let meta: LibMeta = serde_json::from_slice(body).with_context(|| {
            format!("invalid registry metadata JSON: /lib/{name}.json (HTTP {status})")
        })?;
        crate::db::infra::trust::verify_meta(&meta)?;
        cache_meta(shard, name, &meta);
        store_etag(shard, name, etag.as_deref());
        Ok(Some(meta))
    }

    /// The metadata read with the offline fallback: a source error (not a
    /// 404) reads the cache instead — offline, the cache is the truth. A
    /// signature-verification failure is never a source error: serving the
    /// stale cache for tampered metadata would be exactly the downgrade the
    /// signature exists to prevent, so it propagates. `Ok(None)` here
    /// really means the name is unknown everywhere.
    pub fn meta_json_cached(&self, name: &str) -> Result<Option<LibMeta>> {
        let shard = self.cache_shard();
        match self.meta_json(name) {
            Ok(Some(meta)) => {
                cache_meta(&shard, name, &meta);
                Ok(Some(meta))
            }
            Ok(None) => Ok(None),
            Err(e) if e.downcast_ref::<SolveError>().is_some() => Err(e),
            Err(e) => match cached_meta(&shard, name) {
                Some(meta) => Ok(Some(meta)),
                None => Err(e),
            },
        }
    }

    /// `GET /dl/<category>/<name>/<ver>/<name>-<ver>.<tier>.mcl` — the
    /// artifact path in the tree (the file:// download *is* this path; the
    /// HTTP face gets the same shape as a URL in P3).
    pub fn artifact_path(&self, category: &str, name: &str, ver: &str, tier: Tier) -> PathBuf {
        match self {
            RegistrySource::File(root) => root
                .join("dl")
                .join(category)
                .join(name)
                .join(ver)
                .join(format!("{name}-{ver}{}", tier.suffix())),
            RegistrySource::Http { .. } => unreachable!("the HTTP face has no tree path; use fetch_artifact"),
        }
    }

    /// The artifact acquisition seam (§2.1): the file tree reads in place;
    /// the HTTP face streams to a scratch `<dest>` through a `.part` sibling.
    /// What comes back is the path to hash, verify and install — everything
    /// after this seam (sha256 wall, three checks, atomic unpack) is the one
    /// shared path both faces ride.
    pub fn fetch_artifact(
        &self,
        category: &str,
        name: &str,
        ver: &str,
        tier: Tier,
        scratch_dir: &Path,
    ) -> Result<PathBuf> {
        match self {
            RegistrySource::File(root) => {
                let _ = root;
                let path = self.artifact_path(category, name, ver, tier);
                if !path.is_file() {
                    anyhow::bail!(
                        "registry artifact is missing from the tree: {}",
                        path.display()
                    );
                }
                Ok(path)
            }
            RegistrySource::Http { base } => {
                let url = format!(
                    "{base}/dl/{category}/{name}/{ver}/{name}-{ver}{}",
                    tier.suffix()
                );
                let dest = scratch_dir.join(format!("{name}-{ver}{}", tier.suffix()));
                std::fs::create_dir_all(scratch_dir).with_context(|| {
                    format!("cannot create scratch dir {}", scratch_dir.display())
                })?;
                crate::db::infra::httpfetch::download(&url, &dest)
                    .map_err(|e| anyhow::anyhow!("artifact download failed: {e}"))?;
                Ok(dest)
            }
        }
    }

    /// `GET /search.json` — the static search index (§1 endpoint 5): the
    /// whole catalog, one row per pack; filtering happens client-side (the
    /// server stays dumb storage — no query semantics on the wire).
    pub fn search_index(&self) -> Result<Vec<SearchEntry>> {
        let text = match self {
            RegistrySource::File(root) => {
                let path = root.join("search.json");
                if !path.is_file() {
                    anyhow::bail!(
                        "the registry tree carries no search.json (generate one with mcpub/mksearch.sh): {}",
                        path.display()
                    );
                }
                std::fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?
            }
            RegistrySource::Http { base } => {
                let url = format!("{base}/search.json");
                let resp = crate::db::infra::httpfetch::get(&url, None).map_err(|e| {
                    anyhow::anyhow!("registry search index fetch failed: {e}")
                })?;
                if !(200..300).contains(&resp.status) {
                    anyhow::bail!("registry search index fetch failed: HTTP {} for /search.json", resp.status);
                }
                String::from_utf8(resp.body)
                    .with_context(|| "search.json is not UTF-8".to_string())?
            }
        };
        let idx: SearchIndex =
            serde_json::from_str(&text).with_context(|| "invalid search.json".to_string())?;
        Ok(idx.packages)
    }
}

// ── Metadata (§3.1 per-lib JSON) ──

/// One row of `/search.json` (§1 endpoint 5): the catalog face the static
/// generator emits; filtering is client-side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchEntry {
    pub name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub description: Option<String>,
    /// The highest non-yanked version, precomputed by the generator.
    #[serde(default)]
    pub latest: Option<String>,
}

#[derive(Deserialize)]
struct SearchIndex {
    #[serde(default)]
    packages: Vec<SearchEntry>,
}

/// One `/lib/<name>.json`. A package entry carries `versions`; a partno
/// alias entry (emitted per variant by the registry generator) carries
/// `package` — the authoritative name→package mapping, so a partno key in
/// `[dependencies]` expands by lookup, never by name shape. Its `versions`
/// are restricted to the versions where the variant exists.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LibMeta {
    pub name: String,
    #[serde(default)]
    pub category: String,
    /// Present only on a partno alias entry: the package this variant
    /// belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Free-text blurb mirrored into /search.json (P3; absent in older
    /// trees — forward-compatible by the ignore-unknown-fields law).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub versions: BTreeMap<String, VersionMeta>,
}

/// One version row of a `/lib/<name>.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct VersionMeta {
    #[serde(default)]
    pub yanked: bool,
    /// sha256 of the full-tier artifact (`sha256:…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    /// sha256 of the thin-tier artifact — the download integrity anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thin_checksum: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// The pack's declared dependencies (display/RPC face; the installed
    /// pack.toml is the authoritative recursion source).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deps: BTreeMap<String, String>,
    /// Variant names this version ships (the `since` face lives in the
    /// installed pack.toml, which the solver reads after selection).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<String>,
    /// Ed25519 metadata signature `ed25519:<base64(R‖S)>` over the entry's
    /// canonical JSON minus these two fields (§3). Absent = community.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
    /// The signer's key id (first 16 hex of the public key's SHA-256).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyid: Option<String>,
}

// ── Metadata cache (§3.2: <data_root>/cache/meta/<shard>/) ──

/// The metadata cache directory (a `mcc clean --cache` candidate once a
/// clean verb exists). P3 shards it per source: `local/` for file trees,
/// the URL authority for HTTP endpoints (§2.2 — two registries never share
/// a row).
pub fn meta_cache_dir() -> PathBuf {
    crate::cli::datadir::data_root().join("cache").join("meta")
}

fn cache_meta(shard: &str, name: &str, meta: &LibMeta) {
    let dir = meta_cache_dir().join(shard);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(text) = serde_json::to_string_pretty(meta) {
        let _ = std::fs::write(dir.join(format!("{name}.json")), text);
    }
}

fn cached_meta(shard: &str, name: &str) -> Option<LibMeta> {
    let path = meta_cache_dir().join(shard).join(format!("{name}.json"));
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// The ETag sidecar (§2.2): one line beside the cache row. No sidecar =
/// the next read is an unconditional fetch.
fn cached_etag(shard: &str, name: &str) -> Option<String> {
    let path = meta_cache_dir()
        .join(shard)
        .join(format!("{name}.json.etag"));
    std::fs::read_to_string(path).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

fn store_etag(shard: &str, name: &str, etag: Option<&str>) {
    let dir = meta_cache_dir().join(shard);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{name}.json.etag"));
    match etag {
        Some(e) => {
            let _ = std::fs::write(&path, e);
        }
        None => {
            let _ = std::fs::remove_file(&path);
        }
    }
}

// ── mcode.lock (§4.3) ──

pub const LOCK_FILE_NAME: &str = "mcode.lock";

/// One lock entry. A pack-rail dependency records `version` (+ `package`/
/// `partno` on a partno key, `checksum` once fetched); the stdlib rail
/// records only mcode's `rev` (the mcc batch the library shipped with — the
/// reproduction credential is mcc version + mcode rev + device lock).
/// Placement is deliberately absent: it is local-machine state (U387⑩).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LockEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partno: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}

/// The `mcode.lock` body: a flat `name = { … }` table, written normalized
/// two-segment (the repo's version law), read tolerantly.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LockFile {
    #[serde(flatten)]
    pub deps: BTreeMap<String, LockEntry>,
}

impl LockFile {
    /// The lock next to `project_root`'s manifest, if one is present and
    /// parses. Absent ≠ error — "no lock" is the fresh-solve state.
    pub fn load(project_root: &Path) -> Option<LockFile> {
        let path = project_root.join(LOCK_FILE_NAME);
        let text = std::fs::read_to_string(path).ok()?;
        toml::from_str(&text).ok()
    }

    /// Write the lock next to the manifest, atomically (tmp + rename): a
    /// half-written lock must never exist, it is the reproduction
    /// credential. Returns the written path.
    pub fn store(&self, project_root: &Path) -> Result<PathBuf> {
        let path = project_root.join(LOCK_FILE_NAME);
        let body =
            toml::to_string_pretty(self).context("failed to serialize mcode.lock")?;
        let tmp = project_root.join(format!(".{}.tmp-{}", LOCK_FILE_NAME, std::process::id()));
        std::fs::write(&tmp, body)
            .with_context(|| format!("failed to write {}", tmp.display()))?;
        std::fs::rename(&tmp, &path)
            .with_context(|| format!("failed to finalize {}", path.display()))?;
        Ok(path)
    }
}

/// The lock body a solve implies, with the mcode stdlib-rail rev recorded
/// (the mcc batch the library ships with — it tracks the build, not the
/// registry). Shared by the build hook, `lib update` and `lib.resolve`.
pub fn lock_with_mcode_rev(solved: &LockFile) -> LockFile {
    let mut body = solved.clone();
    body.deps.insert(
        "mcode".to_string(),
        LockEntry {
            rev: Some(crate::buildinfo::BUILD.to_string()),
            ..LockEntry::default()
        },
    );
    body
}

/// The first solved entry the stored lock lacks or disagrees with (key
/// absent, or a version/partno/package face drift) — the E2056 face.
pub fn first_stale_key<'a>(stored: &LockFile, solved: &'a LockFile) -> Option<&'a str> {
    solved.deps.iter().find_map(|(k, e)| match stored.deps.get(k) {
        Some(s) if s.version == e.version && s.partno == e.partno && s.package == e.package => None,
        _ => Some(k.as_str()),
    })
}

// ── The solver (§4.2/§5) ──
//
// Deterministic-function law: "dependency declarations + lock -> one definite
// set of `<name>@<ver>#<partno>`".
// All selection is client-side (§3.1); the environment reaches the solver
// through [`SolveSource`] so the pure selection law is testable without a
// registry tree on disk.

/// One declared dependency, normalized out of the `[dependencies]` forms
/// (bare string, partno key, table form).
#[derive(Debug, Clone)]
pub struct SolveDecl {
    /// The `[dependencies]` key — the lock file's key face.
    pub key: String,
    pub req: crate::VersionReq,
    /// A table-form partno; a partno *key* carries its partno through the
    /// metadata alias lookup instead (never a name-shape guess).
    pub partno: Option<String>,
    /// The explicit-placement declaration: `<proj>/deps/` joins the search
    /// roots only when true (the explicit-placement law).
    pub local: bool,
}

/// Where a resolved pack came from. `Registry` marks "was downloaded by this
/// solve"; the caller pairs it with what it installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveOrigin {
    Project,
    DataRoot,
    Registry,
}

/// One resolved pack rail dependency.
#[derive(Debug, Clone)]
pub struct SolvedPack {
    /// The declaration key the entry is recorded under in the lock.
    pub key: String,
    pub package: String,
    /// The canonical two-segment version face.
    pub version: String,
    pub partno: Option<String>,
    pub checksum: Option<String>,
    pub origin: SolveOrigin,
    /// The declaration's `deps/` opt-in — the placement law rides the
    /// declaration, never a global.
    pub local: bool,
}

/// Solve-time failures (§5). Each maps onto one compiler diagnostic / RPC
/// code — no string-matched anyhow blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolveError {
    /// No version satisfies the req (or the name is nowhere).
    Unresolved { name: String, req: String },
    /// One package demanded under two unsatisfiable faces.
    Conflict { package: String, have: String, want: String },
    /// Transitive dependency cycle; `path` names the loop in order.
    Cycle { path: Vec<String> },
    /// The selected version predates the declared partno.
    PartnoUnavailable {
        partno: String,
        package: String,
        selected: String,
        since: String,
        available: Vec<String>,
    },
    /// Registry transport/parse failure (message names the face).
    Registry(String),
    /// The downloaded artifact's sha256 disagrees with the metadata — the
    /// artifact or the metadata row is corrupt; nothing was installed.
    Checksum {
        name: String,
        ver: String,
        want: String,
        got: String,
    },
    /// A keyed metadata signature failed verification against its trust-store
    /// key — tamper, the metadata face of the checksum wall (§3).
    Signature { name: String, ver: String, keyid: String },
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolveError::Unresolved { name, req } => {
                write!(f, "no version of `{name}` satisfies `{req}` in the registry")
            }
            SolveError::Conflict { package, have, want } => {
                write!(f, "conflicting requirements for `{package}`: resolved `{have}`, also demanded `{want}`")
            }
            SolveError::Cycle { path } => {
                write!(f, "dependency cycle: {}", path.join(" -> "))
            }
            SolveError::PartnoUnavailable { partno, package, selected, since, available } => {
                write!(
                    f,
                    "partno `{partno}` of `{package}` does not exist in {selected} (it exists since {since}; versions carrying it: {})",
                    available.join(", ")
                )
            }
            SolveError::Registry(msg) => write!(f, "{msg}"),
            SolveError::Checksum { name, ver, want, got } => {
                write!(
                    f,
                    "checksum mismatch for `{name}@{ver}`: metadata {want}, downloaded {got} — nothing was installed"
                )
            }
            SolveError::Signature { name, ver, keyid } => {
                write!(
                    f,
                    "signature verification failed for `{name}@{ver}` (key {keyid}) — the metadata is not what the publisher signed; nothing was trusted"
                )
            }
        }
    }
}

impl std::error::Error for SolveError {}
/// What the solver may ask about the world. The production face reads the
/// [`RegistrySource`], the declared `deps/` roots and the data root; tests
/// fake it entirely — the selection law has no disk dependency.
pub trait SolveSource {
    /// Registry metadata for a name (package or partno alias); `None` =
    /// unknown everywhere (404).
    fn meta(&self, name: &str) -> Result<Option<LibMeta>, SolveError>;
    /// Whether `name@ver` is already available locally — the declared
    /// `deps/` roots (only when the declaration opted in: `local = true` /
    /// `--here`, the explicit-placement law), then the data root;
    /// `Some(origin)` = reuse, no download.
    fn local_hit(&self, name: &str, ver: &str, local: bool) -> Option<SolveOrigin>;
    /// The authoritative pack.toml dependency reqs of `name@ver` — the
    /// recursion face. The caller installs the pack first when it is
    /// missing (thin archives carry the manifest); `local` carries the
    /// declaration's placement opt-in so a missing fetch lands in the same
    /// root the resolver searched.
    fn pack_deps(&self, name: &str, ver: &str, local: bool)
        -> Result<BTreeMap<String, String>, SolveError>;
    /// The versions of `package` carrying `partno`, ascending — the partno
    /// membership/since face (registry-design §3.1: the metadata `variants`
    /// lists the *partno strings*; pack.toml's `[variants]` keys are the
    /// identifier face and never compared against a partno).
    fn versions_with_partno(&self, package: &str, partno: &str) -> Vec<String>;
}

/// The solver's verdict: the resolved packs (by package name — the loader's
/// pin face) plus the lock body they imply.
#[derive(Debug, Clone)]
pub struct SolveOutcome {
    pub packs: BTreeMap<String, SolvedPack>,
    /// Lock entries by declaration key (pack rail only; the stdlib rail's
    /// mcode rev is the caller's to record — it tracks the mcc build).
    pub lock: LockFile,
    /// No lock existed when the solve started — the caller holding the
    /// write-authority may write this (build included, §4.2).
    pub fresh: bool,
}

/// Solve the dependency declarations against the lock (§4.2): lock entries
/// win outright — the locked version is used as-is and never re-selected
/// (a yanked locked version still installs; reproducibility first), and only the
/// uncovered declarations go through fresh selection (yanked skipped).
/// Deterministic by construction: BTreeMap fronts, highest-qualifying
/// selection, one resolution per package.
pub fn solve<S: SolveSource>(
    decls: &BTreeMap<String, SolveDecl>,
    lock: Option<&LockFile>,
    src: &S,
) -> Result<SolveOutcome, SolveError> {
    let mut out = SolveOutcome {
        packs: BTreeMap::new(),
        lock: LockFile::default(),
        fresh: lock.is_none(),
    };
    // Depth-first resolution: `gray` holds the packages whose dependency
    // subtrees are still being walked, so a dep edge landing on a gray
    // package is the cycle — named in walk order (b3907 ruling 4). Finished
    // packages move to `out.packs`, where a re-encounter is the shared-
    // dependency determinism check, not a cycle.
    let mut gray: Vec<String> = Vec::new();
    for (key, decl) in decls {
        visit(key, decl, lock, src, &mut out, &mut gray)?;
    }
    Ok(out)
}

fn visit<S: SolveSource>(
    key: &str,
    decl: &SolveDecl,
    lock: Option<&LockFile>,
    src: &S,
    out: &mut SolveOutcome,
    gray: &mut Vec<String>,
) -> Result<(), SolveError> {
    if key == "mcode" {
        // Distribution split law (b4489): mcode rides the stdlib rail — version
        // tracks the mcc build, never the registry; the load loop owns
        // it and the caller records its lock rev.
        return Ok(());
    }

    let locked = lock.and_then(|l| l.deps.get(key));
    // Lock-first name resolution: the lock entry names the package
    // itself (the partno form records it in `package`), so a lock hit
    // never reads metadata - a full lock hit means zero network. `alias` marks a
    // partno *key* expanded through the alias entry: its version table is
    // already restricted to where the partno exists, so membership holds
    // by construction and no re-check fires.
    let (package, partno, alias) = match locked {
        Some(entry) if entry.version.is_some() => {
            let package = entry.package.clone().unwrap_or_else(|| key.to_string());
            (package, entry.partno.clone(), false)
        }
        _ => {
            // Name resolution (§4.1): a table-form partno keeps its
            // package key; a plain key resolving to a partno alias
            // entry adopts the alias's package — lookup, never a name
            // shape.
            match src.meta(key)? {
                Some(m) if m.package.is_some() => {
                    (m.package.clone().unwrap(), Some(key.to_string()), true)
                }
                Some(_) => (key.to_string(), decl.partno.clone(), false),
                None => match decl.partno.clone() {
                    Some(p) => (key.to_string(), Some(p), false),
                    None => {
                        return Err(SolveError::Unresolved {
                            name: key.to_string(),
                            req: decl.req_label(),
                        });
                    }
                },
            }
        }
    };

    // Shared-dependency determinism: one package resolves once; a second
    // demand (a later declaration or a transitive edge) must admit the
    // resolved version or the solve fails naming both faces.
    if let Some(have) = out.packs.get(&package) {
        if !req_admits(&decl.req, &have.version) {
            return Err(SolveError::Conflict {
                package,
                have: have.version.clone(),
                want: decl.req_label(),
            });
        }
        return Ok(());
    }
    if let Some(pos) = gray.iter().position(|p| p == &package) {
        let mut path = gray[pos..].to_vec();
        path.push(package);
        return Err(SolveError::Cycle { path });
    }

    let solved = match locked {
        // Lock-first: the locked version ships as-is — zero network when
        // it is already on disk (local_hit before any metadata read), a
        // yanked locked version included (reproducibility first).
        Some(entry) if entry.version.is_some() => {
            let ver = crate::cli::datadir::normalize_version(entry.version.as_deref().unwrap_or("*"))
                .to_string();
            let origin = src.local_hit(&package, &ver, decl.local).ok_or_else(|| {
                SolveError::Unresolved { name: package.clone(), req: format!("={ver} (locked)") }
            })?;
            // No partno membership re-check here: the lock is a verified
            // credential (the solve that wrote it checked membership), and
            // a full lock hit means zero network - reproducibility first.
            SolvedPack {
                key: key.to_string(),
                package: package.clone(),
                version: ver,
                partno: partno.clone(),
                checksum: entry.checksum.clone(),
                origin,
                local: decl.local,
            }
        }
        _ => {
            // Fresh selection: non-yanked versions within the req
            // bounds, highest wins. A partno alias entry carries only
            // the versions where the variant exists, so selection and
            // membership agree by construction.
            let meta = src.meta(key)?.ok_or_else(|| SolveError::Unresolved {
                name: package.clone(),
                req: decl.req_label(),
            })?;
            let (lo, hi) = crate::version_req_bounds(&decl.req);
            let pick = meta
                .versions
                .iter()
                .filter(|(_, m)| !m.yanked)
                .filter(|(v, _)| crate::key_in_bounds(crate::version_key(v), lo, hi))
                .max_by_key(|(v, _)| crate::version_key(v))
                .map(|(v, m)| (v.clone(), m.clone()));
            let Some((ver_raw, vmeta)) = pick else {
                return Err(SolveError::Unresolved { name: package, req: decl.req_label() });
            };
            let ver = crate::cli::datadir::normalize_version(&ver_raw).to_string();
            if let Some(p) = &partno {
                if !alias && !partno_carries(src, &package, &ver, p) {
                    return Err(partno_error(&package, &ver, p, src));
                }
            }
            SolvedPack {
                key: key.to_string(),
                package: package.clone(),
                version: ver.clone(),
                partno: partno.clone(),
                checksum: vmeta.thin_checksum.clone(),
                origin: src
                    .local_hit(&package, &ver, decl.local)
                    .unwrap_or(SolveOrigin::Registry),
                local: decl.local,
            }
        }
    };

    // `package` is recorded only for partno alias keys, where the key is
    // not the package name; table-form and plain keys carry it implicitly.
    out.lock.deps.insert(
        key.to_string(),
        LockEntry {
            package: if package != key { Some(package.clone()) } else { None },
            version: Some(solved.version.clone()),
            partno: partno.clone(),
            rev: None,
            checksum: solved.checksum.clone(),
        },
    );
    // Recursion face: the installed pack.toml deps (§4.2 — resolution
    // only ever happens here, never at install time).
    gray.push(package.clone());
    for (dep, req) in src.pack_deps(&package, &solved.version, decl.local)? {
        let d = SolveDecl {
            key: dep.clone(),
            req: crate::parse_version_req(&req),
            partno: None,
            local: decl.local,
        };
        visit(&dep, &d, lock, src, out, gray)?;
    }
    gray.pop();
    out.packs.insert(package, solved);
    Ok(())
}

impl SolveDecl {
    fn req_label(&self) -> String {
        match &self.partno {
            Some(p) => format!("{}#{}", self.req_display(), p),
            None => self.req_display(),
        }
    }

    fn req_display(&self) -> String {
        match &self.req {
            crate::VersionReq::Any => "*".into(),
            crate::VersionReq::AtLeast(v) => format!(">={v}"),
            crate::VersionReq::Caret(v) => format!("^{v}"),
            crate::VersionReq::Tilde(v) => format!("~{v}"),
            crate::VersionReq::Exact(v) => format!("={v}"),
        }
    }
}

fn req_admits(req: &crate::VersionReq, ver: &str) -> bool {
    let (lo, hi) = crate::version_req_bounds(req);
    crate::key_in_bounds(crate::version_key(ver), lo, hi)
}

/// The partno since law (§5, b4489 follow-up ruling): the selected version
/// must carry the declared partno (the metadata `variants` face, §3.1); the
/// diagnostic names the first version that does.
fn partno_carries(src: &impl SolveSource, package: &str, ver: &str, partno: &str) -> bool {
    src.versions_with_partno(package, partno)
        .iter()
        .any(|v| crate::cli::datadir::normalize_version(v).to_string() == ver)
}

fn partno_error(
    package: &str,
    selected: &str,
    partno: &str,
    src: &impl SolveSource,
) -> SolveError {
    let available = src.versions_with_partno(package, partno);
    SolveError::PartnoUnavailable {
        partno: partno.to_string(),
        package: package.to_string(),
        selected: selected.to_string(),
        since: available.first().cloned().unwrap_or_default(),
        available,
    }
}

// ── The disk-backed source + install driver (§4.2: the solver is automated lib install) ──

/// The production [`SolveSource`]: metadata through the [`RegistrySource`]
/// (the offline cache behind it), local availability across the declared
/// `deps/` dir and the data root, and the recursion/variant faces read from
/// the installed pack.toml — the thin archive carries it, so a pack is
/// placed before its dependency edges are walked.
pub struct DiskSource<'a> {
    pub src: &'a RegistrySource,
    /// The project's `deps/` dir; `None` when no declaration opted in (the
    /// explicit-placement law: it never joins the search otherwise).
    pub deps_dir: Option<PathBuf>,
    pub data_root: PathBuf,
    /// The offline-resolve face (`lib.resolve` offline): a pack not already
    /// on disk is an error naming it — never a download.
    pub no_install: bool,
}

/// Lowercase hex compare, tolerant of the metadata's `sha256:` prefix.
/// Checksum hex is not a language name - normalize both sides, then the
/// exact equality the name-comparison law requires.
fn sha_matches(want: &str, got: &str) -> bool {
    let w = want.strip_prefix("sha256:").unwrap_or(want);
    let g = got.strip_prefix("sha256:").unwrap_or(got);
    w.to_ascii_lowercase() == g.to_ascii_lowercase()
}

/// The typed-error lift (D15): a failure that already *is* a [`SolveError`]
/// (the trust face's tamper verdict) keeps its arm — anyhow's downcast does
/// the typing, never a string match. Everything else is the generic
/// Registry face.
fn solve_err(e: anyhow::Error) -> SolveError {
    match e.downcast::<SolveError>() {
        Ok(se) => se,
        Err(e) => SolveError::Registry(e.to_string()),
    }
}

impl DiskSource<'_> {
    fn installed_dir(&self, name: &str, ver: &str, local: bool) -> Option<(PathBuf, SolveOrigin)> {
        let dir = format!("{name}@{ver}");
        if local {
            if let Some(d) = &self.deps_dir {
                let p = d.join(&dir);
                if p.is_dir() {
                    return Some((p, SolveOrigin::Project));
                }
            }
        }
        let p = self.data_root.join(&dir);
        (p.is_dir()).then_some((p, SolveOrigin::DataRoot))
    }

    /// Fetch `name@ver` (thin tier), verify its sha256 against the metadata,
    /// and unpack it through the single install path under the registry scope
    /// law (law amendment 1). Placement follows the declaration: `local = true`
    /// lands in `<proj>/deps/`, everything else in the data root. A pack
    /// already on disk is a no-op — transitive packs were placed while their
    /// manifests were read.
    fn install_from_registry(&self, name: &str, ver: &str, local: bool) -> Result<(), SolveError> {
        if self.no_install {
            return Err(SolveError::Registry(format!(
                "offline resolve: `{name}@{ver}` is not installed and downloads are off"
            )));
        }
        let target = self.placement_root(local);
        if target.join(format!("{name}@{ver}")).is_dir() {
            return Ok(());
        }
        let meta = self
            .src
            .meta_json_cached(name)
            .map_err(solve_err)?
            .ok_or_else(|| SolveError::Unresolved {
                name: name.to_string(),
                req: format!("={ver}"),
            })?;
        let vmeta = meta.versions.get(ver).ok_or_else(|| SolveError::Unresolved {
            name: name.to_string(),
            req: format!("={ver}"),
        })?;
        let want = vmeta.thin_checksum.as_deref().ok_or_else(|| {
            SolveError::Registry(format!(
                "registry metadata for `{name}@{ver}` carries no thin_checksum — cannot verify the download"
            ))
        })?;
        // Process-unique scratch: the download lands through a `.part`
        // sibling, and two mcc processes installing the same name must not
        // share one — a concurrent rename would pull the sibling out from
        // under the other (observed live as ENOENT at finalize). The dir is
        // removed once consumed; error paths keep it for forensics.
        let scratch = std::env::temp_dir().join(format!("mcc-fetch-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&scratch);
        let path = self
            .src
            .fetch_artifact(&meta.category, name, ver, Tier::Thin, &scratch)
            .map_err(solve_err)?;
        let got = crate::sha256_hex_file(&path)
            .map_err(|e| SolveError::Registry(format!("failed to read {}: {e}", path.display())))?;
        if !sha_matches(want, &got) {
            return Err(SolveError::Checksum {
                name: name.to_string(),
                ver: ver.to_string(),
                want: want.to_string(),
                got,
            });
        }
        crate::install_mcl_at_scope(&path, Some(name), &target, crate::ensure_install_scope_registry)
            .map_err(|e| SolveError::Registry(e.to_string()))?;
        let _ = std::fs::remove_dir_all(&scratch);
        Ok(())
    }

    /// Where a fetch lands: `<proj>/deps/` only when the declaration opted
    /// in (the explicit-placement law); the data root otherwise.
    fn placement_root(&self, local: bool) -> PathBuf {
        match (local, &self.deps_dir) {
            (true, Some(d)) => d.clone(),
            _ => self.data_root.clone(),
        }
    }

    /// The installed pack.toml of `name@ver`. Reading an already-selected
    /// version is not selection: both placement roots are consulted, and a
    /// pack nowhere on disk is fetched first (the thin manifest is the
    /// authoritative recursion face).
    fn pack_toml(
        &self,
        name: &str,
        ver: &str,
        local: bool,
    ) -> Result<crate::cli::packfile::PackToml, SolveError> {
        let mut dir = self.placement_root(local).join(format!("{name}@{ver}"));
        if !dir.is_dir() {
            // Reading an already-selected version may consult the other
            // placement root — this is not selection, so the search law is
            // not at stake here.
            let other = if local {
                self.data_root.join(format!("{name}@{ver}"))
            } else {
                self.deps_dir
                    .as_ref()
                    .map(|d| d.join(format!("{name}@{ver}")))
                    .unwrap_or_default()
            };
            if other.is_dir() {
                dir = other;
            }
        }
        if !dir.is_dir() {
            self.install_from_registry(name, ver, local)?;
            dir = self.placement_root(local).join(format!("{name}@{ver}"));
        }
        let path = dir.join("pack.toml");
        let text = std::fs::read_to_string(&path).map_err(|e| {
            SolveError::Registry(format!("failed to read {}: {e}", path.display()))
        })?;
        toml::from_str(&text)
            .map_err(|e| SolveError::Registry(format!("invalid pack.toml in {}: {e}", dir.display())))
    }
}

impl SolveSource for DiskSource<'_> {
    fn meta(&self, name: &str) -> Result<Option<LibMeta>, SolveError> {
        self.src.meta_json_cached(name).map_err(solve_err)
    }

    fn local_hit(&self, name: &str, ver: &str, local: bool) -> Option<SolveOrigin> {
        self.installed_dir(name, ver, local).map(|(_, o)| o)
    }

    fn pack_deps(
        &self,
        name: &str,
        ver: &str,
        local: bool,
    ) -> Result<BTreeMap<String, String>, SolveError> {
        Ok(self.pack_toml(name, ver, local)?.dependencies)
    }

    fn versions_with_partno(&self, package: &str, partno: &str) -> Vec<String> {
        match self.src.meta_json_cached(package) {
            Ok(Some(meta)) => meta
                .versions
                .into_iter()
                .filter(|(_, m)| m.variants.iter().any(|v| v == partno))
                .map(|(v, _)| v)
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Solve + install (§4.2). Runs [`solve`] against the [`DiskSource`], then
/// fetches every pack the solve marked [`SolveOrigin::Registry`] — thin tier,
/// sha256-verified, into the data root. Returns the outcome plus the
/// `name@ver` faces this run actually downloaded.
pub fn solve_and_install(
    src: &RegistrySource,
    deps_dir: Option<&Path>,
    data_root: &Path,
    decls: &BTreeMap<String, SolveDecl>,
    lock: Option<&LockFile>,
) -> Result<(SolveOutcome, Vec<String>), SolveError> {
    let disk = DiskSource {
        src,
        deps_dir: deps_dir.map(Path::to_path_buf),
        data_root: data_root.to_path_buf(),
        no_install: false,
    };
    let out = solve(decls, lock, &disk)?;
    let mut installed = Vec::new();
    for p in out.packs.values() {
        if p.origin == SolveOrigin::Registry {
            disk.install_from_registry(&p.package, &p.version, p.local)?;
            installed.push(format!("{}@{}", p.package, p.version));
        }
    }
    Ok((out, installed))
}

/// `<proj>/deps/` is local-machine placement (U387⑩): it lands in
/// `.gitignore` (idempotent — the entry is appended only when missing); the
/// reproduction credential is the lock, not the placement.
pub fn ensure_deps_gitignore(project_root: &Path) -> std::io::Result<()> {
    let gi = project_root.join(".gitignore");
    let cur = std::fs::read_to_string(&gi).unwrap_or_default();
    if cur.lines().any(|l| l.trim() == "deps/") {
        return Ok(());
    }
    let mut body = cur;
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str("deps/\n");
    std::fs::write(gi, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::datadir::tests::ENV_LOCK;

    fn scratch(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mcc-registry-{}-{}", tag, std::process::id()))
    }

    #[test]
    fn cli_registry__from_url_maps_file_and_http_shapes() {
        // file:///abs → /abs; a bare absolute path is itself; relative is
        // refused by name, never guessed into a path; http(s) keeps the
        // authority as the P3 cache shard (trailing slash stripped).
        assert_eq!(
            RegistrySource::from_url("file:///tmp/reg").unwrap(),
            RegistrySource::File(PathBuf::from("/tmp/reg"))
        );
        assert_eq!(
            RegistrySource::from_url("/tmp/reg").unwrap(),
            RegistrySource::File(PathBuf::from("/tmp/reg"))
        );
        assert!(RegistrySource::from_url("relative/dir").is_err());
        assert_eq!(
            RegistrySource::from_url("https://reg.example.com/").unwrap(),
            RegistrySource::Http { base: "https://reg.example.com".into() }
        );
    }

    #[test]
    fn cli_registry__artifact_path_follows_the_dl_layout() {
        let src = RegistrySource::from_url("file:///reg").unwrap();
        assert_eq!(
            src.artifact_path("power", "ams1117", "1.2", Tier::Thin),
            PathBuf::from("/reg/dl/power/ams1117/1.2/ams1117-1.2.thin.mcl")
        );
        assert_eq!(
            src.artifact_path("power", "ams1117", "1.2", Tier::Full),
            PathBuf::from("/reg/dl/power/ams1117/1.2/ams1117-1.2.mcl")
        );
        // The tier's integrity anchor field follows §1.2.
        assert_eq!(Tier::Thin.checksum_field(), "thin_checksum");
        assert_eq!(Tier::Full.checksum_field(), "checksum");
    }

    #[test]
    fn cli_registry__meta_json_404_vs_unreachable() {
        let root = scratch("meta");
        std::fs::create_dir_all(root.join("lib")).unwrap();
        let src = RegistrySource::File(root.join("lib2")); // unreachable tree
        assert!(src.meta_json("acme").is_err(), "unreachable is an error");
        let src = RegistrySource::File(root.clone());
        assert_eq!(src.meta_json("acme").unwrap(), None, "404 is None");
        std::fs::write(
            root.join("lib").join("acme.json"),
            r#"{"name":"acme","category":"power","versions":{"1.2":{"yanked":false,"thin_checksum":"sha256:a"}}}"#,
        )
        .unwrap();
        let meta = src.meta_json("acme").unwrap().unwrap();
        assert_eq!(meta.category, "power");
        assert_eq!(meta.versions["1.2"].thin_checksum.as_deref(), Some("sha256:a"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cli_registry__lock_round_trip_all_entry_shapes() {
        let root = scratch("lock");
        std::fs::create_dir_all(&root).unwrap();
        let mut lock = LockFile::default();
        lock.deps.insert(
            "mcode".into(),
            LockEntry { rev: Some("b4434".into()), ..Default::default() },
        );
        lock.deps.insert(
            "ams1117".into(),
            LockEntry {
                version: Some("1.2".into()),
                checksum: Some("sha256:aa".into()),
                ..Default::default()
            },
        );
        lock.deps.insert(
            "ams1117-3.3".into(),
            LockEntry {
                package: Some("ams1117".into()),
                version: Some("1.2".into()),
                partno: Some("ams1117-3.3".into()),
                checksum: Some("sha256:bb".into()),
                ..Default::default()
            },
        );
        lock.store(&root).unwrap();
        let back = LockFile::load(&root).unwrap();
        assert_eq!(back, lock, "the lock is a lossless credential");
        // The rev-only stdlib rail entry round-trips without a version face.
        assert_eq!(back.deps["mcode"].rev.as_deref(), Some("b4434"));
        assert_eq!(back.deps["mcode"].version, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cli_registry__meta_cache_is_the_offline_truth() {
        let _lock = ENV_LOCK.lock().unwrap();
        let prev = std::env::var("MCC_SYSTEM_ROOT").ok();
        let data = scratch("cache-data");
        std::env::set_var("MCC_SYSTEM_ROOT", &data);
        assert_eq!(meta_cache_dir(), data.join("cache").join("meta"));

        // A cache miss with a reachable source returns fresh truth and
        // populates the cache.
        let root = scratch("cache-src");
        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::write(
            root.join("lib").join("acme.json"),
            r#"{"name":"acme","category":"power","versions":{"1.2":{"yanked":false}}}"#,
        )
        .unwrap();
        let src = RegistrySource::File(root.clone());
        assert!(src.meta_json_cached("acme").unwrap().is_some());
        assert!(
            cached_meta("local", "acme").is_some(),
            "the fetch warmed the local-shard cache row"
        );

        // With the tree gone, the cache answers — offline, the cache is the truth.
        let _ = std::fs::remove_dir_all(&root);
        let offline = RegistrySource::File(root.clone());
        assert!(offline.meta_json("acme").is_err());
        assert!(offline.meta_json_cached("acme").unwrap().is_some());

        // A name cached nowhere stays an error once the source is gone.
        assert!(offline.meta_json_cached("nosuch").is_err());

        match prev {
            Some(v) => std::env::set_var("MCC_SYSTEM_ROOT", v),
            None => std::env::remove_var("MCC_SYSTEM_ROOT"),
        }
        let _ = std::fs::remove_dir_all(&data);
    }

    // ── B3: the solver's selection law, over a faked world ──

    /// A fake registry: packages with versions, variants, deps; no disk.
    struct Fake {
        metas: BTreeMap<String, LibMeta>,
        installed: BTreeMap<String, SolveOrigin>, // "name@ver" -> origin
        pack: BTreeMap<(String, String), (BTreeMap<String, String>, Vec<String>)>,
        metas_read: std::cell::Cell<usize>,
    }

    impl Fake {
        fn pkg(&mut self, name: &str, vers: &[(&str, bool, &[&str])]) -> &mut Self {
            let mut versions = BTreeMap::new();
            for (v, yanked, variants) in vers {
                versions.insert(
                    v.to_string(),
                    VersionMeta {
                        yanked: *yanked,
                        thin_checksum: Some(format!("sha256:{name}-{v}")),
                        variants: variants.iter().map(|s| s.to_string()).collect(),
                        ..Default::default()
                    },
                );
            }
            self.metas.insert(
                name.to_string(),
                LibMeta { name: name.to_string(), category: "power".into(), package: None, description: None, versions },
            );
            self
        }
        fn dep(&mut self, name: &str, ver: &str, deps: &[(&str, &str)]) -> &mut Self {
            let entry = self.pack.entry((name.to_string(), ver.to_string())).or_default();
            entry.0.extend(deps.iter().map(|(d, r)| (d.to_string(), r.to_string())));
            self
        }
        fn variants_of(&mut self, name: &str, ver: &str, vars: &[&str]) -> &mut Self {
            let entry = self.pack.entry((name.to_string(), ver.to_string())).or_default();
            entry.1.extend(vars.iter().map(|s| s.to_string()));
            self
        }
        fn install(&mut self, name: &str, ver: &str, origin: SolveOrigin) {
            self.installed.insert(format!("{name}@{ver}"), origin);
        }
        fn meta_reads(&self) -> usize {
            self.metas_read.get()
        }
    }

    impl SolveSource for Fake {
        fn meta(&self, name: &str) -> Result<Option<LibMeta>, SolveError> {
            self.metas_read.set(self.metas_read.get() + 1);
            Ok(self.metas.get(name).cloned())
        }
        fn local_hit(&self, name: &str, ver: &str, _local: bool) -> Option<SolveOrigin> {
            self.installed.get(&format!("{name}@{ver}")).copied()
        }
        fn pack_deps(
            &self,
            name: &str,
            ver: &str,
            _local: bool,
        ) -> Result<BTreeMap<String, String>, SolveError> {
            Ok(self
                .pack
                .get(&(name.to_string(), ver.to_string()))
                .map(|(d, _)| d.clone())
                .unwrap_or_default())
        }
        fn versions_with_partno(&self, package: &str, partno: &str) -> Vec<String> {
            self.metas
                .get(package)
                .map(|m| {
                    m.versions
                        .iter()
                        .filter(|(_, vm)| vm.variants.iter().any(|x| x == partno))
                        .map(|(v, _)| v.clone())
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    fn decl(key: &str, req: &str) -> (String, SolveDecl) {
        (
            key.to_string(),
            SolveDecl {
                key: key.to_string(),
                req: crate::parse_version_req(req),
                partno: None,
                local: false,
            },
        )
    }

    fn err_of(r: Result<SolveOutcome, SolveError>) -> SolveError {
        r.err().expect("expected a solve failure")
    }

    #[test]
    fn cli_registry__solve_picks_highest_non_yanked_in_bounds() {
        let mut f = Fake {
            metas: BTreeMap::new(),
            installed: BTreeMap::new(),
            pack: BTreeMap::new(),
            metas_read: std::cell::Cell::new(0),
        };
        f.pkg("acme", &[("1.2", false, &[]), ("1.9", true, &[]), ("2.0", false, &[])]);
        let decls: BTreeMap<_, _> = [decl("acme", "^1.2")].into();
        let out = solve(&decls, None, &f).unwrap();
        // 1.9 is yanked: fresh solve skips it, 2.0 is outside the ^1.2 band.
        assert_eq!(out.packs["acme"].version, "1.2");
        assert_eq!(out.packs["acme"].origin, SolveOrigin::Registry);
        // Two-segment normalization on the written lock, whatever the metadata spelled.
        assert_eq!(out.lock.deps["acme"].version.as_deref(), Some("1.2"));
    }

    #[test]
    fn cli_registry__lock_first_is_zero_network_and_never_reselects() {
        let mut f = Fake {
            metas: BTreeMap::new(),
            installed: BTreeMap::new(),
            pack: BTreeMap::new(),
            metas_read: std::cell::Cell::new(0),
        };
        // acme 2.0 is the only metadata face at all; the lock says 1.0 and
        // it is on disk. The lock wins with zero metadata reads.
        f.pkg("acme", &[("2.0", false, &[])]);
        f.install("acme", "1.0", SolveOrigin::DataRoot);
        let mut lock = LockFile::default();
        lock.deps.insert(
            "acme".into(),
            LockEntry { version: Some("1.0".into()), checksum: Some("sha256:x".into()), ..Default::default() },
        );
        let decls: BTreeMap<_, _> = [decl("acme", "^1.0")].into();
        let out = solve(&decls, Some(&lock), &f).unwrap();
        assert_eq!(out.packs["acme"].version, "1.0");
        assert_eq!(out.packs["acme"].origin, SolveOrigin::DataRoot);
        assert_eq!(out.packs["acme"].checksum.as_deref(), Some("sha256:x"));
        assert_eq!(f.meta_reads(), 0, "a lock hit never reads metadata");
        assert!(!out.fresh);
    }

    #[test]
    fn cli_registry__locked_yanked_still_installs() {
        let mut f = Fake {
            metas: BTreeMap::new(),
            installed: BTreeMap::new(),
            pack: BTreeMap::new(),
            metas_read: std::cell::Cell::new(0),
        };
        f.pkg("acme", &[("1.0", true, &[])]);
        f.install("acme", "1.0", SolveOrigin::Registry);
        let mut lock = LockFile::default();
        lock.deps.insert(
            "acme".into(),
            LockEntry { version: Some("1.0".into()), ..Default::default() },
        );
        let decls: BTreeMap<_, _> = [decl("acme", "*")].into();
        // Reproducibility first: the lock's yanked version still resolves.
        assert_eq!(solve(&decls, Some(&lock), &f).unwrap().packs["acme"].version, "1.0");
        // But a fresh solve skips it loudly.
        match err_of(solve(&decls, None, &f)) {
            SolveError::Unresolved { name, .. } => assert_eq!(name, "acme"),
            e => panic!("wrong error: {e}"),
        }
    }

    #[test]
    fn cli_registry__cycle_is_named_by_path() {
        let mut f = Fake {
            metas: BTreeMap::new(),
            installed: BTreeMap::new(),
            pack: BTreeMap::new(),
            metas_read: std::cell::Cell::new(0),
        };
        f.pkg("a", &[("1.0", false, &[])]);
        f.pkg("b", &[("1.0", false, &[])]);
        f.dep("a", "1.0", &[("b", "*")]);
        f.dep("b", "1.0", &[("a", "*")]);
        let decls: BTreeMap<_, _> = [decl("a", "*")].into();
        match err_of(solve(&decls, None, &f)) {
            SolveError::Cycle { path } => assert_eq!(path, vec!["a".to_string(), "b".to_string(), "a".to_string()]),
            e => panic!("wrong error: {e}"),
        }
    }

    #[test]
    fn cli_registry__partno_since_is_named_with_available_versions() {
        let mut f = Fake {
            metas: BTreeMap::new(),
            installed: BTreeMap::new(),
            pack: BTreeMap::new(),
            metas_read: std::cell::Cell::new(0),
        };
        f.pkg(
            "ams1117",
            &[("1.2", false, &["ams1117-adj"]), ("1.4", false, &["ams1117-adj", "ams1117-3.3"])],
        );
        f.variants_of("ams1117", "1.2", &["ams1117-adj"]);
        f.variants_of("ams1117", "1.4", &["ams1117-adj", "ams1117-3.3"]);
        // Pinned to 1.2: selected version predates the partno's 1.4 debut.
        let mut d = decl("ams1117", "=1.2").1;
        d.partno = Some("ams1117-3.3".into());
        let decls: BTreeMap<_, _> = [("ams1117".to_string(), d)].into();
        match err_of(solve(&decls, None, &f)) {
            SolveError::PartnoUnavailable { partno, since, available, selected, .. } => {
                assert_eq!(partno, "ams1117-3.3");
                assert_eq!(selected, "1.2");
                assert_eq!(since, "1.4");
                assert_eq!(available, vec!["1.4".to_string()]);
            }
            e => panic!("wrong error: {e}"),
        }
        // Unpinned, the solver selects 1.4 where the partno exists.
        let mut d = decl("ams1117", "^1.2").1;
        d.partno = Some("ams1117-3.3".into());
        let decls: BTreeMap<_, _> = [("ams1117".to_string(), d)].into();
        let out = solve(&decls, None, &f).unwrap();
        assert_eq!(out.packs["ams1117"].version, "1.4");
        assert_eq!(out.packs["ams1117"].partno.as_deref(), Some("ams1117-3.3"));
        assert_eq!(out.lock.deps["ams1117"].package.as_deref(), None);
        assert_eq!(out.lock.deps["ams1117"].partno.as_deref(), Some("ams1117-3.3"));
    }

    #[test]
    fn cli_registry__partno_key_expands_through_the_alias_entry() {
        let mut f = Fake {
            metas: BTreeMap::new(),
            installed: BTreeMap::new(),
            pack: BTreeMap::new(),
            metas_read: std::cell::Cell::new(0),
        };
        // The variants ride the metadata face (partno_carries reads the
        // meta variants, DiskSource-style), so the package meta carries
        // 1.4 with the variant.
        f.pkg("ams1117", &[("1.2", false, &[]), ("1.4", false, &["ams1117-3.3"])]);
        // The alias entry: package face authoritative, versions restricted
        // to where the variant exists.
        let mut versions = BTreeMap::new();
        versions.insert(
            "1.4".to_string(),
            VersionMeta { thin_checksum: Some("sha256:33".into()), variants: vec!["ams1117-3.3".into()], ..Default::default() },
        );
        f.metas.insert(
            "ams1117-3.3".to_string(),
            LibMeta {
                name: "ams1117-3.3".into(),
                category: "power".into(),
                package: Some("ams1117".into()),
                description: None,
                versions,
            },
        );
        f.dep("ams1117", "1.4", &[]);
        f.variants_of("ams1117", "1.4", &["ams1117-3.3"]);
        let decls: BTreeMap<_, _> = [decl("ams1117-3.3", "1.4")].into();
        let out = solve(&decls, None, &f).unwrap();
        assert_eq!(out.packs["ams1117"].package, "ams1117");
        assert_eq!(out.packs["ams1117"].version, "1.4");
        assert_eq!(out.packs["ams1117"].partno.as_deref(), Some("ams1117-3.3"));
        // The lock records the partno form in full (§5).
        let entry = &out.lock.deps["ams1117-3.3"];
        assert_eq!(entry.package.as_deref(), Some("ams1117"));
        assert_eq!(entry.version.as_deref(), Some("1.4"));
        assert_eq!(entry.partno.as_deref(), Some("ams1117-3.3"));
    }

    #[test]
    fn cli_registry__transitive_deps_recurse_and_conflicts_fail() {
        let mut f = Fake {
            metas: BTreeMap::new(),
            installed: BTreeMap::new(),
            pack: BTreeMap::new(),
            metas_read: std::cell::Cell::new(0),
        };
        f.pkg("board", &[("1.0", false, &[])]);
        f.pkg("regulator", &[("1.0", false, &[]), ("1.5", false, &[])]);
        f.pkg("sensor", &[("1.0", false, &[])]);
        f.dep("board", "1.0", &[("regulator", "^1.0"), ("sensor", "*")]);
        f.dep("sensor", "1.0", &[("regulator", ">=1.0")]);
        let decls: BTreeMap<_, _> = [decl("board", "1.0")].into();
        let out = solve(&decls, None, &f).unwrap();
        // The transitive regulator demand (^1.0 from board, >=1.0 from
        // sensor) admits the one resolved copy — determinism, one name one
        // version.
        assert_eq!(out.packs["regulator"].version, "1.5");
        assert_eq!(out.packs.len(), 3);
        // The transitive discovery is in the lock under its own name.
        assert!(out.lock.deps.contains_key("sensor"));

        // A second demand the resolved copy cannot admit is a named conflict.
        f.dep("sensor", "1.0", &[("regulator", ">=9.0")]);
        match err_of(solve(&decls, None, &f)) {
            SolveError::Conflict { package, have, want } => {
                assert_eq!(package, "regulator");
                assert_eq!(have, "1.5");
                assert_eq!(want, ">=9.0");
            }
            e => panic!("wrong error: {e}"),
        }
    }

    #[test]
    fn cli_registry__mcode_rides_the_stdlib_rail() {
        let f = Fake {
            metas: BTreeMap::new(),
            installed: BTreeMap::new(),
            pack: BTreeMap::new(),
            metas_read: std::cell::Cell::new(0),
        };
        let decls: BTreeMap<_, _> = [decl("mcode", "*")].into();
        let out = solve(&decls, None, &f).unwrap();
        // mcode never enters the pack-rail solve or the pack lock entries —
        // the load loop owns it, the caller records its rev.
        assert!(out.packs.is_empty());
        assert!(out.lock.deps.is_empty());
    }
}
