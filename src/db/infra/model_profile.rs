// Copyright (c) 2026 MCode
use serde::Deserialize;
use std::path::Path;

/// One curated sim-capability card for a class or role face
/// (worldmodel-design §7; data layer only -- no solver semantics).
///
/// Unknown fields reject the file (into `invalid`, visibly) instead of being
/// swallowed: TOML's bare-key-after-table-array rule would otherwise silently
/// park a misplaced `missing`/`consumes` inside the last pins element and the
/// card would curate lies.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelProfileCard {
    /// Card identity. Role-face form `FAMILY::ROLE` (e.g. `XTAL::Resonator`);
    /// the role anchors the model split where a face has roles.
    pub key: String,
    pub tier: ProfileTier,
    /// Model kind name, data only (e.g. `crystal_dae`).
    #[serde(default)]
    pub model: Option<String>,
    /// Per-pin relation the model presents (kind x source, worldmodel §3).
    #[serde(default)]
    pub pins: Vec<PinRelation>,
    /// Pins that walk the port convention only (boundary tier).
    #[serde(default)]
    pub boundary_on: Vec<String>,
    /// Spec keys the derive consumes, spelled `spec:<key>`.
    #[serde(default)]
    pub consumes: Vec<String>,
    /// External-context assumptions the tier's promise rests on.
    #[serde(default)]
    pub assumptions: Vec<String>,
    /// Spec keys missing before the model can upgrade its tier.
    #[serde(default)]
    pub missing: Vec<String>,
    /// Address of a vendor model / host equation set (host tier).
    #[serde(default)]
    pub external: Option<String>,
    /// Documentation facet; never gates semantics (asset-catalog §2).
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Default resolution tier (worldmodel §7 ladder).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileTier {
    Descend,
    Derive,
    Boundary,
    Host,
    /// Curated as explicitly not modeled.
    None,
}

/// Relation kind axis (how the relation solves; worldmodel §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelationKind {
    Algebraic,
    Differential,
    Event,
}

/// Relation source axis (where resolve fills it from; worldmodel §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelationSource {
    SpecDerive,
    Module,
    HostA,
    HostB,
    Boundary,
    NotModeled,
}

/// One pin's relation on a card: the pin name plus its kind and source.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinRelation {
    pub pin: String,
    pub kind: RelationKind,
    pub source: RelationSource,
}

/// A card file that exists but does not parse. Kept visible -- the
/// no-silence doctrine applies to the registry too -- never swallowed.
#[derive(Debug, Clone, PartialEq)]
pub struct InvalidCardFile {
    pub file: String,
    pub error: String,
}

/// Everything one library's `sim/` sidecar directory yielded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadedProfiles {
    pub cards: Vec<ModelProfileCard>,
    pub invalid: Vec<InvalidCardFile>,
}

impl LoadedProfiles {
    /// The card for a role face, keyed `FAMILY::ROLE`.
    pub fn card_for(&self, family: &str, role: &str) -> Option<&ModelProfileCard> {
        let key = format!("{family}::{role}");
        self.cards.iter().find(|c| c.key == key)
    }
}

/// Read every `sim/*.toml` under the library root, sorted by file name.
/// A library without a `sim/` directory has no cards -- that is the normal
/// shape, not an error; a file that exists but does not parse lands in
/// `invalid` and stays visible.
pub fn load_lib_profiles(root: &Path) -> LoadedProfiles {
    let dir = root.join("sim");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return LoadedProfiles::default();
    };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    let mut loaded = LoadedProfiles::default();
    for file in files {
        let name = file.to_string_lossy().to_string();
        let Ok(content) = std::fs::read_to_string(&file) else {
            loaded.invalid.push(InvalidCardFile {
                file: name,
                error: "unreadable".to_string(),
            });
            continue;
        };
        match parse_profiles(&content) {
            Ok(mut cards) => loaded.cards.append(&mut cards),
            Err(e) => loaded.invalid.push(InvalidCardFile {
                file: name,
                error: e.to_string(),
            }),
        }
    }
    loaded
}

