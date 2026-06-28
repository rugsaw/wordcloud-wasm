//! Core data models for the layout engine.
//!
//! Three layers of types live here:
//!
//! * [`Item`] — the weighted-text **input** coming from JavaScript.
//! * [`WordPlacement`] — the compact, `Copy`, cache-friendly struct used in the
//!   **hot layout loops**. Stored contiguously as `Vec<WordPlacement>`.
//! * [`Placement`] — the **output** handed back to JavaScript, serialized to the
//!   camelCase shape described in the spec's *Output Model*.
//!
//! [`LayoutConfig`] carries canvas and font parameters; it deserializes from a
//! JS options object with sensible defaults for every field.

use serde::{Deserialize, Serialize};

/// A single weighted text input item.
///
/// Matches the spec's *Input Model*:
/// ```json
/// { "text": "WebAssembly", "weight": 100 }
/// ```
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Item {
    /// The word/label to lay out.
    pub text: String,
    /// Relative importance; drives font size and placement order.
    pub weight: f32,
}

/// Compact, `Copy` placement record used inside the layout hot paths.
///
/// Kept POD-like (no heap fields) so a layout can hold a contiguous
/// `Vec<WordPlacement>` for cache-friendly iteration, as called for in the
/// spec's *Memory Layout* section. The owning word's text is tracked out of
/// band (by index) so this struct stays small.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WordPlacement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    /// Rotation in degrees, stored compactly (e.g. 0 or 90).
    pub rotation: u8,
}

/// Public, JSON-serializable output record.
///
/// Matches the spec's *Output Model*; note the camelCase `fontSize`:
/// ```json
/// { "text": "WebAssembly", "x": 120, "y": 80, "rotation": 0, "fontSize": 48 }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub rotation: u8,
    pub font_size: f32,
}

impl Placement {
    /// Build an output [`Placement`] from an internal [`WordPlacement`] and its
    /// text.
    pub fn from_word_placement(text: String, wp: &WordPlacement) -> Self {
        Placement {
            text,
            x: wp.x,
            y: wp.y,
            rotation: wp.rotation,
            font_size: wp.font_size,
        }
    }
}

/// Canvas and font parameters controlling a layout run.
///
/// Deserializes from a JS options object (camelCase keys). Every field has a
/// default, so an empty `{}` yields a usable configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LayoutConfig {
    /// Canvas width in pixels.
    pub width: f32,
    /// Canvas height in pixels.
    pub height: f32,
    /// Smallest font size (px) assigned to the lowest-weight word.
    pub min_font_size: f32,
    /// Largest font size (px) assigned to the highest-weight word.
    pub max_font_size: f32,
    /// Padding (px) added around each word's footprint to prevent crowding.
    pub padding: f32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        LayoutConfig {
            width: 1024.0,
            height: 768.0,
            min_font_size: 12.0,
            max_font_size: 96.0,
            padding: 2.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_deserializes_from_spec_input() {
        let json = r#"[
            { "text": "WebAssembly", "weight": 100 },
            { "text": "Rust", "weight": 80 }
        ]"#;
        let items: Vec<Item> = serde_json::from_str(json).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], Item { text: "WebAssembly".into(), weight: 100.0 });
        assert_eq!(items[1].text, "Rust");
    }

    #[test]
    fn placement_serializes_to_camel_case() {
        let p = Placement {
            text: "WebAssembly".into(),
            x: 120.0,
            y: 80.0,
            rotation: 0,
            font_size: 48.0,
        };
        let value: serde_json::Value = serde_json::to_value(&p).unwrap();
        // The spec's output uses `fontSize`, not `font_size`.
        assert!(value.get("fontSize").is_some());
        assert!(value.get("font_size").is_none());
        assert_eq!(value["fontSize"], 48.0);
        assert_eq!(value["text"], "WebAssembly");
    }

    #[test]
    fn placements_round_trip() {
        let placements = vec![
            Placement { text: "a".into(), x: 1.0, y: 2.0, rotation: 0, font_size: 10.0 },
            Placement { text: "b".into(), x: 3.0, y: 4.0, rotation: 90, font_size: 20.0 },
        ];
        let json = serde_json::to_string(&placements).unwrap();
        let back: Vec<Placement> = serde_json::from_str(&json).unwrap();
        assert_eq!(placements, back);
    }

    #[test]
    fn layout_config_defaults_apply_for_empty_object() {
        let cfg: LayoutConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg, LayoutConfig::default());
    }

    #[test]
    fn layout_config_partial_override_keeps_other_defaults() {
        let cfg: LayoutConfig = serde_json::from_str(r#"{ "width": 500, "maxFontSize": 64 }"#).unwrap();
        assert_eq!(cfg.width, 500.0);
        assert_eq!(cfg.max_font_size, 64.0);
        // Untouched fields fall back to defaults.
        assert_eq!(cfg.height, LayoutConfig::default().height);
        assert_eq!(cfg.padding, LayoutConfig::default().padding);
    }

    #[test]
    fn placement_built_from_word_placement() {
        let wp = WordPlacement { x: 5.0, y: 6.0, width: 30.0, height: 12.0, font_size: 24.0, rotation: 90 };
        let p = Placement::from_word_placement("hello".into(), &wp);
        assert_eq!(p, Placement { text: "hello".into(), x: 5.0, y: 6.0, rotation: 90, font_size: 24.0 });
    }
}
