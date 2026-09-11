use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;
pub const DEFAULT_LAYOUT: &str = "tkl-ansi";
pub const DEFAULT_PROFILE_ID: &str = "default";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub layout: String,
    #[serde(default)]
    pub disabled_keys: Vec<String>,
    #[serde(default)]
    pub builtin: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default = "default_layout")]
    pub selected_layout: String,
    #[serde(default = "default_profile")]
    pub selected_profile: String,
    #[serde(default)]
    pub onboarded: bool,
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default)]
    pub locale: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

fn default_version() -> u32 {
    CONFIG_VERSION
}
fn default_layout() -> String {
    DEFAULT_LAYOUT.to_string()
}
fn default_profile() -> String {
    DEFAULT_PROFILE_ID.to_string()
}
fn default_theme() -> String {
    "dark".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            selected_layout: DEFAULT_LAYOUT.to_string(),
            selected_profile: DEFAULT_PROFILE_ID.to_string(),
            onboarded: false,
            start_with_windows: false,
            locale: String::new(),
            theme: "dark".to_string(),
            profiles: builtin_profiles(),
        }
    }
}

pub fn builtin_profiles() -> Vec<Profile> {
    vec![
        Profile {
            id: "default".into(),
            name: "Default".into(),
            layout: DEFAULT_LAYOUT.into(),
            disabled_keys: vec![],
            builtin: true,
        },
        Profile {
            id: "gaming".into(),
            name: "Gaming".into(),
            layout: DEFAULT_LAYOUT.into(),
            disabled_keys: vec!["MetaLeft".into(), "MetaRight".into()],
            builtin: true,
        },
        Profile {
            id: "coding".into(),
            name: "Coding".into(),
            layout: DEFAULT_LAYOUT.into(),
            disabled_keys: vec![],
            builtin: true,
        },
    ]
}

impl AppConfig {
    pub fn normalize(mut self) -> Self {
        if self.profiles.is_empty() {
            self.profiles = builtin_profiles();
        }
        if !self
            .profiles
            .iter()
            .any(|p| p.id == self.selected_profile)
        {
            self.selected_profile = self
                .profiles
                .first()
                .map(|p| p.id.clone())
                .unwrap_or_else(|| DEFAULT_PROFILE_ID.to_string());
        }
        if let Some(profile) = self.current_profile() {
            if !self.selected_layout.is_empty() {
                let _ = profile;
            }
        }
        let valid_layouts = [
            "fullsize-ansi",
            "tkl-ansi",
            "75-ansi",
            "65-ansi",
            "60-ansi",
        ];
        if !valid_layouts.contains(&self.selected_layout.as_str()) {
            self.selected_layout = DEFAULT_LAYOUT.to_string();
        }
        if !matches!(self.locale.as_str(), "" | "en" | "ko" | "ja") {
            self.locale.clear();
        }
        if !matches!(self.theme.as_str(), "dark" | "light") {
            self.theme = "dark".to_string();
        }
        self.version = CONFIG_VERSION;
        self
    }

    pub fn current_profile(&self) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == self.selected_profile)
    }

    pub fn current_profile_mut(&mut self) -> Option<&mut Profile> {
        let id = self.selected_profile.clone();
        self.profiles.iter_mut().find(|p| p.id == id)
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn profile_mut(&mut self, id: &str) -> Option<&mut Profile> {
        self.profiles.iter_mut().find(|p| p.id == id)
    }
}
