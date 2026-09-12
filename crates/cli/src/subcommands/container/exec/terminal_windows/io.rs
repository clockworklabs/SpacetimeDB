//! Buffers and OVERLAPPED storage outlive the exact operation, including cancellation.
use super::{check, Shared};
use anyhow::{bail, ensure, Result};
use std::{
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr::{null, null_mut},
    sync::Arc,
};
use windows_sys::{
    Wdk::Storage::FileSystem::{
        FileModeInformation, NtQueryInformationFile, FILE_MODE_INFORMATION, FILE_SYNCHRONOUS_IO_ALERT,
        FILE_SYNCHRONOUS_IO_NONALERT,
    },
    Win32::{
        Foundation::*,
        Storage::FileSystem::*,
        System::{Console::*, Threading::*, IO::*},
    },
};

pub(super) struct Event(OwnedHandle);
impl Event {
    pub fn new() -> Result<Self> {
        // SAFETY: no security attributes, name, or inherited handle; owned once.
        let raw = unsafe { CreateEventW(null(), 1, 0, null()) };
        ensure!(!raw.is_null(), "cannot create terminal event");
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(raw) }))
    }
    pub fn raw(&self) -> HANDLE {
        self.0.as_raw_handle()
    }
    pub fn set(&self) {
        unsafe {
            SetEvent(self.raw());
        }
    }
    pub fn reset(&self) {
        unsafe {
            ResetEvent(self.raw());
        }
    }
    pub fn wait(&self, millis: u32) {
        unsafe {
            WaitForSingleObject(self.raw(), millis);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Console,
    Synchronous,
    OverlappedPipe,
}
pub(super) struct File {
    pub handle: Arc<OwnedHandle>,
    pub kind: Kind,
    pipe: bool,
}
impl File {
    pub fn duplicate(raw: HANDLE) -> Result<Self> {
        ensure!(!raw.is_null() && raw != INVALID_HANDLE_VALUE, "missing terminal handle");
        let mut duplicate = null_mut();
        // SAFETY: duplicate this process's borrowed standard handle, non-inheritable.
        unsafe {
            check(DuplicateHandle(
                GetCurrentProcess(),
                raw,
                GetCurrentProcess(),
                &mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            ))?;
        }
        Self::owned(unsafe { OwnedHandle::from_raw_handle(duplicate) })
    }
    pub fn owned(handle: OwnedHandle) -> Result<Self> {
        let raw = handle.as_raw_handle();
        let file_type = unsafe { GetFileType(raw) };
        let mut mode = 0;
        let kind = if unsafe { GetConsoleMode(raw, &mut mode) } != 0 {
            Kind::Console
        } else {
            ensure!(
                matches!(file_type, FILE_TYPE_DISK | FILE_TYPE_PIPE | FILE_TYPE_CHAR),
                "unsupported terminal handle type"
            );
            // This class-specific query follows .NET 10 SafeFileHandle.GetFileOptions:
            // it returns the file object's mode directly and accepts only SUCCESS.
            // Do not generalize this to other information classes or infer request
            // completion by waiting on an inherited file's shared event.
            // https://github.com/dotnet/runtime/blob/60629d14374c56f1cb51819049ad1fa529307f8d/src/libraries/System.Private.CoreLib/src/Microsoft/Win32/SafeHandles/SafeFileHandle.Windows.cs#L182-L205
            let mut status: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
            let mut info: FILE_MODE_INFORMATION = unsafe { std::mem::zeroed() };
            let result = unsafe {
                NtQueryInformationFile(
                    raw,
                    &mut status,
                    (&mut info as *mut FILE_MODE_INFORMATION).cast(),
                    size_of::<FILE_MODE_INFORMATION>() as u32,
                    FileModeInformation,
                )
            };
            ensure!(result == STATUS_SUCCESS, "cannot classify inherited terminal handle");
            if info.Mode & (FILE_SYNCHRONOUS_IO_ALERT | FILE_SYNCHRONOUS_IO_NONALERT) != 0 {
                Kind::Synchronous
            } else {
                ensure!(
                    file_type == FILE_TYPE_PIPE,
                    "inherited asynchronous seekable files are unsupported for container exec"
                );
                Kind::OverlappedPipe
            }
        };
        Ok(Self {
            handle: Arc::new(handle),
            kind,
            pipe: file_type == FILE_TYPE_PIPE,
        })
    }
    pub fn raw(&self) -> HANDLE {
        self.handle.as_raw_handle()
    }

    pub fn read(&self, bytes: &mut [u8], shared: &Shared) -> Result<usize> {
        if self.kind == Kind::Console {
            bail!("console input requires Unicode decoding");
        }
        loop {
            match self.transfer(bytes.as_mut_ptr(), bytes.len(), false, shared)? {
                // A zero-byte pipe message is not EOF. Only a broken pipe is.
                Some(0) if self.pipe => continue,
                Some(count) => return Ok(count),
                None => return Ok(0),
            }
        }
    }
    pub fn write(&self, bytes: &[u8], shared: &Shared) -> Result<usize> {
        Ok(self
            .transfer(bytes.as_ptr().cast_mut(), bytes.len(), true, shared)?
            .unwrap_or(0))
    }
    fn transfer(&self, bytes: *mut u8, length: usize, write: bool, shared: &Shared) -> Result<Option<usize>> {
        ensure!(length <= super::MAX_DATA_BYTES, "terminal I/O exceeds bound");
        if shared.stopped() {
            return Ok(None);
        }
        let mut count = 0;
        let mut pending = if self.kind == Kind::OverlappedPipe {
            Some(Pending::new(self.raw())?)
        } else {
            None
        };
        let overlap = pending.as_mut().map_or(null_mut(), |value| &mut *value.overlap);
        // SAFETY: the caller owns this bounded buffer until final completion;
        // Pending retains its stable OVERLAPPED/event and drains on unwind.
        let ok = unsafe {
            if write {
                WriteFile(self.raw(), bytes, length as u32, &mut count, overlap)
            } else {
                ReadFile(self.raw(), bytes, length as u32, &mut count, overlap)
            }
        };
        let mut error = if ok == 0 {
            unsafe { GetLastError() }
        } else {
            ERROR_SUCCESS
        };
        if error == ERROR_IO_PENDING {
            let operation = pending.as_mut().expect("overlapped operation must own completion");
            operation.active = true;
            // Cancellation may have arrived before ReadFile/WriteFile registered.
            // The issuing owner rechecks it and cancels the exact operation.
            loop {
                if shared.stopped() {
                    operation.cancel();
                }
                let done = unsafe { GetOverlappedResult(self.raw(), &*operation.overlap, &mut count, 0) };
                error = if done != 0 {
                    ERROR_SUCCESS
                } else {
                    unsafe { GetLastError() }
                };
                if error != ERROR_IO_INCOMPLETE {
                    operation.active = false;
                    break;
                }
                operation.event.wait(20);
            }
        }
        if shared.stopped() {
            return Ok(None);
        }
        if !write && matches!(error, ERROR_BROKEN_PIPE | ERROR_HANDLE_EOF) {
            return Ok(None);
        }
        ensure!(
            error == ERROR_SUCCESS || (!write && error == ERROR_MORE_DATA),
            "terminal I/O failed"
        );
        ensure!(count as usize <= length, "invalid terminal I/O count");
        Ok(Some(count as usize))
    }
    pub fn read_console(&self, units: &mut [u16], shared: &Shared) -> Result<usize> {
        loop {
            let mut count = 0;
            if shared.stopped() {
                return Ok(0);
            }
            let ok = unsafe {
                SetLastError(ERROR_SUCCESS);
                ReadConsoleW(
                    self.raw(),
                    units.as_mut_ptr().cast(),
                    units.len() as u32,
                    &mut count,
                    null(),
                )
            };
            if shared.stopped() {
                return Ok(0);
            }
            // Cooked console Ctrl+C/Break is forwarded by our control handler.
            // It must interrupt and resume this read, not turn into EOF/failure.
            if count == 0 && unsafe { GetLastError() } == ERROR_OPERATION_ABORTED {
                continue;
            }
            check(ok)?;
            ensure!(count as usize <= units.len(), "invalid console input count");
            return Ok(count as usize);
        }
    }
}

struct Pending {
    handle: HANDLE,
    overlap: Box<OVERLAPPED>,
    event: Event,
    active: bool,
}
impl Pending {
    fn new(handle: HANDLE) -> Result<Self> {
        let event = Event::new()?;
        let mut overlap: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
        // The inherited file may belong to another owner's completion port.
        // The low bit suppresses IOCP packets for this exact operation without
        // changing the shared file's modes. Wait/close uses the unflagged event.
        overlap.hEvent = event.raw().map_addr(|address| address | 1);
        Ok(Self {
            handle,
            overlap,
            event,
            active: false,
        })
    }
    fn cancel(&self) {
        unsafe {
            CancelIoEx(self.handle, &*self.overlap);
        }
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        if self.active {
            self.cancel();
            let mut count = 0;
            // An unwind never abandons an accepted I/O operation or its memory.
            unsafe {
                GetOverlappedResult(self.handle, &*self.overlap, &mut count, 1);
            }
        }
    }
}

pub(super) struct ConsoleState {
    files: Vec<(Arc<OwnedHandle>, u32)>,
    code_page: Option<u32>,
    restored: bool,
}
impl ConsoleState {
    pub fn prepare(files: &[Option<File>; 3], tty: bool) -> Result<Self> {
        let mut this = Self {
            files: Vec::new(),
            code_page: None,
            restored: false,
        };
        // Capture every alias before mutating any shared console buffer.
        for file in files.iter().flatten().filter(|file| file.kind == Kind::Console) {
            let mut mode = 0;
            unsafe {
                check(GetConsoleMode(file.raw(), &mut mode))?;
            }
            this.files.push((file.handle.clone(), mode));
        }
        if files[1..].iter().flatten().any(|file| file.kind == Kind::Console) {
            let original = unsafe { GetConsoleOutputCP() };
            ensure!(original != 0, "cannot read console output code page");
            this.code_page = Some(original);
            unsafe {
                check(SetConsoleOutputCP(65001))?;
            }
        }
        for (index, file) in files.iter().enumerate().filter_map(|(i, f)| f.as_ref().map(|f| (i, f))) {
            if file.kind != Kind::Console {
                continue;
            }
            let mut mode = 0;
            unsafe {
                check(GetConsoleMode(file.raw(), &mut mode))?;
            }
            if index == 0 && tty {
                mode = (mode | ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_EXTENDED_FLAGS)
                    & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT | ENABLE_QUICK_EDIT_MODE);
            } else if index != 0 {
                mode |= ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
            }
            unsafe {
                check(SetConsoleMode(file.raw(), mode))?;
            }
        }
        Ok(this)
    }
    pub fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        let mut failed = false;
        for (file, mode) in &self.files {
            failed |= unsafe { SetConsoleMode(file.as_raw_handle(), *mode) } == 0;
        }
        if let Some(code_page) = self.code_page {
            failed |= unsafe { SetConsoleOutputCP(code_page) } == 0;
        }
        self.restored = !failed;
        ensure!(!failed, "could not restore console settings");
        Ok(())
    }
}
impl Drop for ConsoleState {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