/// Parse one card file: an optional `[meta]` table plus a `[[card]]` array.
fn parse_profiles(content: &str) -> anyhow::Result<Vec<ModelProfileCard>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct File {
        #[serde(default)]
        #[allow(dead_code)]
        meta: Option<toml::Value>,
        #[serde(default)]
        card: Vec<ModelProfileCard>,
    }
    let file: File = toml::from_str(content)?;
    Ok(file.card)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_card() {
        let cards = parse_profiles(
            r#"
            [meta]
            schema = 1

            [[card]]
            key = "XTAL::Resonator"
            tier = "derive"
            model = "crystal_dae"
            consumes = ["spec:frequency"]
            boundary_on = ["X1"]
            missing = ["esr"]
            assumptions = ["nominal-only"]
            external = "host-a:crystal/neg-r"
            notes = ["curated"]

            [[card.pins]]
            pin = "X1"
            kind = "differential"
            source = "spec-derive"
        "#,
        )
        .unwrap();
        assert_eq!(cards.len(), 1);
        let c = &cards[0];
        assert_eq!(c.key, "XTAL::Resonator");
        assert_eq!(c.tier, ProfileTier::Derive);
        assert_eq!(c.model.as_deref(), Some("crystal_dae"));
        assert_eq!(c.pins.len(), 1);
        assert_eq!(c.pins[0].kind, RelationKind::Differential);
        assert_eq!(c.pins[0].source, RelationSource::SpecDerive);
        assert_eq!(c.consumes, vec!["spec:frequency"]);
        assert_eq!(c.boundary_on, vec!["X1"]);
        assert_eq!(c.missing, vec!["esr"]);
        assert_eq!(c.external.as_deref(), Some("host-a:crystal/neg-r"));
    }

    #[test]
    fn unknown_tier_word_is_rejected_not_renamed() {
        assert!(parse_profiles(
            r#"[[card]]
            key = "X::Y"
            tier = "derived"
        "#
        )
        .is_err());
    }

    #[test]
    fn card_for_matches_the_role_key_exactly() {
        let cards = parse_profiles(
            r#"[[card]]
            key = "XTAL::Resonator"
            tier = "derive"
        "#,
        )
        .unwrap();
        let loaded = LoadedProfiles { cards, invalid: vec![] };
        assert!(loaded.card_for("XTAL", "Resonator").is_some());
        // Exact name comparison: no case normalization, no prefix match.
        assert!(loaded.card_for("XTAL", "resonator").is_none());
        assert!(loaded.card_for("XTAL", "Oscillator").is_none());
    }

    #[test]
    fn missing_sim_directory_is_the_normal_empty_shape() {
        let tmp = std::env::temp_dir().join(format!("mcc-profiles-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let loaded = load_lib_profiles(&tmp);
        assert!(loaded.cards.is_empty());
        assert!(loaded.invalid.is_empty());
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn scalar_keys_after_the_pins_array_reject_the_file_visibly() {
        // TOML would attach these to the last pins element; swallowing them
        // would curate a card with empty `missing` -- silence is the defect.
        let tmp = std::env::temp_dir().join(format!("mcc-profiles-order-{}", std::process::id()));
        let sim = tmp.join("sim");
        std::fs::create_dir_all(&sim).unwrap();
        std::fs::write(
            sim.join("misordered.toml"),
            "[[card]]\nkey = \"X::Y\"\ntier = \"derive\"\n\n[[card.pins]]\npin = \"P\"\nkind = \"algebraic\"\nsource = \"boundary\"\n\nmissing = [\"esr\"]\n",
        )
        .unwrap();
        let loaded = load_lib_profiles(&tmp);
        assert!(loaded.cards.is_empty(), "misordered card must not parse as curated");
        assert_eq!(loaded.invalid.len(), 1);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn unreadable_card_stays_visible_as_invalid() {
        let tmp = std::env::temp_dir().join(format!("mcc-profiles-bad-{}", std::process::id()));
        let sim = tmp.join("sim");
        std::fs::create_dir_all(&sim).unwrap();
        std::fs::write(sim.join("broken.toml"), "[[card]]\nkey = 5\n").unwrap();
        let loaded = load_lib_profiles(&tmp);
        assert!(loaded.cards.is_empty());
        assert_eq!(loaded.invalid.len(), 1);
        assert!(loaded.invalid[0].file.ends_with("broken.toml"));
        std::fs::remove_dir_all(&tmp).ok();
    }
}
