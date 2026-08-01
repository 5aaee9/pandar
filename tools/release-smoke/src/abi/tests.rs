use std::{collections::BTreeSet, path::Path};

use super::expected_symbols;

#[test]
fn canonical_export_map_is_exact_for_each_profile() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();

    let stable = pandar_studio_profile::profile("02.07.01.62").unwrap();
    let symbols = expected_symbols(repo_root, stable).unwrap();

    assert_eq!(symbols.network_count, 108);
    assert_eq!(symbols.file_transfer_count, 21);
    assert_eq!(symbols.all.len(), 129);
    assert!(!symbols.all.contains("bambu_network_sync_ams_filaments"));

    let beta = pandar_studio_profile::profile("02.08.01.55").unwrap();
    let symbols = expected_symbols(repo_root, beta).unwrap();
    assert_eq!(symbols.network_count, 109);
    assert_eq!(symbols.file_transfer_count, 21);
    assert_eq!(symbols.all.len(), 130);
    assert!(symbols.all.contains("bambu_network_sync_ams_filaments"));
}

#[test]
fn exact_export_validation_rejects_128_and_130_target_symbols() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let profile = pandar_studio_profile::profile("02.07.01.62").unwrap();
    let expected = expected_symbols(repo_root, profile).unwrap().all;
    let mut missing_one = expected.clone();
    missing_one.remove("bambu_network_get_version");
    let mut extra_one = expected.clone();
    extra_one.insert("bambu_network_decoy".to_owned());

    assert!(super::validate_exact_exports(&expected, &missing_one).is_err());
    assert!(super::validate_exact_exports(&expected, &extra_one).is_err());
    super::validate_exact_exports(&expected, &expected).unwrap();
}

#[test]
fn native_symbol_parsers_ignore_undefined_target_prefix_symbols() {
    assert_eq!(
        super::parse_exported_symbols(
            super::SymbolOutput::Nm,
            "                 U bambu_network_missing\n0000 T _ft_abi_version"
        ),
        BTreeSet::from(["ft_abi_version".to_owned()])
    );
    assert_eq!(
        super::parse_exported_symbols(
            super::SymbolOutput::Readelf,
            "1: 0 0 FUNC GLOBAL DEFAULT UND bambu_network_missing\n2: 1 0 FUNC GLOBAL DEFAULT 12 bambu_network_get_version"
        ),
        BTreeSet::from(["bambu_network_get_version".to_owned()])
    );
}

#[test]
fn dumpbin_symbol_parser_keeps_icf_alias_export_names() {
    assert_eq!(
        super::parse_exported_symbols(
            super::SymbolOutput::PeDumpbin,
            "109 6C 00000000 bambu_network_build_login_info = bambu_network_build_login_cmd"
        ),
        BTreeSet::from(["bambu_network_build_login_info".to_owned()])
    );
}

#[test]
fn source_companion_requires_sentinel_and_forbids_bambu_exports() {
    let sentinel = BTreeSet::from(["pandar_bambu_source_sentinel".to_owned()]);
    super::validate_source_exports(&sentinel).unwrap();
    assert!(super::validate_source_exports(&BTreeSet::new()).is_err());

    let mut unsafe_exports = sentinel;
    unsafe_exports.insert("Bambu_Create".to_owned());
    assert!(super::validate_source_exports(&unsafe_exports).is_err());
}

#[test]
fn source_symbol_parser_keeps_only_sentinel_and_bambu_exports() {
    assert_eq!(
        super::parse_source_exports(
            super::SymbolOutput::Nm,
            "0000 T pandar_bambu_source_sentinel\n0001 T Bambu_Create\n0002 T unrelated"
        ),
        BTreeSet::from([
            "Bambu_Create".to_owned(),
            "pandar_bambu_source_sentinel".to_owned(),
        ])
    );
}
