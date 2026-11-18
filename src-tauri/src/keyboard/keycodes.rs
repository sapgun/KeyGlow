use serde::{Deserialize, Serialize};

/// Stable visual/web identifiers. These strings are the IPC contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum KeyCode {
    Escape = 0,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    PrintScreen,
    ScrollLock,
    Pause,
    Backquote,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Digit0,
    Minus,
    Equal,
    Backspace,
    Tab,
    KeyQ,
    KeyW,
    KeyE,
    KeyR,
    KeyT,
    KeyY,
    KeyU,
    KeyI,
    KeyO,
    KeyP,
    BracketLeft,
    BracketRight,
    Backslash,
    CapsLock,
    KeyA,
    KeyS,
    KeyD,
    KeyF,
    KeyG,
    KeyH,
    KeyJ,
    KeyK,
    KeyL,
    Semicolon,
    Quote,
    Enter,
    ShiftLeft,
    KeyZ,
    KeyX,
    KeyC,
    KeyV,
    KeyB,
    KeyN,
    KeyM,
    Comma,
    Period,
    Slash,
    ShiftRight,
    ControlLeft,
    MetaLeft,
    AltLeft,
    Space,
    AltRight,
    MetaRight,
    ContextMenu,
    ControlRight,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowLeft,
    ArrowUp,
    ArrowRight,
    ArrowDown,
    NumLock,
    NumpadDivide,
    NumpadMultiply,
    NumpadSubtract,
    NumpadAdd,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadDecimal,
    NumpadEnter,
    /// ISO extra key between Left Shift and Z (scan 0x56, VK_OEM_102).
    /// S6: same VK as IntlRo; the scan code disambiguates (see identify_key).
    IntlBackslash,
    /// JIS yen key between Equal and Backspace (scan 0x7D).
    /// S6: VK_OEM_5 is provisional (AHK documents vkDCsc07D); confirm on
    /// real JIS hardware. Scan 0x7D disambiguates from Backslash (0x2B).
    IntlYen,
    /// JIS ro key between Slash and Right Shift (scan 0x73, VK_OEM_102).
    /// S6: same VK as IntlBackslash; the scan code disambiguates.
    IntlRo,
    /// JIS henkan key, right of Space (scan 0x79, VK_CONVERT 0x1C).
    Convert,
    /// JIS muhenkan key, left of Space (scan 0x7B, VK_NONCONVERT 0x1D).
    NonConvert,
    /// JIS kana key, right of Convert (scan 0x70, VK_KANA 0x15).
    KanaMode,
}

impl KeyCode {
    pub const COUNT: usize = 110;

    pub const ALL: [KeyCode; Self::COUNT] = [
        Self::Escape,
        Self::F1,
        Self::F2,
        Self::F3,
        Self::F4,
        Self::F5,
        Self::F6,
        Self::F7,
        Self::F8,
        Self::F9,
        Self::F10,
        Self::F11,
        Self::F12,
        Self::PrintScreen,
        Self::ScrollLock,
        Self::Pause,
        Self::Backquote,
        Self::Digit1,
        Self::Digit2,
        Self::Digit3,
        Self::Digit4,
        Self::Digit5,
        Self::Digit6,
        Self::Digit7,
        Self::Digit8,
        Self::Digit9,
        Self::Digit0,
        Self::Minus,
        Self::Equal,
        Self::Backspace,
        Self::Tab,
        Self::KeyQ,
        Self::KeyW,
        Self::KeyE,
        Self::KeyR,
        Self::KeyT,
        Self::KeyY,
        Self::KeyU,
        Self::KeyI,
        Self::KeyO,
        Self::KeyP,
        Self::BracketLeft,
        Self::BracketRight,
        Self::Backslash,
        Self::CapsLock,
        Self::KeyA,
        Self::KeyS,
        Self::KeyD,
        Self::KeyF,
        Self::KeyG,
        Self::KeyH,
        Self::KeyJ,
        Self::KeyK,
        Self::KeyL,
        Self::Semicolon,
        Self::Quote,
        Self::Enter,
        Self::ShiftLeft,
        Self::KeyZ,
        Self::KeyX,
        Self::KeyC,
        Self::KeyV,
        Self::KeyB,
        Self::KeyN,
        Self::KeyM,
        Self::Comma,
        Self::Period,
        Self::Slash,
        Self::ShiftRight,
        Self::ControlLeft,
        Self::MetaLeft,
        Self::AltLeft,
        Self::Space,
        Self::AltRight,
        Self::MetaRight,
        Self::ContextMenu,
        Self::ControlRight,
        Self::Insert,
        Self::Delete,
        Self::Home,
        Self::End,
        Self::PageUp,
        Self::PageDown,
        Self::ArrowLeft,
        Self::ArrowUp,
        Self::ArrowRight,
        Self::ArrowDown,
        Self::NumLock,
        Self::NumpadDivide,
        Self::NumpadMultiply,
        Self::NumpadSubtract,
        Self::NumpadAdd,
        Self::Numpad0,
        Self::Numpad1,
        Self::Numpad2,
        Self::Numpad3,
        Self::Numpad4,
        Self::Numpad5,
        Self::Numpad6,
        Self::Numpad7,
        Self::Numpad8,
        Self::Numpad9,
        Self::NumpadDecimal,
        Self::NumpadEnter,
        Self::IntlBackslash,
        Self::IntlYen,
        Self::IntlRo,
        Self::Convert,
        Self::NonConvert,
        Self::KanaMode,
    ];

