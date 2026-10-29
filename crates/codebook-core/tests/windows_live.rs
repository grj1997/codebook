#![cfg(all(windows, feature = "native"))]
use codebook_core::{
    model::{Credential, FillProfile},
    windows::desktop,
};
use std::{sync::mpsc, thread, time::Duration};
use windows::{
    core::w,
    Win32::{
        Foundation::*,
        System::{
            LibraryLoader::GetModuleHandleW,
            Threading::{AttachThreadInput, GetCurrentThreadId},
        },
        UI::{Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
    },
};

unsafe extern "system" fn procedure(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if message == WM_CLOSE {
        let _ = DestroyWindow(window);
        return LRESULT(0);
    }
    if message == WM_DESTROY {
        PostQuitMessage(0);
        return LRESULT(0);
    }
    DefWindowProcW(window, message, w, l)
}
struct Fixture {
    window: u64,
    username: u64,
    password: u64,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(
                Some(desktop::hwnd(self.window)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}
fn fixture() -> Fixture {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || unsafe {
        let instance = HINSTANCE(GetModuleHandleW(None).unwrap().0);
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: w!("CodebookDisposableLogin"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let window = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("CodebookDisposableLogin"),
            w!("Codebook 测试登录框 · 仅使用虚构数据"),
            WS_OVERLAPPEDWINDOW,
            80,
            80,
            470,
            220,
            None,
            None,
            Some(instance),
            None,
        )
        .unwrap();
        let username = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            25,
            30,
            390,
            30,
            Some(window),
            Some(HMENU(1001usize as *mut _)),
            None,
            None,
        )
        .unwrap();
        let password = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            w!(""),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_BORDER
                | WINDOW_STYLE((ES_AUTOHSCROLL | ES_PASSWORD) as u32),
            25,
            85,
            390,
            30,
            Some(window),
            Some(HMENU(1002usize as *mut _)),
            None,
            None,
        )
        .unwrap();
        let current_thread = GetCurrentThreadId();
        let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        // Test-only fixture focus: temporarily share the interactive queue, never
        // used by the product's restoration logic, which must fail closed.
        let attached = AttachThreadInput(current_thread, foreground_thread, true).as_bool();
        let _ = ShowWindow(window, SW_SHOW);
        let _ = SetForegroundWindow(window);
        let _ = SetFocus(Some(username));
        if attached {
            let _ = AttachThreadInput(current_thread, foreground_thread, false);
        }
        sender
            .send((
                window.0 as usize as u64,
                username.0 as usize as u64,
                password.0 as usize as u64,
            ))
            .unwrap();
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
    let (window, username, password) = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    thread::sleep(Duration::from_millis(300));
    Fixture {
        window,
        username,
        password,
    }
}
fn text(window: u64) -> String {
    let mut buffer = [0u16; 4096];
    let n = unsafe { GetWindowTextW(desktop::hwnd(window), &mut buffer) };
    String::from_utf16_lossy(&buffer[..n.max(0) as usize])
}

#[test]
fn changed_title_invalidates_original_target_without_needing_foreground() {
    let fixture = fixture();
    let (path, started) = desktop::process_identity(std::process::id()).unwrap();
    let target = codebook_core::context::WindowTarget {
        hwnd: fixture.window,
        pid: std::process::id(),
        path,
        started,
        title: text(fixture.window),
        focus_hwnd: fixture.username,
        focus: None,
    };
    assert!(desktop::same_window(&target).is_ok());
    unsafe {
        SetWindowTextW(desktop::hwnd(fixture.window), w!("Different workspace")).unwrap();
    }
    assert!(desktop::same_window(&target).is_err());
}

#[test]
#[ignore = "opens a disposable native test window; run explicitly on an interactive Windows desktop"]
fn uia_fill_uses_real_windows_fields_and_refuses_closed_target() {
    let fixture = fixture();
    let target = desktop::capture_foreground().unwrap();
    assert_eq!(
        target.hwnd, fixture.window,
        "Test window could not obtain foreground focus"
    );
    assert!(
        target.focus.is_some(),
        "Windows did not expose the standard edit control"
    );
    let entry = Credential {
        id: "fixture".into(),
        title: "虚构账户".into(),
        username: "user@example.test".into(),
        password: "literal{TAB}密码!".into(),
        notes: "".into(),
        bindings: vec![],
        profile: FillProfile::default(),
    };
    desktop::fill(&target, &entry, "auto").unwrap();
    assert_eq!(text(fixture.username), "user@example.test");
    assert_eq!(text(fixture.password), "literal{TAB}密码!");
    unsafe {
        let _ = PostMessageW(
            Some(desktop::hwnd(fixture.window)),
            WM_CLOSE,
            WPARAM(0),
            LPARAM(0),
        );
    }
    thread::sleep(Duration::from_millis(150));
    assert!(desktop::fill(&target, &entry, "auto").is_err());
}
