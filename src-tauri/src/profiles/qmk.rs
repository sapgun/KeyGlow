//! S9: QMK / VIA keymap import and export (pure conversion, no Win32).
//!
//! KeyGlow is a Windows keyboard-control app, not a firmware tool:
//! flashing QMK firmware is out of scope. What this module does is
//! translate between a profile's keymap (base disabled/remap + layers)
//! and the two keymap formats keyboards actually ship in:
//!
//! - **QMK `keymap.json`**: `{"keyboard", "keymap", "layout":
//!   "LAYOUT_60_ansi", "layers": [[...], ...]}`. Each layer is one
//!   entry per physical key, in the layout's position order (row-major,
//!   `(y, x)` sorted — the frontend passes `key_order` from the layout
//!   registry, so the backend never has to know a layout's geometry).
//!   `XXXXXXX` is `KC_NO` (disabled), `_______` is `KC_TRNS` (fall
//!   through to the layer below). `KC_MO(n)` / `KC_TG(n)` switch to
//!   layer `n` (hold / toggle). A layer's entries resolve top-down:
//!   `KC_TRNS` inherits the first non-transparent entry below it.
//! - **VIA JSON**: the same `layers` shape, but entries are 16-bit hex
//!   strings (`"0x0004"`). Only basic HID usages are supported
//!   (`0x04`–`0xE7`); `0x0000` is `KC_NO`, `0x0001` is `KC_TRNS`.
//!   VIA's 16-bit layer codes (`MO(n)`/`TG(n)`/...) are **not**
//!   supported: their numeric values differ between QMK versions and
//!   no authoritative table could be verified, so guessing would
//!   silently mistranslate a keymap. VIA files that contain one are
//!   rejected with a reason, and the user is pointed at QMK JSON.
//!
//! Import is destructive by design and says so: it replaces the
//! profile's base disabled/remap set (from layer 0) and its layers
//! (from layers 1..n). Macros are untouched. Every import is validated
//! before anything is stored — unknown keycodes, unsupported constructs
//! (`TO`/`DF`/`OSL`/`LT`/`LM`/`TT`/`MT`/mod-taps), a layer with no
//! switch key, two switch keys for one layer, a switch key that can
//! never be reached, or a keycode the profile cannot express (a
//! `Pause` or toggle-key target) are all rejected with the reason.
//! The produced profile is run through the same `validate_layer` /
//! `validate_remap_rule` checks the UI uses, so an imported profile
//! always passes `normalize`.

use std::collections::HashSet;

use serde::Serialize;

use crate::keyboard::KeyCode;
use crate::profiles::models::{
    is_toggle_key, validate_layer, validate_remap_rule, LayerDef, LayerMode, Profile, RemapRule,
};

/// Maximum keymap JSON size (S6 importer pattern).
pub const MAX_KEYMAP_JSON_BYTES: usize = 64 * 1024;
/// Maximum QMK/VIA layers (QMK itself allows up to 32; KeyGlow layers
/// are capped at 8 by the profile model, and the extra headroom here
/// only exists so the error can name the real cap).
pub const MAX_IMPORT_LAYERS: usize = 9;

/// One layer of the import preview, for the confirmation dialog.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QmkLayerSummary {
    pub index: usize,
    pub switch_key: Option<String>,
    pub switch_mode: Option<String>,
    pub remap_count: usize,
    pub disabled_count: usize,
}

/// What the import dialog shows before applying: the keymap's identity
/// and a per-layer summary. Nothing is stored by validation.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QmkImportPreview {
    pub keyboard: Option<String>,
    pub keymap: Option<String>,
    pub layout: String,
    pub layers: Vec<QmkLayerSummary>,
    pub base_remap_count: usize,
    pub base_disabled_count: usize,
}

/// The validated result of an import: a profile's new base keymap and
/// layers. The command applies it (and bumps `runtime_revision`).
#[derive(Clone, Debug)]
pub struct QmkImport {
    pub base_disabled: Vec<String>,
    pub base_remappings: Vec<RemapRule>,
    pub layers: Vec<LayerDef>,
}

/// One parsed layer entry, after the format-specific decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    /// `KC_NO` / `0x0000`: the key does nothing in this layer.
    No,
    /// `KC_TRNS` / `0x0001`: fall through to the layer below.
    Trns,
    /// A plain keycode.
    Key(KeyCode),
    /// `KC_MO(n)` (hold) or `KC_TG(n)` (toggle) to layer `n`.
    Switch { target: usize, toggle: bool },
}

struct ParsedKeymap {
    keyboard: Option<String>,
    keymap: Option<String>,
    layout: String,
    layers: Vec<Vec<Entry>>,
}

