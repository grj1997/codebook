use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TargetBinding {
    Desktop {
        path: String,
        #[serde(default)]
        title_contains: String,
    },
    Website {
        origin: String,
    },
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum FillStep {
    Username,
    Password,
    Tab,
    ShiftTab,
    Delay(u64),
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FillProfile {
    pub steps: Vec<FillStep>,
}
impl Default for FillProfile {
    fn default() -> Self {
        Self {
            steps: vec![FillStep::Username, FillStep::Tab, FillStep::Password],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    pub id: String,
    pub title: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub bindings: Vec<TargetBinding>,
    #[serde(default)]
    pub profile: FillProfile,
}
impl Drop for Credential {
    fn drop(&mut self) {
        self.password.zeroize();
        self.username.zeroize();
        self.notes.zeroize();
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VaultData {
    pub entries: Vec<Credential>,
    #[serde(default)]
    pub paired_extensions: Vec<String>,
    #[serde(default)]
    pub settings: Settings,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub shortcut: String,
    pub mouse_enabled: bool,
    pub double_click_ms: Option<u32>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcut: "Ctrl+Alt+K".into(),
            mouse_enabled: true,
            double_click_ms: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialSummary {
    pub id: String,
    pub title: String,
    pub username_hint: String,
    pub bindings: Vec<TargetBinding>,
}

pub fn normalize_origin(input: &str) -> Result<String, String> {
    let url = url::Url::parse(input.trim()).map_err(|_| "请输入完整的 HTTPS 网站地址")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("只允许不含用户名和密码的 HTTPS 网站地址".into());
    }
    Ok(url.origin().ascii_serialization())
}
pub fn normalize_path(path: &str) -> String {
    path.trim_start_matches("\\\\?\\")
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}
pub fn validate_credential(entry: &mut Credential) -> Result<(), String> {
    entry.title = entry.title.trim().to_string();
    if entry.title.is_empty() || entry.title.chars().count() > 160 {
        return Err("账户名称需要 1～160 个字符".into());
    }
    if entry.username.len() > 4096
        || entry.password.is_empty()
        || entry.password.len() > 4096
        || entry.notes.len() > 16384
        || entry.username.contains('\0')
        || entry.password.contains('\0')
    {
        return Err("账号或密码为空、过长或包含不支持的空字符".into());
    }
    if entry.bindings.len() > 64 {
        return Err("一个账户最多关联 64 个目标".into());
    }
    for binding in &mut entry.bindings {
        match binding {
            TargetBinding::Website { origin } => *origin = normalize_origin(origin)?,
            TargetBinding::Desktop {
                path,
                title_contains,
            } => {
                let clean = normalize_path(path);
                let absolute = clean.starts_with("\\\\")
                    || clean.as_bytes().get(1) == Some(&b':')
                        && clean.as_bytes().get(2) == Some(&b'\\');
                if !absolute
                    || !clean.ends_with(".exe")
                    || clean.contains('\0')
                    || clean.len() > 32768
                    || title_contains.len() > 512
                {
                    return Err("应用关联需要完整 EXE 路径，窗口标题条件最多 512 字节".into());
                }
                *path = clean;
            }
        }
    }
    if entry.profile.steps.is_empty() || entry.profile.steps.len() > 32 {
        return Err("输入顺序需要 1～32 个步骤".into());
    }
    let mut delay = 0;
    for step in &entry.profile.steps {
        if let FillStep::Delay(ms) = step {
            if *ms > 2000 {
                return Err("单次等待不能超过 2000 毫秒".into());
            }
            delay += ms;
        }
    }
    if delay > 5000 {
        return Err("总等待不能超过 5000 毫秒".into());
    }
    Ok(())
}
pub fn matches_binding(
    binding: &TargetBinding,
    path: &str,
    title: &str,
    origin: Option<&str>,
) -> bool {
    match binding {
        TargetBinding::Website { origin: expected } => {
            origin.is_some_and(|actual| actual == expected)
        }
        TargetBinding::Desktop {
            path: expected,
            title_contains,
        } => {
            origin.is_none()
                && normalize_path(expected) == normalize_path(path)
                && (title_contains.is_empty() || title.contains(title_contains))
        }
    }
}
pub fn validate_settings(settings: &Settings) -> Result<(), String> {
    if settings.shortcut.is_empty()
        || settings.shortcut.len() > 80
        || settings
            .double_click_ms
            .is_some_and(|ms| !(150..=1000).contains(&ms))
    {
        return Err("快捷键无效，双击间隔需为 150～1000 毫秒".into());
    }
    Ok(())
}
pub fn summarize(entry: &Credential) -> CredentialSummary {
    let prefix: String = entry.username.chars().take(2).collect();
    CredentialSummary {
        id: entry.id.clone(),
        title: entry.title.clone(),
        username_hint: if prefix.is_empty() {
            "未设置用户名".into()
        } else {
            format!("{prefix}••••")
        },
        bindings: entry.bindings.clone(),
    }
}
