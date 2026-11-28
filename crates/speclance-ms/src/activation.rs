//! Canonical activation names stored in `Spectrum::activation`.
//!
//! The store holds the short names `CID`, `HCD`, `ETD`, `EThcD`, `ECD`,
//! `IRMPD`, `UVPD`, `PQD`, `SID`, `MPID` and `PD`. The mzML reader and the
//! vendor ingest emit them, and the mzML writer parses them back. `MPD` is
//! accepted as an alias of `MPID`. Long names in existing stores are accepted
//! when writing, with the meaning the vendor ingest gave them (see
//! [`LEGACY_VENDOR_NAMES`]); other long PSI-MS term names are accepted too.
//!
//! mzML terms (PSI-MS accessions) and the names they read as:
//!
//! | accession    | term                                          | name    |
//! |--------------|-----------------------------------------------|---------|
//! | MS:1000133   | collision-induced dissociation                | CID     |
//! | MS:1000422   | beam-type collision-induced dissociation      | HCD     |
//! | MS:1002481   | higher energy beam-type CID                   | HCD     |
//! | MS:1000598   | electron transfer dissociation                | ETD     |
//! | MS:1002631   | electron-transfer/higher-energy CID           | EThcD   |
//! | MS:1000250   | electron capture dissociation                 | ECD     |
//! | MS:1000262   | infrared multiphoton dissociation             | IRMPD   |
//! | MS:1000599   | pulsed q dissociation                         | PQD     |
//! | MS:1003246   | ultraviolet photodissociation                 | UVPD    |
//! | MS:1000136   | surface-induced dissociation                  | SID     |
//! | MS:1000435   | photodissociation (synonyms multiphoton, MPD) | MPID    |
//!
//! `PD` has no PSI-MS term. `openmassspec-core` writes it as the userParam
//! `openmassspec: activation` with value `pd` (and `unknown` for no
//! activation), and the mzML reader reads the pair back.
//!
//! `MS:1002481` is the PSI-MS term named "higher energy beam-type collision-
//! induced dissociation", which is HCD; it reads as `HCD`. The writer emits
//! its parent `MS:1000422`. `MS:1000435` covers both multiphoton dissociation
//! and MPD, so one name (`MPID`) stands for it. `MS:1001880` (in-source CID) is
//! an ion-source process, not precursor activation, so it is not mapped.
//!
//! The supplemental terms `MS:1002678` (supplemental beam-type CID) and
//! `MS:1002679` (supplemental CID) modify a primary activation. ETD with
//! `MS:1002678` reads as `EThcD`, as does ETD with `MS:1002631`.

use openmassspec_io::core::Activation as A;

/// Name of the userParam `openmassspec-core` writes in `<activation>` for an
/// activation that has no PSI-MS term (value `pd`) or that the reader did not
/// state (value `unknown`).
pub(crate) const CORE_ACTIVATION_MARKER: &str = "openmassspec: activation";

/// Short name stored for an activation. Round-trips through [`parse_activation`]
/// except `MPD`, which is an alias of `MPID`.
pub(crate) fn activation_label(act: A) -> String {
    let name = match act {
        A::CID => "CID",
        A::HCD => "HCD",
        A::ETD => "ETD",
        A::EThcD => "EThcD",
        A::MPID => "MPID",
        A::ECD => "ECD",
        A::IRMPD => "IRMPD",
        A::PD => "PD",
        A::PQD => "PQD",
        A::UVPD => "UVPD",
        A::SID => "SID",
        // `Activation` is #[non_exhaustive]: keep the variant name for new
        // ones; the writer rejects a name it cannot parse.
        other => return format!("{other:?}"),
    };
    name.to_string()
}

/// Long names the vendor ingest stores, with the activation each one meant.
/// The ingest wrote the same string for more than one activation in two cases:
/// `HCD` and `SID` both as "beam-type collision-induced dissociation", and `ETD`
/// and `EThcD` both as "electron transfer dissociation". Those read as `HCD`
/// and `ETD`.
pub(crate) const LEGACY_VENDOR_NAMES: [(&str, A); 9] = [
    ("collision-induced dissociation", A::CID),
    ("beam-type collision-induced dissociation", A::HCD),
    ("electron transfer dissociation", A::ETD),
    (
        "supplemental beam-type collision-induced dissociation",
        A::MPID,
    ),
    ("electron capture dissociation", A::ECD),
    ("infrared multiphoton dissociation", A::IRMPD),
    ("in-source collision-induced dissociation", A::PD),
    ("pulsed q dissociation", A::PQD),
    ("ultraviolet photodissociation", A::UVPD),
];

