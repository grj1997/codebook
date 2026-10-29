use crate::model::*;
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"CBVAULT1";
const HEADER: usize = 52;
const MAX_FILE: usize = 16 * 1024 * 1024;
const AUTH_ERROR: &str = "主密码不正确，或密码库文件已损坏";
struct Unlocked {
    key: Zeroizing<[u8; 32]>,
    salt: [u8; 16],
    data: VaultData,
    disk_bytes: Vec<u8>,
}
pub struct VaultStore {
    pub path: PathBuf,
    unlocked: Option<Unlocked>,
}

fn new_password(password: &str) -> Result<(), String> {
    if password.chars().count() < 12 || password.len() > 1024 || password.contains('\0') {
        return Err("主密码至少需要 12 个字符，最多 1024 字节".into());
    }
    Ok(())
}
fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut result = [0; N];
    getrandom::getrandom(&mut result).map_err(|_| "系统随机数不可用")?;
    Ok(result)
}
fn derive(password: &str, salt: &[u8; 16]) -> Result<Zeroizing<[u8; 32]>, String> {
    if password.len() > 1024 {
        return Err(AUTH_ERROR.into());
    }
    let params = Params::new(65536, 3, 1, Some(32)).map_err(|_| "加密参数不可用")?;
    let mut key = Zeroizing::new([0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| "主密码密钥派生失败")?;
    Ok(key)
}
fn encode(key: &[u8; 32], salt: &[u8; 16], data: &VaultData) -> Result<Vec<u8>, String> {
    let mut header = Vec::with_capacity(HEADER);
    header.extend_from_slice(MAGIC);
    for value in [1u32, 65536, 3, 1] {
        header.extend_from_slice(&value.to_le_bytes());
    }
    header.extend_from_slice(salt);
    let nonce = random::<12>()?;
    header.extend_from_slice(&nonce);
    let plaintext = Zeroizing::new(serde_json::to_vec(data).map_err(|_| "账户数据编码失败")?);
    if plaintext.len() > MAX_FILE - HEADER - 16 {
        return Err("密码库超过 16 MiB 限制".into());
    }
    let ciphertext = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "加密初始化失败")?
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: &header,
            },
        )
        .map_err(|_| "密码库加密失败")?;
    header.extend_from_slice(&ciphertext);
    Ok(header)
}
fn read(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|_| "无法读取密码库文件")?;
    if file.metadata().map_err(|_| "无法读取密码库信息")?.len() > MAX_FILE as u64 {
        return Err("密码库超过 16 MiB 限制".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "密码库读取失败")?;
    if bytes.len() > MAX_FILE {
        return Err("密码库超过 16 MiB 限制".into());
    }
    Ok(bytes)
}
fn validate_data(data: &mut VaultData) -> Result<(), String> {
    if data.entries.len() > 5000 || data.paired_extensions.len() > 32 {
        return Err("密码库条目超过限制".into());
    }
    let mut ids = HashSet::new();
    for entry in &mut data.entries {
        validate_credential(entry)?;
        if uuid::Uuid::parse_str(&entry.id).is_err() || !ids.insert(entry.id.clone()) {
            return Err("账户标识无效或重复".into());
        }
    }
    for extension in &data.paired_extensions {
        if extension.len() != 32 || !extension.bytes().all(|b| (b'a'..=b'p').contains(&b)) {
            return Err("浏览器配对标识无效".into());
        }
    }
    validate_settings(&data.settings)
}
fn decode(bytes: Vec<u8>, password: &str) -> Result<Unlocked, String> {
    if bytes.len() < HEADER + 16 || &bytes[..8] != MAGIC {
        return Err("不是有效的 Codebook 密码库".into());
    }
    for (offset, expected) in [(8, 1u32), (12, 65536), (16, 3), (20, 1)] {
        if bytes[offset..offset + 4] != expected.to_le_bytes() {
            return Err("不支持的密码库版本或加密参数".into());
        }
    }
    let salt: [u8; 16] = bytes[24..40].try_into().map_err(|_| AUTH_ERROR)?;
    let key = derive(password, &salt)?;
    let plaintext = Zeroizing::new(
        Aes256Gcm::new_from_slice(key.as_ref())
            .map_err(|_| AUTH_ERROR)?
            .decrypt(
                Nonce::from_slice(&bytes[40..52]),
                Payload {
                    msg: &bytes[HEADER..],
                    aad: &bytes[..HEADER],
                },
            )
            .map_err(|_| AUTH_ERROR)?,
    );
    let mut data: VaultData = serde_json::from_slice(&plaintext).map_err(|_| AUTH_ERROR)?;
    validate_data(&mut data)?;
    Ok(Unlocked {
        key,
        salt,
        data,
        disk_bytes: bytes,
    })
}
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("密码库路径无效")?;
    fs::create_dir_all(parent).map_err(|_| "无法创建密码库目录")?;
    let pending = path.with_extension("pending");
    if fs::symlink_metadata(&pending).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("临时文件路径不安全".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&pending)
        .map_err(|_| "无法创建加密临时文件，原密码库未改变")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "密码库写入失败，原密码库未改变")?;
    drop(file);
    fs::rename(&pending, path).map_err(|_| "密码库替换失败，原密码库未改变".into())
}
impl VaultStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            unlocked: None,
        }
    }
    pub fn exists(&self) -> bool {
        self.path.exists()
    }
    pub fn is_unlocked(&self) -> bool {
        self.unlocked.is_some()
    }
    pub fn data(&self) -> Result<&VaultData, String> {
        self.unlocked
            .as_ref()
            .map(|u| &u.data)
            .ok_or("请先解锁密码库".into())
    }
    pub fn summaries(&self) -> Vec<CredentialSummary> {
        self.data()
            .map(|d| d.entries.iter().map(summarize).collect())
            .unwrap_or_default()
    }
    pub fn create(&mut self, password: &str) -> Result<(), String> {
        if self.exists() {
            return Err("密码库已经存在，请解锁或恢复备份".into());
        }
        new_password(password)?;
        let salt = random()?;
        let key = derive(password, &salt)?;
        let data = VaultData::default();
        let bytes = encode(&key, &salt, &data)?;
        if self.exists() {
            return Err("密码库已由其他实例创建".into());
        }
        write_atomic(&self.path, &bytes)?;
        self.unlocked = Some(Unlocked {
            key,
            salt,
            data,
            disk_bytes: bytes,
        });
        Ok(())
    }
    pub fn unlock(&mut self, password: &str) -> Result<(), String> {
        let unlocked = decode(read(&self.path)?, password)?;
        self.unlocked = Some(unlocked);
        Ok(())
    }
    pub fn lock(&mut self) {
        self.unlocked = None;
    }
    fn persist(&mut self, mut data: VaultData) -> Result<(), String> {
        validate_data(&mut data)?;
        let unlocked = self.unlocked.as_ref().ok_or("请先解锁密码库")?;
        if read(&self.path)? != unlocked.disk_bytes {
            return Err("密码库被其他程序修改，请重新锁定并解锁".into());
        }
        let bytes = encode(&unlocked.key, &unlocked.salt, &data)?;
        fs::copy(
            &self.path,
            self.path.with_file_name("vault.previous.cbvault"),
        )
        .map_err(|_| "无法保留上一份密码库，保存已停止")?;
        write_atomic(&self.path, &bytes)?;
        let unlocked = self.unlocked.as_mut().ok_or("请先解锁密码库")?;
        unlocked.data = data;
        unlocked.disk_bytes = bytes;
        Ok(())
    }
    pub fn upsert(&mut self, mut entry: Credential) -> Result<Credential, String> {
        validate_credential(&mut entry)?;
        let mut data = self.data()?.clone();
        if entry.id.is_empty() {
            entry.id = uuid::Uuid::new_v4().to_string();
            data.entries.push(entry.clone());
        } else {
            let index = data
                .entries
                .iter()
                .position(|e| e.id == entry.id)
                .ok_or("账户已不存在")?;
            data.entries[index] = entry.clone();
        }
        self.persist(data)?;
        Ok(entry)
    }
    pub fn get(&self, id: &str) -> Result<Credential, String> {
        self.data()?
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or("账户已不存在".into())
    }
    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let mut data = self.data()?.clone();
        let before = data.entries.len();
        data.entries.retain(|e| e.id != id);
        if before == data.entries.len() {
            return Err("账户已不存在".into());
        }
        self.persist(data)
    }
    pub fn update_settings(&mut self, settings: Settings) -> Result<(), String> {
        validate_settings(&settings)?;
        let mut data = self.data()?.clone();
        data.settings = settings;
        self.persist(data)
    }
    pub fn pair(&mut self, id: &str, allow: bool) -> Result<(), String> {
        let mut data = self.data()?.clone();
        data.paired_extensions.retain(|e| e != id);
        if allow {
            data.paired_extensions.push(id.into());
        }
        self.persist(data)
    }
    pub fn paired(&self, id: &str) -> bool {
        self.data()
            .is_ok_and(|d| d.paired_extensions.iter().any(|e| e == id))
    }
    pub fn backup(&self, path: &Path) -> Result<(), String> {
        let unlocked = self.unlocked.as_ref().ok_or("请先解锁密码库")?;
        let bytes = read(&self.path)?;
        if bytes != unlocked.disk_bytes {
            return Err("密码库被其他程序修改，备份已停止；请重新锁定并解锁".into());
        }
        if fs::canonicalize(path)
            .ok()
            .is_some_and(|p| fs::canonicalize(&self.path).ok() == Some(p))
        {
            return Err("备份位置不能是当前密码库".into());
        }
        if path.parent().and_then(|p| fs::canonicalize(p).ok())
            == self.path.parent().and_then(|p| fs::canonicalize(p).ok())
            && [
                "vault.previous.cbvault",
                "vault.pre-restore.cbvault",
                "vault.pending",
            ]
            .iter()
            .any(|name| {
                path.file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
            })
        {
            return Err("请选择密码库自动备份和临时文件以外的位置".into());
        }
        write_atomic(path, &bytes)
    }
    pub fn restore(&mut self, path: &Path, password: &str) -> Result<(), String> {
        let restored = decode(read(path)?, password)?;
        if self.exists() {
            let mut rollback = self.path.with_file_name("vault.pre-restore.cbvault");
            if fs::canonicalize(path)
                .ok()
                .is_some_and(|p| fs::canonicalize(&rollback).ok() == Some(p))
            {
                rollback = self.path.with_file_name(format!(
                    "vault.pre-restore-{}.cbvault",
                    uuid::Uuid::new_v4()
                ));
            }
            fs::copy(&self.path, rollback).map_err(|_| "无法保留恢复前密码库")?;
        }
        write_atomic(&self.path, &restored.disk_bytes)?;
        self.unlocked = Some(restored);
        Ok(())
    }
    pub fn verify_backup(path: &Path, password: &str) -> Result<usize, String> {
        let verified = decode(read(path)?, password)?;
        Ok(verified.data.entries.len())
    }
    pub fn change_password(&mut self, current: &str, next: &str) -> Result<(), String> {
        new_password(next)?;
        let unlocked = self.unlocked.as_ref().ok_or("请先解锁密码库")?;
        let bytes = read(&self.path)?;
        if bytes != unlocked.disk_bytes {
            return Err("密码库已被其他程序修改，请重新打开".into());
        }
        let _verified = decode(bytes, current)?;
        let salt = random()?;
        let key = derive(next, &salt)?;
        let bytes = encode(&key, &salt, &unlocked.data)?;
        fs::copy(
            &self.path,
            self.path.with_file_name("vault.previous.cbvault"),
        )
        .map_err(|_| "无法保留原密码库")?;
        write_atomic(&self.path, &bytes)?;
        let unlocked = self.unlocked.as_mut().ok_or("请先解锁密码库")?;
        unlocked.key = key;
        unlocked.salt = salt;
        unlocked.disk_bytes = bytes;
        Ok(())
    }
}