fn validate_key_order(key_order: &[String]) -> Result<Vec<KeyCode>, String> {
    if key_order.is_empty() || key_order.len() > 120 {
        return Err(format!(
            "layout key order has {} keys; expected 1..=120",
            key_order.len()
        ));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(key_order.len());
    for id in key_order {
        let code = KeyCode::from_id(id)
            .ok_or_else(|| format!("unknown key in layout order: {id}"))?;
        if !seen.insert(code) {
            return Err(format!("duplicate key in layout order: {id}"));
        }
        out.push(code);
    }
    Ok(out)
}

fn normalize_layout_name(name: &str) -> String {
    let name = name.strip_prefix("LAYOUT_").unwrap_or(name);
    name.replace('_', "-").to_lowercase()
}

fn check_json_size(raw: &str) -> Result<(), String> {
    if raw.len() > MAX_KEYMAP_JSON_BYTES {
        return Err(format!(
            "keymap JSON is {} bytes; the cap is {}",
            raw.len(),
            MAX_KEYMAP_JSON_BYTES
        ));
    }
    Ok(())
}

// --- QMK keycode tables -----------------------------------------------------

/// Basic QMK keycodes (one physical key). Returns `None` for anything
/// that is not a plain keycode — layer switches, mod-taps, and unknown
/// codes are the caller's problem, with a reason.
fn qmk_basic_code(code: &str) -> Option<KeyCode> {
    Some(match code {
        "KC_A" => KeyCode::KeyA,
        "KC_B" => KeyCode::KeyB,
        "KC_C" => KeyCode::KeyC,
        "KC_D" => KeyCode::KeyD,
        "KC_E" => KeyCode::KeyE,
        "KC_F" => KeyCode::KeyF,
        "KC_G" => KeyCode::KeyG,
        "KC_H" => KeyCode::KeyH,
        "KC_I" => KeyCode::KeyI,
        "KC_J" => KeyCode::KeyJ,
        "KC_K" => KeyCode::KeyK,
        "KC_L" => KeyCode::KeyL,
        "KC_M" => KeyCode::KeyM,
        "KC_N" => KeyCode::KeyN,
        "KC_O" => KeyCode::KeyO,
        "KC_P" => KeyCode::KeyP,
        "KC_Q" => KeyCode::KeyQ,
        "KC_R" => KeyCode::KeyR,
        "KC_S" => KeyCode::KeyS,
        "KC_T" => KeyCode::KeyT,
        "KC_U" => KeyCode::KeyU,
        "KC_V" => KeyCode::KeyV,
        "KC_W" => KeyCode::KeyW,
        "KC_X" => KeyCode::KeyX,
        "KC_Y" => KeyCode::KeyY,
        "KC_Z" => KeyCode::KeyZ,
        "KC_1" => KeyCode::Digit1,
        "KC_2" => KeyCode::Digit2,
        "KC_3" => KeyCode::Digit3,
        "KC_4" => KeyCode::Digit4,
        "KC_5" => KeyCode::Digit5,
        "KC_6" => KeyCode::Digit6,
        "KC_7" => KeyCode::Digit7,
        "KC_8" => KeyCode::Digit8,
        "KC_9" => KeyCode::Digit9,
        "KC_0" => KeyCode::Digit0,
        "KC_ENTER" | "KC_ENT" => KeyCode::Enter,
        "KC_ESCAPE" | "KC_ESC" => KeyCode::Escape,
        "KC_BACKSPACE" | "KC_BSPC" => KeyCode::Backspace,
        "KC_TAB" => KeyCode::Tab,
        "KC_SPACE" | "KC_SPC" => KeyCode::Space,
        "KC_MINUS" | "KC_MINS" => KeyCode::Minus,
        "KC_EQUAL" | "KC_EQL" => KeyCode::Equal,
        "KC_LEFT_BRACKET" | "KC_LBRC" => KeyCode::BracketLeft,
        "KC_RIGHT_BRACKET" | "KC_RBRC" => KeyCode::BracketRight,
        "KC_BACKSLASH" | "KC_BSLS" => KeyCode::Backslash,
        // ISO `#~` (next to Enter) is the same physical key as the ANSI
        // backslash (scan 0x2B); the ISO extra key (between Left Shift
        // and Z, scan 0x56) is `KC_NUBS`.
        "KC_NONUS_HASH" | "KC_NUHS" => KeyCode::Backslash,
        "KC_NONUS_BACKSLASH" | "KC_NUBS" => KeyCode::IntlBackslash,
        "KC_GRAVE" | "KC_GRV" => KeyCode::Backquote,
        "KC_SEMICOLON" | "KC_SCLN" => KeyCode::Semicolon,
        "KC_QUOTE" | "KC_QUOT" => KeyCode::Quote,
        "KC_COMMA" | "KC_COMM" => KeyCode::Comma,
        "KC_DOT" => KeyCode::Period,
        "KC_SLASH" | "KC_SLSH" => KeyCode::Slash,
        "KC_CAPS_LOCK" | "KC_CAPS" => KeyCode::CapsLock,
        "KC_F1" => KeyCode::F1,
        "KC_F2" => KeyCode::F2,
        "KC_F3" => KeyCode::F3,
        "KC_F4" => KeyCode::F4,
        "KC_F5" => KeyCode::F5,
        "KC_F6" => KeyCode::F6,
        "KC_F7" => KeyCode::F7,
        "KC_F8" => KeyCode::F8,
        "KC_F9" => KeyCode::F9,
        "KC_F10" => KeyCode::F10,
        "KC_F11" => KeyCode::F11,
        "KC_F12" => KeyCode::F12,
        "KC_PRINT_SCREEN" | "KC_PSCR" => KeyCode::PrintScreen,
        "KC_SCROLL_LOCK" | "KC_SCRL" => KeyCode::ScrollLock,
        "KC_PAUSE" | "KC_PAUS" => KeyCode::Pause,
        "KC_INSERT" | "KC_INS" => KeyCode::Insert,
        "KC_DELETE" | "KC_DEL" => KeyCode::Delete,
        "KC_HOME" => KeyCode::Home,
        "KC_END" => KeyCode::End,
        "KC_PAGE_UP" | "KC_PGUP" => KeyCode::PageUp,
        "KC_PAGE_DOWN" | "KC_PGDN" => KeyCode::PageDown,
        "KC_RIGHT" | "KC_RGHT" => KeyCode::ArrowRight,
        "KC_LEFT" => KeyCode::ArrowLeft,
        "KC_DOWN" => KeyCode::ArrowDown,
        "KC_UP" => KeyCode::ArrowUp,
        "KC_NUM_LOCK" | "KC_NLCK" => KeyCode::NumLock,
        "KC_KP_SLASH" | "KC_PSLS" => KeyCode::NumpadDivide,
        "KC_KP_ASTERISK" | "KC_PAST" => KeyCode::NumpadMultiply,
        "KC_KP_MINUS" | "KC_PMNS" => KeyCode::NumpadSubtract,
        "KC_KP_PLUS" | "KC_PPLS" => KeyCode::NumpadAdd,
        "KC_KP_ENTER" | "KC_PENT" => KeyCode::NumpadEnter,
        "KC_KP_1" | "KC_P1" => KeyCode::Numpad1,
        "KC_KP_2" | "KC_P2" => KeyCode::Numpad2,
        "KC_KP_3" | "KC_P3" => KeyCode::Numpad3,
        "KC_KP_4" | "KC_P4" => KeyCode::Numpad4,
        "KC_KP_5" | "KC_P5" => KeyCode::Numpad5,
        "KC_KP_6" | "KC_P6" => KeyCode::Numpad6,
        "KC_KP_7" | "KC_P7" => KeyCode::Numpad7,
        "KC_KP_8" | "KC_P8" => KeyCode::Numpad8,
        "KC_KP_9" | "KC_P9" => KeyCode::Numpad9,
        "KC_KP_0" | "KC_P0" => KeyCode::Numpad0,
        "KC_KP_DOT" | "KC_PDOT" => KeyCode::NumpadDecimal,
        "KC_LEFT_SHIFT" | "KC_LSFT" => KeyCode::ShiftLeft,
        "KC_RIGHT_SHIFT" | "KC_RSFT" => KeyCode::ShiftRight,
        "KC_LEFT_CTRL" | "KC_LCTL" => KeyCode::ControlLeft,
        "KC_RIGHT_CTRL" | "KC_RCTL" => KeyCode::ControlRight,
        "KC_LEFT_GUI" | "KC_LGUI" => KeyCode::MetaLeft,
        "KC_RIGHT_GUI" | "KC_RGUI" => KeyCode::MetaRight,
        "KC_LEFT_ALT" | "KC_LALT" => KeyCode::AltLeft,
        "KC_RIGHT_ALT" | "KC_RALT" => KeyCode::AltRight,
        "KC_APPLICATION" | "KC_APP" => KeyCode::ContextMenu,
        // Japanese keys (HID usages 0x87–0x8B).
        "KC_RO" | "KC_INT1" => KeyCode::IntlRo,
        "KC_KANA" | "KC_INT2" => KeyCode::KanaMode,
        "KC_JYEN" | "KC_INT3" => KeyCode::IntlYen,
        "KC_HENK" | "KC_INT4" => KeyCode::Convert,
        "KC_MHEN" | "KC_INT5" => KeyCode::NonConvert,
        _ => return None,
    })
}

/// The canonical QMK spelling of a keycode (used by export).
pub fn keycode_to_qmk(code: KeyCode) -> &'static str {
    match code {
        KeyCode::Escape => "KC_ESC",
        KeyCode::F1 => "KC_F1",
        KeyCode::F2 => "KC_F2",
        KeyCode::F3 => "KC_F3",
        KeyCode::F4 => "KC_F4",
        KeyCode::F5 => "KC_F5",
        KeyCode::F6 => "KC_F6",
        KeyCode::F7 => "KC_F7",
        KeyCode::F8 => "KC_F8",
        KeyCode::F9 => "KC_F9",
        KeyCode::F10 => "KC_F10",
        KeyCode::F11 => "KC_F11",
        KeyCode::F12 => "KC_F12",
        KeyCode::PrintScreen => "KC_PSCR",
        KeyCode::ScrollLock => "KC_SCRL",
        KeyCode::Pause => "KC_PAUS",
        KeyCode::Insert => "KC_INS",
        KeyCode::Delete => "KC_DEL",
        KeyCode::Home => "KC_HOME",
        KeyCode::End => "KC_END",
        KeyCode::PageUp => "KC_PGUP",
        KeyCode::PageDown => "KC_PGDN",
        KeyCode::ArrowRight => "KC_RGHT",
        KeyCode::ArrowLeft => "KC_LEFT",
        KeyCode::ArrowDown => "KC_DOWN",
        KeyCode::ArrowUp => "KC_UP",
        KeyCode::NumLock => "KC_NLCK",
        KeyCode::NumpadDivide => "KC_PSLS",
        KeyCode::NumpadMultiply => "KC_PAST",
        KeyCode::NumpadSubtract => "KC_PMNS",
        KeyCode::NumpadAdd => "KC_PPLS",
        KeyCode::NumpadEnter => "KC_PENT",
        KeyCode::Numpad1 => "KC_P1",
        KeyCode::Numpad2 => "KC_P2",
        KeyCode::Numpad3 => "KC_P3",
        KeyCode::Numpad4 => "KC_P4",
        KeyCode::Numpad5 => "KC_P5",
        KeyCode::Numpad6 => "KC_P6",
        KeyCode::Numpad7 => "KC_P7",
        KeyCode::Numpad8 => "KC_P8",
        KeyCode::Numpad9 => "KC_P9",
        KeyCode::Numpad0 => "KC_P0",
        KeyCode::NumpadDecimal => "KC_PDOT",
        KeyCode::Backspace => "KC_BSPC",
        KeyCode::Tab => "KC_TAB",
        KeyCode::Enter => "KC_ENT",
        KeyCode::Space => "KC_SPC",
        KeyCode::CapsLock => "KC_CAPS",
        KeyCode::ShiftLeft => "KC_LSFT",
        KeyCode::ShiftRight => "KC_RSFT",
        KeyCode::ControlLeft => "KC_LCTL",
        KeyCode::ControlRight => "KC_RCTL",
        KeyCode::MetaLeft => "KC_LGUI",
        KeyCode::MetaRight => "KC_RGUI",
        KeyCode::AltLeft => "KC_LALT",
        KeyCode::AltRight => "KC_RALT",
        KeyCode::ContextMenu => "KC_APP",
        KeyCode::Backquote => "KC_GRV",
        KeyCode::Minus => "KC_MINS",
        KeyCode::Equal => "KC_EQL",
        KeyCode::BracketLeft => "KC_LBRC",
        KeyCode::BracketRight => "KC_RBRC",
        KeyCode::Backslash => "KC_BSLS",
        KeyCode::Semicolon => "KC_SCLN",
        KeyCode::Quote => "KC_QUOT",
        KeyCode::Comma => "KC_COMM",
        KeyCode::Period => "KC_DOT",
        KeyCode::Slash => "KC_SLSH",
        KeyCode::IntlBackslash => "KC_NUBS",
        KeyCode::IntlYen => "KC_JYEN",
        KeyCode::IntlRo => "KC_RO",
        KeyCode::Convert => "KC_HENK",
        KeyCode::NonConvert => "KC_MHEN",
        KeyCode::KanaMode => "KC_KANA",
        KeyCode::KeyA => "KC_A",
        KeyCode::KeyB => "KC_B",
        KeyCode::KeyC => "KC_C",
        KeyCode::KeyD => "KC_D",
        KeyCode::KeyE => "KC_E",
        KeyCode::KeyF => "KC_F",
        KeyCode::KeyG => "KC_G",
        KeyCode::KeyH => "KC_H",
        KeyCode::KeyI => "KC_I",
        KeyCode::KeyJ => "KC_J",
        KeyCode::KeyK => "KC_K",
        KeyCode::KeyL => "KC_L",
        KeyCode::KeyM => "KC_M",
        KeyCode::KeyN => "KC_N",
        KeyCode::KeyO => "KC_O",
        KeyCode::KeyP => "KC_P",
        KeyCode::KeyQ => "KC_Q",
        KeyCode::KeyR => "KC_R",
        KeyCode::KeyS => "KC_S",
        KeyCode::KeyT => "KC_T",
        KeyCode::KeyU => "KC_U",
        KeyCode::KeyV => "KC_V",
        KeyCode::KeyW => "KC_W",
        KeyCode::KeyX => "KC_X",
        KeyCode::KeyY => "KC_Y",
        KeyCode::KeyZ => "KC_Z",
        KeyCode::Digit1 => "KC_1",
        KeyCode::Digit2 => "KC_2",
        KeyCode::Digit3 => "KC_3",
        KeyCode::Digit4 => "KC_4",
        KeyCode::Digit5 => "KC_5",
        KeyCode::Digit6 => "KC_6",
        KeyCode::Digit7 => "KC_7",
        KeyCode::Digit8 => "KC_8",
        KeyCode::Digit9 => "KC_9",
        KeyCode::Digit0 => "KC_0",
    }
}

