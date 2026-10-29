use crate::{runtime, state::*};
use codebook_core::{
    context::TargetContext,
    gesture::{DoubleRightClick, RightClick},
    windows::desktop,
};
use std::{
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::AppHandle;
use windows::Win32::{
    Foundation::*,
    UI::{Input::KeyboardAndMouse::GetDoubleClickTime, WindowsAndMessaging::*},
};

#[derive(Clone)]
struct CachedTarget {
    target: TargetContext,
    captured: Instant,
    interval: u64,
}
struct Trigger {
    target: TargetContext,
    x: i32,
    y: i32,
}
#[derive(Default)]
struct Sequence {
    detector: DoubleRightClick,
    first: Option<CachedTarget>,
    pending: Option<Trigger>,
}
struct Shared {
    cache: Arc<Mutex<Option<CachedTarget>>>,
    sequence: Mutex<Sequence>,
    sender: SyncSender<Trigger>,
    started: Instant,
}
static HOOK: OnceLock<Shared> = OnceLock::new();

unsafe extern "system" fn callback(code: i32, message: WPARAM, parameter: LPARAM) -> LRESULT {
    if code >= 0 {
        if let Some(shared) = HOOK.get() {
            if let Ok(mut sequence) = shared.sequence.try_lock() {
                if message.0 as u32 == WM_RBUTTONUP {
                    if let Some(trigger) = sequence.pending.take() {
                        let _ = shared.sender.try_send(trigger);
                    }
                } else if message.0 as u32 == WM_RBUTTONDOWN {
                    let event = &*(parameter.0 as *const MSLLHOOKSTRUCT);
                    if event.flags & LLMHF_INJECTED == 0 {
                        let cached = shared
                            .cache
                            .try_lock()
                            .ok()
                            .and_then(|cache| cache.clone())
                            .filter(|cache| cache.captured.elapsed() < Duration::from_millis(600));
                        let candidate = sequence.first.as_ref().or(cached.as_ref());
                        let actual = GetAncestor(GetForegroundWindow(), GA_ROOTOWNER);
                        let target_id = candidate
                            .filter(|c| {
                                actual
                                    == GetAncestor(
                                        desktop::hwnd(c.target.window().hwnd),
                                        GA_ROOTOWNER,
                                    )
                            })
                            .map(|c| c.target.window().hwnd)
                            .unwrap_or(actual.0 as usize as u64);
                        let editable = cached.as_ref().is_some_and(|cache| {
                            cache.target.window().hwnd == target_id
                                && cache.target.window().focus.as_ref().is_some_and(|focus| {
                                    event.pt.x >= focus.bounds[0]
                                        && event.pt.x < focus.bounds[2]
                                        && event.pt.y >= focus.bounds[1]
                                        && event.pt.y < focus.bounds[3]
                                })
                        });
                        let interval = candidate
                            .map(|c| c.interval)
                            .unwrap_or(GetDoubleClickTime() as u64);
                        let click = RightClick {
                            at_ms: shared.started.elapsed().as_millis() as u64,
                            x: event.pt.x,
                            y: event.pt.y,
                            target_id,
                            editable,
                            injected: false,
                        };
                        if sequence
                            .detector
                            .click(
                                click,
                                interval,
                                GetSystemMetrics(SM_CXDOUBLECLK),
                                GetSystemMetrics(SM_CYDOUBLECLK),
                            )
                            .is_some()
                        {
                            if let Some(first) = sequence.first.take() {
                                sequence.pending = Some(Trigger {
                                    target: first.target,
                                    x: event.pt.x,
                                    y: event.pt.y,
                                });
                            }
                        } else if editable {
                            sequence.first = cached;
                        }
                    }
                }
            }
        }
    }
    // Always preserve the original application's right click events.
    CallNextHookEx(None, code, message, parameter)
}
pub fn start(app: AppHandle, state: Arc<AppState>) {
    let cache = Arc::new(Mutex::new(None::<CachedTarget>));
    let (sender, receiver) = mpsc::sync_channel::<Trigger>(8);
    if HOOK
        .set(Shared {
            cache: cache.clone(),
            sequence: Mutex::new(Sequence::default()),
            sender,
            started: Instant::now(),
        })
        .is_err()
    {
        return;
    }
    let app_worker = app.clone();
    let state_worker = state.clone();
    thread::spawn(move || {
        for trigger in receiver {
            let _ = runtime::trigger_context(
                &app_worker,
                &state_worker,
                trigger.target,
                Some((trigger.x, trigger.y)),
            );
        }
    });
    thread::spawn(move || {
        match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(callback), None, 0) } {
            Ok(hook) => unsafe {
                let mut message = MSG::default();
                while GetMessageW(&mut message, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                let _ = UnhookWindowsHookEx(hook);
            },
            Err(_) => runtime::notify(&app, "鼠标手势未能启用，仍可使用键盘快捷键"),
        }
    });
    thread::spawn(move || loop {
        let result = (|| {
            if !lock(&state.vault)?.is_unlocked() {
                return Ok(None);
            }
            let settings = lock(&state.settings)?.clone();
            if !settings.mouse_enabled {
                return Ok(None);
            }
            let window = desktop::capture_foreground()?;
            if window.pid == std::process::id() || window.focus.is_none() {
                return Ok(None);
            }
            let context = runtime::build_context(&state, window, None)?;
            if runtime::matching(&state, &context)?.is_empty() {
                return Ok(None);
            }
            if let TargetContext::Browser { target, .. } = &context {
                if !target.editable {
                    return Ok(None);
                }
            }
            Ok::<_, String>(Some(CachedTarget {
                target: context,
                captured: Instant::now(),
                interval: settings
                    .double_click_ms
                    .map(u64::from)
                    .unwrap_or_else(|| unsafe { GetDoubleClickTime() as u64 }),
            }))
        })();
        if let Ok(mut cache) = cache.lock() {
            *cache = result.unwrap_or(None);
        }
        thread::sleep(Duration::from_millis(150));
    });
}
