use crate::commands::keyboard::{enable_all_keys, set_cat_lock};
use crate::commands::profiles::select_profile;
use crate::state::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("missing default window icon".into()))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("KeyGlow")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "open" => crate::show_main(app),
                "enable_all" => {
                    if let Some(state) = app.try_state::<AppState>() {
                        let _ = enable_all_keys(app.clone(), state);
                    }
                }
                "cat_lock" => {
                    if let Some(state) = app.try_state::<AppState>() {
                        let next = !state.controller.is_cat_locked();
                        let _ = set_cat_lock(app.clone(), state, next);
                    }
                }
                "exit" => {
                    if let Some(state) = app.try_state::<AppState>() {
                        (state.shutdown)();
                    }
                    app.exit(0);
                }
                other if other.starts_with("profile:") => {
                    let profile_id = other.trim_start_matches("profile:");
                    if let Some(state) = app.try_state::<AppState>() {
                        let _ = select_profile(app.clone(), state, profile_id.to_string());
                    }
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn refresh(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(menu) = build_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let locale = app
        .try_state::<AppState>()
        .map(|s| s.config.lock().locale.clone())
        .unwrap_or_default();
    let open = MenuItem::with_id(app, "open", crate::i18n::t(&locale, "tray.open"), true, None::<&str>)?;
    let cat_locked = app
        .try_state::<AppState>()
        .map(|s| s.controller.is_cat_locked())
        .unwrap_or(false);
    let enable_all = MenuItem::with_id(
        app,
        "enable_all",
        crate::i18n::t(&locale, "tray.enable_all"),
        true,
        None::<&str>,
    )?;
    let cat_lock = MenuItem::with_id(
        app,
        "cat_lock",
        crate::i18n::t(
            &locale,
            if cat_locked {
                "tray.cat_unlock"
            } else {
                "tray.cat_lock"
            },
        ),
        true,
        None::<&str>,
    )?;
    let exit = MenuItem::with_id(app, "exit", crate::i18n::t(&locale, "tray.exit"), true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let sep4 = PredefinedMenuItem::separator(app)?;

    let mut items: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = vec![&open, &sep1];

    let profiles = app
        .try_state::<AppState>()
        .map(|s| s.config.lock().profiles.clone())
        .unwrap_or_default();
    let selected = app
        .try_state::<AppState>()
        .map(|s| s.config.lock().selected_profile.clone())
        .unwrap_or_default();

    let profile_items: Vec<MenuItem<Wry>> = profiles
        .iter()
        .filter_map(|profile| {
            let mark = if profile.id == selected { "● " } else { "○ " };
            let name = crate::i18n::builtin_profile_name(&locale, &profile.id, &profile.name);
            MenuItem::with_id(
                app,
                format!("profile:{}", profile.id),
                format!("{}{}: {}", mark, crate::i18n::t(&locale, "tray.profile"), name),
                true,
                None::<&str>,
            )
            .ok()
        })
        .collect();

    for item in &profile_items {
        items.push(item);
    }
    items.push(&sep2);
    items.push(&cat_lock);
    items.push(&sep4);
    items.push(&enable_all);
    items.push(&sep3);
    items.push(&exit);

    Menu::with_items(app, &items)
}
