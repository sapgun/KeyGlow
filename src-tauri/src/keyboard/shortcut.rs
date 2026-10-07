use super::KeyCode;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmergencyShortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: String,
}

impl Default for EmergencyShortcut {
    fn default() -> Self {
        Self { ctrl: true, shift: true, alt: false, key: "F12".into() }
    }
}

#[derive(Clone, Copy)]
pub struct ShortcutBinding {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: KeyCode,
}

impl EmergencyShortcut {
    pub fn compile(&self) -> Result<ShortcutBinding, String> {
        if [self.ctrl, self.shift, self.alt].into_iter().filter(|v| *v).count() < 2 {
            return Err("Choose at least two modifiers (Ctrl, Shift, Alt).".into());
        }
        let key = KeyCode::from_id(&self.key).ok_or("Unknown shortcut key.")?;
        if matches!(key, KeyCode::ControlLeft | KeyCode::ControlRight | KeyCode::ShiftLeft
            | KeyCode::ShiftRight | KeyCode::AltLeft | KeyCode::AltRight | KeyCode::MetaLeft | KeyCode::MetaRight) {
            return Err("Choose a non-modifier key.".into());
        }
        Ok(ShortcutBinding { ctrl: self.ctrl, shift: self.shift, alt: self.alt, key })
    }

    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl { parts.push("Ctrl"); }
        if self.shift { parts.push("Shift"); }
        if self.alt { parts.push("Alt"); }
        parts.push(self.key.strip_prefix("Key").or_else(|| self.key.strip_prefix("Digit")).unwrap_or(&self.key));
        parts.join(" + ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_accidental_or_unreachable_shortcuts() {
        for key in ["Fn", "NotAKey", "ControlLeft", "ShiftRight", "MetaLeft"] {
            assert!(EmergencyShortcut { key: key.into(), ..Default::default() }.compile().is_err());
        }
        assert!(EmergencyShortcut { shift: false, ..Default::default() }.compile().is_err());
    }
    #[test]
    fn label_and_binding_support_functionless_keyboards() {
        let spec = EmergencyShortcut { key: "KeyU".into(), ..Default::default() };
        assert!(matches!(spec.compile().unwrap().key, KeyCode::KeyU));
        assert_eq!(spec.label(), "Ctrl + Shift + U");
    }
}
