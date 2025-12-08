use std::fs;
use std::path::PathBuf;

use datatui::dialog::StyleSet;

fn sample_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("sample-data")
        .join("rule-set-example.yml")
}

#[test]
fn deserialize_sample_style_set() {
    let path = sample_path();
    let yaml = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    let style_set: StyleSet =
        serde_yaml::from_str(&yaml).expect("sample rule-set-example.yml should deserialize");

    assert_eq!(style_set.name, "Style Set Name");
    assert!(!style_set.rules.is_empty());
    assert!(style_set.description.contains("test"));
}

#[test]
fn serialize_roundtrip_preserves_style_set() {
    let path = sample_path();
    let yaml = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    let style_set: StyleSet =
        serde_yaml::from_str(&yaml).expect("sample rule-set-example.yml should deserialize");

    let serialized = serde_yaml::to_string(&style_set).expect("serialize StyleSet");
    let decoded: StyleSet =
        serde_yaml::from_str(&serialized).expect("deserialize serialized StyleSet");

    assert_eq!(style_set, decoded);
}
