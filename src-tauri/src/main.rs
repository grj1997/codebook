#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod bridge;
mod commands;
mod mouse;
mod runtime;
mod state;

use state::*;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            runtime::show_main(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let app = app.clone();
                    let state = app.state::<Arc<AppState>>().inner().clone();
                    std::thread::spawn(move || {
                        if let Err(error) = runtime::trigger(&app, &state, None) {
                            runtime::notify(&app, &error);
                        }
                    });
                })
                .build(),
        )
        .setup(|app| {
            let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA unavailable")?;
            let state = Arc::new(AppState::new(
                std::path::PathBuf::from(local)
                    .join("Codebook")
                    .join("vault.cbvault"),
            ));
            app.manage(state.clone());
            let open = MenuItem::with_id(app, "open", "打开 Codebook", true, None::<&str>)?;
            let lock_item = MenuItem::with_id(app, "lock", "锁定密码库", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &lock_item, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("Codebook · 本地密码库")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => runtime::show_main(app),
                    "lock" => {
                        let app = app.clone();
                        let state = app.state::<Arc<AppState>>().inner().clone();
                        std::thread::spawn(move || {
                            if let Ok(_gate) = lock(&state.fill_gate) {
                                if let Ok(mut vault) = lock(&state.vault) {
                                    vault.lock();
                                }
                                runtime::cancel(&app, &state);
                                runtime::sync_browsers(&state);
                                runtime::changed(&app);
                            }
                        });
                    }
                    "quit" => {
                        if let Ok(mut vault) = lock(&app.state::<Arc<AppState>>().vault) {
                            vault.lock();
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        runtime::show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            if app.global_shortcut().register("Ctrl+Alt+K").is_err() {
                runtime::notify(app.handle(), "默认快捷键已被占用，请解锁后到设置中更换");
            }
            bridge::start(app.handle().clone(), state.clone());
            mouse::start(app.handle().clone(), state);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window.label() == "picker" {
                    let state = window.state::<Arc<AppState>>();
                    runtime::cancel(window.app_handle(), &state);
                } else {
                    let _ = window.emit("window-hidden", ());
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::vault_create,
            commands::vault_unlock,
            commands::vault_lock,
            commands::credential_get,
            commands::credential_save,
            commands::credential_delete,
            commands::credential_copy,
            commands::list_apps,
            commands::describe_app,
            commands::save_settings,
            commands::vault_change_password,
            commands::vault_backup,
            commands::vault_verify_backup,
            commands::vault_restore,
            commands::browser_connections,
            commands::browser_pair,
            commands::browser_revoke,
            commands::quit_app,
            commands::picker_state,
            commands::fill_confirm,
            commands::picker_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("Codebook failed to start");
}
