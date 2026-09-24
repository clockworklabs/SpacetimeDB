// The process-wide console handler must stay exclusively owned across awaits.
#![allow(clippy::await_holding_lock)]
use super::*;
use std::{
    fs::OpenOptions,
    os::windows::{
        fs::OpenOptionsExt,
        io::{FromRawHandle, OwnedHandle},
    },
    ptr::{null, null_mut},
    time::Instant,
};
use windows_sys::Win32::{
    Storage::FileSystem::*,
    System::{Pipes::*, Threading::*},
};

#[path = "console_tests.rs"]
mod console;

// The real console child has a separate process. These pipe fixtures serialize
// only our removable process-wide Ctrl handler, never the user's console modes.
static TEST_OWNER: Mutex<()> = Mutex::new(());

fn finish<T>(primary: std::thread::Result<Result<T>>, cleanup: impl IntoIterator<Item = Result<()>>) -> Result<T> {
    let errors: Vec<_> = cleanup.into_iter().filter_map(Result::err).collect();
    let details = errors
        .iter()
        .map(|error| format!("{error:#}"))
        .collect::<Vec<_>>()
        .join("; ");
    match primary {
        Ok(Ok(value)) if errors.is_empty() => Ok(value),
        Ok(Ok(_)) => bail!("owned Windows fixture cleanup failed: {details}"),
        Ok(Err(error)) if errors.is_empty() => Err(error),
        Ok(Err(error)) => Err(error.context(format!("owned Windows fixture cleanup also failed: {details}"))),
        Err(panic) => {
            if !errors.is_empty() {
                eprintln!("owned Windows fixture cleanup also failed: {details}");
            }
            std::panic::resume_unwind(panic)
        }
    }
}
fn pipe() -> (OwnedHandle, OwnedHandle) {
    let (mut read, mut write) = (null_mut(), null_mut());
    unsafe {
        check(CreatePipe(&mut read, &mut write, null(), 4096)).unwrap();
        (OwnedHandle::from_raw_handle(read), OwnedHandle::from_raw_handle(write))
    }
}
fn fixture() -> (Terminal, Io, OwnedHandle, OwnedHandle, OwnedHandle) {
    let (input, input_writer) = pipe();
    let (output_reader, output) = pipe();
    let (error_reader, error) = pipe();
    let (terminal, io) = Prepared::new(
        [
            Some(File::owned(input).unwrap()),
            Some(File::owned(output).unwrap()),
            Some(File::owned(error).unwrap()),
        ],
        false,
    )
    .unwrap()
    .start()
    .unwrap();
    (terminal, io, input_writer, output_reader, error_reader)
}
fn receive(handle: &OwnedHandle, size: usize) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut received = Vec::new();
    while received.len() < size {
        let mut available = 0;
        unsafe {
            check(PeekNamedPipe(
                handle.as_raw_handle(),
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            ))
            .unwrap();
        }
        assert!(Instant::now() < deadline, "owned pipe output timed out");
        if available == 0 {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        let mut bytes = vec![0; (available as usize).min(size - received.len())];
        let mut count = 0;
        unsafe {
            check(ReadFile(
                handle.as_raw_handle(),
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                &mut count,
                null_mut(),
            ))
            .unwrap();
        }
        received.extend_from_slice(&bytes[..count as usize]);
    }
    received
}
fn write(handle: &OwnedHandle, bytes: &[u8]) {
    let mut count = 0;
    unsafe {
        check(WriteFile(
            handle.as_raw_handle(),
            bytes.as_ptr(),
            bytes.len() as u32,
            &mut count,
            null_mut(),
        ))
        .unwrap();
    }
    assert_eq!(count as usize, bytes.len());
}

