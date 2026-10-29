use codebook_core::model::*;

#[test]
fn website_origin_is_canonical_and_never_expands_to_subdomains() {
    assert_eq!(
        normalize_origin("https://EXAMPLE.com:443/login?q=x").unwrap(),
        "https://example.com"
    );
    assert!(normalize_origin("http://example.com").is_err());
    assert!(normalize_origin("https://user:pass@example.com").is_err());
    let b = TargetBinding::Website {
        origin: "https://example.com".into(),
    };
    assert!(matches_binding(&b, "", "", Some("https://example.com")));
    assert!(!matches_binding(
        &b,
        "",
        "",
        Some("https://evil.example.com")
    ));
    assert!(!matches_binding(&b, "chrome.exe", "example.com", None));
}

#[test]
fn desktop_matches_path_not_display_name() {
    let b = TargetBinding::Desktop {
        path: "C:\\Apps\\client.exe".into(),
        title_contains: "登录".into(),
    };
    assert!(matches_binding(&b, "c:/apps/CLIENT.exe", "账号登录", None));
    assert!(!matches_binding(
        &b,
        "C:\\Other\\client.exe",
        "账号登录",
        None
    ));
    assert!(!matches_binding(
        &b,
        "C:\\Apps\\client.exe",
        "聊天窗口",
        None
    ));
}

#[test]
fn rejects_oversized_or_command_like_autotype_profiles() {
    let mut e = Credential {
        id: "".into(),
        title: "a".into(),
        username: "u".into(),
        password: "literal{ENTER}".into(),
        notes: "".into(),
        bindings: vec![],
        profile: FillProfile::default(),
    };
    assert!(validate_credential(&mut e).is_ok());
    e.profile.steps = vec![FillStep::Delay(60000)];
    assert!(validate_credential(&mut e).is_err());
    assert!(serde_json::from_str::<FillStep>(r#"{"kind":"execute","value":"cmd.exe"}"#).is_err());
}
