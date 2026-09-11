pub fn normalize(locale: &str) -> &'static str {
    match locale {
        "ko" => "ko",
        "ja" => "ja",
        _ => "en",
    }
}

pub fn t(locale: &str, key: &str) -> &'static str {
    match (normalize(locale), key) {
        ("ko", "tray.open") => "KeyGlow 열기",
        ("ko", "tray.enable_all") => "모든 키 켜기",
        ("ko", "tray.exit") => "종료",
        ("ko", "tray.cat_lock") => "고양이 잠금 (모든 키 차단)",
        ("ko", "tray.cat_unlock") => "고양이 잠금 해제",
        ("ko", "tray.profile") => "프로필",
        ("ja", "tray.open") => "KeyGlow を開く",
        ("ja", "tray.enable_all") => "すべてのキーを有効",
        ("ja", "tray.exit") => "終了",
        ("ja", "tray.cat_lock") => "猫ロック（全キー遮断）",
        ("ja", "tray.cat_unlock") => "猫ロック解除",
        ("ja", "tray.profile") => "プロファイル",
        (_, "tray.open") => "Open KeyGlow",
        (_, "tray.enable_all") => "Enable All Keys",
        (_, "tray.exit") => "Exit",
        (_, "tray.cat_lock") => "Cat lock (block all keys)",
        (_, "tray.cat_unlock") => "Unlock cat lock",
        (_, "tray.profile") => "Profile",
        _ => "KeyGlow",
    }
}

pub fn builtin_profile_name(locale: &str, id: &str, fallback: &str) -> String {
    match (normalize(locale), id) {
        ("ko", "default") => "기본".into(),
        ("ko", "gaming") => "게임".into(),
        ("ko", "coding") => "코딩".into(),
        ("ja", "default") => "デフォルト".into(),
        ("ja", "gaming") => "ゲーム".into(),
        ("ja", "coding") => "コーディング".into(),
        ("en", "default") => "Default".into(),
        ("en", "gaming") => "Gaming".into(),
        ("en", "coding") => "Coding".into(),
        _ => fallback.to_string(),
    }
}