// --- VIA usage table ---------------------------------------------------------

/// A VIA hex code to a keycode. Only basic HID usages are supported;
/// `None` means "not a basic keycode" (layer codes, vendor codes, ...).
fn via_usage_to_keycode(usage: u32) -> Option<KeyCode> {
    Some(match usage {
        0x04..=0x1D => {
            const LETTERS: [KeyCode; 26] = [
                KeyCode::KeyA,
                KeyCode::KeyB,
                KeyCode::KeyC,
                KeyCode::KeyD,
                KeyCode::KeyE,
                KeyCode::KeyF,
                KeyCode::KeyG,
                KeyCode::KeyH,
                KeyCode::KeyI,
                KeyCode::KeyJ,
                KeyCode::KeyK,
                KeyCode::KeyL,
                KeyCode::KeyM,
                KeyCode::KeyN,
                KeyCode::KeyO,
                KeyCode::KeyP,
                KeyCode::KeyQ,
                KeyCode::KeyR,
                KeyCode::KeyS,
                KeyCode::KeyT,
                KeyCode::KeyU,
                KeyCode::KeyV,
                KeyCode::KeyW,
                KeyCode::KeyX,
                KeyCode::KeyY,
                KeyCode::KeyZ,
            ];
            LETTERS[(usage - 0x04) as usize]
        }
        0x1E..=0x27 => {
            const DIGITS: [KeyCode; 10] = [
                KeyCode::Digit1,
                KeyCode::Digit2,
                KeyCode::Digit3,
                KeyCode::Digit4,
                KeyCode::Digit5,
                KeyCode::Digit6,
                KeyCode::Digit7,
                KeyCode::Digit8,
                KeyCode::Digit9,
                KeyCode::Digit0,
            ];
            DIGITS[(usage - 0x1E) as usize]
        }
        0x28 => KeyCode::Enter,
        0x29 => KeyCode::Escape,
        0x2A => KeyCode::Backspace,
        0x2B => KeyCode::Tab,
        0x2C => KeyCode::Space,
        0x2D => KeyCode::Minus,
        0x2E => KeyCode::Equal,
        0x2F => KeyCode::BracketLeft,
        0x30 => KeyCode::BracketRight,
        0x31 => KeyCode::Backslash,
        // 0x32 (Non-US #~) is the same physical key as backslash.
        0x32 => KeyCode::Backslash,
        0x33 => KeyCode::Semicolon,
        0x34 => KeyCode::Quote,
        0x35 => KeyCode::Backquote,
        0x36 => KeyCode::Comma,
        0x37 => KeyCode::Period,
        0x38 => KeyCode::Slash,
        0x39 => KeyCode::CapsLock,
        0x3A..=0x45 => {
            const FKEYS: [KeyCode; 12] = [
                KeyCode::F1,
                KeyCode::F2,
                KeyCode::F3,
                KeyCode::F4,
                KeyCode::F5,
                KeyCode::F6,
                KeyCode::F7,
                KeyCode::F8,
                KeyCode::F9,
                KeyCode::F10,
                KeyCode::F11,
                KeyCode::F12,
            ];
            FKEYS[(usage - 0x3A) as usize]
        }
        0x46 => KeyCode::PrintScreen,
        0x47 => KeyCode::ScrollLock,
        0x48 => KeyCode::Pause,
        0x49 => KeyCode::Insert,
        0x4A => KeyCode::Home,
        0x4B => KeyCode::PageUp,
        0x4C => KeyCode::Delete,
        0x4D => KeyCode::End,
        0x4E => KeyCode::PageDown,
        0x4F => KeyCode::ArrowRight,
        0x50 => KeyCode::ArrowLeft,
        0x51 => KeyCode::ArrowDown,
        0x52 => KeyCode::ArrowUp,
        0x53 => KeyCode::NumLock,
        0x54 => KeyCode::NumpadDivide,
        0x55 => KeyCode::NumpadMultiply,
        0x56 => KeyCode::NumpadSubtract,
        0x57 => KeyCode::NumpadAdd,
        0x58 => KeyCode::NumpadEnter,
        0x59 => KeyCode::Numpad1,
        0x5A => KeyCode::Numpad2,
        0x5B => KeyCode::Numpad3,
        0x5C => KeyCode::Numpad4,
        0x5D => KeyCode::Numpad5,
        0x5E => KeyCode::Numpad6,
        0x5F => KeyCode::Numpad7,
        0x60 => KeyCode::Numpad8,
        0x61 => KeyCode::Numpad9,
        0x62 => KeyCode::Numpad0,
        0x63 => KeyCode::NumpadDecimal,
        0x64 => KeyCode::IntlBackslash,
        0x65 => KeyCode::ContextMenu,
        0x87 => KeyCode::IntlRo,
        0x88 => KeyCode::KanaMode,
        0x89 => KeyCode::IntlYen,
        0x8A => KeyCode::Convert,
        0x8B => KeyCode::NonConvert,
        0xE0 => KeyCode::ControlLeft,
        0xE1 => KeyCode::ShiftLeft,
        0xE2 => KeyCode::AltLeft,
        0xE3 => KeyCode::MetaLeft,
        0xE4 => KeyCode::ControlRight,
        0xE5 => KeyCode::ShiftRight,
        0xE6 => KeyCode::AltRight,
        0xE7 => KeyCode::MetaRight,
        _ => return None,
    })
}

