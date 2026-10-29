use std::{thread, time::Duration};
use windows::{
    core::w,
    Win32::{
        Foundation::*,
        System::{DataExchange::*, Memory::*},
    },
};
use zeroize::Zeroizing;

struct Clipboard;
impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}
fn open(owner: HWND) -> Result<Clipboard, String> {
    for _ in 0..5 {
        if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
            return Ok(Clipboard);
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err("剪贴板正在被其他程序使用，请稍后重试".into())
}
unsafe fn set(format: u32, bytes: &[u8]) -> Result<(), String> {
    let memory = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len())
        .map_err(|_| "无法分配剪贴板空间")?;
    let data = GlobalLock(memory);
    if data.is_null() {
        let _ = GlobalFree(Some(memory));
        return Err("无法写入剪贴板".into());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), data as *mut u8, bytes.len());
    let _ = GlobalUnlock(memory);
    if SetClipboardData(format, Some(HANDLE(memory.0))).is_err() {
        let _ = GlobalFree(Some(memory));
        return Err("无法写入剪贴板".into());
    }
    Ok(())
}
pub fn copy(owner: u64, text: &str) -> Result<(), String> {
    let owner = HWND(owner as usize as *mut _);
    let sequence;
    {
        let _clipboard = open(owner)?;
        let utf16 = Zeroizing::new(text.encode_utf16().chain(Some(0)).collect::<Vec<_>>());
        let bytes =
            unsafe { std::slice::from_raw_parts(utf16.as_ptr().cast::<u8>(), utf16.len() * 2) };
        unsafe {
            EmptyClipboard().map_err(|_| "无法清理剪贴板")?;
            set(13, bytes)?; // CF_UNICODETEXT
                             // Opt out of Windows clipboard history and cloud clipboard.
            for format in [
                RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")),
                RegisterClipboardFormatW(w!("CanUploadToCloudClipboard")),
            ] {
                if format == 0 || set(format, &0u32.to_le_bytes()).is_err() {
                    let _ = EmptyClipboard();
                    return Err("无法设置剪贴板隐私标记".into());
                }
            }
            sequence = GetClipboardSequenceNumber();
        }
    }
    let owner_value = owner.0 as usize as u64;
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(30));
        let owner = HWND(owner_value as usize as *mut _);
        if unsafe { GetClipboardSequenceNumber() } != sequence {
            return;
        }
        if let Ok(_clipboard) = open(owner) {
            if unsafe { GetClipboardSequenceNumber() } == sequence {
                unsafe {
                    let _ = EmptyClipboard();
                }
            }
        }
    });
    Ok(())
}
