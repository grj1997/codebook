use crate::{
    context::{FocusAnchor, WindowTarget},
    model::{normalize_path, Credential, FillStep},
};
use serde::Serialize;
use std::{mem::size_of, path::Path, thread, time::Duration};
use windows::{
    core::{BSTR, PWSTR},
    Win32::{
        Foundation::*,
        System::{Com::*, Threading::*, Variant::VARIANT},
        UI::{Accessibility::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
};

fn win_error(_: windows::core::Error) -> String {
    "Windows 无法访问此窗口或输入控件".into()
}
pub fn hwnd(value: u64) -> HWND {
    HWND(value as usize as *mut _)
}
struct ProcessHandle(HANDLE);
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct Apartment(bool);
impl Apartment {
    fn new() -> Self {
        Self(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() })
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
}
fn automation() -> Result<IUIAutomation, String> {
    unsafe {
        let client: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(win_error)?;
        use windows::core::Interface;
        if let Ok(client2) = client.cast::<IUIAutomation2>() {
            let _ = client2.SetConnectionTimeout(500);
            let _ = client2.SetTransactionTimeout(500);
        }
        Ok(client)
    }
}
pub fn process_identity(pid: u32) -> Result<(String, u64), String> {
    unsafe {
        let process = ProcessHandle(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(win_error)?,
        );
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
        .map_err(win_error)?;
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user)
            .map_err(win_error)?;
        Ok((
            String::from_utf16_lossy(&buffer[..length as usize]),
            ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64,
        ))
    }
}
fn window_text(window: HWND) -> String {
    unsafe {
        let count = GetWindowTextLengthW(window).clamp(0, 4096);
        let mut text = vec![0; count as usize + 1];
        let n = GetWindowTextW(window, &mut text);
        String::from_utf16_lossy(&text[..n.max(0) as usize])
    }
}
fn gui(window: HWND) -> Result<GUITHREADINFO, String> {
    unsafe {
        let mut info = GUITHREADINFO {
            cbSize: size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(GetWindowThreadProcessId(window, None), &mut info).map_err(win_error)?;
        Ok(info)
    }
}
unsafe fn anchor(element: &IUIAutomationElement, pid: u32) -> Option<FocusAnchor> {
    if element.CurrentProcessId().ok()? as u32 != pid
        || element.CurrentControlType().ok()? != UIA_EditControlTypeId
        || !element.CurrentIsEnabled().ok()?.as_bool()
        || element.CurrentIsOffscreen().ok()?.as_bool()
    {
        return None;
    }
    let rect = element.CurrentBoundingRectangle().ok()?;
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return None;
    }
    if let Ok(value) = element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
    {
        if value.CurrentIsReadOnly().ok()?.as_bool() {
            return None;
        }
    }
    Some(FocusAnchor {
        bounds: [rect.left, rect.top, rect.right, rect.bottom],
        automation_id: element.CurrentAutomationId().ok()?.to_string(),
        class_name: element.CurrentClassName().ok()?.to_string(),
        is_password: element.CurrentIsPassword().ok()?.as_bool(),
    })
}
pub fn capture_foreground() -> Result<WindowTarget, String> {
    let _com = Apartment::new();
    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() {
            return Err("没有可用的前台窗口".into());
        }
        let mut pid = 0;
        GetWindowThreadProcessId(window, Some(&mut pid));
        let (path, started) = process_identity(pid)?;
        let focus = automation()
            .ok()
            .and_then(|a| a.GetFocusedElement().ok())
            .and_then(|e| anchor(&e, pid));
        let focus_hwnd = gui(window)
            .map(|g| g.hwndFocus.0 as usize as u64)
            .unwrap_or(0);
        Ok(WindowTarget {
            hwnd: window.0 as usize as u64,
            pid,
            path,
            started,
            title: window_text(window),
            focus_hwnd,
            focus,
        })
    }
}
pub fn same_window(target: &WindowTarget) -> Result<(), String> {
    unsafe {
        if !IsWindow(Some(hwnd(target.hwnd))).as_bool() {
            return Err("原窗口已关闭，请重新调用".into());
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd(target.hwnd), Some(&mut pid));
        let (path, started) = process_identity(pid)?;
        if pid != target.pid
            || started != target.started
            || normalize_path(&path) != normalize_path(&target.path)
            || window_text(hwnd(target.hwnd)) != target.title
        {
            return Err("目标程序已经改变，请重新调用".into());
        }
        Ok(())
    }
}
pub fn foreground_is_target(target: &WindowTarget) -> bool {
    unsafe {
        GetAncestor(GetForegroundWindow(), GA_ROOT) == GetAncestor(hwnd(target.hwnd), GA_ROOT)
    }
}
pub fn no_menu(target: &WindowTarget) -> Result<(), String> {
    let info = gui(hwnd(target.hwnd))?;
    if info.flags.0 & (GUI_INMENUMODE.0 | GUI_POPUPMENUMODE.0 | GUI_SYSTEMMENUMODE.0) != 0 {
        return Err("原菜单仍在打开，请关闭菜单后重新调用".into());
    }
    Ok(())
}
pub fn restore_target(target: &WindowTarget) -> Result<(), String> {
    same_window(target)?;
    unsafe {
        if IsIconic(hwnd(target.hwnd)).as_bool() {
            let _ = ShowWindow(hwnd(target.hwnd), SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd(target.hwnd));
    }
    for _ in 0..10 {
        if foreground_is_target(target) {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    if !foreground_is_target(target) {
        return Err("无法恢复目标窗口，请回到登录框重新调用".into());
    }
    no_menu(target)?;
    let _com = Apartment::new();
    if let Some(expected) = &target.focus {
        unsafe {
            let client = automation()?;
            let element = client
                .ElementFromPoint(POINT {
                    x: (expected.bounds[0] + expected.bounds[2]) / 2,
                    y: (expected.bounds[1] + expected.bounds[3]) / 2,
                })
                .map_err(win_error)?;
            if anchor(&element, target.pid).as_ref() != Some(expected) {
                return Err("原输入框已经变化，请重新调用".into());
            }
            element.SetFocus().map_err(win_error)?;
            let focused = client.GetFocusedElement().map_err(win_error)?;
            if !client
                .CompareElements(&element, &focused)
                .map_err(win_error)?
                .as_bool()
            {
                return Err("无法恢复原输入框焦点".into());
            }
        }
    } else if target.focus_hwnd != 0
        && gui(hwnd(target.hwnd))?.hwndFocus.0 as usize as u64 != target.focus_hwnd
    {
        return Err("输入焦点已改变，请先点击起始输入框再调用".into());
    }
    Ok(())
}
fn check_input(target: &WindowTarget) -> Result<(), String> {
    same_window(target)?;
    if !foreground_is_target(target) {
        return Err("前台窗口发生变化，已停止填充".into());
    }
    no_menu(target)
}
fn key_input(key: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                ..Default::default()
            },
        },
    }
}
fn send_inputs(inputs: &[INPUT]) -> Result<(), String> {
    if unsafe { SendInput(inputs, size_of::<INPUT>() as i32) } != inputs.len() as u32 {
        return Err("Windows 拒绝输入，目标可能以更高权限运行".into());
    }
    Ok(())
}
fn guarded_units(
    text: &str,
    mut validate: impl FnMut() -> Result<(), String>,
    mut send: impl FnMut(u16) -> Result<(), String>,
) -> Result<(), String> {
    for unit in text.encode_utf16() {
        validate()?;
        send(unit)?;
    }
    validate()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typing_stops_when_focus_changes_inside_same_window() {
        use std::cell::{Cell, RefCell};
        let focus = Cell::new(1);
        let sent = RefCell::new(Vec::new());
        let result = guarded_units(
            "secret",
            || {
                if focus.get() == 1 {
                    Ok(())
                } else {
                    Err("focus changed".into())
                }
            },
            |unit| {
                sent.borrow_mut().push(unit);
                focus.set(2);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(*sent.borrow(), vec![b's' as u16]);
    }
}
enum TextFocus {
    Automation {
        client: IUIAutomation,
        element: IUIAutomationElement,
        expected: FocusAnchor,
    },
    Native {
        control: HWND,
        style: isize,
    },
}
impl TextFocus {
    fn capture(target: &WindowTarget) -> Result<Self, String> {
        check_input(target)?;
        unsafe {
            if let Ok(client) = automation() {
                if let Ok(element) = client.GetFocusedElement() {
                    if let Some(expected) = anchor(&element, target.pid) {
                        return Ok(Self::Automation {
                            client,
                            element,
                            expected,
                        });
                    }
                }
            }
            // A standard Win32 Edit is a verifiable fallback when UIA is unavailable.
            // Self-drawn controls without an accessible identity are rejected.
            let control = gui(hwnd(target.hwnd))?.hwndFocus;
            let mut class = [0u16; 256];
            let count = GetClassNameW(control, &mut class);
            let style = GetWindowLongPtrW(control, GWL_STYLE);
            if control.0.is_null()
                || GetAncestor(control, GA_ROOT) != GetAncestor(hwnd(target.hwnd), GA_ROOT)
                || !String::from_utf16_lossy(&class[..count.max(0) as usize])
                    .eq_ignore_ascii_case("Edit")
                || !IsWindowEnabled(control).as_bool()
                || !IsWindowVisible(control).as_bool()
                || style & ES_READONLY as isize != 0
            {
                return Err("无法确认可编辑的输入框，已停止填充".into());
            }
            Ok(Self::Native { control, style })
        }
    }
    fn is_password(&self) -> bool {
        match self {
            Self::Automation { expected, .. } => expected.is_password,
            Self::Native { style, .. } => style & ES_PASSWORD as isize != 0,
        }
    }
    fn verify(&self, target: &WindowTarget) -> Result<(), String> {
        check_input(target)?;
        let same = unsafe {
            match self {
                Self::Automation {
                    client,
                    element,
                    expected,
                } => {
                    let focused = client.GetFocusedElement().map_err(win_error)?;
                    client
                        .CompareElements(element, &focused)
                        .map_err(win_error)?
                        .as_bool()
                        && anchor(&focused, target.pid).as_ref() == Some(expected)
                }
                Self::Native { control, style } => {
                    gui(hwnd(target.hwnd))?.hwndFocus == *control
                        && IsWindow(Some(*control)).as_bool()
                        && IsWindowEnabled(*control).as_bool()
                        && IsWindowVisible(*control).as_bool()
                        && GetWindowLongPtrW(*control, GWL_STYLE) == *style
                }
            }
        };
        if same {
            Ok(())
        } else {
            Err("输入框焦点或属性发生变化，已停止填充".into())
        }
    }
}
fn type_text(
    target: &WindowTarget,
    focus: &TextFocus,
    text: &str,
    password: bool,
) -> Result<(), String> {
    if password && !focus.is_password() {
        return Err("无法确认当前为密码框，已停止输入密码；请检查输入顺序".into());
    }
    guarded_units(
        text,
        || focus.verify(target),
        |unit| {
            let down = KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: KEYEVENTF_UNICODE,
                ..Default::default()
            };
            let up = KEYBDINPUT {
                dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                ..down
            };
            send_inputs(&[
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 { ki: down },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 { ki: up },
                },
            ])
        },
    )
}
fn set_value(
    client: &IUIAutomation,
    element: &IUIAutomationElement,
    target: &WindowTarget,
    value: &str,
) -> Result<(), String> {
    check_input(target)?;
    unsafe {
        if anchor(element, target.pid).is_none() {
            return Err("目标输入框已失效".into());
        }
        element.SetFocus().map_err(win_error)?;
        let focused = client.GetFocusedElement().map_err(win_error)?;
        if !client
            .CompareElements(element, &focused)
            .map_err(win_error)?
            .as_bool()
        {
            return Err("输入框焦点恢复失败".into());
        }
        check_input(target)?;
        let pattern: IUIAutomationValuePattern = element
            .GetCurrentPatternAs(UIA_ValuePatternId)
            .map_err(|_| "此输入框不支持直接填充，请选择输入顺序".to_string())?;
        pattern.SetValue(&BSTR::from(value)).map_err(win_error)
    }
}
pub fn fill(target: &WindowTarget, entry: &Credential, mode: &str) -> Result<(), String> {
    for _ in 0..40 {
        let held = unsafe {
            [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
                .iter()
                .any(|key| GetAsyncKeyState(key.0 as i32) < 0)
        };
        if !held {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    if unsafe {
        [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
            .iter()
            .any(|key| GetAsyncKeyState(key.0 as i32) < 0)
    } {
        return Err("请松开修饰键后重新调用".into());
    }
    restore_target(target)?;
    let _com = Apartment::new();
    if mode == "sequence" {
        let mut focus = TextFocus::capture(target)?;
        for step in &entry.profile.steps {
            focus.verify(target)?;
            match step {
                FillStep::Username => type_text(target, &focus, &entry.username, false)?,
                FillStep::Password => type_text(target, &focus, &entry.password, true)?,
                FillStep::Tab => send_inputs(&[key_input(VK_TAB, false), key_input(VK_TAB, true)])?,
                FillStep::ShiftTab => send_inputs(&[
                    key_input(VK_SHIFT, false),
                    key_input(VK_TAB, false),
                    key_input(VK_TAB, true),
                    key_input(VK_SHIFT, true),
                ])?,
                FillStep::Delay(ms) => {
                    for _ in 0..(*ms / 25) {
                        thread::sleep(Duration::from_millis(25));
                        focus.verify(target)?;
                    }
                    thread::sleep(Duration::from_millis(*ms % 25));
                }
            }
            if matches!(step, FillStep::Tab | FillStep::ShiftTab) {
                thread::sleep(Duration::from_millis(40));
                focus = TextFocus::capture(target)?;
            }
        }
        return Ok(());
    }
    let client = automation()?;
    unsafe {
        let focused = client.GetFocusedElement().map_err(win_error)?;
        if mode == "username" || mode == "password" {
            return set_value(
                &client,
                &focused,
                target,
                if mode == "username" {
                    &entry.username
                } else {
                    &entry.password
                },
            );
        }
        if mode != "auto" {
            return Err("不支持的填充方式".into());
        }
        if target.focus.is_none() {
            return Err("未识别到原输入框，请先聚焦输入框，或选择输入顺序".into());
        }
        let walker = client.ControlViewWalker().map_err(win_error)?;
        let mut scope = focused.clone();
        let condition = client
            .CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_EditControlTypeId.0),
            )
            .map_err(win_error)?;
        for _ in 0..6 {
            scope = match walker.GetParentElement(&scope) {
                Ok(parent) => parent,
                Err(_) => break,
            };
            if scope.CurrentProcessId().map_err(win_error)? as u32 != target.pid {
                break;
            }
            let candidates = scope
                .FindAll(TreeScope_Subtree, &condition)
                .map_err(win_error)?;
            let length = candidates.Length().map_err(win_error)?;
            if length > 64 {
                break;
            }
            let mut usernames = Vec::new();
            let mut passwords = Vec::new();
            for i in 0..length {
                let field = candidates.GetElement(i).map_err(win_error)?;
                if let Some(a) = anchor(&field, target.pid) {
                    if a.is_password {
                        passwords.push(field);
                    } else {
                        usernames.push(field);
                    }
                }
            }
            if usernames.len() == 1 && passwords.len() == 1 {
                let username = &usernames[0];
                let password = &passwords[0];
                let _: IUIAutomationValuePattern = username
                    .GetCurrentPatternAs(UIA_ValuePatternId)
                    .map_err(win_error)?;
                let _: IUIAutomationValuePattern = password
                    .GetCurrentPatternAs(UIA_ValuePatternId)
                    .map_err(win_error)?;
                set_value(&client, username, target, &entry.username)?;
                return set_value(&client, password, target, &entry.password);
            }
        }
    }
    Err("登录字段不明确，请选择仅填当前框，或使用已配置的输入顺序".into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub path: String,
    pub title: String,
}
unsafe extern "system" fn enumerate(window: HWND, parameter: LPARAM) -> windows::core::BOOL {
    if !IsWindowVisible(window).as_bool() || GetWindowTextLengthW(window) == 0 {
        return true.into();
    }
    let mut pid = 0;
    GetWindowThreadProcessId(window, Some(&mut pid));
    if pid == std::process::id() {
        return true.into();
    }
    if let Ok((path, _)) = process_identity(pid) {
        let list = &mut *(parameter.0 as *mut Vec<AppInfo>);
        if !list
            .iter()
            .any(|a| normalize_path(&a.path) == normalize_path(&path))
        {
            let name = Path::new(&path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            list.push(AppInfo {
                name,
                path,
                title: window_text(window),
            });
        }
    }
    true.into()
}
pub fn list_apps() -> Result<Vec<AppInfo>, String> {
    let mut apps: Vec<AppInfo> = Vec::new();
    unsafe {
        EnumWindows(Some(enumerate), LPARAM(&mut apps as *mut _ as isize)).map_err(win_error)?;
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    Ok(apps)
}
pub fn describe_app(path: &str) -> Result<AppInfo, String> {
    let path = std::fs::canonicalize(path).map_err(|_| "无法找到应用文件")?;
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        return Err("请选择 EXE 应用文件".into());
    }
    let text = path
        .to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .to_string();
    Ok(AppInfo {
        name: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        path: text,
        title: String::new(),
    })
}
pub fn browser_name(path: &str) -> Option<&'static str> {
    let base = path.rsplit(['\\', '/']).next()?.to_ascii_lowercase();
    match base.as_str() {
        "chrome.exe" => Some("Google Chrome"),
        "msedge.exe" => Some("Microsoft Edge"),
        _ => None,
    }
}