/// The VIA hex usage of a keycode, if it has a basic HID usage.
fn keycode_to_via_usage(code: KeyCode) -> Option<u32> {
    Some(match code {
        KeyCode::KeyA => 0x04,
        KeyCode::KeyB => 0x05,
        KeyCode::KeyC => 0x06,
        KeyCode::KeyD => 0x07,
        KeyCode::KeyE => 0x08,
        KeyCode::KeyF => 0x09,
        KeyCode::KeyG => 0x0A,
        KeyCode::KeyH => 0x0B,
        KeyCode::KeyI => 0x0C,
        KeyCode::KeyJ => 0x0D,
        KeyCode::KeyK => 0x0E,
        KeyCode::KeyL => 0x0F,
        KeyCode::KeyM => 0x10,
        KeyCode::KeyN => 0x11,
        KeyCode::KeyO => 0x12,
        KeyCode::KeyP => 0x13,
        KeyCode::KeyQ => 0x14,
        KeyCode::KeyR => 0x15,
        KeyCode::KeyS => 0x16,
        KeyCode::KeyT => 0x17,
        KeyCode::KeyU => 0x18,
        KeyCode::KeyV => 0x19,
        KeyCode::KeyW => 0x1A,
        KeyCode::KeyX => 0x1B,
        KeyCode::KeyY => 0x1C,
        KeyCode::KeyZ => 0x1D,
        KeyCode::Digit1 => 0x1E,
        KeyCode::Digit2 => 0x1F,
        KeyCode::Digit3 => 0x20,
        KeyCode::Digit4 => 0x21,
        KeyCode::Digit5 => 0x22,
        KeyCode::Digit6 => 0x23,
        KeyCode::Digit7 => 0x24,
        KeyCode::Digit8 => 0x25,
        KeyCode::Digit9 => 0x26,
        KeyCode::Digit0 => 0x27,
        KeyCode::Enter => 0x28,
        KeyCode::Escape => 0x29,
        KeyCode::Backspace => 0x2A,
        KeyCode::Tab => 0x2B,
        KeyCode::Space => 0x2C,
        KeyCode::Minus => 0x2D,
        KeyCode::Equal => 0x2E,
        KeyCode::BracketLeft => 0x2F,
        KeyCode::BracketRight => 0x30,
        KeyCode::Backslash => 0x31,
        KeyCode::Semicolon => 0x33,
        KeyCode::Quote => 0x34,
        KeyCode::Backquote => 0x35,
        KeyCode::Comma => 0x36,
        KeyCode::Period => 0x37,
        KeyCode::Slash => 0x38,
        KeyCode::CapsLock => 0x39,
        KeyCode::F1 => 0x3A,
        KeyCode::F2 => 0x3B,
        KeyCode::F3 => 0x3C,
        KeyCode::F4 => 0x3D,
        KeyCode::F5 => 0x3E,
        KeyCode::F6 => 0x3F,
        KeyCode::F7 => 0x40,
        KeyCode::F8 => 0x41,
        KeyCode::F9 => 0x42,
        KeyCode::F10 => 0x43,
        KeyCode::F11 => 0x44,
        KeyCode::F12 => 0x45,
        KeyCode::PrintScreen => 0x46,
        KeyCode::ScrollLock => 0x47,
        KeyCode::Pause => 0x48,
        KeyCode::Insert => 0x49,
        KeyCode::Home => 0x4A,
        KeyCode::PageUp => 0x4B,
        KeyCode::Delete => 0x4C,
        KeyCode::End => 0x4D,
        KeyCode::PageDown => 0x4E,
        KeyCode::ArrowRight => 0x4F,
        KeyCode::ArrowLeft => 0x50,
        KeyCode::ArrowDown => 0x51,
        KeyCode::ArrowUp => 0x52,
        KeyCode::NumLock => 0x53,
        KeyCode::NumpadDivide => 0x54,
        KeyCode::NumpadMultiply => 0x55,
        KeyCode::NumpadSubtract => 0x56,
        KeyCode::NumpadAdd => 0x57,
        KeyCode::NumpadEnter => 0x58,
        KeyCode::Numpad1 => 0x59,
        KeyCode::Numpad2 => 0x5A,
        KeyCode::Numpad3 => 0x5B,
        KeyCode::Numpad4 => 0x5C,
        KeyCode::Numpad5 => 0x5D,
        KeyCode::Numpad6 => 0x5E,
        KeyCode::Numpad7 => 0x5F,
        KeyCode::Numpad8 => 0x60,
        KeyCode::Numpad9 => 0x61,
        KeyCode::Numpad0 => 0x62,
        KeyCode::NumpadDecimal => 0x63,
        KeyCode::IntlBackslash => 0x64,
        KeyCode::ContextMenu => 0x65,
        KeyCode::IntlRo => 0x87,
        KeyCode::KanaMode => 0x88,
        KeyCode::IntlYen => 0x89,
        KeyCode::Convert => 0x8A,
        KeyCode::NonConvert => 0x8B,
        KeyCode::ShiftLeft => 0xE1,
        KeyCode::ShiftRight => 0xE5,
        KeyCode::ControlLeft => 0xE0,
        KeyCode::ControlRight => 0xE4,
        KeyCode::MetaLeft => 0xE3,
        KeyCode::MetaRight => 0xE7,
        KeyCode::AltLeft => 0xE2,
        KeyCode::AltRight => 0xE6,
    })
}

