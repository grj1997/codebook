#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use codebook_core::protocol::{read_frame, write_frame};
use serde_json::json;
use std::{io, path::Path, thread, time::Duration};
use zeroize::Zeroizing;

fn extension_id() -> String {
    serde_json::from_str::<serde_json::Value>(include_str!("../../../shared/extension-id.json"))
        .expect("compiled identity")["id"]
        .as_str()
        .expect("extension ID")
        .into()
}
fn report(message: &str) {
    if let Ok(bytes) = serde_json::to_vec(&json!({ "type": "error", "message": message })) {
        let _ = write_frame(&mut io::stdout().lock(), &bytes);
    }
}
#[cfg(windows)]
fn register(directory: &Path) -> Result<(), String> {
    use windows::{core::PCWSTR, Win32::System::Registry::*};
    let directory = std::fs::canonicalize(directory).map_err(|_| "安装目录无效")?;
    let executable = directory.join("codebook-native-host.exe");
    if !executable.is_file() {
        return Err("安装目录缺少本机通信程序".into());
    }
    let clean_path = |path: &Path| {
        path.to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .to_string()
    };
    let manifest = directory.join("codebook-native-host.json");
    let data = json!({ "name": "local.codebook.host", "description": "Codebook local vault bridge", "path": clean_path(&executable), "type": "stdio", "allowed_origins": [format!("chrome-extension://{}/", extension_id())] });
    std::fs::write(
        &manifest,
        serde_json::to_vec_pretty(&data).map_err(|_| "注册配置生成失败")?,
    )
    .map_err(|_| "无法写入本机通信配置")?;
    let value: Vec<u16> = clean_path(&manifest)
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let bytes = unsafe { std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), value.len() * 2) };
    for prefix in [r"SOFTWARE\Google\Chrome", r"SOFTWARE\Microsoft\Edge"] {
        let key: Vec<u16> = format!(r"{prefix}\NativeMessagingHosts\local.codebook.host")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        unsafe {
            let mut handle = HKEY::default();
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut handle,
                None,
            )
            .ok()
            .map_err(|_| "无法注册当前用户的浏览器通信主机")?;
            let result = RegSetValueExW(handle, PCWSTR::null(), None, REG_SZ, Some(bytes)).ok();
            let _ = RegCloseKey(handle);
            result.map_err(|_| "无法写入浏览器主机位置")?;
        }
    }
    Ok(())
}
#[cfg(windows)]
fn unregister() {
    use windows::{core::PCWSTR, Win32::System::Registry::*};
    for prefix in [r"SOFTWARE\Google\Chrome", r"SOFTWARE\Microsoft\Edge"] {
        let key: Vec<u16> = format!(r"{prefix}\NativeMessagingHosts\local.codebook.host")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        unsafe {
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()));
        }
    }
}
#[cfg(windows)]
fn bridge(origin: &str) -> Result<(), String> {
    let id = extension_id();
    if origin.trim_end_matches('/') != format!("chrome-extension://{id}") {
        return Err("扩展来源未获允许".into());
    }
    let name = codebook_core::windows::pipe::pipe_name()?;
    let mut pipe = None;
    for _ in 0..15 {
        match codebook_core::windows::pipe::open_client(&name) {
            Ok(file) => {
                pipe = Some(file);
                break;
            }
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
    let mut pipe = pipe.ok_or("Codebook 未运行，请先启动桌面程序")?;
    let mut input = pipe.try_clone().map_err(|_| "本机通信通道不可用")?;
    thread::spawn(move || {
        let mut stdout = io::stdout().lock();
        while let Ok(Some(bytes)) = read_frame(&mut input) {
            let bytes = Zeroizing::new(bytes);
            if write_frame(&mut stdout, &bytes).is_err() {
                break;
            }
        }
        // Exit when the desktop disconnects so Chrome can update its badge.
        std::process::exit(0);
    });
    let mut stdin = io::stdin().lock();
    while let Some(bytes) = read_frame(&mut stdin).map_err(|_| "浏览器消息格式无效")? {
        let bytes = Zeroizing::new(bytes);
        let message: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "浏览器消息格式无效")?;
        if !message.is_object() {
            return Err("浏览器消息格式无效".into());
        }
        let envelope = serde_json::to_vec(&json!({ "extensionId": id, "message": message }))
            .map_err(|_| "无法编码消息")?;
        write_frame(&mut pipe, &envelope).map_err(|_| "Codebook 已断开连接")?;
    }
    Ok(())
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    {
        if args.first().is_some_and(|s| s == "--register") {
            let result = args
                .get(1)
                .ok_or("缺少安装目录".into())
                .and_then(|path| register(Path::new(path)));
            if result.is_err() {
                std::process::exit(1);
            }
            return;
        }
        if args.first().is_some_and(|s| s == "--unregister") {
            unregister();
            return;
        }
        if let Err(error) = bridge(args.first().map(String::as_str).unwrap_or("")) {
            report(&error);
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = args;
        report("此版本仅支持 Windows");
    }
}
