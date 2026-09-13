//! Windows stdio workers remain owned through cancellation and console restoration.
//! Inherited synchronous handles cannot be converted to overlapped handles by dup.
#[path = "terminal_windows/control.rs"]
mod control;
#[path = "terminal_windows/io.rs"]
mod io;
#[cfg(test)]
#[path = "terminal_windows/tests.rs"]
mod tests;

use super::session::{Input, Io, Output, OutputSender, OutputStream};
use anyhow::{bail, ensure, Context, Result};
use futures::{stream, Stream, StreamExt};
use io::{ConsoleState, Event, File, Kind};
use spacetimedb_lib::container::exec::{ClientControl, TerminalSize, MAX_DATA_BYTES};
use std::{
    os::windows::io::AsRawHandle,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, MutexGuard,
    },
    thread::JoinHandle,
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use windows_sys::Win32::{
    Foundation::*,
    System::{Console::*, IO::CancelSynchronousIo},
};

fn check(ok: BOOL) -> Result<()> {
    ensure!(ok != 0, "Windows terminal operation failed");
    Ok(())
}
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

struct Shared {
    stop: AtomicBool,
    wake: Event,
    restored: Event,
    failure: Mutex<Option<oneshot::Sender<Result<()>>>>,
    controls: mpsc::Sender<Result<ClientControl>>,
    callbacks: Mutex<usize>,
    callbacks_done: Condvar,
}
impl Shared {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
    fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.wake.set();
    }
    fn fail(&self) {
        self.stop();
        if let Some(sender) = lock(&self.failure).take() {
            let _ = sender.send(Err(anyhow::anyhow!("Windows terminal stopped")));
        }
    }
}

pub(super) fn dimensions() -> Result<TerminalSize> {
    let mut input_mode = 0;
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    unsafe {
        check(GetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), &mut input_mode))
            .context("PTY requires attached console input")?;
        check(GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &mut info))
            .context("PTY requires attached console output")?;
    }
    let size = TerminalSize {
        rows: u16::try_from(i32::from(info.srWindow.Bottom) - i32::from(info.srWindow.Top) + 1)?,
        columns: u16::try_from(i32::from(info.srWindow.Right) - i32::from(info.srWindow.Left) + 1)?,
    };
    size.validate().context("invalid console dimensions")?;
    Ok(size)
}

pub(super) struct Prepared {
    files: [Option<File>; 3],
    tty: bool,
}
impl Prepared {
    pub(super) fn stdio(stdin: bool, tty: bool) -> Result<Self> {
        let handles = unsafe {
            [
                GetStdHandle(STD_INPUT_HANDLE),
                GetStdHandle(STD_OUTPUT_HANDLE),
                GetStdHandle(STD_ERROR_HANDLE),
            ]
        };
        Self::new(
            [
                stdin.then(|| File::duplicate(handles[0])).transpose()?,
                Some(File::duplicate(handles[1])?),
                (!tty).then(|| File::duplicate(handles[2])).transpose()?,
            ],
            tty,
        )
    }
    fn new(files: [Option<File>; 3], tty: bool) -> Result<Self> {
        ensure!(files[1].is_some(), "terminal output is required");
        ensure!(
            !tty || files[..2]
                .iter()
                .all(|file| file.as_ref().is_some_and(|file| file.kind == Kind::Console)),
            "PTY requires attached console input and output"
        );
        Ok(Self { files, tty })
    }
    pub(super) fn start(self) -> Result<(Terminal, Io)> {
        let (input_sender, input) = mpsc::channel(1);
        let (output_sender, output) = mpsc::channel(1);
        let (completed, completion) = oneshot::channel();
        let (control_sender, controls) = mpsc::channel(8);
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            wake: Event::new()?,
            restored: Event::new()?,
            failure: Mutex::new(Some(completed)),
            controls: control_sender,
            callbacks: Mutex::new(0),
            callbacks_done: Condvar::new(),
        });
        // Reserve callback/console ownership before any shared console mutation.
        let registration = control::Registration::new(shared.clone())?;
        let mut terminal = Terminal {
            shared: shared.clone(),
            workers: Vec::new(),
            console: None,
            registration: Some(registration),
            controls: Some(controls),
            tty: self.tty,
            finished: false,
            failed: false,
        };
        terminal.console = Some(ConsoleState::prepare(&self.files, self.tty)?);
        let [stdin, stdout, stderr] = self.files;
        if let Some(stdin) = stdin {
            terminal.spawn("container-exec-input", move |shared| {
                read_input(stdin, input_sender, shared)
            })?;
        }
        terminal.spawn("container-exec-output", move |shared| {
            write_output(stdout.unwrap(), stderr, output, shared)
        })?;
        let notification = shared.clone();
        Ok((
            terminal,
            Io {
                input,
                output: OutputSender {
                    sender: output_sender,
                    wake: Some(Arc::new(move || notification.wake.set())),
                },
                completion,
            },
        ))
    }
}