    pub fn index(self) -> usize {
        self as u8 as usize
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Escape => "Escape",
            Self::F1 => "F1",
            Self::F2 => "F2",
            Self::F3 => "F3",
            Self::F4 => "F4",
            Self::F5 => "F5",
            Self::F6 => "F6",
            Self::F7 => "F7",
            Self::F8 => "F8",
            Self::F9 => "F9",
            Self::F10 => "F10",
            Self::F11 => "F11",
            Self::F12 => "F12",
            Self::PrintScreen => "PrintScreen",
            Self::ScrollLock => "ScrollLock",
            Self::Pause => "Pause",
            Self::Backquote => "Backquote",
            Self::Digit1 => "Digit1",
            Self::Digit2 => "Digit2",
            Self::Digit3 => "Digit3",
            Self::Digit4 => "Digit4",
            Self::Digit5 => "Digit5",
            Self::Digit6 => "Digit6",
            Self::Digit7 => "Digit7",
            Self::Digit8 => "Digit8",
            Self::Digit9 => "Digit9",
            Self::Digit0 => "Digit0",
            Self::Minus => "Minus",
            Self::Equal => "Equal",
            Self::Backspace => "Backspace",
            Self::Tab => "Tab",
            Self::KeyQ => "KeyQ",
            Self::KeyW => "KeyW",
            Self::KeyE => "KeyE",
            Self::KeyR => "KeyR",
            Self::KeyT => "KeyT",
            Self::KeyY => "KeyY",
            Self::KeyU => "KeyU",
            Self::KeyI => "KeyI",
            Self::KeyO => "KeyO",
            Self::KeyP => "KeyP",
            Self::BracketLeft => "BracketLeft",
            Self::BracketRight => "BracketRight",
            Self::Backslash => "Backslash",
            Self::CapsLock => "CapsLock",
            Self::KeyA => "KeyA",
            Self::KeyS => "KeyS",
            Self::KeyD => "KeyD",
            Self::KeyF => "KeyF",
            Self::KeyG => "KeyG",
            Self::KeyH => "KeyH",
            Self::KeyJ => "KeyJ",
            Self::KeyK => "KeyK",
            Self::KeyL => "KeyL",
            Self::Semicolon => "Semicolon",
            Self::Quote => "Quote",
            Self::Enter => "Enter",
            Self::ShiftLeft => "ShiftLeft",
            Self::KeyZ => "KeyZ",
            Self::KeyX => "KeyX",
            Self::KeyC => "KeyC",
            Self::KeyV => "KeyV",
            Self::KeyB => "KeyB",
            Self::KeyN => "KeyN",
            Self::KeyM => "KeyM",
            Self::Comma => "Comma",
            Self::Period => "Period",
            Self::Slash => "Slash",
            Self::ShiftRight => "ShiftRight",
            Self::ControlLeft => "ControlLeft",
            Self::MetaLeft => "MetaLeft",
            Self::AltLeft => "AltLeft",
            Self::Space => "Space",
            Self::AltRight => "AltRight",
            Self::MetaRight => "MetaRight",
            Self::ContextMenu => "ContextMenu",
            Self::ControlRight => "ControlRight",
            Self::Insert => "Insert",
            Self::Delete => "Delete",
            Self::Home => "Home",
            Self::End => "End",
            Self::PageUp => "PageUp",
            Self::PageDown => "PageDown",
            Self::ArrowLeft => "ArrowLeft",
            Self::ArrowUp => "ArrowUp",
            Self::ArrowRight => "ArrowRight",
            Self::ArrowDown => "ArrowDown",
            Self::NumLock => "NumLock",
            Self::NumpadDivide => "NumpadDivide",
            Self::NumpadMultiply => "NumpadMultiply",
            Self::NumpadSubtract => "NumpadSubtract",
            Self::NumpadAdd => "NumpadAdd",
            Self::Numpad0 => "Numpad0",
            Self::Numpad1 => "Numpad1",
            Self::Numpad2 => "Numpad2",
            Self::Numpad3 => "Numpad3",
            Self::Numpad4 => "Numpad4",
            Self::Numpad5 => "Numpad5",
            Self::Numpad6 => "Numpad6",
            Self::Numpad7 => "Numpad7",
            Self::Numpad8 => "Numpad8",
            Self::Numpad9 => "Numpad9",
            Self::NumpadDecimal => "NumpadDecimal",
            Self::NumpadEnter => "NumpadEnter",
            Self::IntlBackslash => "IntlBackslash",
            Self::IntlYen => "IntlYen",
            Self::IntlRo => "IntlRo",
            Self::Convert => "Convert",
            Self::NonConvert => "NonConvert",
            Self::KanaMode => "KanaMode",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "Escape" => Self::Escape,
            "F1" => Self::F1,
            "F2" => Self::F2,
            "F3" => Self::F3,
            "F4" => Self::F4,
            "F5" => Self::F5,
            "F6" => Self::F6,
            "F7" => Self::F7,
            "F8" => Self::F8,
            "F9" => Self::F9,
            "F10" => Self::F10,
            "F11" => Self::F11,
            "F12" => Self::F12,
            "PrintScreen" => Self::PrintScreen,
            "ScrollLock" => Self::ScrollLock,
            "Pause" => Self::Pause,
            "Backquote" => Self::Backquote,
            "Digit1" => Self::Digit1,
            "Digit2" => Self::Digit2,
            "Digit3" => Self::Digit3,
            "Digit4" => Self::Digit4,
            "Digit5" => Self::Digit5,
            "Digit6" => Self::Digit6,
            "Digit7" => Self::Digit7,
            "Digit8" => Self::Digit8,
            "Digit9" => Self::Digit9,
            "Digit0" => Self::Digit0,
            "Minus" => Self::Minus,
            "Equal" => Self::Equal,
            "Backspace" => Self::Backspace,
            "Tab" => Self::Tab,
            "KeyQ" => Self::KeyQ,
            "KeyW" => Self::KeyW,
            "KeyE" => Self::KeyE,
            "KeyR" => Self::KeyR,
            "KeyT" => Self::KeyT,
            "KeyY" => Self::KeyY,
            "KeyU" => Self::KeyU,
            "KeyI" => Self::KeyI,
            "KeyO" => Self::KeyO,
            "KeyP" => Self::KeyP,
            "BracketLeft" => Self::BracketLeft,
            "BracketRight" => Self::BracketRight,
            "Backslash" => Self::Backslash,
            "CapsLock" => Self::CapsLock,
            "KeyA" => Self::KeyA,
            "KeyS" => Self::KeyS,
            "KeyD" => Self::KeyD,
            "KeyF" => Self::KeyF,
            "KeyG" => Self::KeyG,
            "KeyH" => Self::KeyH,
            "KeyJ" => Self::KeyJ,
            "KeyK" => Self::KeyK,
            "KeyL" => Self::KeyL,
            "Semicolon" => Self::Semicolon,
            "Quote" => Self::Quote,
            "Enter" => Self::Enter,
            "ShiftLeft" => Self::ShiftLeft,
            "KeyZ" => Self::KeyZ,
            "KeyX" => Self::KeyX,
            "KeyC" => Self::KeyC,
            "KeyV" => Self::KeyV,
            "KeyB" => Self::KeyB,
            "KeyN" => Self::KeyN,
            "KeyM" => Self::KeyM,
            "Comma" => Self::Comma,
            "Period" => Self::Period,
            "Slash" => Self::Slash,
            "ShiftRight" => Self::ShiftRight,
            "ControlLeft" => Self::ControlLeft,
            "MetaLeft" => Self::MetaLeft,
            "AltLeft" => Self::AltLeft,
            "Space" => Self::Space,
            "AltRight" => Self::AltRight,
            "MetaRight" => Self::MetaRight,
            "ContextMenu" => Self::ContextMenu,
            "ControlRight" => Self::ControlRight,
            "Insert" => Self::Insert,
            "Delete" => Self::Delete,
            "Home" => Self::Home,
            "End" => Self::End,
            "PageUp" => Self::PageUp,
            "PageDown" => Self::PageDown,
            "ArrowLeft" => Self::ArrowLeft,
            "ArrowUp" => Self::ArrowUp,
            "ArrowRight" => Self::ArrowRight,
            "ArrowDown" => Self::ArrowDown,
            "NumLock" => Self::NumLock,
            "NumpadDivide" => Self::NumpadDivide,
            "NumpadMultiply" => Self::NumpadMultiply,
            "NumpadSubtract" => Self::NumpadSubtract,
            "NumpadAdd" => Self::NumpadAdd,
            "Numpad0" => Self::Numpad0,
            "Numpad1" => Self::Numpad1,
            "Numpad2" => Self::Numpad2,
            "Numpad3" => Self::Numpad3,
            "Numpad4" => Self::Numpad4,
            "Numpad5" => Self::Numpad5,
            "Numpad6" => Self::Numpad6,
            "Numpad7" => Self::Numpad7,
            "Numpad8" => Self::Numpad8,
            "Numpad9" => Self::Numpad9,
            "NumpadDecimal" => Self::NumpadDecimal,
            "NumpadEnter" => Self::NumpadEnter,
            "IntlBackslash" => Self::IntlBackslash,
            "IntlYen" => Self::IntlYen,
            "IntlRo" => Self::IntlRo,
            "Convert" => Self::Convert,
            "NonConvert" => Self::NonConvert,
            "KanaMode" => Self::KanaMode,
            _ => return None,
        })
    }
}