#[test]
fn native_windows_pipe_backpressure_cancellation_joins_both_workers() {
    let _owner = lock(&TEST_OWNER);
    let (mut terminal, io, _input_writer, _output_reader, _error_reader) = fixture();
    let (ack, mut completed) = oneshot::channel();
    io.output
        .sender
        .try_send(Output {
            stream: OutputStream::Stdout,
            bytes: vec![42; MAX_DATA_BYTES],
            completed: ack,
        })
        .unwrap();
    terminal.shared.wake.set();
    std::thread::sleep(Duration::from_millis(50));
    assert!(completed.try_recv().is_err());
    let started = Instant::now();
    terminal.finish().unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(terminal.workers.is_empty());
    terminal.finish().unwrap();
    assert!(completed.blocking_recv().is_err());
}

#[test]
fn native_windows_cancel_before_io_registration_and_worker_panic_are_joined() {
    let _owner = lock(&TEST_OWNER);
    for panic in [false, true] {
        let (mut terminal, _io, _writer, _reader, _error_reader) = fixture();
        let (entered_tx, entered) = std::sync::mpsc::channel();
        terminal
            .spawn("owned-race", move |shared| {
                entered_tx.send(()).unwrap();
                while !shared.stopped() {
                    std::thread::yield_now();
                }
                if panic {
                    panic!("owned terminal panic");
                }
                // Intentionally issue a synchronous call after cancellation was
                // requested. Repeated thread cancellation must catch registration.
                let (read, _write) = pipe();
                let mut byte = 0;
                let mut count = 0;
                unsafe {
                    ReadFile(read.as_raw_handle(), &mut byte, 1, &mut count, null_mut());
                }
                Ok(())
            })
            .unwrap();
        entered.recv_timeout(Duration::from_secs(2)).unwrap();
        let result = terminal.finish();
        assert_eq!(result.is_err(), panic);
        assert!(terminal.workers.is_empty());
    }
}

#[tokio::test]
async fn native_windows_pipe_bytes_eof_and_signal_numbers() {
    let _owner = lock(&TEST_OWNER);
    let (mut terminal, mut io, writer, reader, error_reader) = fixture();
    write(&writer, &[0, 255, 1, 13, 10]);
    drop(writer);
    let Input::Data(bytes) = io.input.recv().await.unwrap() else {
        panic!("missing input bytes")
    };
    assert_eq!(bytes, [0, 255, 1, 13, 10]);
    assert!(matches!(io.input.recv().await, Some(Input::Eof)));
    let (completed, written) = oneshot::channel();
    io.output
        .sender
        .send(Output {
            stream: OutputStream::Stdout,
            bytes: vec![255, 0, 13, 10],
            completed,
        })
        .await
        .unwrap();
    terminal.shared.wake.set();
    written.await.unwrap().unwrap();
    assert_eq!(receive(&reader, 4), [255, 0, 13, 10]);
    let (completed, written) = oneshot::channel();
    io.output
        .sender
        .send(Output {
            stream: OutputStream::Stderr,
            bytes: vec![0, 254, 128],
            completed,
        })
        .await
        .unwrap();
    terminal.shared.wake.set();
    written.await.unwrap().unwrap();
    assert_eq!(receive(&error_reader, 3), [0, 254, 128]);
    let mut signals = terminal.signals().unwrap();
    assert_eq!(control::invoke(CTRL_C_EVENT), 1);
    assert!(matches!(signals.next().await, Some(Ok(ClientControl::Signal(2)))));
    assert_eq!(control::invoke(CTRL_BREAK_EVENT), 1);
    assert!(matches!(signals.next().await, Some(Ok(ClientControl::Signal(3)))));
    terminal.finish().unwrap();
}

#[test]
fn native_windows_synchronous_file_offset_and_asynchronous_file_rejection() {
    let _owner = lock(&TEST_OWNER);
    use std::io::{Read, Seek, SeekFrom, Write};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("owned-redirection");
    let mut file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.write_all(b"abcdef").unwrap();
    file.seek(SeekFrom::Start(2)).unwrap();
    let classified = File::duplicate(file.as_raw_handle()).unwrap();
    assert_eq!(classified.kind, Kind::Synchronous);
    let (mut terminal, _io, _writer, _reader, _error_reader) = fixture();
    let mut bytes = [0; 2];
    assert_eq!(classified.read(&mut bytes, &terminal.shared).unwrap(), 2);
    assert_eq!(&bytes, b"cd");
    let mut remainder = Vec::new();
    file.read_to_end(&mut remainder).unwrap();
    assert_eq!(&remainder, b"ef");
    let asynchronous = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OVERLAPPED)
        .open(path)
        .unwrap();
    assert!(File::duplicate(asynchronous.as_raw_handle())
        .err()
        .unwrap()
        .to_string()
        .contains("asynchronous seekable"));
    terminal.finish().unwrap();
}

