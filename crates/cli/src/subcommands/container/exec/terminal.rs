//! Standard descriptors share open-file descriptions with their duplicates.
//! Save all flags before changing any, and restore them only after the one I/O
//! worker has joined. Nonblocking writes are necessary even after select: another
//! writer can fill a pipe between readiness and write. Never use Tokio stdin's
//! uncancellable blocking worker here.
use super::session::{Input, Io, Output, OutputSender, OutputStream};
use anyhow::{bail, ensure, Context, Result};
use futures::{stream, Stream, StreamExt};
use rustix::{
    event::{fd_set_insert, fd_set_num_elements, select, FdSetElement, Timespec},
    fd::{AsRawFd, OwnedFd},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    io::{read, write, Errno},
    termios::{tcgetattr, tcgetpgrp, tcgetwinsize, tcsetattr, OptionalActions, Termios},
};
use spacetimedb_lib::container::exec::{ClientControl, TerminalSize, MAX_DATA_BYTES};
use std::{
    os::unix::net::UnixStream,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};
use tokio::sync::{mpsc, oneshot};

pub(super) fn dimensions() -> Result<TerminalSize> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    ensure!(
        tcgetpgrp(&stdin).ok() == Some(rustix::process::getpgrp())
            && tcgetpgrp(&stdout).ok() == Some(rustix::process::getpgrp()),
        "PTY requires a foreground-owned stdin and stdout terminal"
    );
    let size = tcgetwinsize(&stdout).context("cannot read terminal dimensions")?;
    let size = TerminalSize {
        rows: size.ws_row,
        columns: size.ws_col,
    };
    size.validate().context("invalid terminal dimensions")?;
    Ok(size)
}

type Signals = Pin<Box<dyn Stream<Item = Result<ClientControl>> + Send>>;
pub(super) fn signals(tty: bool) -> Result<Signals> {
    use tokio::signal::unix::{signal, SignalKind};
    let mappings = [
        (SignalKind::hangup(), 1),
        (SignalKind::interrupt(), 2),
        (SignalKind::quit(), 3),
        (SignalKind::terminate(), 15),
        (SignalKind::user_defined1(), 10),
        (SignalKind::user_defined2(), 12),
    ];
    let mut streams: Vec<Signals> = Vec::new();
    for (kind, linux) in mappings {
        let signal = signal(kind).context("cannot install exec signal handler")?;
        streams.push(Box::pin(stream::unfold(signal, move |mut signal| async move {
            signal.recv().await.map(|()| (Ok(ClientControl::Signal(linux)), signal))
        })));
    }
    if tty {
        let signal = signal(SignalKind::window_change()).context("cannot install terminal resize handler")?;
        streams.push(Box::pin(stream::unfold(signal, |mut signal| async move {
            signal
                .recv()
                .await
                .map(|()| (dimensions().map(ClientControl::Resize), signal))
        })));
    }
    Ok(stream::select_all(streams).boxed())
}

