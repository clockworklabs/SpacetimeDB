//! The parent owns a ConPTY, an exact test child and a separately drained output
//! pipe. No desktop, inherited account configuration, or user console is needed.
use super::*;
use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain([0]).collect()
}
struct Child {
    process: Option<OwnedHandle>,
    console: HPCON,
    input: Option<OwnedHandle>,
    startup_pipes: Option<(OwnedHandle, OwnedHandle)>,
    reader: Option<JoinHandle<Result<Vec<u8>>>>,
    waited: bool,
}
impl Child {
    fn start(root: &Path, mode: &str) -> Result<Self> {
        let (input_read, input) = pipe();
        let (output_read, output_write) = pipe();
        let mut this = Self {
            process: None,
            console: 0,
            input: Some(input),
            startup_pipes: Some((input_read, output_write)),
            reader: None,
            waited: false,
        };
        ensure!(
            unsafe {
                CreatePseudoConsole(
                    COORD { X: 120, Y: 48 },
                    this.startup_pipes.as_ref().unwrap().0.as_raw_handle(),
                    this.startup_pipes.as_ref().unwrap().1.as_raw_handle(),
                    0,
                    &mut this.console,
                )
            } == 0,
            "cannot create owned pseudoconsole"
        );
        // Drain concurrently before launching a process that can write console output.
        this.reader = Some(std::thread::spawn(move || {
            let mut all = Vec::new();
            let mut bytes = [0; 4096];
            loop {
                let mut count = 0;
                let ok = unsafe {
                    ReadFile(
                        output_read.as_raw_handle(),
                        bytes.as_mut_ptr(),
                        bytes.len() as u32,
                        &mut count,
                        null_mut(),
                    )
                };
                if ok == 0 {
                    ensure!(
                        unsafe { GetLastError() } == ERROR_BROKEN_PIPE,
                        "owned console drain failed"
                    );
                    break;
                }
                if count == 0 {
                    break;
                }
                if all.len() < 1024 * 1024 {
                    all.extend_from_slice(&bytes[..count as usize]);
                }
            }
            Ok(all)
        }));
        let mut length = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut length);
        }
        ensure!(length > 0 && length <= 4096, "invalid owned process attribute size");
        let mut storage = vec![0usize; length.div_ceil(size_of::<usize>())];
        let attributes = storage.as_mut_ptr().cast();
        unsafe {
            check(InitializeProcThreadAttributeList(attributes, 1, 0, &mut length))?;
        }
        struct Attributes(windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST);
        impl Drop for Attributes {
            fn drop(&mut self) {
                unsafe {
                    DeleteProcThreadAttributeList(self.0);
                }
            }
        }
        let attributes = Attributes(attributes);
        unsafe {
            check(UpdateProcThreadAttribute(
                attributes.0,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                this.console as *const _,
                size_of::<HPCON>(),
                null_mut(),
                null(),
            ))?;
        }
        let executable = std::env::current_exe()?;
        let module = module_path!().split_once("::").unwrap().1;
        let selected = format!("{module}::windows_console_child");
        let mut command = wide(OsStr::new(&format!(
            "\"{}\" --exact {selected} --ignored --nocapture --test-threads=1",
            executable.display()
        )));
        // Inherit no endpoint, auth, proxy or CLI settings. SystemRoot is only
        // the Windows DLL/runtime base; all writable locations belong to root.
        let system = std::env::var_os("SystemRoot").context("missing Windows system directory")?;
        let mut environment = Vec::new();
        for (key, value) in [
            ("STDB_EXEC_CONSOLE_MODE", OsStr::new(mode)),
            ("STDB_EXEC_CONSOLE_ROOT", root.as_os_str()),
            ("SystemRoot", system.as_os_str()),
            ("TEMP", root.as_os_str()),
            ("TMP", root.as_os_str()),
        ] {
            environment.extend(format!("{key}=").encode_utf16());
            environment.extend(value.encode_wide());
            environment.push(0);
        }
        environment.push(0);
        let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        // Without this flag, Windows can duplicate the parent's redirected
        // handles even with bInheritHandles=false. Explicit null standard
        // handles let the pseudoconsole supply its own console handles.
        // https://github.com/microsoft/terminal/discussions/15814
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.lpAttributeList = attributes.0;
        let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        unsafe {
            check(CreateProcessW(
                wide(executable.as_os_str()).as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                environment.as_ptr().cast(),
                wide(root.as_os_str()).as_ptr(),
                &startup.StartupInfo,
                &mut process,
            ))?;
        }
        this.process = Some(unsafe { OwnedHandle::from_raw_handle(process.hProcess) });
        drop(unsafe { OwnedHandle::from_raw_handle(process.hThread) });
        this.startup_pipes.take();
        Ok(this)
    }
    fn complete(&mut self) -> Result<()> {
        let process = self.process.as_ref().context("owned child not started")?;
        let completed = unsafe { WaitForSingleObject(process.as_raw_handle(), 10_000) } == WAIT_OBJECT_0;
        ensure!(completed, "owned console child timed out");
        self.waited = true;
        let mut code = 1;
        unsafe {
            check(GetExitCodeProcess(process.as_raw_handle(), &mut code))?;
        }
        ensure!(code == 0, "owned console child failed");
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        let mut errors = Vec::new();
        if let Some(process) = &self.process
            && !self.waited
        {
            unsafe {
                // An early fixture failure may already have exited before the
                // ready marker. Observe that exact child before forcing it.
                self.waited = WaitForSingleObject(process.as_raw_handle(), 0) == WAIT_OBJECT_0;
                if !self.waited {
                    errors.push(anyhow::anyhow!("owned console child required forced cleanup"));
                    if let Err(error) = check(TerminateProcess(process.as_raw_handle(), 1)) {
                        errors.push(error.context("cannot terminate owned console child"));
                    }
                    self.waited = WaitForSingleObject(process.as_raw_handle(), INFINITE) == WAIT_OBJECT_0;
                }
            }
            if !self.waited {
                errors.push(anyhow::anyhow!("owned console child wait failed"));
            }
        }
        self.input.take();
        // Also close parent's startup copies on every partial-start failure;
        // retaining output_write would keep the drain waiting after ConPTY close.
        self.startup_pipes.take();
        // Keep the drain alive while closing ConPTY; close can emit final output.
        if self.console != 0 {
            unsafe {
                ClosePseudoConsole(self.console);
            }
            self.console = 0;
        }
        if let Some(reader) = self.reader.take() {
            match reader.join() {
                Ok(Ok(_)) => (),
                Ok(Err(error)) => errors.push(error),
                Err(_) => errors.push(anyhow::anyhow!("owned console drain panicked")),
            }
        }
        ensure!(
            errors.is_empty(),
            "{}",
            errors
                .iter()
                .map(|error| format!("{error:#}"))
                .collect::<Vec<_>>()
                .join("; ")
        );
        Ok(())
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("owned console cleanup failed: {error:#}");
        }
    }
}

