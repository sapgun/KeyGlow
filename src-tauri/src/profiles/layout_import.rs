//! Layout importer (S6): validate an imported layout JSON before it is
//! stored or rendered. Only validated JSON is accepted; anything else
//! is rejected and the current layout is left untouched (the caller
//! keeps the original; this module never mutates config).
//!
//! Limits (shared with the TS validator in `src/lib/layout-import.ts`):
//! - raw JSON at most 64 KiB
//! - at most 120 keys
//! - coordinates in [0, 64], key size in (0, 16]
//! - id format `^[a-z0-9][a-z0-9-]{0,63}$`, no collision with builtin ids
//! - every key code must be a known `KeyCode`, except `Fn` which is
//!   allowed only with `unsupported: true`
//! - no unknown fields (`deny_unknown_fields`), no duplicate codes,
//!   no overlapping keys, declared width/height must match the keys
//!
//! Scan/VK mapping notes (ISO/JIS): the hook reports scan 0x2B as
//! `Backslash` on all three layouts (ANSI `\`, ISO `#`, JIS `]` —
//! label only). `IntlYen`'s VK_OEM_5 is provisional (see keycodes.rs);
//! confirm the scan/VK pairs on real ISO/JIS hardware.

use crate::keyboard::KeyCode;
use crate::profiles::layout_registry::{is_builtin_layout_id, is_valid_layout_id_format};
use serde::{Deserialize, Serialize};