/// Reverse of [`identify_key`]: the (virtual key, scan code, extended)
/// triple that identifies this key on the wire.
///
/// S8: `SendInput` injects by scan code (`KEYEVENTF_SCANCODE`), so the
/// position — not the virtual key — identifies the target. This is the
/// same source of truth as `identify_key`, and the roundtrip test below
/// proves `identify_key(to_vk_scan(k)) == k` for all 110 codes, including
/// the S6 shared-VK disambiguations (0xE2/0x56 vs 0xE2/0x73, 0xDC/0x2B
/// vs 0xDC/0x7D). Scan codes are PS/2 set 1.
pub fn to_vk_scan(code: KeyCode) -> (u32, u32, bool) {
    use KeyCode::*;
    match code {
        Escape => (0x1B, 0x01, false),
        F1 => (0x70, 0x3B, false),
        F2 => (0x71, 0x3C, false),
        F3 => (0x72, 0x3D, false),
        F4 => (0x73, 0x3E, false),
        F5 => (0x74, 0x3F, false),
        F6 => (0x75, 0x40, false),
        F7 => (0x76, 0x41, false),
        F8 => (0x77, 0x42, false),
        F9 => (0x78, 0x43, false),
        F10 => (0x79, 0x44, false),
        F11 => (0x7A, 0x57, false),
        F12 => (0x7B, 0x58, false),
        PrintScreen => (0x2C, 0x37, true),
        ScrollLock => (0x91, 0x46, false),
        Pause => (0x13, 0x45, false),
        Backquote => (0xC0, 0x29, false),
        Digit1 => (0x31, 0x02, false),
        Digit2 => (0x32, 0x03, false),
        Digit3 => (0x33, 0x04, false),
        Digit4 => (0x34, 0x05, false),
        Digit5 => (0x35, 0x06, false),
        Digit6 => (0x36, 0x07, false),
        Digit7 => (0x37, 0x08, false),
        Digit8 => (0x38, 0x09, false),
        Digit9 => (0x39, 0x0A, false),
        Digit0 => (0x30, 0x0B, false),
        Minus => (0xBD, 0x0C, false),
        Equal => (0xBB, 0x0D, false),
        Backspace => (0x08, 0x0E, false),
        Tab => (0x09, 0x0F, false),
        KeyQ => (0x51, 0x10, false),
        KeyW => (0x57, 0x11, false),
        KeyE => (0x45, 0x12, false),
        KeyR => (0x52, 0x13, false),
        KeyT => (0x54, 0x14, false),
        KeyY => (0x59, 0x15, false),
        KeyU => (0x55, 0x16, false),
        KeyI => (0x49, 0x17, false),
        KeyO => (0x4F, 0x18, false),
        KeyP => (0x50, 0x19, false),
        BracketLeft => (0xDB, 0x1A, false),
        BracketRight => (0xDD, 0x1B, false),
        Backslash => (0xDC, 0x2B, false),
        CapsLock => (0x14, 0x3A, false),
        KeyA => (0x41, 0x1E, false),
        KeyS => (0x53, 0x1F, false),
        KeyD => (0x44, 0x20, false),
        KeyF => (0x46, 0x21, false),
        KeyG => (0x47, 0x22, false),
        KeyH => (0x48, 0x23, false),
        KeyJ => (0x4A, 0x24, false),
        KeyK => (0x4B, 0x25, false),
        KeyL => (0x4C, 0x26, false),
        Semicolon => (0xBA, 0x27, false),
        Quote => (0xDE, 0x28, false),
        Enter => (0x0D, 0x1C, false),
        ShiftLeft => (0xA0, 0x2A, false),
        KeyZ => (0x5A, 0x2C, false),
        KeyX => (0x58, 0x2D, false),
        KeyC => (0x43, 0x2E, false),
        KeyV => (0x56, 0x2F, false),
        KeyB => (0x42, 0x30, false),
        KeyN => (0x4E, 0x31, false),
        KeyM => (0x4D, 0x32, false),
        Comma => (0xBC, 0x33, false),
        Period => (0xBE, 0x34, false),
        Slash => (0xBF, 0x35, false),
        ShiftRight => (0xA1, 0x36, false),
        ControlLeft => (0xA2, 0x1D, false),
        MetaLeft => (0x5B, 0x5B, true),
        AltLeft => (0xA4, 0x38, false),
        Space => (0x20, 0x39, false),
        AltRight => (0xA5, 0x38, true),
        MetaRight => (0x5C, 0x5C, true),
        ContextMenu => (0x5D, 0x5D, true),
        ControlRight => (0xA3, 0x1D, true),
        Insert => (0x2D, 0x52, true),
        Delete => (0x2E, 0x53, true),
        Home => (0x24, 0x47, true),
        End => (0x23, 0x4F, true),
        PageUp => (0x21, 0x49, true),
        PageDown => (0x22, 0x51, true),
        ArrowLeft => (0x25, 0x4B, true),
        ArrowUp => (0x26, 0x48, true),
        ArrowRight => (0x27, 0x4D, true),
        ArrowDown => (0x28, 0x50, true),
        NumLock => (0x90, 0x45, false),
        NumpadDivide => (0x6F, 0x35, true),
        NumpadMultiply => (0x6A, 0x37, false),
        NumpadSubtract => (0x6D, 0x4A, false),
        NumpadAdd => (0x6B, 0x4E, false),
        Numpad0 => (0x60, 0x52, false),
        Numpad1 => (0x61, 0x4F, false),
        Numpad2 => (0x62, 0x50, false),
        Numpad3 => (0x63, 0x51, false),
        Numpad4 => (0x64, 0x4B, false),
        Numpad5 => (0x65, 0x4C, false),
        Numpad6 => (0x66, 0x4D, false),
        Numpad7 => (0x67, 0x47, false),
        Numpad8 => (0x68, 0x48, false),
        Numpad9 => (0x69, 0x49, false),
        NumpadDecimal => (0x6E, 0x53, false),
        NumpadEnter => (0x0D, 0x1C, true),
        IntlBackslash => (0xE2, 0x56, false),
        IntlYen => (0xDC, 0x7D, false),
        IntlRo => (0xE2, 0x73, false),
        Convert => (0x1C, 0x79, false),
        NonConvert => (0x1D, 0x7B, false),
        KanaMode => (0x15, 0x70, false),
    }
}

