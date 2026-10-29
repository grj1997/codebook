use codebook_core::{context::*, gesture::*, protocol::*};
use std::io::Cursor;

fn window() -> WindowTarget {
    WindowTarget {
        hwnd: 42,
        pid: 7,
        started: 10,
        path: "C:\\App\\client.exe".into(),
        title: "登录".into(),
        focus_hwnd: 0,
        focus: None,
    }
}
#[test]
fn fill_tokens_expire_are_single_use_and_bind_account_and_target() {
    let mut tickets = TicketRegistry::default();
    let target = TargetContext::Desktop { window: window() };
    let ticket = tickets.issue(target.clone(), vec!["account-1".into()], 100);
    assert!(tickets.consume(&ticket.token, "other", 101).is_err());
    let ticket = tickets.issue(target.clone(), vec!["account-1".into()], 100);
    assert!(tickets.consume(&ticket.token, "account-1", 30_101).is_err());
    let ticket = tickets.issue(target, vec!["account-1".into()], 100);
    let consumed = tickets.consume(&ticket.token, "account-1", 101).unwrap();
    assert_eq!(consumed.target.window().hwnd, 42);
    assert!(tickets.consume(&ticket.token, "account-1", 102).is_err());
}

fn click(at: u64, target: u64, editable: bool) -> RightClick {
    RightClick {
        at_ms: at,
        x: 20,
        y: 30,
        target_id: target,
        editable,
        injected: false,
    }
}
#[test]
fn double_right_click_keeps_original_input_when_menu_takes_focus() {
    let mut detector = DoubleRightClick::default();
    assert!(detector.click(click(0, 42, true), 500, 4, 4).is_none());
    let original = detector.click(click(120, 42, false), 500, 4, 4).unwrap();
    assert!(original.editable);
    assert_eq!(original.at_ms, 0);
    assert!(detector.click(click(180, 42, false), 500, 4, 4).is_none());
}
#[test]
fn gestures_reject_timeout_movement_target_change_and_injected_input() {
    for second in [
        click(501, 42, true),
        click(100, 43, true),
        RightClick {
            x: 90,
            ..click(100, 42, true)
        },
    ] {
        let mut detector = DoubleRightClick::default();
        assert!(detector.click(click(0, 42, true), 500, 4, 4).is_none());
        assert!(detector.click(second, 500, 4, 4).is_none());
    }
    let mut detector = DoubleRightClick::default();
    detector.click(
        RightClick {
            injected: true,
            ..click(0, 42, true)
        },
        500,
        4,
        4,
    );
    assert!(detector.click(click(100, 42, true), 500, 4, 4).is_none());
}

#[test]
fn native_frames_are_bounded_and_reject_partial_payloads() {
    let payload = br#"{"type":"context"}"#;
    let mut output = Vec::new();
    write_frame(&mut output, payload).unwrap();
    assert_eq!(
        read_frame(&mut Cursor::new(output)).unwrap().unwrap(),
        payload
    );
    assert!(read_frame(&mut Cursor::new((2_000_000u32).to_le_bytes())).is_err());
    assert!(read_frame(&mut Cursor::new(vec![5, 0, 0, 0, 1])).is_err());
    assert!(read_frame(&mut Cursor::new(vec![1, 0])).is_err());
    assert!(read_frame(&mut Cursor::new(Vec::<u8>::new()))
        .unwrap()
        .is_none());
}