struct Descriptors {
    files: [OwnedFd; 3],
    flags: [OFlags; 3],
    terminal: Option<Termios>,
    restored: bool,
}
impl Descriptors {
    fn prepare(files: [OwnedFd; 3], tty: bool) -> Result<Self> {
        let flags = [
            fcntl_getfl(&files[0])?,
            fcntl_getfl(&files[1])?,
            fcntl_getfl(&files[2])?,
        ];
        let mut this = Self {
            files,
            flags,
            terminal: None,
            restored: false,
        };
        if tty {
            ensure!(
                tcgetpgrp(&this.files[0]).ok() == Some(rustix::process::getpgrp())
                    && tcgetpgrp(&this.files[1]).ok() == Some(rustix::process::getpgrp()),
                "PTY requires a foreground-owned terminal"
            );
            let original = tcgetattr(&this.files[0])?;
            let mut raw = original.clone();
            raw.make_raw();
            this.terminal = Some(original);
            tcsetattr(&this.files[0], OptionalActions::Now, &raw)?;
        }
        for (fd, flags) in this.files.iter().zip(this.flags) {
            fcntl_setfl(fd, flags | OFlags::NONBLOCK).context("cannot enable cancellable terminal I/O")?;
        }
        Ok(this)
    }
    fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        let mut failed = false;
        if let Some(terminal) = &self.terminal {
            failed |= tcsetattr(&self.files[0], OptionalActions::Now, terminal).is_err();
        }
        for (fd, flags) in self.files.iter().zip(self.flags) {
            failed |= fcntl_setfl(fd, flags).is_err();
        }
        self.restored = !failed;
        ensure!(!failed, "could not restore terminal settings or descriptor flags");
        Ok(())
    }
}
impl Drop for Descriptors {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

pub(super) struct Terminal {
    stop: Arc<AtomicBool>,
    wake: Arc<UnixStream>,
    worker: Option<JoinHandle<Result<()>>>,
}
impl Terminal {
    pub(super) fn stdio(stdin: bool, tty: bool) -> Result<(Self, Io)> {
        Self::open(
            [
                rustix::io::fcntl_dupfd_cloexec(std::io::stdin(), 3)?,
                rustix::io::fcntl_dupfd_cloexec(std::io::stdout(), 3)?,
                rustix::io::fcntl_dupfd_cloexec(std::io::stderr(), 3)?,
            ],
            stdin,
            tty,
        )
    }
    fn open(files: [OwnedFd; 3], stdin: bool, tty: bool) -> Result<(Self, Io)> {
        let mut descriptors = Descriptors::prepare(files, tty)?;
        let (wake, wake_reader) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        wake_reader.set_nonblocking(true)?;
        let wake = Arc::new(wake);
        let stop = Arc::new(AtomicBool::new(false));
        let (input_tx, input) = mpsc::channel(1);
        let (output_tx, output) = mpsc::channel(1);
        let (completed, completion) = oneshot::channel();
        let stopped = stop.clone();
        let worker = std::thread::Builder::new()
            .name("container-exec-terminal".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    pump(&descriptors.files, &wake_reader, stopped, stdin, input_tx, output)
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("terminal I/O worker panicked")));
                let restored = descriptors.restore();
                let result = match (result, restored) {
                    (Ok(()), Ok(())) => Ok(()),
                    (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
                    (Err(error), Err(restored)) => {
                        Err(error.context(format!("terminal restoration also failed: {restored}")))
                    }
                };
                let _ = completed.send(
                    result
                        .as_ref()
                        .map(|_| ())
                        .map_err(|_| anyhow::anyhow!("terminal I/O failed")),
                );
                result
            })
            .context("cannot start terminal worker")?;
        let notification = wake.clone();
        let output = OutputSender {
            sender: output_tx,
            wake: Some(Arc::new(move || {
                let _ = write(&*notification, &[1]);
            })),
        };
        Ok((
            Self {
                stop,
                wake,
                worker: Some(worker),
            },
            Io {
                input,
                output,
                completion,
            },
        ))
    }
    pub(super) fn finish(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        let _ = write(&*self.wake, &[1]);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("terminal worker join failed"))??;
        }
        Ok(())
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn pump(
    files: &[OwnedFd; 3],
    wake: &UnixStream,
    stop: Arc<AtomicBool>,
    mut stdin: bool,
    input: mpsc::Sender<Input>,
    mut output: mpsc::Receiver<Output>,
) -> Result<()> {
    let mut incoming = None;
    let mut outgoing: Option<(Output, usize)> = None;
    let mut bytes = vec![0; MAX_DATA_BYTES];
    while !stop.load(Ordering::Acquire) {
        if outgoing.is_none() {
            outgoing = output.try_recv().ok().map(|value| (value, 0));
        }
        let mut progress = false;
        if let Some((value, offset)) = &mut outgoing {
            let fd = match value.stream {
                OutputStream::Stdout => &files[1],
                OutputStream::Stderr => &files[2],
                _ => bail!("invalid terminal output channel"),
            };
            match write(fd, &value.bytes[*offset..]) {
                Ok(0) => bail!("terminal output closed"),
                Ok(count) => {
                    *offset += count;
                    progress = true;
                }
                Err(Errno::AGAIN | Errno::INTR) => {}
                Err(_) => bail!("terminal output failed"),
            }
            if *offset == value.bytes.len() {
                let (value, _) = outgoing.take().unwrap();
                let _ = value.completed.send(Ok(()));
            }
        }
        if stdin && incoming.is_none() {
            match read(&files[0], &mut bytes) {
                Ok(0) => {
                    incoming = Some(Input::Eof);
                    stdin = false;
                }
                Ok(count) => {
                    incoming = Some(Input::Data(bytes[..count].to_vec()));
                    progress = true;
                }
                Err(Errno::AGAIN | Errno::INTR) => {}
                Err(_) => bail!("terminal input failed"),
            }
        }
        if let Some(value) = incoming.take() {
            match input.try_send(value) {
                Ok(()) => progress = true,
                Err(mpsc::error::TrySendError::Full(value)) => incoming = Some(value),
                Err(mpsc::error::TrySendError::Closed(_)) => bail!("terminal input consumer stopped"),
            }
        }
        if progress {
            continue;
        }
        let write_fd = outgoing
            .as_ref()
            .map(|(value, _)| files[if value.stream == OutputStream::Stdout { 1 } else { 2 }].as_raw_fd());
        let read_fd = (stdin && incoming.is_none()).then(|| files[0].as_raw_fd());
        let max = [Some(wake.as_raw_fd()), read_fd, write_fd]
            .into_iter()
            .flatten()
            .max()
            .unwrap()
            + 1;
        let elements = fd_set_num_elements(max as usize, 3);
        let mut reads = vec![FdSetElement::default(); elements];
        let mut writes = reads.clone();
        fd_set_insert(&mut reads, wake.as_raw_fd());
        if let Some(fd) = read_fd {
            fd_set_insert(&mut reads, fd);
        }
        if let Some(fd) = write_fd {
            fd_set_insert(&mut writes, fd);
        }
        // select supports macOS terminals, unlike poll. Every registered FD is
        // owned for the worker's entire lifetime; the sets use its exact bound.
        let result = unsafe {
            select(
                max,
                Some(&mut reads),
                Some(&mut writes),
                None,
                Some(&Timespec {
                    tv_sec: 0,
                    tv_nsec: 20_000_000,
                }),
            )
        };
        if let Err(error) = result
            && error != Errno::INTR
        {
            bail!("terminal readiness failed");
        }
        let mut notifications = [0; 64];
        while read(wake, &mut notifications).is_ok_and(|count| count != 0) {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn cancelled_backpressured_terminal_joins_and_restores_shared_descriptor_flags() {
        let (input, _input_peer) = UnixStream::pair().unwrap();
        let (output, _output_peer) = UnixStream::pair().unwrap();
        output.set_nonblocking(true).unwrap();
        let block = [0; 8192];
        while write(&output, &block).is_ok() {}
        output.set_nonblocking(false).unwrap();
        let input_flags = fcntl_getfl(&input).unwrap();
        let output_flags = fcntl_getfl(&output).unwrap();
        let (mut terminal, io) = Terminal::open(
            [
                rustix::io::dup(&input).unwrap(),
                rustix::io::dup(&output).unwrap(),
                rustix::io::dup(&output).unwrap(),
            ],
            true,
            false,
        )
        .unwrap();
        assert!(fcntl_getfl(&output).unwrap().contains(OFlags::NONBLOCK));
        let (completed, mut written) = oneshot::channel();
        io.output
            .sender
            .send(Output {
                stream: OutputStream::Stdout,
                bytes: vec![42; MAX_DATA_BYTES],
                completed,
            })
            .await
            .unwrap();
        if let Some(wake) = &io.output.wake {
            wake();
        }
        assert!(tokio::time::timeout(Duration::from_millis(30), &mut written)
            .await
            .is_err());
        let started = Instant::now();
        terminal.finish().unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(terminal.worker.is_none());
        assert_eq!(fcntl_getfl(&input).unwrap(), input_flags);
        assert_eq!(fcntl_getfl(&output).unwrap(), output_flags);
        io.completion.await.unwrap().unwrap();
        assert!(written.await.is_err());
        terminal.finish().unwrap();
    }

    #[tokio::test]
    async fn terminal_drop_joins_idle_input_and_refuses_unowned_pty_without_flag_changes() {
        let (input, _input_peer) = UnixStream::pair().unwrap();
        let (output, _output_peer) = UnixStream::pair().unwrap();
        let flags = fcntl_getfl(&input).unwrap();
        let files = || {
            [
                rustix::io::dup(&input).unwrap(),
                rustix::io::dup(&output).unwrap(),
                rustix::io::dup(&output).unwrap(),
            ]
        };
        assert!(Terminal::open(files(), true, true).is_err());
        assert_eq!(fcntl_getfl(&input).unwrap(), flags);
        let (terminal, io) = Terminal::open(files(), true, false).unwrap();
        drop(terminal);
        io.completion.await.unwrap().unwrap();
        assert_eq!(fcntl_getfl(&input).unwrap(), flags);
        assert!(!fcntl_getfl(&output).unwrap().contains(OFlags::NONBLOCK));
    }
}

#[cfg(test)]
#[path = "terminal_pty_tests.rs"]
mod pty_tests;