/// Activation for a stored name, or `None` when the name is not recognised.
pub(crate) fn parse_activation(name: &str) -> Option<A> {
    let n = name.trim().to_ascii_lowercase();
    let short = match n.as_str() {
        "cid" => Some(A::CID),
        "hcd" => Some(A::HCD),
        "etd" => Some(A::ETD),
        "ethcd" => Some(A::EThcD),
        "mpid" | "mpd" => Some(A::MPID),
        "ecd" => Some(A::ECD),
        "irmpd" => Some(A::IRMPD),
        "pd" => Some(A::PD),
        "pqd" => Some(A::PQD),
        "uvpd" => Some(A::UVPD),
        "sid" => Some(A::SID),
        _ => None,
    };
    if short.is_some() {
        return short;
    }
    if let Some((_, act)) = LEGACY_VENDOR_NAMES.iter().find(|(l, _)| *l == n) {
        return Some(*act);
    }
    // Other PSI-MS term names.
    if n.contains("electron-transfer/higher") {
        Some(A::EThcD)
    } else if n.contains("electron transfer") {
        Some(A::ETD)
    } else if n.contains("electron capture") {
        Some(A::ECD)
    } else if n.contains("supplemental") || n.contains("in-source") {
        None
    } else if n.contains("beam-type") {
        Some(A::HCD)
    } else if n.contains("surface-induced") {
        Some(A::SID)
    } else if n.contains("infrared multiphoton") {
        Some(A::IRMPD)
    } else if n.contains("ultraviolet") {
        Some(A::UVPD)
    } else if n.contains("pulsed q") {
        Some(A::PQD)
    } else if n == "photodissociation" || n.contains("multiphoton dissociation") {
        Some(A::MPID)
    } else if n.contains("collision-induced") {
        Some(A::CID)
    } else {
        None
    }
}

const SUPPLEMENTAL_BEAM_CID: &str = "MS:1002678";
const ETHCD: &str = "MS:1002631";
const ETD: &str = "MS:1000598";

/// Activation named by a single primary dissociation accession.
fn primary(acc: &str) -> Option<A> {
    match acc {
        "MS:1000133" => Some(A::CID),
        "MS:1000422" | "MS:1002481" => Some(A::HCD),
        ETD => Some(A::ETD),
        ETHCD => Some(A::EThcD),
        "MS:1000599" => Some(A::PQD),
        "MS:1000250" => Some(A::ECD),
        "MS:1000262" => Some(A::IRMPD),
        "MS:1003246" => Some(A::UVPD),
        "MS:1000136" => Some(A::SID),
        "MS:1000435" => Some(A::MPID),
        _ => None,
    }
}

