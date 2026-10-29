use std::{
    io::{self, Read, Write},
    mem::size_of,
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    sync::Arc,
};
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
        System::{Pipes::*, Threading::*, IO::*},
    },
};

#[derive(Clone)]
pub struct Pipe(Arc<OwnedHandle>);
fn io_error(error: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error((error.code().0 as u32 & 0xffff) as i32)
}
impl Pipe {
    fn handle(&self) -> HANDLE {
        HANDLE(self.0.as_raw_handle())
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(self.clone())
    }
    fn event() -> io::Result<OwnedHandle> {
        let handle = unsafe { CreateEventW(None, true, false, None) }.map_err(io_error)?;
        Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) })
    }
    fn complete(
        &self,
        result: windows::core::Result<()>,
        operation: &OVERLAPPED,
        count: &mut u32,
    ) -> io::Result<()> {
        if let Err(error) = result {
            if error.code() != ERROR_IO_PENDING.to_hresult() {
                return Err(io_error(error));
            }
        }
        unsafe { GetOverlappedResult(self.handle(), operation, count, true) }.map_err(io_error)
    }
}
impl Read for Pipe {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let event = Self::event()?;
        let mut operation = OVERLAPPED {
            hEvent: HANDLE(event.as_raw_handle()),
            ..Default::default()
        };
        let mut count = 0;
        let result = unsafe {
            ReadFile(
                self.handle(),
                Some(bytes),
                Some(&mut count),
                Some(&mut operation),
            )
        };
        match self.complete(result, &operation, &mut count) {
            Ok(()) => Ok(count as usize),
            Err(error) if error.raw_os_error() == Some(ERROR_BROKEN_PIPE.0 as i32) => Ok(0),
            Err(error) => Err(error),
        }
    }
}
impl Write for Pipe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let event = Self::event()?;
        let mut operation = OVERLAPPED {
            hEvent: HANDLE(event.as_raw_handle()),
            ..Default::default()
        };
        let mut count = 0;
        let result = unsafe {
            WriteFile(
                self.handle(),
                Some(bytes),
                Some(&mut count),
                Some(&mut operation),
            )
        };
        self.complete(result, &operation, &mut count)?;
        Ok(count as usize)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn user_sid() -> Result<String, String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|_| "无法读取当前 Windows 用户")?;
        let result = (|| {
            let mut length = 0;
            let _ = GetTokenInformation(token, TokenUser, None, 0, &mut length);
            if length == 0 || length > 65536 {
                return Err("用户 SID 无效".into());
            }
            let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
            GetTokenInformation(
                token,
                TokenUser,
                Some(buffer.as_mut_ptr().cast()),
                length,
                &mut length,
            )
            .map_err(|_| "无法读取用户 SID")?;
            let info = &*(buffer.as_ptr() as *const TOKEN_USER);
            let mut sid = PWSTR::null();
            ConvertSidToStringSidW(info.User.Sid, &mut sid).map_err(|_| "无法转换用户 SID")?;
            let value = sid.to_string().map_err(|_| "用户 SID 编码无效");
            let _ = LocalFree(Some(HLOCAL(sid.0.cast())));
            value.map_err(String::from)
        })();
        let _ = CloseHandle(token);
        result
    }
}
pub fn pipe_name() -> Result<String, String> {
    Ok(format!(r"\\.\pipe\Codebook.{}.v1", user_sid()?))
}
pub fn create_server(first: bool) -> Result<Pipe, String> {
    create_server_named(first, &pipe_name()?)
}
pub fn open_client(name: &str) -> io::Result<Pipe> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OVERLAPPED.0)
        .open(name)?;
    Ok(Pipe(Arc::new(file.into())))
}
fn create_server_named(first: bool, pipe: &str) -> Result<Pipe, String> {
    let sid = user_sid()?;
    let name: Vec<u16> = pipe.encode_utf16().chain(Some(0)).collect();
    let sddl: Vec<u16> = format!("D:P(A;;GA;;;{sid})")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
        .map_err(|_| "无法设置本机通信权限")?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let flags = PIPE_ACCESS_DUPLEX
            | FILE_FLAG_OVERLAPPED
            | if first {
                FILE_FLAG_FIRST_PIPE_INSTANCE
            } else {
                FILE_FLAGS_AND_ATTRIBUTES(0)
            };
        let handle = CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            flags,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            16,
            65536,
            65536,
            0,
            Some(&attributes),
        );
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        if handle == INVALID_HANDLE_VALUE {
            return Err("无法创建本机通信通道；可能已有其他实例占用".into());
        }
        let file = Pipe(Arc::new(OwnedHandle::from_raw_handle(handle.0)));
        let event = Pipe::event().map_err(|_| "无法创建连接事件")?;
        let mut operation = OVERLAPPED {
            hEvent: HANDLE(event.as_raw_handle()),
            ..Default::default()
        };
        let result = ConnectNamedPipe(handle, Some(&mut operation));
        if !result
            .as_ref()
            .is_err_and(|e| e.code() == ERROR_PIPE_CONNECTED.to_hresult())
        {
            file.complete(result, &operation, &mut 0)
                .map_err(|_| "浏览器连接失败")?;
        }
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        sync::mpsc,
        thread,
        time::Duration,
    };
    #[test]
    fn a_pending_read_does_not_block_a_simultaneous_write() {
        let name = format!("{}.test.{}", pipe_name().unwrap(), uuid::Uuid::new_v4());
        let server_name = name.clone();
        thread::spawn(move || {
            let mut server = create_server_named(true, &server_name).unwrap();
            let mut request = [0];
            server.read_exact(&mut request).unwrap();
            assert_eq!(request, [42]);
            server.write_all(&[43]).unwrap();
        });
        let mut client = (0..100)
            .find_map(|_| match open_client(&name) {
                Ok(pipe) => Some(pipe),
                Err(_) => {
                    thread::sleep(Duration::from_millis(10));
                    None
                }
            })
            .expect("server pipe did not become ready");
        let mut reader = client.try_clone().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            started_tx.send(()).unwrap();
            let mut response = [0];
            reader.read_exact(&mut response).unwrap();
            done_tx.send(response).unwrap();
        });
        started_rx.recv().unwrap();
        thread::sleep(Duration::from_millis(100));
        thread::spawn(move || client.write_all(&[42]).unwrap());
        assert_eq!(
            done_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("pending read blocked outbound write"),
            [43]
        );
    }
}