fn overlapped_pipe(output: bool) -> (OwnedHandle, OwnedHandle) {
    let unique = tempfile::tempdir().unwrap();
    let name: Vec<u16> = format!(
        r"\\.\pipe\spacetimedb-exec-{}-{}",
        std::process::id(),
        unique.path().file_name().unwrap().to_string_lossy()
    )
    .encode_utf16()
    .chain([0])
    .collect();
    let access = if output {
        PIPE_ACCESS_OUTBOUND
    } else {
        PIPE_ACCESS_INBOUND
    };
    let raw = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            access | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            4096,
            4096,
            0,
            null(),
        )
    };
    assert_ne!(raw, INVALID_HANDLE_VALUE);
    let server = unsafe { OwnedHandle::from_raw_handle(raw) };
    let access = if output { GENERIC_READ } else { GENERIC_WRITE };
    let raw = unsafe { CreateFileW(name.as_ptr(), access, 0, null(), OPEN_EXISTING, 0, null_mut()) };
    assert_ne!(raw, INVALID_HANDLE_VALUE);
    let client = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut connected: windows_sys::Win32::System::IO::OVERLAPPED = unsafe { std::mem::zeroed() };
    // The exact client has already connected; no asynchronous accept remains.
    assert_eq!(unsafe { ConnectNamedPipe(server.as_raw_handle(), &mut connected) }, 0);
    assert_eq!(unsafe { GetLastError() }, ERROR_PIPE_CONNECTED);
    (server, client)
}

#[tokio::test]
async fn native_windows_overlapped_pipe_cancel_observes_exact_completion() {
    let _owner = lock(&TEST_OWNER);
    for output in [false, true] {
        let (server, client) = overlapped_pipe(output);
        let file = File::owned(server).unwrap();
        assert_eq!(file.kind, Kind::OverlappedPipe);
        let (unused_reader, ordinary_output) = pipe();
        let files = if output {
            [None, Some(file), None]
        } else {
            [Some(file), Some(File::owned(ordinary_output).unwrap()), None]
        };
        let (mut terminal, mut io) = Prepared::new(files, false).unwrap().start().unwrap();
        let mut ack = None;
        if output {
            let (completed, written) = oneshot::channel();
            io.output
                .sender
                .send(Output {
                    stream: OutputStream::Stdout,
                    bytes: vec![0x81; MAX_DATA_BYTES],
                    completed,
                })
                .await
                .unwrap();
            terminal.shared.wake.set();
            ack = Some(written);
        } else {
            write(&client, &[]);
            write(&client, &[0, 255, 13, 10]);
            let Input::Data(bytes) = tokio::time::timeout(Duration::from_secs(2), io.input.recv())
                .await
                .unwrap()
                .unwrap()
            else {
                panic!("zero pipe message became EOF")
            };
            assert_eq!(bytes, [0, 255, 13, 10]);
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
        let started = Instant::now();
        terminal.finish().unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(terminal.workers.is_empty());
        if let Some(written) = ack {
            assert!(written.await.is_err());
        }
        drop(unused_reader);
    }
}

#[tokio::test]
async fn native_windows_close_before_ready_restores_then_releases_callback() {
    let _owner = lock(&TEST_OWNER);
    use futures::{FutureExt, StreamExt};
    let (mut terminal, mut io, _writer, _reader, _error_reader) = fixture();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!(
        "http://{}/v1/database/{}/container/exec",
        listener.local_addr().unwrap(),
        spacetimedb_lib::Identity::ZERO.to_hex()
    );
    let (started, start) = std::sync::mpsc::channel();
    let callback = std::thread::spawn(move || {
        start.recv_timeout(Duration::from_secs(5)).unwrap();
        control::invoke(CTRL_CLOSE_EVENT)
    });
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_hdr_async(
            socket,
            |_request: &tokio_tungstenite::tungstenite::handshake::server::Request,
             mut response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                response.headers_mut().insert(
                    "sec-websocket-protocol",
                    spacetimedb_lib::container::exec::SUBPROTOCOL.parse().unwrap(),
                );
                Ok(response)
            },
        )
        .await
        .unwrap();
        assert!(socket.next().await.is_some());
        started.send(()).unwrap();
        assert!(socket.next().await.is_none_or(|value| value.is_err()));
    });
    let start = spacetimedb_lib::container::exec::ExecStart {
        generation: 1,
        argv: vec!["literal-command".into()],
        working_directory: None,
        environment: Default::default(),
        stdin: true,
        terminal: None,
    };
    let result = std::panic::AssertUnwindSafe(async {
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            super::super::session::run(
                origin.parse().unwrap(),
                "Bearer owned-loopback-fixture".parse().unwrap(),
                spacetimedb_lib::Identity::ZERO,
                start,
                &mut io,
                futures::stream::pending(),
            ),
        )
        .await?;
        ensure!(result.is_err(), "close was reported as a guest exit");
        Ok(())
    })
    .catch_unwind()
    .await;
    // Join and release the handler even if the assertion below will fail.
    let terminal_result = terminal.finish();
    let callback_result = callback
        .join()
        .map_err(|_| anyhow::anyhow!("close callback panicked"))
        .and_then(|result| {
            ensure!(result == 1, "close callback was not handled");
            Ok(())
        });
    let server_result = server.await.context("owned close fixture server failed");
    finish(result, [terminal_result, callback_result, server_result]).unwrap();
    assert_eq!(*lock(&terminal.shared.callbacks), 0);
}