/// Resolve the dissociation accessions of one `<activation>` block.
///
/// Returns the stored name, or `None` when no primary term is present, and a
/// flag per input accession saying whether the name accounts for it. Terms not
/// accounted for (unrecognised terms, a second different primary term, a
/// supplemental term the name cannot express) are left for the caller to keep
/// in `cv_params`.
///
/// ETD together with `MS:1002631` or `MS:1002678` is `EThcD`. The first
/// primary term in document order sets the name; another term that names the
/// same activation is accounted for, a different one is not.
pub(crate) fn resolve_activation(accessions: &[&str]) -> (Option<String>, Vec<bool>) {
    let mut consumed = vec![false; accessions.len()];
    let has = |acc: &str| accessions.contains(&acc);
    let ethcd_pair = has(ETD) && (has(ETHCD) || has(SUPPLEMENTAL_BEAM_CID));

    let name = if ethcd_pair {
        for (i, a) in accessions.iter().enumerate() {
            if matches!(*a, ETD | ETHCD | SUPPLEMENTAL_BEAM_CID) {
                consumed[i] = true;
            }
        }
        Some(A::EThcD)
    } else {
        let mut chosen: Option<A> = None;
        for (i, a) in accessions.iter().enumerate() {
            if let Some(act) = primary(a) {
                match chosen {
                    None => {
                        chosen = Some(act);
                        consumed[i] = true;
                    }
                    Some(c) if c == act => consumed[i] = true,
                    Some(_) => {}
                }
            }
        }
        chosen
    };
    // `MS:1002679` (supplemental CID) is never accounted for: the name has no
    // slot for it, so the caller keeps it in `cv_params`.
    (name.map(activation_label), consumed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [A; 11] = [
        A::CID,
        A::HCD,
        A::ETD,
        A::EThcD,
        A::MPID,
        A::ECD,
        A::IRMPD,
        A::PD,
        A::PQD,
        A::UVPD,
        A::SID,
    ];

    #[test]
    fn every_activation_round_trips_through_its_label() {
        for a in ALL {
            assert_eq!(parse_activation(&activation_label(a)), Some(a), "{a:?}");
        }
    }

    #[test]
    fn mpd_is_an_alias_of_mpid() {
        assert_eq!(parse_activation("MPD"), Some(A::MPID));
    }

    #[test]
    fn term_names_parse() {
        let cases = [
            ("collision-induced dissociation", A::CID),
            ("beam-type collision-induced dissociation", A::HCD),
            (
                "higher energy beam-type collision-induced dissociation",
                A::HCD,
            ),
            ("electron transfer dissociation", A::ETD),
            (
                "electron-transfer/higher-energy collision dissociation",
                A::EThcD,
            ),
            ("surface-induced dissociation", A::SID),
            ("photodissociation", A::MPID),
            ("infrared multiphoton dissociation", A::IRMPD),
        ];
        for (name, act) in cases {
            assert_eq!(parse_activation(name), Some(act), "{name}");
        }
    }

    /// Strings the vendor ingest wrote (`activation_name` in `vendor.rs` from
    /// commit 89f2837, "feat(prolance-ms-thermo): ingest .raw directly via
    /// opentfraw iter_spectra", through origin/main) parse to the activation
    /// they were written for.
    #[test]
    fn legacy_vendor_names_parse_to_the_intended_activation() {
        let written = [
            (A::CID, "collision-induced dissociation"),
            (A::HCD, "beam-type collision-induced dissociation"),
            (A::ETD, "electron transfer dissociation"),
            (A::EThcD, "electron transfer dissociation"),
            (
                A::MPID,
                "supplemental beam-type collision-induced dissociation",
            ),
            (A::ECD, "electron capture dissociation"),
            (A::IRMPD, "infrared multiphoton dissociation"),
            (A::PD, "in-source collision-induced dissociation"),
            (A::PQD, "pulsed q dissociation"),
            (A::UVPD, "ultraviolet photodissociation"),
            (A::SID, "beam-type collision-induced dissociation"),
        ];
        for (act, name) in written {
            let got = parse_activation(name).unwrap();
            let want = match act {
                A::EThcD => A::ETD,
                A::SID => A::HCD,
                other => other,
            };
            assert_eq!(got, want, "{name}");
        }
        for (name, act) in LEGACY_VENDOR_NAMES {
            assert_eq!(parse_activation(name), Some(act), "{name}");
        }
    }

    #[test]
    fn unknown_names_do_not_parse() {
        assert_eq!(parse_activation("flux capacitor"), None);
    }

    #[test]
    fn single_terms_resolve() {
        let cases = [
            ("MS:1000133", "CID"),
            ("MS:1000422", "HCD"),
            ("MS:1002481", "HCD"),
            ("MS:1000598", "ETD"),
            ("MS:1002631", "EThcD"),
            ("MS:1000599", "PQD"),
            ("MS:1000250", "ECD"),
            ("MS:1000262", "IRMPD"),
            ("MS:1003246", "UVPD"),
            ("MS:1000136", "SID"),
            ("MS:1000435", "MPID"),
        ];
        for (acc, want) in cases {
            let (name, used) = resolve_activation(&[acc]);
            assert_eq!(name.as_deref(), Some(want), "{acc}");
            assert_eq!(used, vec![true]);
        }
    }

    #[test]
    fn unmapped_terms_resolve_to_nothing() {
        for acc in ["MS:1001880", "MS:1002678", "MS:1002679", "MS:9999999"] {
            let (name, used) = resolve_activation(&[acc]);
            assert_eq!(name, None, "{acc}");
            assert_eq!(used, vec![false]);
        }
    }

    #[test]
    fn etd_pairs_resolve_to_ethcd() {
        for pair in [
            ["MS:1000598", "MS:1002631"],
            ["MS:1002631", "MS:1000598"],
            ["MS:1000598", "MS:1002678"],
        ] {
            let (name, used) = resolve_activation(&pair);
            assert_eq!(name.as_deref(), Some("EThcD"), "{pair:?}");
            assert_eq!(used, vec![true, true]);
        }
    }

    #[test]
    fn extra_terms_are_not_accounted_for() {
        let (name, used) = resolve_activation(&["MS:1000133", "MS:1002679"]);
        assert_eq!(name.as_deref(), Some("CID"));
        assert_eq!(used, vec![true, false]);

        let (name, used) = resolve_activation(&["MS:1000598", "MS:1000133"]);
        assert_eq!(name.as_deref(), Some("ETD"));
        assert_eq!(used, vec![true, false]);

        let (name, used) = resolve_activation(&["MS:1000422", "MS:1002481"]);
        assert_eq!(name.as_deref(), Some("HCD"));
        assert_eq!(used, vec![true, true]);
    }
}
