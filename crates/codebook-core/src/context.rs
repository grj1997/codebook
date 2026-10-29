use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FocusAnchor {
    pub bounds: [i32; 4],
    pub automation_id: String,
    pub class_name: String,
    pub is_password: bool,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WindowTarget {
    pub hwnd: u64,
    pub pid: u32,
    pub started: u64,
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub focus_hwnd: u64,
    pub focus: Option<FocusAnchor>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTarget {
    pub tab_id: i64,
    pub window_id: i64,
    pub document_id: String,
    pub origin: String,
    pub content_nonce: String,
    pub editable: bool,
    pub active: bool,
    pub focused_at: u64,
    pub browser: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TargetContext {
    Desktop {
        window: WindowTarget,
    },
    Browser {
        session_id: String,
        window: WindowTarget,
        target: BrowserTarget,
    },
}
impl TargetContext {
    pub fn window(&self) -> &WindowTarget {
        match self {
            Self::Desktop { window } | Self::Browser { window, .. } => window,
        }
    }
    pub fn origin(&self) -> Option<&str> {
        match self {
            Self::Browser { target, .. } => Some(&target.origin),
            _ => None,
        }
    }
}
#[derive(Clone)]
pub struct FillTicket {
    pub token: String,
    pub target: TargetContext,
    pub account_ids: Vec<String>,
    pub expires_at: u64,
}
#[derive(Default)]
pub struct TicketRegistry {
    pub pending: Option<FillTicket>,
}
impl TicketRegistry {
    pub fn issue(
        &mut self,
        target: TargetContext,
        account_ids: Vec<String>,
        now_ms: u64,
    ) -> FillTicket {
        let ticket = FillTicket {
            token: uuid::Uuid::new_v4().to_string(),
            target,
            account_ids,
            expires_at: now_ms.saturating_add(30_000),
        };
        self.pending = Some(ticket.clone());
        ticket
    }
    pub fn consume(
        &mut self,
        token: &str,
        account_id: &str,
        now_ms: u64,
    ) -> Result<FillTicket, String> {
        let ticket = self.pending.take().ok_or("本次调用已失效，请重新触发")?;
        if ticket.token != token
            || now_ms >= ticket.expires_at
            || !ticket.account_ids.iter().any(|id| id == account_id)
        {
            return Err("填充授权无效或已过期，请重新调用".into());
        }
        Ok(ticket)
    }
    pub fn clear(&mut self) {
        self.pending = None;
    }
}