// --- Parsing -----------------------------------------------------------------

/// Parse one QMK entry string into an `Entry`. `layer`/`pos` name the
/// entry for error messages.
fn parse_qmk_entry(entry: &str, layer: usize, pos: usize) -> Result<Entry, String> {
    let at = format!("layer {layer} position {pos}");
    if entry == "XXXXXXX" || entry == "KC_NO" {
        return Ok(Entry::No);
    }
    if entry == "_______" || entry == "KC_TRNS" {
        return Ok(Entry::Trns);
    }
    // Layer switches: KC_MO(n) / MO(n), KC_TG(n) / TG(n).
    for (prefix, toggle) in [("KC_MO(", false), ("MO(", false), ("KC_TG(", true), ("TG(", true)] {
        if let Some(rest) = entry.strip_prefix(prefix) {
            if let Some(num) = rest.strip_suffix(')') {
                let target: usize = num.trim().parse().map_err(|_| {
                    format!("{at}: bad layer number in {entry:?}")
                })?;
                return Ok(Entry::Switch { target, toggle });
            }
            return Err(format!("{at}: malformed layer key {entry:?}"));
        }
    }
    if let Some(code) = qmk_basic_code(entry) {
        return Ok(Entry::Key(code));
    }
    // Known-but-unsupported constructs, with the reason named.
    for (prefix, reason) in [
        ("KC_TO(", "TO (default layer)"),
        ("TO(", "TO (default layer)"),
        ("KC_DF(", "DF (default layer)"),
        ("DF(", "DF (default layer)"),
        ("KC_OSL(", "OSL (one-shot layer)"),
        ("OSL(", "OSL (one-shot layer)"),
        ("KC_LT(", "LT (tap/hold)"),
        ("LT(", "LT (tap/hold)"),
        ("KC_LM(", "LM (layer modifier)"),
        ("LM(", "LM (layer modifier)"),
        ("KC_TT(", "TT (tap toggle)"),
        ("TT(", "TT (tap toggle)"),
        ("KC_MT(", "MT (mod tap)"),
        ("MT(", "MT (mod tap)"),
        ("LCTL(", "mod-tap / combined keycode"),
        ("LSFT(", "mod-tap / combined keycode"),
        ("LALT(", "mod-tap / combined keycode"),
        ("LGUI(", "mod-tap / combined keycode"),
        ("RCTL(", "mod-tap / combined keycode"),
        ("RSFT(", "mod-tap / combined keycode"),
        ("RALT(", "mod-tap / combined keycode"),
        ("RGUI(", "mod-tap / combined keycode"),
        ("KC_", "unknown QMK keycode"),
    ] {
        if entry.starts_with(prefix) {
            return Err(format!("{at}: {entry:?} is not supported ({reason})"));
        }
    }
    Err(format!("{at}: unknown keycode {entry:?}"))
}

fn parse_qmk_keymap(raw: &str, layout_id: &str, key_order: &[String]) -> Result<ParsedKeymap, String> {
    check_json_size(raw)?;
    let order = validate_key_order(key_order)?;
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| "keymap JSON must be an object".to_string())?;
    let layout = obj
        .get("layout")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "keymap JSON is missing the \"layout\" field".to_string())?;
    if normalize_layout_name(layout) != normalize_layout_name(layout_id) {
        return Err(format!(
            "keymap layout is {layout:?}, but this profile uses {layout_id}; change the profile layout first"
        ));
    }
    let layers = obj
        .get("layers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "keymap JSON is missing the \"layers\" array".to_string())?;
    if layers.is_empty() || layers.len() > MAX_IMPORT_LAYERS {
        return Err(format!(
            "keymap has {} layers; expected 1..={}",
            layers.len(),
            MAX_IMPORT_LAYERS
        ));
    }
    let mut parsed_layers = Vec::with_capacity(layers.len());
    for (i, layer) in layers.iter().enumerate() {
        let entries = layer
            .as_array()
            .ok_or_else(|| format!("layer {i} is not an array"))?;
        if entries.len() != order.len() {
            return Err(format!(
                "layer {i} has {} entries, but layout {layout_id} has {} keys",
                entries.len(),
                order.len()
            ));
        }
        let mut parsed = Vec::with_capacity(entries.len());
        for (p, entry) in entries.iter().enumerate() {
            let entry = entry
                .as_str()
                .ok_or_else(|| format!("layer {i} position {p} is not a string"))?;
            parsed.push(parse_qmk_entry(entry, i, p)?);
        }
        parsed_layers.push(parsed);
    }
    Ok(ParsedKeymap {
        keyboard: obj
            .get("keyboard")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        keymap: obj
            .get("keymap")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        layout: layout_id.to_string(),
        layers: parsed_layers,
    })
}

fn parse_via_entry(value: &serde_json::Value, layer: usize, pos: usize) -> Result<Entry, String> {
    let at = format!("layer {layer} position {pos}");
    let usage: u32 = match value {
        serde_json::Value::String(s) => {
            let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).ok_or_else(|| {
                format!("{at}: VIA entry {s:?} is not a 0x hex code")
            })?;
            u32::from_str_radix(s, 16)
                .map_err(|_| format!("{at}: VIA entry {value} is not a hex code"))?
        }
        serde_json::Value::Number(n) => n
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| format!("{at}: VIA entry {value} is not a keycode"))?,
        _ => return Err(format!("{at}: VIA entry is not a string or number")),
    };
    if usage == 0x0000 {
        return Ok(Entry::No);
    }
    if usage == 0x0001 {
        return Ok(Entry::Trns);
    }
    match via_usage_to_keycode(usage) {
        Some(code) => Ok(Entry::Key(code)),
        None => Err(format!(
            "{at}: VIA code 0x{usage:04X} is not a basic keycode — layer keys (MO/TG/...) cannot be expressed as VIA hex codes; use a QMK keymap.json instead"
        )),
    }
}

fn parse_via_keymap(raw: &str, layout_id: &str, key_order: &[String]) -> Result<ParsedKeymap, String> {
    check_json_size(raw)?;
    let order = validate_key_order(key_order)?;
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| "VIA JSON must be an object".to_string())?;
    // VIA files carry no layout field; the profile's layout is the
    // authority, and other top-level fields (name, vendorProductId,
    // matrix, lighting, macros) are ignored.
    let layers = obj
        .get("layers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "VIA JSON is missing the \"layers\" array".to_string())?;
    if layers.is_empty() || layers.len() > MAX_IMPORT_LAYERS {
        return Err(format!(
            "VIA keymap has {} layers; expected 1..={}",
            layers.len(),
            MAX_IMPORT_LAYERS
        ));
    }
    let mut parsed_layers = Vec::with_capacity(layers.len());
    for (i, layer) in layers.iter().enumerate() {
        let entries = layer
            .as_array()
            .ok_or_else(|| format!("layer {i} is not an array"))?;
        if entries.len() != order.len() {
            return Err(format!(
                "layer {i} has {} entries, but layout {layout_id} has {} keys",
                entries.len(),
                order.len()
            ));
        }
        let mut parsed = Vec::with_capacity(entries.len());
        for (p, entry) in entries.iter().enumerate() {
            parsed.push(parse_via_entry(entry, i, p)?);
        }
        parsed_layers.push(parsed);
    }
    Ok(ParsedKeymap {
        keyboard: obj
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        keymap: None,
        layout: layout_id.to_string(),
        layers: parsed_layers,
    })
}