#[test]
fn native_windows_conpty_normal_error_cancel_restore_console() {
    let _owner = lock(&TEST_OWNER);
    for mode in ["normal", "error", "cancel"] {
        let root = tempfile::tempdir().unwrap();
        let mut child = Child::start(root.path(), mode).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !root.path().join("ready").exists() {
                assert!(Instant::now() < deadline, "owned console child did not become ready");
                std::thread::sleep(Duration::from_millis(10));
            }
            write(child.input.as_ref().unwrap(), "🦀\u{1b}[A\u{3}".as_bytes());
            assert_eq!(
                unsafe { ResizePseudoConsole(child.console, COORD { X: 100, Y: 40 }) },
                0
            );
            child.complete()?;
            assert_eq!(
                std::fs::read(root.path().join("restored")).unwrap(),
                b"all console settings restored"
            );
            Ok(())
        }));
        let cleanup = child.close();
        finish(result, [cleanup]).unwrap();
    }
}

#[test]
#[ignore = "exact owned ConPTY child, invoked by native_windows_conpty_normal_error_cancel_restore_console"]
fn windows_console_child() {
    let root = PathBuf::from(std::env::var_os("STDB_EXEC_CONSOLE_ROOT").expect("owned console child only"));
    let mode = std::env::var("STDB_EXEC_CONSOLE_MODE").unwrap();
    assert!(matches!(mode.as_str(), "normal" | "error" | "cancel"));
    let handles = unsafe {
        [
            GetStdHandle(STD_INPUT_HANDLE),
            GetStdHandle(STD_OUTPUT_HANDLE),
            GetStdHandle(STD_ERROR_HANDLE),
        ]
    };
    let original: Vec<_> = handles
        .iter()
        .enumerate()
        .map(|(index, handle)| {
            let mut value = 0;
            unsafe {
                assert!(
                    GetConsoleMode(*handle, &mut value) != 0,
                    "owned console standard handle {index} is not a console: Win32 error {}",
                    GetLastError()
                );
            }
            value
        })
        .collect();
    let code_page = unsafe { GetConsoleOutputCP() };
    let initial = dimensions().unwrap();
    assert_eq!((initial.rows, initial.columns), (48, 120));
    let (mut terminal, mut io) = Prepared::stdio(true, true).unwrap().start().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut controls = {
        let _entered = runtime.enter();
        terminal.signals().unwrap()
    };
    let mut raw = 0;
    unsafe {
        check(GetConsoleMode(handles[0], &mut raw)).unwrap();
    }
    assert_eq!(
        raw & (ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT),
        0
    );
    assert_ne!(raw & ENABLE_VIRTUAL_TERMINAL_INPUT, 0);
    std::fs::write(root.join("ready"), b"ready").unwrap();
    let result = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), async {
            let expected = "🦀\u{1b}[A\u{3}".as_bytes();
            let mut received = Vec::new();
            while received.len() < expected.len() {
                let Input::Data(bytes) = io.input.recv().await.context("missing console input")? else {
                    bail!("early console EOF")
                };
                received.extend(bytes);
            }
            ensure!(received == expected, "console UTF-8/VT input differs");
            loop {
                if let Some(Ok(ClientControl::Resize(size))) = controls.next().await
                    && (size.rows, size.columns) == (40, 100)
                {
                    break;
                }
            }
            let (completed, written) = oneshot::channel();
            io.output
                .sender
                .send(Output {
                    stream: if mode == "error" {
                        OutputStream::Stdin
                    } else {
                        OutputStream::Stdout
                    },
                    bytes: "owned 🦀 output\r\n".as_bytes().to_vec(),
                    completed,
                })
                .await?;
            terminal.shared.wake.set();
            if mode == "error" {
                ensure!(written.await.is_err(), "invalid output channel accepted");
            } else {
                written.await??;
            }
            Ok::<(), anyhow::Error>(())
        })
        .await?
    });
    let cleanup = if mode == "cancel" {
        drop(terminal);
        Ok(())
    } else {
        terminal.finish()
    };
    result.unwrap();
    assert_eq!(cleanup.is_err(), mode == "error");
    for (handle, previous) in handles.iter().zip(original) {
        let mut value = 0;
        unsafe {
            check(GetConsoleMode(*handle, &mut value)).unwrap();
        }
        assert_eq!(value, previous);
    }
    assert_eq!(unsafe { GetConsoleOutputCP() }, code_page);
    std::fs::write(root.join("restored"), b"all console settings restored").unwrap();
}

#[test]
fn native_windows_console_cleanup_reports_drain_failure() {
    for panic in [false, true] {
        let mut child = Child {
            process: None,
            console: 0,
            input: None,
            startup_pipes: None,
            reader: Some(std::thread::spawn(move || {
                assert!(!panic, "owned drain panic");
                bail!("owned drain failure")
            })),
            waited: false,
        };
        let error = child.close().unwrap_err();
        assert!(error.to_string().contains(if panic { "panicked" } else { "failure" }));
        assert!(child.reader.is_none());
    }
}