#[test]
fn native_windows_console_unicode_surrogates_are_not_split() {
    let mut pending = None;
    assert_eq!(decode_console(&[0xd83e], &mut pending).unwrap(), "");
    assert_eq!(
        decode_console(&[0xdd80, 27, 91, 65], &mut pending).unwrap(),
        "🦀\u{1b}[A"
    );
    assert!(decode_console(&[0xdc00], &mut pending).is_err());
}

#[test]
fn native_windows_overlapped_pipe_suppresses_inherited_completion_port_packets() {
    let _owner = lock(&TEST_OWNER);
    use windows_sys::Win32::System::IO::{CreateIoCompletionPort, GetQueuedCompletionStatus};
    for output in [false, true] {
        let (server, client) = overlapped_pipe(output);
        let raw = unsafe { CreateIoCompletionPort(server.as_raw_handle(), null_mut(), 123, 1) };
        assert!(!raw.is_null());
        let port = unsafe { OwnedHandle::from_raw_handle(raw) };
        // Dup preserves the existing IOCP association. No shared completion mode
        // is changed by classification or the adapter's per-operation events.
        let file = File::duplicate(server.as_raw_handle()).unwrap();
        let (mut terminal, _io, _writer, _reader, _error_reader) = fixture();
        let primary = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if output {
                assert_eq!(file.write(&[0, 255, 7], &terminal.shared)?, 3);
                assert_eq!(receive(&client, 3), [0, 255, 7]);
            } else {
                write(&client, &[0, 255, 8]);
                let mut bytes = [0; 3];
                assert_eq!(file.read(&mut bytes, &terminal.shared)?, 3);
                assert_eq!(bytes, [0, 255, 8]);
            }
            let (mut bytes, mut key, mut overlap) = (0, 0, null_mut());
            let result =
                unsafe { GetQueuedCompletionStatus(port.as_raw_handle(), &mut bytes, &mut key, &mut overlap, 50) };
            assert_eq!(result, 0);
            assert_eq!(unsafe { GetLastError() }, WAIT_TIMEOUT);
            assert!(overlap.is_null(), "adapter operation escaped to inherited IOCP");
            Ok(())
        }));
        let cleanup = terminal.finish();
        finish(primary, [cleanup]).unwrap();
    }
}