pub(super) struct Terminal {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<Result<()>>>,
    console: Option<ConsoleState>,
    registration: Option<control::Registration>,
    controls: Option<mpsc::Receiver<Result<ClientControl>>>,
    tty: bool,
    finished: bool,
    failed: bool,
}
type Signals = Pin<Box<dyn Stream<Item = Result<ClientControl>> + Send>>;
impl Terminal {
    fn spawn(&mut self, name: &str, work: impl FnOnce(&Shared) -> Result<()> + Send + 'static) -> Result<()> {
        let shared = self.shared.clone();
        self.workers.push(
            std::thread::Builder::new()
                .name(name.into())
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&shared)))
                        .unwrap_or_else(|_| Err(anyhow::anyhow!("Windows terminal worker panicked")));
                    if result.is_err() {
                        shared.fail();
                    }
                    result
                })
                .context("cannot start Windows terminal worker")?,
        );
        Ok(())
    }
    pub(super) fn signals(&mut self) -> Result<Signals> {
        let receiver = self.controls.take().context("terminal controls already consumed")?;
        let mut streams: Vec<Signals> = vec![Box::pin(stream::unfold(receiver, |mut receiver| async move {
            receiver.recv().await.map(|control| (control, receiver))
        }))];
        if self.tty {
            let initial = dimensions()?;
            let interval = tokio::time::interval(Duration::from_millis(200));
            streams.push(Box::pin(stream::unfold(
                (interval, initial),
                |(mut interval, mut previous)| async move {
                    loop {
                        interval.tick().await;
                        match dimensions() {
                            Ok(size) if size == previous => {}
                            Ok(size) => {
                                previous = size;
                                return Some((Ok(ClientControl::Resize(size)), (interval, previous)));
                            }
                            Err(error) => return Some((Err(error), (interval, previous))),
                        }
                    }
                },
            )));
        }
        Ok(stream::select_all(streams).boxed())
    }
    pub(super) fn finish(&mut self) -> Result<()> {
        if self.finished {
            ensure!(!self.failed, "Windows terminal cleanup previously failed");
            return Ok(());
        }
        let mut errors = Vec::new();
        if let Some(registration) = &mut self.registration
            && let Err(error) = registration.seal()
        {
            errors.push(error);
        }
        self.shared.stop();
        // Repeat cancellation until actual completion, including registration races.
        while self.workers.iter().any(|worker| !worker.is_finished()) {
            for worker in self.workers.iter().filter(|worker| !worker.is_finished()) {
                unsafe {
                    CancelSynchronousIo(worker.as_raw_handle());
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        for worker in self.workers.drain(..) {
            match worker.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => errors.push(error),
                Err(_) => errors.push(anyhow::anyhow!("Windows terminal worker join failed")),
            }
        }
        if let Some(console) = &mut self.console
            && let Err(error) = console.restore()
        {
            errors.push(error);
        }
        // A close callback waits on this event. Signal before waiting for it.
        self.shared.restored.set();
        let mut count = lock(&self.shared.callbacks);
        while *count != 0 {
            count = self
                .shared
                .callbacks_done
                .wait(count)
                .unwrap_or_else(|error| error.into_inner());
        }
        drop(count);
        self.registration.take();
        self.finished = true;
        self.failed = !errors.is_empty();
        if !errors.is_empty() {
            let error = errors.remove(0);
            return Err(error.context(format!(
                "Windows terminal cleanup failed ({} additional failures)",
                errors.len()
            )));
        }
        Ok(())
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn send_input(sender: &mpsc::Sender<Input>, mut value: Input, shared: &Shared) -> Result<()> {
    while !shared.stopped() {
        match sender.try_send(value) {
            Ok(()) => return Ok(()),
            Err(mpsc::error::TrySendError::Closed(_)) => bail!("terminal input consumer stopped"),
            Err(mpsc::error::TrySendError::Full(returned)) => value = returned,
        }
        // No blocking queue send can prevent cancellation. Input consumption has
        // no native wake hook, so this one bounded pending chunk polls at 20ms.
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
fn read_input(file: File, sender: mpsc::Sender<Input>, shared: &Shared) -> Result<()> {
    let mut bytes = vec![0u8; MAX_DATA_BYTES];
    let mut units = vec![0u16; MAX_DATA_BYTES / 4];
    let mut high_surrogate = None;
    while !shared.stopped() {
        let size = if file.kind == Kind::Console {
            let count = file.read_console(&mut units, shared)?;
            if shared.stopped() {
                return Ok(());
            }
            if count == 0 {
                0
            } else {
                let text = decode_console(&units[..count], &mut high_surrogate)?;
                bytes[..text.len()].copy_from_slice(text.as_bytes());
                if text.is_empty() {
                    continue;
                }
                text.len()
            }
        } else {
            file.read(&mut bytes, shared)?
        };
        if shared.stopped() {
            return Ok(());
        }
        if size == 0 {
            ensure!(high_surrogate.is_none(), "incomplete console Unicode input");
            return send_input(&sender, Input::Eof, shared);
        }
        send_input(&sender, Input::Data(bytes[..size].to_vec()), shared)?;
    }
    Ok(())
}
fn decode_console(units: &[u16], high: &mut Option<u16>) -> Result<String> {
    let mut combined = Vec::with_capacity(units.len() + 1);
    combined.extend(high.take());
    combined.extend_from_slice(units);
    if combined.last().is_some_and(|unit| (0xd800..=0xdbff).contains(unit)) {
        *high = combined.pop();
    }
    String::from_utf16(&combined).context("invalid console Unicode input")
}
fn write_output(
    stdout: File,
    stderr: Option<File>,
    mut receiver: mpsc::Receiver<Output>,
    shared: &Shared,
) -> Result<()> {
    while !shared.stopped() {
        shared.wake.reset();
        match receiver.try_recv() {
            Ok(value) => {
                let file = match value.stream {
                    OutputStream::Stdout => &stdout,
                    OutputStream::Stderr => stderr.as_ref().context("terminal stderr is unavailable")?,
                    _ => bail!("invalid terminal output channel"),
                };
                let mut offset = 0;
                while offset < value.bytes.len() && !shared.stopped() {
                    let count = file.write(&value.bytes[offset..], shared)?;
                    if shared.stopped() {
                        return Ok(());
                    }
                    ensure!(count != 0, "terminal output closed");
                    offset += count;
                }
                if !shared.stopped() {
                    let _ = value.completed.send(Ok(()));
                }
            }
            Err(mpsc::error::TryRecvError::Empty) => shared.wake.wait(20),
            Err(mpsc::error::TryRecvError::Disconnected) => return Ok(()),
        }
    }
    Ok(())
}
