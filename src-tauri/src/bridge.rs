use crate::{runtime, state::*};
use codebook_core::{
    context::BrowserTarget,
    model::normalize_origin,
    protocol::{read_frame, write_frame},
    windows::pipe,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    sync::{mpsc, Arc},
    thread,
    time::Instant,
};
use tauri::AppHandle;
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    extension_id: String,
    message: Value,
}
fn allowed_id() -> String {
    serde_json::from_str::<Value>(include_str!("../../shared/extension-id.json"))
        .expect("compiled extension identity")["id"]
        .as_str()
        .expect("extension ID")
        .into()
}
pub fn start(app: AppHandle, state: Arc<AppState>) {
    thread::spawn(move || {
        let mut first = true;
        loop {
            let file = match pipe::create_server(first) {
                Ok(file) => file,
                Err(error) => {
                    runtime::notify(&app, &error);
                    return;
                }
            };
            first = false;
            let app = app.clone();
            let state = state.clone();
            thread::spawn(move || {
                let _ = connection(app, state, file);
            });
        }
    });
}
fn connection(app: AppHandle, state: Arc<AppState>, mut file: pipe::Pipe) -> Result<(), String> {
    let mut writer = file.try_clone().map_err(|_| "通信通道不可用")?;
    let (sender, receiver) = mpsc::sync_channel::<Zeroizing<Vec<u8>>>(16);
    let id = uuid::Uuid::new_v4().to_string();
    thread::spawn(move || {
        for bytes in receiver {
            if write_frame(&mut writer, &bytes).is_err() {
                break;
            }
        }
    });
    let allowed = allowed_id();
    loop {
        let bytes = match read_frame(&mut file) {
            Ok(Some(bytes)) if bytes.len() <= 65536 => bytes,
            _ => break,
        };
        let envelope: Envelope = match serde_json::from_slice(&bytes) {
            Ok(message) => message,
            Err(_) => break,
        };
        if envelope.extension_id != allowed {
            break;
        }
        {
            let mut sessions = lock(&state.sessions)?;
            if !sessions.contains_key(&id) {
                if sessions.len() >= 16 {
                    break;
                }
                sessions.insert(
                    id.clone(),
                    BrowserSession {
                        id: id.clone(),
                        extension_id: allowed.clone(),
                        browser: String::new(),
                        target: None,
                        last_seen: Instant::now(),
                        sender: sender.clone(),
                    },
                );
            }
            if let Some(session) = sessions.get_mut(&id) {
                session.last_seen = Instant::now();
            }
        }
        if let Err(error) = handle(&app, &state, &id, envelope.message) {
            if let Ok(sessions) = lock(&state.sessions) {
                if let Some(session) = sessions.get(&id) {
                    runtime::send(session, &json!({ "type": "error", "message": error }));
                }
            }
        }
    }
    if let Ok(mut sessions) = lock(&state.sessions) {
        sessions.remove(&id);
    }
    if let Ok(mut tickets) = lock(&state.tickets) {
        if tickets.pending.as_ref().is_some_and(|t| matches!(&t.target, codebook_core::context::TargetContext::Browser { session_id, .. } if session_id == &id)) { tickets.clear(); }
    }
    Ok(())
}
fn handle(app: &AppHandle, state: &Arc<AppState>, id: &str, message: Value) -> Result<(), String> {
    let kind = message["type"].as_str().ok_or("消息缺少类型")?;
    match kind {
        "hello" => {
            let browser = message["browser"]
                .as_str()
                .filter(|name| ["Google Chrome", "Microsoft Edge"].contains(name))
                .ok_or("不支持的浏览器")?;
            {
                let mut sessions = lock(&state.sessions)?;
                sessions.get_mut(id).ok_or("连接已关闭")?.browser = browser.into();
            }
            runtime::sync_browsers(state);
        }
        "context" | "requestFill" => {
            let target: BrowserTarget =
                serde_json::from_value(message["target"].clone()).map_err(|_| "网站上下文无效")?;
            if target.tab_id < 0
                || target.window_id < 0
                || target.document_id.is_empty()
                || target.document_id.len() > 128
                || target.content_nonce.len() > 128
                || target.content_nonce.len() < 16
                || target.origin.len() > 2048
                || normalize_origin(&target.origin)? != target.origin
            {
                return Err("网站上下文无效".into());
            }
            {
                let mut sessions = lock(&state.sessions)?;
                let session = sessions.get_mut(id).ok_or("连接已关闭")?;
                if session.browser != target.browser {
                    return Err("浏览器身份不一致".into());
                }
                session.target = Some(target);
            }
            if kind == "requestFill" {
                runtime::trigger(app, state, Some(id))?;
            }
        }
        "inactive" => {
            if let Some(session) = lock(&state.sessions)?.get_mut(id) {
                session.target = None;
            }
        }
        "fillResult" => {
            if message["ok"].as_bool() == Some(false) {
                runtime::notify(
                    app,
                    "浏览器未能完成填充，请回到原登录框重新调用，或检查扩展图标提示",
                );
            }
        }
        _ => return Err("不支持的浏览器消息".into()),
    }
    Ok(())
}
