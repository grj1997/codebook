use crate::{runtime, state::*};
use codebook_core::{
    model::*,
    windows::{clipboard, desktop},
};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tauri::{AppHandle, Manager, State};
use zeroize::Zeroizing;
type Managed<'a> = State<'a, Arc<AppState>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    exists: bool,
    unlocked: bool,
    entries: Vec<CredentialSummary>,
    settings: Settings,
    path: String,
}
#[tauri::command]
pub fn vault_status(state: Managed<'_>) -> Result<Status, String> {
    let vault = lock(&state.vault)?;
    Ok(Status {
        exists: vault.exists(),
        unlocked: vault.is_unlocked(),
        entries: vault.summaries(),
        settings: lock(&state.settings)?.clone(),
        path: vault.path.to_string_lossy().into(),
    })
}
async fn vault_job(
    state: Arc<AppState>,
    job: impl FnOnce(&mut codebook_core::vault::VaultStore) -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut vault = lock(&state.vault)?;
        job(&mut vault)
    })
    .await
    .map_err(|_| "密码库操作中断")?
}
fn after_unlock(app: &AppHandle, state: &AppState) {
    if let Err(error) = runtime::loaded_settings(app, state) {
        runtime::notify(app, &error);
    }
    runtime::sync_browsers(state);
    runtime::changed(app);
}
#[tauri::command]
pub async fn vault_create(
    app: AppHandle,
    state: Managed<'_>,
    password: String,
) -> Result<(), String> {
    let password = Zeroizing::new(password);
    vault_job(state.inner().clone(), move |v| v.create(&password)).await?;
    after_unlock(&app, &state);
    Ok(())
}
#[tauri::command]
pub async fn vault_unlock(
    app: AppHandle,
    state: Managed<'_>,
    password: String,
) -> Result<(), String> {
    let password = Zeroizing::new(password);
    vault_job(state.inner().clone(), move |v| v.unlock(&password)).await?;
    after_unlock(&app, &state);
    Ok(())
}
#[tauri::command]
pub async fn vault_lock(app: AppHandle, state: Managed<'_>) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _gate = lock(&state.fill_gate)?;
        lock(&state.vault)?.lock();
        runtime::cancel(&app, &state);
        runtime::sync_browsers(&state);
        runtime::changed(&app);
        Ok(())
    })
    .await
    .map_err(|_| "锁定操作中断")?
}
#[tauri::command]
pub fn credential_get(state: Managed<'_>, id: String) -> Result<Credential, String> {
    lock(&state.vault)?.get(&id)
}
#[tauri::command]
pub async fn credential_save(
    app: AppHandle,
    state: Managed<'_>,
    entry: Credential,
) -> Result<(), String> {
    vault_job(state.inner().clone(), move |v| v.upsert(entry).map(|_| ())).await?;
    runtime::changed(&app);
    Ok(())
}
#[tauri::command]
pub async fn credential_delete(
    app: AppHandle,
    state: Managed<'_>,
    id: String,
) -> Result<(), String> {
    vault_job(state.inner().clone(), move |v| v.delete(&id)).await?;
    runtime::changed(&app);
    Ok(())
}
#[tauri::command]
pub async fn credential_copy(
    app: AppHandle,
    state: Managed<'_>,
    id: String,
    field: String,
) -> Result<(), String> {
    let entry = lock(&state.vault)?.get(&id)?;
    let owner = app
        .get_webview_window("main")
        .ok_or("主窗口不可用")?
        .hwnd()
        .map_err(|_| "主窗口不可用")?
        .0 as usize as u64;
    tauri::async_runtime::spawn_blocking(move || match field.as_str() {
        "username" => clipboard::copy(owner, &entry.username),
        "password" => clipboard::copy(owner, &entry.password),
        _ => Err("不支持复制此字段".into()),
    })
    .await
    .map_err(|_| "复制操作中断")?
}
#[tauri::command]
pub async fn list_apps(state: Managed<'_>) -> Result<Vec<desktop::AppInfo>, String> {
    lock(&state.vault)?.data()?;
    tauri::async_runtime::spawn_blocking(desktop::list_apps)
        .await
        .map_err(|_| "应用列表读取中断")?
}
#[tauri::command]
pub fn describe_app(state: Managed<'_>, path: String) -> Result<desktop::AppInfo, String> {
    lock(&state.vault)?.data()?;
    desktop::describe_app(&path)
}
#[tauri::command]
pub fn save_settings(app: AppHandle, state: Managed<'_>, settings: Settings) -> Result<(), String> {
    validate_settings(&settings)?;
    runtime::apply_settings(&app, &state, settings)?;
    runtime::changed(&app);
    Ok(())
}
#[tauri::command]
pub async fn vault_change_password(
    state: Managed<'_>,
    current: String,
    next: String,
) -> Result<(), String> {
    let current = Zeroizing::new(current);
    let next = Zeroizing::new(next);
    vault_job(state.inner().clone(), move |v| {
        v.change_password(&current, &next)
    })
    .await
}
#[tauri::command]
pub async fn vault_backup(state: Managed<'_>, path: String) -> Result<(), String> {
    vault_job(state.inner().clone(), move |v| {
        v.backup(&PathBuf::from(path))
    })
    .await
}
#[tauri::command]
pub async fn vault_verify_backup(path: String, password: String) -> Result<usize, String> {
    let password = Zeroizing::new(password);
    tauri::async_runtime::spawn_blocking(move || {
        codebook_core::vault::VaultStore::verify_backup(&PathBuf::from(path), &password)
    })
    .await
    .map_err(|_| "备份验证中断")?
}
#[tauri::command]
pub async fn vault_restore(
    app: AppHandle,
    state: Managed<'_>,
    path: String,
    password: String,
) -> Result<(), String> {
    let password = Zeroizing::new(password);
    vault_job(state.inner().clone(), move |v| {
        v.restore(&PathBuf::from(path), &password)
    })
    .await?;
    runtime::cancel(&app, &state);
    after_unlock(&app, &state);
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    id: String,
    extension_id: String,
    paired: bool,
    browser: String,
    origin: Option<String>,
}
#[tauri::command]
pub fn browser_connections(state: Managed<'_>) -> Result<Vec<ConnectionView>, String> {
    let vault = lock(&state.vault)?;
    vault.data()?;
    let sessions = lock(&state.sessions)?;
    Ok(sessions
        .values()
        .map(|s| ConnectionView {
            id: s.id.clone(),
            extension_id: s.extension_id.clone(),
            paired: vault.paired(&s.extension_id),
            browser: s.browser.clone(),
            origin: s.target.as_ref().map(|t| t.origin.clone()),
        })
        .collect())
}
#[tauri::command]
pub async fn browser_pair(state: Managed<'_>, id: String) -> Result<(), String> {
    let extension = lock(&state.sessions)?
        .get(&id)
        .ok_or("浏览器连接已失效")?
        .extension_id
        .clone();
    vault_job(state.inner().clone(), move |v| v.pair(&extension, true)).await?;
    runtime::sync_browsers(&state);
    Ok(())
}
#[tauri::command]
pub async fn browser_revoke(
    app: AppHandle,
    state: Managed<'_>,
    extension_id: String,
) -> Result<(), String> {
    vault_job(state.inner().clone(), move |v| v.pair(&extension_id, false)).await?;
    runtime::cancel(&app, &state);
    runtime::sync_browsers(&state);
    Ok(())
}
#[tauri::command]
pub fn picker_state(state: Managed<'_>) -> Result<Option<PickerView>, String> {
    runtime::picker_view(&state)
}
#[tauri::command]
pub async fn fill_confirm(
    app: AppHandle,
    state: Managed<'_>,
    token: String,
    id: String,
    mode: String,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || runtime::fill(&app, &state, &token, &id, &mode))
        .await
        .map_err(|_| "填充操作中断")?
}
#[tauri::command]
pub fn picker_cancel(app: AppHandle, state: Managed<'_>) {
    runtime::cancel(&app, &state);
}
#[tauri::command]
pub fn quit_app(app: AppHandle, state: Managed<'_>) {
    if let Ok(mut vault) = lock(&state.vault) {
        vault.lock();
    }
    app.exit(0);
}
