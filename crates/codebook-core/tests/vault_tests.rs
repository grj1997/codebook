use codebook_core::{model::*, vault::VaultStore};

fn entry() -> Credential {
    Credential {
        id: String::new(),
        title: "测试账户".into(),
        username: "user@example.com".into(),
        password: "秘密{Tab}🔑!".into(),
        notes: "私密备注".into(),
        bindings: vec![TargetBinding::Website {
            origin: "https://example.com".into(),
        }],
        profile: FillProfile::default(),
    }
}

#[test]
fn vault_encrypts_entire_record_and_survives_reopen() {
    let dir = tempfile::tempdir().unwrap().keep();
    let path = dir.join("vault.cbvault");
    let mut store = VaultStore::new(path.clone());
    store.create("correct horse battery staple").unwrap();
    let saved = store.upsert(entry()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    for secret in ["user@example.com", "测试账户", "私密备注", "example.com"] {
        assert!(!bytes.windows(secret.len()).any(|w| w == secret.as_bytes()));
    }
    store.lock();
    assert!(store.get(&saved.id).is_err());
    assert!(store.unlock("wrong password").is_err());
    store.unlock("correct horse battery staple").unwrap();
    assert_eq!(store.get(&saved.id).unwrap().password, "秘密{Tab}🔑!");
}

#[test]
fn tampering_and_excessive_kdf_are_rejected_without_overwriting() {
    let dir = tempfile::tempdir().unwrap().keep();
    let path = dir.join("v");
    let mut store = VaultStore::new(path.clone());
    store.create("correct horse battery staple").unwrap();
    store.lock();
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(&path, &bytes).unwrap();
    assert!(store.unlock("correct horse battery staple").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    bytes[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(&path, &bytes).unwrap();
    assert!(store.unlock("correct horse battery staple").is_err());
}

#[test]
fn password_change_and_restore_keep_old_backup_usable() {
    let dir = tempfile::tempdir().unwrap().keep();
    let mut store = VaultStore::new(dir.join("v"));
    store.create("correct horse battery staple").unwrap();
    let saved = store.upsert(entry()).unwrap();
    let backup = dir.join("backup");
    store.backup(&backup).unwrap();
    store
        .change_password(
            "correct horse battery staple",
            "new correct master password",
        )
        .unwrap();
    store.lock();
    assert!(store.unlock("correct horse battery staple").is_err());
    store.unlock("new correct master password").unwrap();
    assert!(store.restore(&backup, "wrong password").is_err());
    assert!(store.get(&saved.id).is_ok());
    store
        .restore(&backup, "correct horse battery staple")
        .unwrap();
    assert_eq!(store.get(&saved.id).unwrap().username, "user@example.com");
}

#[test]
fn failed_save_does_not_change_memory_or_existing_vault() {
    let dir = tempfile::tempdir().unwrap().keep();
    let path = dir.join("v");
    let mut store = VaultStore::new(path.clone());
    store.create("correct horse battery staple").unwrap();
    let saved = store.upsert(entry()).unwrap();
    let before = std::fs::read(&path).unwrap();
    std::fs::create_dir(path.with_extension("pending")).unwrap();
    let mut modified = saved.clone();
    modified.password = "changed".into();
    assert!(store.upsert(modified).is_err());
    assert_eq!(store.get(&saved.id).unwrap().password, "秘密{Tab}🔑!");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn export_refuses_changed_disk_and_preserves_existing_backup() {
    let dir = tempfile::tempdir().unwrap().keep();
    let path = dir.join("vault.cbvault");
    let backup = dir.join("safe.cbvault");
    let mut store = VaultStore::new(path.clone());
    store.create("correct horse battery staple").unwrap();
    store.backup(&backup).unwrap();
    let original = std::fs::read(&backup).unwrap();
    std::fs::write(&path, b"externally corrupted").unwrap();
    assert!(store.backup(&backup).is_err());
    assert_eq!(std::fs::read(&backup).unwrap(), original);
}

#[test]
fn failed_restore_preserves_selected_automatic_backup() {
    let dir = tempfile::tempdir().unwrap().keep();
    let path = dir.join("vault.cbvault");
    let backup = dir.join("vault.pre-restore.cbvault");
    let mut store = VaultStore::new(path.clone());
    store.create("correct horse battery staple").unwrap();
    std::fs::copy(&path, &backup).unwrap();
    let original = std::fs::read(&backup).unwrap();
    store.upsert(entry()).unwrap();
    assert_eq!(
        VaultStore::verify_backup(&backup, "correct horse battery staple").unwrap(),
        0
    );
    assert!(VaultStore::verify_backup(&backup, "wrong password").is_err());
    let active = std::fs::read(&path).unwrap();
    std::fs::create_dir(path.with_extension("pending")).unwrap();
    assert!(store
        .restore(&backup, "correct horse battery staple")
        .is_err());
    assert_eq!(std::fs::read(&backup).unwrap(), original);
    assert_eq!(std::fs::read(&path).unwrap(), active);
    assert_eq!(store.summaries().len(), 1);
}