// --- Conversion ----------------------------------------------------------------

/// Resolve one position's entry in layer `i`: walk down through
/// `KC_TRNS` to the first concrete entry; all-transparent means
/// "identity" (no rule).
fn resolve_entry(parsed: &ParsedKeymap, layer: usize, pos: usize) -> Entry {
    let mut i = layer;
    loop {
        match parsed.layers[i][pos] {
            Entry::Trns => {
                if i == 0 {
                    return Entry::Trns;
                }
                i -= 1;
            }
            other => return other,
        }
    }
}

fn parsed_to_import(
    parsed: &ParsedKeymap,
    layout_id: &str,
    key_order: &[KeyCode],
) -> Result<QmkImport, String> {
    let layer_count = parsed.layers.len();
    // switches[target] = (physical switch key, toggle?)
    let mut switches: Vec<Option<(KeyCode, bool)>> = vec![None; layer_count];
    let mut base_disabled: Vec<String> = vec![];
    let mut base_remappings: Vec<RemapRule> = vec![];
    let mut layers: Vec<LayerDef> = vec![];

    for i in 0..layer_count {
        let mut disabled: Vec<String> = vec![];
        let mut remappings: Vec<RemapRule> = vec![];
        for (p, physical) in key_order.iter().enumerate() {
            match resolve_entry(parsed, i, p) {
                Entry::No => disabled.push(physical.as_str().to_string()),
                Entry::Key(code) => {
                    if code == *physical {
                        // Identity: no rule.
                    } else if code == KeyCode::Pause {
                        return Err(format!(
                            "layer {i} position {p}: {} cannot be a remap target (Pause has no key-up)",
                            physical.as_str()
                        ));
                    } else if is_toggle_key(code.as_str()) {
                        return Err(format!(
                            "layer {i} position {p}: {} cannot be a remap target (toggle key; LED state is not tracked)",
                            code.as_str()
                        ));
                    } else if *physical == KeyCode::Pause {
                        return Err(format!(
                            "layer {i} position {p}: Pause cannot be remapped"
                        ));
                    } else {
                        remappings.push(RemapRule {
                            from: physical.as_str().to_string(),
                            to: code.as_str().to_string(),
                        });
                    }
                }
                Entry::Switch { target, toggle } => {
                    if target == 0 {
                        return Err(format!(
                            "layer {i} position {p}: MO(0)/TG(0) is meaningless — layer 0 is always active"
                        ));
                    }
                    if target >= layer_count {
                        return Err(format!(
                            "layer {i} position {p}: switch to layer {target}, but the keymap has only {layer_count} layers"
                        ));
                    }
                    match switches[target] {
                        Some((existing, existing_toggle)) => {
                            if existing != *physical {
                                return Err(format!(
                                    "layer {target} has two switch keys ({} and {})",
                                    existing.as_str(),
                                    physical.as_str()
                                ));
                            }
                            if existing_toggle != toggle {
                                return Err(format!(
                                    "layer {target} is switched by both MO and TG on {}",
                                    physical.as_str()
                                ));
                            }
                        }
                        None => switches[target] = Some((*physical, toggle)),
                    }
                }
                Entry::Trns => {}
            }
        }
        if i == 0 {
            base_disabled = disabled;
            base_remappings = remappings;
        } else {
            layers.push(LayerDef {
                id: format!("qmk-layer-{i}"),
                name: format!("Layer {i}"),
                switch_key: String::new(), // filled in below
                mode: LayerMode::Hold,
                disabled_keys: disabled,
                remappings,
            });
        }
    }

    // Every layer above 0 needs exactly one switch key, and the key must
    // be reachable: it has to appear as a switch entry in some layer
    // other than the layer itself (otherwise no press can ever reach it).
    for n in 1..layer_count {
        let (switch_key, toggle) = switches[n].ok_or_else(|| {
            format!(
                "layer {n} has no switch key — add KC_MO({n}) or KC_TG({n}) to a lower layer"
            )
        })?;
        let pos = key_order
            .iter()
            .position(|k| *k == switch_key)
            .expect("switch key is in the key order");
        let reachable = (0..layer_count).any(|i| {
            i != n && matches!(parsed.layers[i][pos], Entry::Switch { target, .. } if target == n)
        });
        if !reachable {
            return Err(format!(
                "layer {n}'s switch key {} only appears inside layer {n} itself; it can never be reached",
                switch_key.as_str()
            ));
        }
        if switch_key == KeyCode::Pause {
            return Err(format!(
                "layer {n}'s switch key is Pause, which has no key-up"
            ));
        }
        let layer = &mut layers[n - 1];
        layer.switch_key = switch_key.as_str().to_string();
        layer.mode = if toggle {
            LayerMode::Toggle
        } else {
            LayerMode::Hold
        };
    }

    // Run the produced profile through the same validators the UI uses,
    // so an imported profile always passes `normalize`. Base rules are
    // validated incrementally — each rule against the rules validated
    // before it — because `validate_remap_rule` rejects a source that
    // already has a rule, and a probe holding the rule itself would
    // always fail that check.
    let mut seen_sources: HashSet<&str> = HashSet::new();
    let mut validated_rules: Vec<RemapRule> = vec![];
    for rule in &base_remappings {
        if !seen_sources.insert(rule.from.as_str()) {
            return Err(format!("base remap source {} appears twice", rule.from));
        }
        let probe_rules = Profile {
            id: "qmk-import".into(),
            name: "QMK import".into(),
            layout: layout_id.into(),
            disabled_keys: base_disabled.clone(),
            remappings: validated_rules.clone(),
            macros: vec![],
            layers: layers.clone(),
            builtin: false,
        };
        validate_remap_rule(&rule.from, &rule.to, &probe_rules).map_err(|e| {
            format!("base remap {} -> {} is invalid: {e}", rule.from, rule.to)
        })?;
        validated_rules.push(rule.clone());
    }
    let probe = Profile {
        id: "qmk-import".into(),
        name: "QMK import".into(),
        layout: layout_id.into(),
        disabled_keys: base_disabled.clone(),
        remappings: base_remappings.clone(),
        macros: vec![],
        layers: layers.clone(),
        builtin: false,
    };
    for layer in &layers {
        validate_layer(&probe, layer)
            .map_err(|e| format!("layer {} is invalid: {e}", layer.id))?;
    }
    Ok(QmkImport {
        base_disabled,
        base_remappings,
        layers,
    })
}

fn preview_from_import(parsed: &ParsedKeymap, import: &QmkImport) -> QmkImportPreview {
    let mut layers = Vec::with_capacity(import.layers.len() + 1);
    layers.push(QmkLayerSummary {
        index: 0,
        switch_key: None,
        switch_mode: None,
        remap_count: import.base_remappings.len(),
        disabled_count: import.base_disabled.len(),
    });
    for (i, layer) in import.layers.iter().enumerate() {
        layers.push(QmkLayerSummary {
            index: i + 1,
            switch_key: Some(layer.switch_key.clone()),
            switch_mode: Some(match layer.mode {
                LayerMode::Hold => "hold".to_string(),
                LayerMode::Toggle => "toggle".to_string(),
            }),
            remap_count: layer.remappings.len(),
            disabled_count: layer.disabled_keys.len(),
        });
    }
    QmkImportPreview {
        keyboard: parsed.keyboard.clone(),
        keymap: parsed.keymap.clone(),
        layout: parsed.layout.clone(),
        layers,
        base_remap_count: import.base_remappings.len(),
        base_disabled_count: import.base_disabled.len(),
    }
}

