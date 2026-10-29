use codebook_core::{
    context::{BrowserTarget, TargetContext, TicketRegistry},
    model::{CredentialSummary, Settings},
    vault::VaultStore,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{mpsc::SyncSender, Mutex, MutexGuard},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

pub fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    mutex
        .lock()
        .map_err(|_| "内部状态不可用，请重新启动 Codebook".into())
}
pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub struct BrowserSession {
    pub id: String,
    pub extension_id: String,
    pub browser: String,
    pub target: Option<BrowserTarget>,
    pub last_seen: Instant,
    pub sender: SyncSender<Zeroizing<Vec<u8>>>,
}
pub struct AppState {
    pub vault: Mutex<VaultStore>,
    pub tickets: Mutex<TicketRegistry>,
    pub sessions: Mutex<HashMap<String, BrowserSession>>,
    pub settings: Mutex<Settings>,
    pub fill_gate: Mutex<()>,
    pub started: Instant,
    pub last_trigger: Mutex<Option<(u64, Instant)>>,
}
impl AppState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            vault: Mutex::new(VaultStore::new(path)),
            tickets: Mutex::new(TicketRegistry::default()),
            sessions: Mutex::new(HashMap::new()),
            settings: Mutex::new(Settings::default()),
            fill_gate: Mutex::new(()),
            started: Instant::now(),
            last_trigger: Mutex::new(None),
        }
    }
    pub fn clock(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerView {
    pub token: String,
    pub target_label: String,
    pub entries: Vec<CredentialSummary>,
    pub browser: bool,
    pub expires_at: u64,
}
pub fn target_label(target: &TargetContext) -> String {
    match target {
        TargetContext::Browser { target, .. } => target.origin.clone(),
        TargetContext::Desktop { window } => format!(
            "{} · {}",
            window.path.rsplit(['\\', '/']).next().unwrap_or("应用"),
            window.title
        ),
    }
}