/// Maximum raw JSON size for an imported layout.
pub const MAX_IMPORT_BYTES: usize = 64 * 1024;
/// Maximum number of keys in an imported layout (largest builtin is 109).
pub const MAX_IMPORT_KEYS: usize = 120;
/// Maximum coordinate / layout dimension, in key units.
pub const MAX_COORD: f64 = 64.0;
/// Maximum key width / height, in key units.
pub const MAX_KEY_SIZE: f64 = 16.0;
/// Tolerance when recomputing width/height from the keys (float rounding).
const DIMENSION_TOLERANCE: f64 = 0.01;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportedKey {
    pub code: String,
    pub vk: String,
    #[serde(rename = "vkCode")]
    pub vk_code: u32,
    pub label: String,
    #[serde(default)]
    pub sub_label: Option<String>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub category: String,
    #[serde(default)]
    pub unsupported: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportedLayout {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub category: String,
    #[serde(rename = "unitWidth")]
    pub unit_width: f64,
    pub gap: f64,
    pub width: f64,
    pub height: f64,
    pub keys: Vec<ImportedKey>,
}

/// Parse and validate an imported layout JSON string.
pub fn validate_imported_layout(raw: &str) -> Result<ImportedLayout, String> {
    if raw.len() > MAX_IMPORT_BYTES {
        return Err(format!(
            "layout JSON too large: {} bytes (max {MAX_IMPORT_BYTES})",
            raw.len()
        ));
    }
    let layout: ImportedLayout =
        serde_json::from_str(raw).map_err(|e| format!("invalid layout JSON: {e}"))?;
    validate_layout(&layout)?;
    Ok(layout)
}

/// Validate an already-parsed layout (used by import and by
/// `AppConfig::normalize`, which revalidates stored custom layouts).
pub fn validate_layout(layout: &ImportedLayout) -> Result<(), String> {
    if !is_valid_layout_id_format(&layout.id) {
        return Err(format!("invalid layout id: {:?}", layout.id));
    }
    if is_builtin_layout_id(&layout.id) {
        return Err(format!(
            "layout id {:?} collides with a builtin layout",
            layout.id
        ));
    }
    if layout.name.is_empty() || layout.name.chars().count() > 128 {
        return Err("layout name must be 1-128 characters".to_string());
    }
    if layout.category.is_empty() || layout.category.chars().count() > 64 {
        return Err("layout category must be 1-64 characters".to_string());
    }
    if !(layout.unit_width > 0.0 && layout.unit_width <= 200.0) {
        return Err(format!("invalid unitWidth: {}", layout.unit_width));
    }
    if !(layout.gap >= 0.0 && layout.gap <= 50.0) {
        return Err(format!("invalid gap: {}", layout.gap));
    }
    if !(layout.width > 0.0 && layout.width <= MAX_COORD) {
        return Err(format!("invalid width: {}", layout.width));
    }
    if !(layout.height > 0.0 && layout.height <= MAX_COORD) {
        return Err(format!("invalid height: {}", layout.height));
    }
    if layout.keys.is_empty() || layout.keys.len() > MAX_IMPORT_KEYS {
        return Err(format!(
            "key count must be 1-{}, got {}",
            MAX_IMPORT_KEYS,
            layout.keys.len()
        ));
    }
    let mut seen: Vec<&str> = Vec::with_capacity(layout.keys.len());
    let mut max_x: f64 = 0.0;
    let mut max_y: f64 = 0.0;
    for key in &layout.keys {
        validate_key(key, layout)?;
        if seen.contains(&key.code.as_str()) {
            return Err(format!("duplicate key code: {}", key.code));
        }
        seen.push(key.code.as_str());
        max_x = max_x.max(key.x + key.w);
        max_y = max_y.max(key.y + key.h);
    }
    if (max_x - layout.width).abs() > DIMENSION_TOLERANCE
        || (max_y - layout.height).abs() > DIMENSION_TOLERANCE
    {
        return Err(format!(
            "declared size {}x{} does not match keys {}x{}",
            layout.width, layout.height, max_x, max_y
        ));
    }
    // Overlap check: no two keys may share area.
    for (i, a) in layout.keys.iter().enumerate() {
        for b in layout.keys.iter().skip(i + 1) {
            if a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h {
                return Err(format!("overlapping keys: {} / {}", a.code, b.code));
            }
        }
    }
    Ok(())
}

fn validate_key(key: &ImportedKey, layout: &ImportedLayout) -> Result<(), String> {
    // Known code, or Fn marked unsupported (the only unsupported key
    // the generator emits). Any other unknown code is rejected.
    if key.code == "Fn" {
        if !key.unsupported {
            return Err("Fn key must be marked unsupported".to_string());
        }
    } else {
        if key.unsupported {
            return Err(format!("key {} must not be marked unsupported", key.code));
        }
        if KeyCode::from_id(&key.code).is_none() {
            return Err(format!("unknown key code: {}", key.code));
        }
    }
    if key.vk.is_empty() || key.vk.chars().count() > 32 {
        return Err(format!("invalid vk for {}: {:?}", key.code, key.vk));
    }
    if key.vk_code > 0xFF {
        return Err(format!("invalid vkCode for {}: {}", key.code, key.vk_code));
    }
    // The Space key has an empty label by design (blank spacebar);
    // only the length is bounded.
    if key.label.chars().count() > 32 {
        return Err(format!("invalid label for {}", key.code));
    }
    if let Some(sub) = &key.sub_label {
        if sub.chars().count() > 32 {
            return Err(format!("invalid subLabel for {}", key.code));
        }
    }
    if key.category.is_empty() || key.category.chars().count() > 32 {
        return Err(format!("invalid category for {}", key.code));
    }
    if !(key.x >= 0.0 && key.x <= MAX_COORD && key.y >= 0.0 && key.y <= MAX_COORD) {
        return Err(format!(
            "key {} out of range: {},{}",
            key.code, key.x, key.y
        ));
    }
    if !(key.w > 0.0 && key.w <= MAX_KEY_SIZE && key.h > 0.0 && key.h <= MAX_KEY_SIZE) {
        return Err(format!(
            "key {} has invalid size: {}x{}",
            key.code, key.w, key.h
        ));
    }
    if key.x + key.w > layout.width + DIMENSION_TOLERANCE
        || key.y + key.h > layout.height + DIMENSION_TOLERANCE
    {
        return Err(format!("key {} exceeds layout bounds", key.code));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generated ISO layout is a valid import (minus its builtin id,
    /// which is swapped for a custom one here).
    fn iso_json_with_id(id: &str) -> String {
        let raw = include_str!("../../../src/layouts/fullsize-iso.json");
        let mut value: serde_json::Value = serde_json::from_str(raw).unwrap();
        value["id"] = serde_json::Value::String(id.to_string());
        serde_json::to_string(&value).unwrap()
    }

    #[test]
    fn valid_iso_layout_imports() {
        let layout = validate_imported_layout(&iso_json_with_id("my-iso")).unwrap();
        assert_eq!(layout.id, "my-iso");
        assert_eq!(layout.keys.len(), 105);
        assert!(layout.keys.iter().any(|k| k.code == "IntlBackslash"));
    }

    #[test]
    fn valid_jis_layout_imports() {
        let raw = include_str!("../../../src/layouts/fullsize-jis.json");
        let mut value: serde_json::Value = serde_json::from_str(raw).unwrap();
        value["id"] = serde_json::Value::String("my-jis".to_string());
        let layout = validate_imported_layout(&serde_json::to_string(&value).unwrap()).unwrap();
        assert_eq!(layout.keys.len(), 109);
        assert!(layout.keys.iter().any(|k| k.code == "Convert"));
        assert!(layout.keys.iter().any(|k| k.code == "IntlYen"));
    }

    fn mutate_iso<F: FnOnce(&mut serde_json::Value)>(f: F) -> String {
        let raw = iso_json_with_id("my-iso");
        let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        f(&mut value);
        serde_json::to_string(&value).unwrap()
    }

    #[test]
    fn rejects_builtin_id_collision() {
        assert!(validate_imported_layout(&iso_json_with_id("fullsize-iso")).is_err());
        assert!(validate_imported_layout(&iso_json_with_id("tkl-ansi")).is_err());
    }

    #[test]
    fn rejects_bad_id_format() {
        assert!(validate_imported_layout(&iso_json_with_id("My_Layout")).is_err());
        assert!(validate_imported_layout(&iso_json_with_id("")).is_err());
    }

    #[test]
    fn rejects_oversize_json() {
        let big = " ".repeat(MAX_IMPORT_BYTES + 1);
        assert!(validate_imported_layout(&big).is_err());
    }

    #[test]
    fn rejects_too_many_keys() {
        let raw = mutate_iso(|v| {
            let keys = v["keys"].as_array().unwrap().clone();
            let mut extra = keys.clone();
            extra.truncate(MAX_IMPORT_KEYS + 1 - keys.len());
            let mut all = keys;
            all.extend(extra);
            v["keys"] = serde_json::Value::Array(all);
        });
        // Duplicates also fail, but the count check fires first.
        assert!(validate_imported_layout(&raw).is_err());
    }

    #[test]
    fn rejects_out_of_range_coordinate() {
        let raw = mutate_iso(|v| {
            v["keys"][0]["x"] = serde_json::Value::from(99.0);
        });
        assert!(validate_imported_layout(&raw).is_err());
    }

    #[test]
    fn rejects_duplicate_code() {
        let raw = mutate_iso(|v| {
            let first = v["keys"][0].clone();
            v["keys"][1] = first;
        });
        assert!(validate_imported_layout(&raw).is_err());
    }

    #[test]
    fn rejects_unknown_key_code() {
        let raw = mutate_iso(|v| {
            v["keys"][0]["code"] = serde_json::Value::String("NotAKey".to_string());
        });
        assert!(validate_imported_layout(&raw).is_err());
    }

    #[test]
    fn rejects_unknown_field() {
        let raw = mutate_iso(|v| {
            v["evil"] = serde_json::Value::Bool(true);
        });
        assert!(validate_imported_layout(&raw).is_err());
        let raw = mutate_iso(|v| {
            v["keys"][0]["evil"] = serde_json::Value::Bool(true);
        });
        assert!(validate_imported_layout(&raw).is_err());
    }

    #[test]
    fn rejects_overlapping_keys() {
        let raw = mutate_iso(|v| {
            v["keys"][1]["x"] = v["keys"][0]["x"].clone();
            v["keys"][1]["y"] = v["keys"][0]["y"].clone();
        });
        assert!(validate_imported_layout(&raw).is_err());
    }

    #[test]
    fn rejects_dimension_mismatch() {
        let raw = mutate_iso(|v| {
            v["width"] = serde_json::Value::from(99.0);
        });
        assert!(validate_imported_layout(&raw).is_err());
    }

    #[test]
    fn fn_key_requires_unsupported_flag() {
        // A 75-ansi layout (has Fn) imports fine with its flag...
        let raw = include_str!("../../../src/layouts/75-ansi.json");
        let mut value: serde_json::Value = serde_json::from_str(raw).unwrap();
        value["id"] = serde_json::Value::String("my-75".to_string());
        assert!(validate_imported_layout(&serde_json::to_string(&value).unwrap()).is_ok());
        // ...but Fn without unsupported is rejected.
        let raw = mutate_iso(|v| {
            v["keys"][0]["code"] = serde_json::Value::String("Fn".to_string());
        });
        assert!(validate_imported_layout(&raw).is_err());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(validate_imported_layout("{not json").is_err());
        assert!(validate_imported_layout("").is_err());
    }
}