// --- Public API ------------------------------------------------------------------

/// Validate a QMK `keymap.json` without storing anything. Returns the
/// preview the dialog shows before the user confirms.
pub fn validate_qmk_keymap(
    raw: &str,
    layout_id: &str,
    key_order: &[String],
) -> Result<QmkImportPreview, String> {
    let parsed = parse_qmk_keymap(raw, layout_id, key_order)?;
    let order = validate_key_order(key_order)?;
    let import = parsed_to_import(&parsed, layout_id, &order)?;
    Ok(preview_from_import(&parsed, &import))
}

/// Validate and convert a QMK `keymap.json` into a profile keymap.
pub fn qmk_keymap_to_profile(
    raw: &str,
    layout_id: &str,
    key_order: &[String],
) -> Result<QmkImport, String> {
    let parsed = parse_qmk_keymap(raw, layout_id, key_order)?;
    let order = validate_key_order(key_order)?;
    parsed_to_import(&parsed, layout_id, &order)
}

/// Validate a VIA JSON keymap without storing anything.
pub fn validate_via_keymap(
    raw: &str,
    layout_id: &str,
    key_order: &[String],
) -> Result<QmkImportPreview, String> {
    let parsed = parse_via_keymap(raw, layout_id, key_order)?;
    let order = validate_key_order(key_order)?;
    let import = parsed_to_import(&parsed, layout_id, &order)?;
    Ok(preview_from_import(&parsed, &import))
}

/// Validate and convert a VIA JSON keymap into a profile keymap.
pub fn via_keymap_to_profile(
    raw: &str,
    layout_id: &str,
    key_order: &[String],
) -> Result<QmkImport, String> {
    let parsed = parse_via_keymap(raw, layout_id, key_order)?;
    let order = validate_key_order(key_order)?;
    parsed_to_import(&parsed, layout_id, &order)
}

/// Export a profile as a QMK `keymap.json` string. Layer 0 is the base
/// keymap; layers 1..n are the profile's layers. A switch key appears
/// as `KC_MO(n)` / `KC_TG(n)` in every layer except its own (where it
/// falls through with `KC_TRNS`); a position with no rule in a layer
/// is `KC_TRNS`.
pub fn profile_to_qmk_keymap(
    keyboard: &str,
    keymap: &str,
    layout_id: &str,
    key_order: &[String],
    base_disabled: &[String],
    base_remappings: &[RemapRule],
    layers: &[LayerDef],
) -> Result<String, String> {
    let order = validate_key_order(key_order)?;
    let mut out_layers: Vec<Vec<String>> = Vec::with_capacity(layers.len() + 1);
    let switch_of = |key: KeyCode| -> Option<(usize, bool)> {
        layers.iter().enumerate().find_map(|(i, l)| {
            if l.switch_key == key.as_str() {
                Some((i + 1, l.mode == LayerMode::Toggle))
            } else {
                None
            }
        })
    };
    let entry_for = |physical: KeyCode,
                     disabled: &[String],
                     remappings: &[RemapRule]|
     -> String {
        if disabled.iter().any(|k| k == physical.as_str()) {
            return "XXXXXXX".to_string();
        }
        if let Some(rule) = remappings.iter().find(|r| r.from == physical.as_str()) {
            if let Some(to) = KeyCode::from_id(&rule.to) {
                return keycode_to_qmk(to).to_string();
            }
        }
        keycode_to_qmk(physical).to_string()
    };
    // Layer 0: the base keymap.
    let base: Vec<String> = order
        .iter()
        .map(|physical| {
            if let Some((n, toggle)) = switch_of(*physical) {
                return if toggle {
                    format!("KC_TG({n})")
                } else {
                    format!("KC_MO({n})")
                };
            }
            entry_for(*physical, base_disabled, base_remappings)
        })
        .collect();
    out_layers.push(base);
    // Layers 1..n.
    for (i, layer) in layers.iter().enumerate() {
        let layer_index = i + 1;
        let entries: Vec<String> = order
            .iter()
            .map(|physical| {
                if let Some((n, toggle)) = switch_of(*physical) {
                    if n != layer_index {
                        return if toggle {
                            format!("KC_TG({n})")
                        } else {
                            format!("KC_MO({n})")
                        };
                    }
                    // The layer's own switch key falls through.
                    return "_______".to_string();
                }
                if layer.disabled_keys.iter().any(|k| k == physical.as_str()) {
                    return "XXXXXXX".to_string();
                }
                if let Some(rule) = layer.remappings.iter().find(|r| r.from == physical.as_str())
                {
                    if let Some(to) = KeyCode::from_id(&rule.to) {
                        return keycode_to_qmk(to).to_string();
                    }
                }
                "_______".to_string()
            })
            .collect();
        out_layers.push(entries);
    }
    let doc = serde_json::json!({
        "version": 1,
        "keyboard": keyboard,
        "keymap": keymap,
        "layout": format!("LAYOUT_{}", layout_id.replace('-', "_")),
        "layers": out_layers,
    });
    serde_json::to_string_pretty(&doc).map_err(|e| format!("failed to serialize keymap: {e}"))
}

