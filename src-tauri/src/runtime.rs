use crate::state::*;
use codebook_core::{
    context::*,
    model::{matches_binding, summarize, Settings},
    windows::desktop,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

pub fn notify(app: &AppHandle, message: &str) {
    show_main(app);
    let _ = app.emit_to("main", "operation-error", message);
}
pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
pub fn changed(app: &AppHandle) {
    let _ = app.emit_to("main", "vault-changed", ());
}
pub fn sync_browsers(state: &AppState) {
    if let (Ok(vault), Ok(sessions)) = (lock(&state.vault), lock(&state.sessions)) {
        for session in sessions.values() {
            send(
                session,
                &serde_json::json!({ "type": "status", "paired": vault.paired(&session.extension_id) }),
            );
        }
    }
}
pub fn send(session: &BrowserSession, message: &serde_json::Value) -> bool {
    serde_json::to_vec(message).ok().is_some_and(|bytes| {
        session
            .sender
            .try_send(zeroize::Zeroizing::new(bytes))
            .is_ok()
    })
}
pub fn apply_settings(app: &AppHandle, state: &AppState, settings: Settings) -> Result<(), String> {
    let new: Shortcut = settings
        .shortcut
        .parse()
        .map_err(|_| "快捷键格式无效，例如 Ctrl+Alt+K")?;
    let previous = lock(&state.settings)?.clone();
    if previous.shortcut != settings.shortcut || !app.global_shortcut().is_registered(new) {
        app.global_shortcut()
            .register(new)
            .map_err(|_| "此快捷键已被占用，请更换组合")?;
        if let Err(error) = lock(&state.vault)?.update_settings(settings.clone()) {
            let _ = app.global_shortcut().unregister(new);
            return Err(error);
        }
        if previous.shortcut != settings.shortcut {
            let _ = app.global_shortcut().unregister(previous.shortcut.as_str());
        }
    } else {
        lock(&state.vault)?.update_settings(settings.clone())?;
    }
    *lock(&state.settings)? = settings;
    Ok(())
}
pub fn loaded_settings(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let settings = lock(&state.vault)?.data()?.settings.clone();
    let previous = lock(&state.settings)?.clone();
    if settings.shortcut != previous.shortcut
        || !app
            .global_shortcut()
            .is_registered(settings.shortcut.as_str())
    {
        let parsed: Shortcut = settings
            .shortcut
            .parse()
            .map_err(|_| "已保存的快捷键格式无效")?;
        app.global_shortcut()
            .register(parsed)
            .map_err(|_| "密码库已解锁，但保存的快捷键被占用。请到设置更换快捷键")?;
        if settings.shortcut != previous.shortcut {
            let _ = app.global_shortcut().unregister(previous.shortcut.as_str());
        }
    }
    *lock(&state.settings)? = settings;
    Ok(())
}
pub fn build_context(
    state: &AppState,
    window: WindowTarget,
    explicit_session: Option<&str>,
) -> Result<TargetContext, String> {
    desktop::same_window(&window)?;
    if let Some(browser) = desktop::browser_name(&window.path) {
        let vault = lock(&state.vault)?;
        let sessions = lock(&state.sessions)?;
        let candidates: Vec<_> = sessions
            .values()
            .filter(|session| {
                explicit_session.is_none_or(|id| id == session.id)
                    && session.browser == browser
                    && session.last_seen.elapsed() < Duration::from_secs(45)
                    && vault.paired(&session.extension_id)
                    && session.target.as_ref().is_some_and(|target| target.active)
            })
            .collect();
        if candidates.len() != 1 {
            return Err(
                "浏览器目标不明确或尚未连接，请从目标网站点击 Codebook 扩展，并在设置中完成配对"
                    .into(),
            );
        }
        let session = candidates[0];
        let target = session.target.clone().ok_or("没有可用的网站上下文")?;
        return Ok(TargetContext::Browser {
            session_id: session.id.clone(),
            window,
            target,
        });
    }
    if explicit_session.is_some() {
        return Err("请保持目标浏览器在前台，再点击扩展".into());
    }
    Ok(TargetContext::Desktop { window })
}
pub fn matching(
    state: &AppState,
    context: &TargetContext,
) -> Result<Vec<codebook_core::model::CredentialSummary>, String> {
    let vault = lock(&state.vault)?;
    let window = context.window();
    Ok(vault
        .data()?
        .entries
        .iter()
        .filter(|e| {
            e.bindings
                .iter()
                .any(|b| matches_binding(b, &window.path, &window.title, context.origin()))
        })
        .map(summarize)
        .collect())
}
pub fn trigger(
    app: &AppHandle,
    state: &Arc<AppState>,
    explicit_session: Option<&str>,
) -> Result<(), String> {
    if !lock(&state.vault)?.is_unlocked() {
        show_main(app);
        return Ok(());
    }
    let window = desktop::capture_foreground()?;
    if window.pid == std::process::id() {
        return Err("请先返回需要填充的应用输入框".into());
    }
    let context = build_context(state, window, explicit_session)?;
    trigger_context(app, state, context, None)
}
pub fn trigger_context(
    app: &AppHandle,
    state: &Arc<AppState>,
    context: TargetContext,
    position: Option<(i32, i32)>,
) -> Result<(), String> {
    if !lock(&state.vault)?.is_unlocked() {
        return Err("请先解锁密码库".into());
    }
    desktop::same_window(context.window())?;
    if !desktop::foreground_is_target(context.window()) {
        return Err("目标窗口已改变".into());
    }
    if position.is_some() {
        if !lock(&state.settings)?.mouse_enabled || context.window().focus.is_none() {
            return Ok(());
        }
        if let TargetContext::Browser { target, .. } = &context {
            if !target.editable {
                return Ok(());
            }
        }
    }
    let entries = matching(state, &context)?;
    if position.is_some() && entries.is_empty() {
        return Ok(());
    }
    {
        let mut last = lock(&state.last_trigger)?;
        if last.as_ref().is_some_and(|(hwnd, at)| {
            *hwnd == context.window().hwnd && at.elapsed() < Duration::from_millis(500)
        }) {
            return Ok(());
        }
        *last = Some((context.window().hwnd, Instant::now()));
    }
    lock(&state.tickets)?.issue(
        context,
        entries.iter().map(|e| e.id.clone()).collect(),
        state.clock(),
    );
    let picker = app.get_webview_window("picker").ok_or("快捷浮窗不可用")?;
    if let Some((x, y)) = position {
        if let Ok(monitors) = picker.available_monitors() {
            if let Some(monitor) = monitors.into_iter().find(|m| {
                x >= m.position().x
                    && y >= m.position().y
                    && x < m.position().x + m.size().width as i32
                    && y < m.position().y + m.size().height as i32
            }) {
                let width = (420.0 * monitor.scale_factor()) as i32;
                let height = (475.0 * monitor.scale_factor()) as i32;
                let left = (x + 12)
                    .min(monitor.position().x + monitor.size().width as i32 - width)
                    .max(monitor.position().x);
                let top = (y + 12)
                    .min(monitor.position().y + monitor.size().height as i32 - height)
                    .max(monitor.position().y);
                let _ = picker.set_position(PhysicalPosition::new(left, top));
            }
        }
    } else {
        let _ = picker.center();
    }
    let _ = picker.emit("picker-ready", ());
    picker.show().map_err(|_| "无法显示账户浮窗")?;
    let _ = picker.set_focus();
    Ok(())
}
pub fn picker_view(state: &AppState) -> Result<Option<PickerView>, String> {
    let ticket = lock(&state.tickets)?.pending.clone();
    let Some(ticket) = ticket else {
        return Ok(None);
    };
    if state.clock() >= ticket.expires_at || !lock(&state.vault)?.is_unlocked() {
        return Ok(None);
    }
    let entries = matching(state, &ticket.target)?
        .into_iter()
        .filter(|e| ticket.account_ids.contains(&e.id))
        .collect();
    Ok(Some(PickerView {
        token: ticket.token,
        target_label: target_label(&ticket.target),
        entries,
        browser: ticket.target.origin().is_some(),
        expires_at: unix_ms() + ticket.expires_at.saturating_sub(state.clock()),
    }))
}
pub fn cancel(app: &AppHandle, state: &AppState) {
    if let Ok(mut tickets) = lock(&state.tickets) {
        tickets.clear();
    }
    if let Some(picker) = app.get_webview_window("picker") {
        let _ = picker.hide();
    }
}
pub fn same_document(a: &BrowserTarget, b: &BrowserTarget) -> bool {
    a.tab_id == b.tab_id
        && a.window_id == b.window_id
        && a.document_id == b.document_id
        && a.content_nonce == b.content_nonce
        && a.origin == b.origin
}
pub fn fill(
    app: &AppHandle,
    state: &Arc<AppState>,
    token: &str,
    id: &str,
    mode: &str,
) -> Result<(), String> {
    let _gate = lock(&state.fill_gate)?;
    if !["auto", "username", "password", "sequence"].contains(&mode) {
        return Err("不支持的填充方式".into());
    }
    let ticket = lock(&state.tickets)?.consume(token, id, state.clock())?;
    let entry = lock(&state.vault)?.get(id)?;
    if !entry.bindings.iter().any(|b| {
        matches_binding(
            b,
            &ticket.target.window().path,
            &ticket.target.window().title,
            ticket.target.origin(),
        )
    }) {
        return Err("账户关联已经变化，请重新调用".into());
    }
    if let Some(picker) = app.get_webview_window("picker") {
        let _ = picker.hide();
    }
    let result = (|| match &ticket.target {
        TargetContext::Desktop { window } => desktop::fill(window, &entry, mode),
        TargetContext::Browser {
            session_id,
            window,
            target,
        } => {
            if mode == "sequence" {
                return Err("网站请使用表单填充方式".into());
            }
            desktop::restore_target(window)?;
            let vault = lock(&state.vault)?;
            let sessions = lock(&state.sessions)?;
            let session = sessions.get(session_id).ok_or("浏览器已经断开连接")?;
            if !vault.paired(&session.extension_id)
                || !session
                    .target
                    .as_ref()
                    .is_some_and(|current| same_document(target, current))
            {
                return Err("目标网站已改变或授权已撤销，请重新调用".into());
            }
            #[derive(serde::Serialize)]
            struct FillResponse<'a> {
                r#type: &'static str,
                token: &'a str,
                target: &'a BrowserTarget,
                username: &'a str,
                password: &'a str,
                mode: &'a str,
            }
            let response = FillResponse {
                r#type: "fill",
                token,
                target,
                username: &entry.username,
                password: &entry.password,
                mode,
            };
            let bytes = zeroize::Zeroizing::new(
                serde_json::to_vec(&response).map_err(|_| "无法编码填充响应")?,
            );
            let sent = session.sender.try_send(bytes).is_ok();
            if sent {
                Ok(())
            } else {
                Err("浏览器通信繁忙或已断开，请重新调用".into())
            }
        }
    })();
    if result.is_err() {
        if let Some(picker) = app.get_webview_window("picker") {
            let _ = picker.show();
            let _ = picker.set_focus();
        }
    }
    result
}