/// Map a low-level Windows key event to a stable KeyGlow identifier.
///
/// Left/right modifiers are distinguished using the virtual-key when Windows
/// supplies `VK_L*`/`VK_R*`, otherwise scan code (Shift) or the extended bit
/// (Ctrl/Alt/Enter).
///
/// S6: two VKs are shared and need the scan code to disambiguate:
/// - VK_OEM_102 (0xE2): scan 0x56 is the ISO extra key (IntlBackslash),
///   scan 0x73 is the JIS ro key (IntlRo).
/// - VK_OEM_5 (0xDC): scan 0x2B is the ANSI Backslash / ISO # / JIS ] key
///   (all the same physical key, reported as Backslash; layouts differ
///   only by label), scan 0x7D is the JIS yen key (IntlYen, VK provisional).
pub fn identify_key(vk: u32, scan: u32, extended: bool) -> Option<KeyCode> {
    match vk {
        0x10 | 0xA0 | 0xA1 => {
            if vk == 0xA1 || scan == 0x36 {
                Some(KeyCode::ShiftRight)
            } else {
                Some(KeyCode::ShiftLeft)
            }
        }
        0x11 | 0xA2 | 0xA3 => {
            if vk == 0xA3 || extended {
                Some(KeyCode::ControlRight)
            } else {
                Some(KeyCode::ControlLeft)
            }
        }
        0x12 | 0xA4 | 0xA5 => {
            if vk == 0xA5 || extended {
                Some(KeyCode::AltRight)
            } else {
                Some(KeyCode::AltLeft)
            }
        }
        0x0D => {
            if extended {
                Some(KeyCode::NumpadEnter)
            } else {
                Some(KeyCode::Enter)
            }
        }
        0x03 => Some(KeyCode::Pause),
        0x1B => Some(KeyCode::Escape),
        0x70 => Some(KeyCode::F1),
        0x71 => Some(KeyCode::F2),
        0x72 => Some(KeyCode::F3),
        0x73 => Some(KeyCode::F4),
        0x74 => Some(KeyCode::F5),
        0x75 => Some(KeyCode::F6),
        0x76 => Some(KeyCode::F7),
        0x77 => Some(KeyCode::F8),
        0x78 => Some(KeyCode::F9),
        0x79 => Some(KeyCode::F10),
        0x7A => Some(KeyCode::F11),
        0x7B => Some(KeyCode::F12),
        0x2C => Some(KeyCode::PrintScreen),
        0x91 => Some(KeyCode::ScrollLock),
        0x13 => Some(KeyCode::Pause),
        0xC0 => Some(KeyCode::Backquote),
        0x31 => Some(KeyCode::Digit1),
        0x32 => Some(KeyCode::Digit2),
        0x33 => Some(KeyCode::Digit3),
        0x34 => Some(KeyCode::Digit4),
        0x35 => Some(KeyCode::Digit5),
        0x36 => Some(KeyCode::Digit6),
        0x37 => Some(KeyCode::Digit7),
        0x38 => Some(KeyCode::Digit8),
        0x39 => Some(KeyCode::Digit9),
        0x30 => Some(KeyCode::Digit0),
        0xBD => Some(KeyCode::Minus),
        0xBB => Some(KeyCode::Equal),
        0x08 => Some(KeyCode::Backspace),
        0x09 => Some(KeyCode::Tab),
        0x51 => Some(KeyCode::KeyQ),
        0x57 => Some(KeyCode::KeyW),
        0x45 => Some(KeyCode::KeyE),
        0x52 => Some(KeyCode::KeyR),
        0x54 => Some(KeyCode::KeyT),
        0x59 => Some(KeyCode::KeyY),
        0x55 => Some(KeyCode::KeyU),
        0x49 => Some(KeyCode::KeyI),
        0x4F => Some(KeyCode::KeyO),
        0x50 => Some(KeyCode::KeyP),
        0xDB => Some(KeyCode::BracketLeft),
        0xDD => Some(KeyCode::BracketRight),
        0xDC => {
            // S6: same VK for the ANSI Backslash / ISO # / JIS ] key
            // (scan 0x2B) and the JIS yen key (scan 0x7D, VK provisional).
            if scan == 0x7D {
                Some(KeyCode::IntlYen)
            } else {
                Some(KeyCode::Backslash)
            }
        }
        0xE2 => {
            // S6: VK_OEM_102 is shared by the ISO extra key (scan 0x56)
            // and the JIS ro key (scan 0x73).
            if scan == 0x73 {
                Some(KeyCode::IntlRo)
            } else {
                Some(KeyCode::IntlBackslash)
            }
        }
        0x1C => Some(KeyCode::Convert),
        0x1D => Some(KeyCode::NonConvert),
        0x15 => Some(KeyCode::KanaMode),
        0x14 => Some(KeyCode::CapsLock),
        0x41 => Some(KeyCode::KeyA),
        0x53 => Some(KeyCode::KeyS),
        0x44 => Some(KeyCode::KeyD),
        0x46 => Some(KeyCode::KeyF),
        0x47 => Some(KeyCode::KeyG),
        0x48 => Some(KeyCode::KeyH),
        0x4A => Some(KeyCode::KeyJ),
        0x4B => Some(KeyCode::KeyK),
        0x4C => Some(KeyCode::KeyL),
        0xBA => Some(KeyCode::Semicolon),
        0xDE => Some(KeyCode::Quote),
        0x5A => Some(KeyCode::KeyZ),
        0x58 => Some(KeyCode::KeyX),
        0x43 => Some(KeyCode::KeyC),
        0x56 => Some(KeyCode::KeyV),
        0x42 => Some(KeyCode::KeyB),
        0x4E => Some(KeyCode::KeyN),
        0x4D => Some(KeyCode::KeyM),
        0xBC => Some(KeyCode::Comma),
        0xBE => Some(KeyCode::Period),
        0xBF => Some(KeyCode::Slash),
        0x5B => Some(KeyCode::MetaLeft),
        0x5C => Some(KeyCode::MetaRight),
        0x20 => Some(KeyCode::Space),
        0x5D => Some(KeyCode::ContextMenu),
        0x2D => Some(KeyCode::Insert),
        0x2E => Some(KeyCode::Delete),
        0x24 => Some(KeyCode::Home),
        0x23 => Some(KeyCode::End),
        0x21 => Some(KeyCode::PageUp),
        0x22 => Some(KeyCode::PageDown),
        0x25 => Some(KeyCode::ArrowLeft),
        0x26 => Some(KeyCode::ArrowUp),
        0x27 => Some(KeyCode::ArrowRight),
        0x28 => Some(KeyCode::ArrowDown),
        0x90 => Some(KeyCode::NumLock),
        0x6F => Some(KeyCode::NumpadDivide),
        0x6A => Some(KeyCode::NumpadMultiply),
        0x6D => Some(KeyCode::NumpadSubtract),
        0x6B => Some(KeyCode::NumpadAdd),
        0x60 => Some(KeyCode::Numpad0),
        0x61 => Some(KeyCode::Numpad1),
        0x62 => Some(KeyCode::Numpad2),
        0x63 => Some(KeyCode::Numpad3),
        0x64 => Some(KeyCode::Numpad4),
        0x65 => Some(KeyCode::Numpad5),
        0x66 => Some(KeyCode::Numpad6),
        0x67 => Some(KeyCode::Numpad7),
        0x68 => Some(KeyCode::Numpad8),
        0x69 => Some(KeyCode::Numpad9),
        0x6E => Some(KeyCode::NumpadDecimal),
        0x0C => Some(KeyCode::Numpad5),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_matches_all() {
        assert_eq!(KeyCode::ALL.len(), KeyCode::COUNT);
        for (i, code) in KeyCode::ALL.iter().enumerate() {
            assert_eq!(code.index(), i, "{code:?} index mismatch");
        }
    }

    #[test]
    fn roundtrip_ids() {
        for code in KeyCode::ALL {
            let s = code.as_str();
            assert_eq!(KeyCode::from_id(s), Some(code));
            let json = serde_json::to_string(&code).unwrap();
            assert_eq!(json, format!("\"{s}\""));
            let back: KeyCode = serde_json::from_str(&json).unwrap();
            assert_eq!(back, code);
        }
    }

    #[test]
    fn distinguishes_modifiers() {
        assert_eq!(identify_key(0xA0, 0x2A, false), Some(KeyCode::ShiftLeft));
        assert_eq!(identify_key(0xA1, 0x36, false), Some(KeyCode::ShiftRight));
        assert_eq!(identify_key(0x10, 0x2A, false), Some(KeyCode::ShiftLeft));
        assert_eq!(identify_key(0x10, 0x36, false), Some(KeyCode::ShiftRight));
        assert_eq!(identify_key(0x11, 0x1D, false), Some(KeyCode::ControlLeft));
        assert_eq!(identify_key(0x11, 0x1D, true), Some(KeyCode::ControlRight));
        assert_eq!(identify_key(0x12, 0x38, false), Some(KeyCode::AltLeft));
        assert_eq!(identify_key(0x12, 0x38, true), Some(KeyCode::AltRight));
        assert_eq!(identify_key(0x0D, 0x1C, false), Some(KeyCode::Enter));
        assert_eq!(identify_key(0x0D, 0x1C, true), Some(KeyCode::NumpadEnter));
        assert_eq!(identify_key(0x5B, 0x5B, false), Some(KeyCode::MetaLeft));
        assert_eq!(identify_key(0x5C, 0x5C, true), Some(KeyCode::MetaRight));
    }

    #[test]
    fn letters_and_function_keys() {
        assert_eq!(identify_key(0x41, 0x1E, false), Some(KeyCode::KeyA));
        assert_eq!(identify_key(0x1B, 0x01, false), Some(KeyCode::Escape));
        assert_eq!(identify_key(0x70, 0x3B, false), Some(KeyCode::F1));
        assert_eq!(identify_key(0x20, 0x39, false), Some(KeyCode::Space));
        assert_eq!(identify_key(0xFF, 0x00, false), None);
    }

    #[test]
    fn reverse_table_roundtrips_all_codes() {
        // S8 (ADR §2, D4): the SendInput reverse table and the hook's
        // forward table are the same source of truth. Every code must
        // survive the round trip, including the S6 shared-VK splits.
        for code in KeyCode::ALL {
            let (vk, scan, extended) = to_vk_scan(code);
            assert_eq!(
                identify_key(vk, scan, extended),
                Some(code),
                "{code:?} (vk {vk:#x}, scan {scan:#x}, ext {extended}) did not round-trip"
            );
        }
    }

    #[test]
    fn iso_jis_keys_identified() {
        // S6: shared VKs disambiguated by scan code.
        assert_eq!(
            identify_key(0xE2, 0x56, false),
            Some(KeyCode::IntlBackslash)
        );
        assert_eq!(identify_key(0xE2, 0x73, false), Some(KeyCode::IntlRo));
        assert_eq!(identify_key(0xDC, 0x7D, false), Some(KeyCode::IntlYen));
        // Scan 0x2B is one physical key on all three layouts: ANSI
        // Backslash, ISO #, JIS ]. The hook reports it as Backslash;
        // layouts differ only by label.
        assert_eq!(identify_key(0xDC, 0x2B, false), Some(KeyCode::Backslash));
        assert_eq!(identify_key(0x1C, 0x79, false), Some(KeyCode::Convert));
        assert_eq!(identify_key(0x1D, 0x7B, false), Some(KeyCode::NonConvert));
        assert_eq!(identify_key(0x15, 0x70, false), Some(KeyCode::KanaMode));
    }

    /// S6: the Rust KeyCode catalog and the TS key catalog
    /// (src/lib/key-catalog.json, generated from scripts/generate-layouts.mjs)
    /// must name exactly the same key set. A drift in either direction
    /// fails here (Windows CI) and in scripts/test-catalog-sync.mjs.
    #[test]
    fn catalog_matches_ts_key_catalog() {
        let raw = include_str!("../../../src/lib/key-catalog.json");
        let parsed: serde_json::Value =
            serde_json::from_str(raw).expect("key-catalog.json must parse");
        let codes = parsed["codes"]
            .as_array()
            .expect("key-catalog.json needs a codes array");
        let ts: std::collections::HashSet<&str> = codes
            .iter()
            .map(|v| v.as_str().expect("catalog codes must be strings"))
            .collect();
        let rust: std::collections::HashSet<&str> =
            KeyCode::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(rust, ts, "Rust KeyCode set != TS key-catalog.json set");
        // Every catalog code round-trips through from_id.
        for code in &ts {
            assert!(KeyCode::from_id(code).is_some(), "unknown code {code}");
        }
    }
}