/// Export a profile as a VIA JSON keymap (hex codes). VIA cannot
/// express layers, so a profile with layers is rejected with the
/// reason — use QMK export for layered profiles.
pub fn profile_to_via_keymap(
    keymap_name: &str,
    layout_id: &str,
    key_order: &[String],
    base_disabled: &[String],
    base_remappings: &[RemapRule],
    layers: &[LayerDef],
) -> Result<String, String> {
    if !layers.is_empty() {
        return Err(
            "VIA export cannot express layers — use QMK export for layered profiles".into(),
        );
    }
    let order = validate_key_order(key_order)?;
    let entries: Vec<String> = order
        .iter()
        .map(|physical| {
            if base_disabled.iter().any(|k| k == physical.as_str()) {
                return "0x0000".to_string();
            }
            if let Some(rule) = base_remappings.iter().find(|r| r.from == physical.as_str()) {
                if let Some(to) = KeyCode::from_id(&rule.to) {
                    if let Some(usage) = keycode_to_via_usage(to) {
                        return format!("0x{usage:04X}");
                    }
                }
            }
            match keycode_to_via_usage(*physical) {
                Some(usage) => format!("0x{usage:04X}"),
                None => "0x0000".to_string(),
            }
        })
        .collect();
    let doc = serde_json::json!({
        "name": keymap_name,
        "layers": [entries],
    });
    let _ = layout_id;
    serde_json::to_string_pretty(&doc).map_err(|e| format!("failed to serialize keymap: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order_60() -> Vec<String> {
        // A minimal 3-key order for tests: A, B, C positions.
        vec!["KeyA".into(), "KeyB".into(), "KeyC".into()]
    }

    fn qmk_json(layers: &str) -> String {
        format!(
            r#"{{"keyboard": "test/kb", "keymap": "test", "layout": "LAYOUT_60_ansi", "layers": {layers}}}"#
        )
    }

    #[test]
    fn qmk_import_base_layer_only() {
        let raw = qmk_json(r#"[["KC_A", "XXXXXXX", "KC_Z"]]"#);
        let import = qmk_keymap_to_profile(&raw, "60-ansi", &order_60()).unwrap();
        assert_eq!(import.base_disabled, vec!["KeyB".to_string()]);
        assert_eq!(import.base_remappings.len(), 1);
        assert_eq!(import.base_remappings[0].from, "KeyC");
        assert_eq!(import.base_remappings[0].to, "KeyZ");
        assert!(import.layers.is_empty());
        let preview = validate_qmk_keymap(&raw, "60-ansi", &order_60()).unwrap();
        assert_eq!(preview.base_remap_count, 1);
        assert_eq!(preview.base_disabled_count, 1);
    }

    #[test]
    fn qmk_import_layers_with_mo_and_trns() {
        let raw = qmk_json(
            r#"[["KC_MO(1)", "KC_B", "KC_C"],
                ["_______", "KC_X", "XXXXXXX"]]"#,
        );
        let import = qmk_keymap_to_profile(&raw, "60-ansi", &order_60()).unwrap();
        assert_eq!(import.base_remappings.len(), 0);
        assert_eq!(import.layers.len(), 1);
        let layer = &import.layers[0];
        assert_eq!(layer.switch_key, "KeyA");
        assert_eq!(layer.mode, LayerMode::Hold);
        // Layer 1: A falls through (its own switch), B -> X, C disabled.
        assert_eq!(layer.disabled_keys, vec!["KeyC".to_string()]);
        assert_eq!(layer.remappings.len(), 1);
        assert_eq!(layer.remappings[0].from, "KeyB");
        assert_eq!(layer.remappings[0].to, "KeyX");
    }

    #[test]
    fn qmk_import_rejects_missing_switch_key() {
        let raw = qmk_json(r#"[["KC_A", "KC_B", "KC_C"], ["KC_A", "KC_B", "KC_C"]]"#);
        let err = qmk_keymap_to_profile(&raw, "60-ansi", &order_60()).unwrap_err();
        assert!(err.contains("no switch key"), "{err}");
    }

    #[test]
    fn qmk_import_rejects_unreachable_switch_key() {
        // MO(1) only inside layer 1 itself: no lower layer can reach it.
        let raw = qmk_json(r#"[["KC_A", "KC_B", "KC_C"], ["KC_MO(1)", "KC_B", "KC_C"]]"#);
        let err = qmk_keymap_to_profile(&raw, "60-ansi", &order_60()).unwrap_err();
        assert!(err.contains("never be reached"), "{err}");
    }

    #[test]
    fn qmk_import_rejects_unsupported_constructs() {
        for code in ["KC_LT(1, KC_A)", "KC_TO(1)", "KC_MT(KC_LCTL, KC_A)", "KC_FOO"] {
            let raw = qmk_json(&format!(r#"[["{code}", "KC_B", "KC_C"]]"#));
            let err = qmk_keymap_to_profile(&raw, "60-ansi", &order_60()).unwrap_err();
            assert!(err.contains("not supported") || err.contains("unknown"), "{code}: {err}");
        }
    }

    #[test]
    fn qmk_import_rejects_layout_mismatch_and_bad_length() {
        let raw = qmk_json(r#"[["KC_A", "KC_B", "KC_C"]]"#);
        let err = qmk_keymap_to_profile(&raw, "tkl-ansi", &order_60()).unwrap_err();
        assert!(err.contains("layout"), "{err}");
        let raw = qmk_json(r#"[["KC_A", "KC_B"]]"#);
        let err = qmk_keymap_to_profile(&raw, "60-ansi", &order_60()).unwrap_err();
        assert!(err.contains("entries"), "{err}");
    }

    #[test]
    fn qmk_export_roundtrip() {
        let import = QmkImport {
            base_disabled: vec!["KeyB".into()],
            base_remappings: vec![RemapRule {
                from: "KeyC".into(),
                to: "KeyZ".into(),
            }],
            layers: vec![LayerDef {
                id: "qmk-layer-1".into(),
                name: "Layer 1".into(),
                switch_key: "KeyA".into(),
                mode: LayerMode::Hold,
                disabled_keys: vec!["KeyC".into()],
                remappings: vec![RemapRule {
                    from: "KeyB".into(),
                    to: "KeyX".into(),
                }],
            }],
        };
        let json = profile_to_qmk_keymap(
            "test/kb",
            "test",
            "60-ansi",
            &order_60(),
            &import.base_disabled,
            &import.base_remappings,
            &import.layers,
        )
        .unwrap();
        // Export -> import must reproduce the same keymap.
        let back = qmk_keymap_to_profile(&json, "60-ansi", &order_60()).unwrap();
        assert_eq!(back.base_disabled, import.base_disabled);
        assert_eq!(back.base_remappings, import.base_remappings);
        assert_eq!(back.layers.len(), 1);
        assert_eq!(back.layers[0].switch_key, "KeyA");
        assert_eq!(back.layers[0].disabled_keys, import.layers[0].disabled_keys);
        assert_eq!(back.layers[0].remappings, import.layers[0].remappings);
    }

    #[test]
    fn via_import_basic_and_rejects_layer_codes() {
        let raw = r#"{"name": "test", "layers": [["0x0004", "0x0000", "0x001D"]]}"#;
        let import = via_keymap_to_profile(raw, "60-ansi", &order_60()).unwrap();
        assert_eq!(import.base_disabled, vec!["KeyB".to_string()]);
        assert_eq!(import.base_remappings.len(), 1);
        assert_eq!(import.base_remappings[0].from, "KeyC");
        assert_eq!(import.base_remappings[0].to, "KeyZ");
        // A 16-bit layer code is rejected with the QMK pointer.
        let raw = r#"{"name": "test", "layers": [["0x5220", "0x0005", "0x0006"]]}"#;
        let err = via_keymap_to_profile(raw, "60-ansi", &order_60()).unwrap_err();
        assert!(err.contains("QMK keymap.json"), "{err}");
    }

    #[test]
    fn via_export_rejects_layered_profiles() {
        let layers = vec![LayerDef {
            id: "l1".into(),
            name: "Fn".into(),
            switch_key: "KeyA".into(),
            mode: LayerMode::Hold,
            disabled_keys: vec![],
            remappings: vec![],
        }];
        let err = profile_to_via_keymap("test", "60-ansi", &order_60(), &[], &[], &layers)
            .unwrap_err();
        assert!(err.contains("QMK export"), "{err}");
        let json = profile_to_via_keymap("test", "60-ansi", &order_60(), &[], &[], &[]).unwrap();
        assert!(json.contains("0x0004"));
    }

    #[test]
    fn keycode_qmk_roundtrip() {
        // Every keycode maps to a QMK code and back (except the two
        // alias collisions: NUHS->Backslash and the 0x32 VIA usage).
        for code in KeyCode::ALL {
            let qmk = keycode_to_qmk(code);
            let back = qmk_basic_code(qmk).unwrap();
            assert_eq!(back, code, "{code:?} -> {qmk}");
        }
    }

    #[test]
    fn keycode_via_roundtrip() {
        for code in KeyCode::ALL {
            let usage = keycode_to_via_usage(code).unwrap();
            let back = via_usage_to_keycode(usage).unwrap();
            // 0x32 (NUHS) collides with Backslash by design.
            assert_eq!(back, code, "{code:?} -> 0x{usage:04X}");
        }
    }
}
