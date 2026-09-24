//! A child session owns its own controlling PTY; the test runner's terminal and
//! process group are never modified. The parent retains and waits every child.
use super::*;
use rustix::{
    fs::{open, Mode},
    pty::{grantpt, openpt, ptsname, unlockpt, OpenptFlags},
    termios::{tcsetwinsize, LocalModes, Winsize},
};
use std::{
    fs::File,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct ChildOwner(Child);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn foreground_pty_restores_after_normal_error_and_cancel() {
    for mode in ["normal", "error", "cancel"] {
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
        rustix::io::fcntl_setfd(&master, rustix::io::FdFlags::CLOEXEC).unwrap();
        grantpt(&master).unwrap();
        unlockpt(&master).unwrap();
        let path = ptsname(&master, Vec::new()).unwrap();
        let slave = open(&path, OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC, Mode::empty()).unwrap();
        tcsetwinsize(
            &slave,
            Winsize {
                ws_row: 48,
                ws_col: 120,
                ws_xpixel: 0,
                ws_ypixel: 0,
            },
        )
        .unwrap();
        // Prime Darwin's kernel FWASWRITTEN state before the snapshot;
        // the child test harness writes to this same open-file description.
        assert_eq!(write(&slave, b"owned PTY fixture\n").unwrap(), 18);
        let before = configuration(tcgetattr(&slave).unwrap());
        let flags = fcntl_getfl(&slave).unwrap();
        let root = tempfile::tempdir().unwrap();
        let file: File = rustix::io::dup(&slave).unwrap().into();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "subcommands::container::execute::terminal::pty_tests::foreground_pty_child",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_clear()
            .env("SPACETIMEDB_EXEC_PTY_CHILD", mode)
            .env("TMPDIR", root.path())
            .current_dir(root.path())
            .stdin(Stdio::from(file.try_clone().unwrap()))
            .stdout(Stdio::from(file.try_clone().unwrap()))
            .stderr(Stdio::from(file))
            .spawn()
            .unwrap();
        let mut child = ChildOwner(child);
        fcntl_setfl(&master, fcntl_getfl(&master).unwrap() | OFlags::NONBLOCK).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut captured = Vec::new();
        let mut verified = false;
        let mut buffer = [0; 4096];
        let status = loop {
            while let Ok(count) = read(&master, &mut buffer) {
                if count == 0 {
                    break;
                }
                captured.extend_from_slice(&buffer[..count]);
                assert!(captured.len() < 64 * 1024);
            }
            if !verified
                && captured
                    .windows(b"PTY_RESTORED".len())
                    .any(|text| text == b"PTY_RESTORED")
            {
                assert_eq!(configuration(tcgetattr(&slave).unwrap()), before, "mode {mode}");
                assert_eq!(fcntl_getfl(&slave).unwrap(), flags, "mode {mode}");
                assert_eq!(write(&master, b"y\n").unwrap(), 2);
                verified = true;
            }
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "owned PTY child timed out");
            std::thread::sleep(Duration::from_millis(5));
        };
        // Keep the session leader alive for the independent slave inspection:
        // macOS revokes that slave when the controlling session exits.
        // Then positively wait, even after try_wait.
        child.0.wait().unwrap();
        assert!(status.success(), "mode {mode}: {}", String::from_utf8_lossy(&captured));
        assert!(verified, "mode {mode} did not acknowledge restored state");
        assert_eq!(fcntl_getfl(&slave).unwrap(), flags, "mode {mode}");
    }
}

#[tokio::test]
#[ignore = "owned child of foreground_pty_restores_after_normal_error_and_cancel"]
async fn foreground_pty_child() {
    let mode = std::env::var("SPACETIMEDB_EXEC_PTY_CHILD").expect("requires owned PTY parent");
    assert!(["normal", "error", "cancel"].contains(&mode.as_str()));
    rustix::process::setsid().unwrap();
    rustix::process::ioctl_tiocsctty(std::io::stdin()).unwrap();
    rustix::termios::tcsetpgrp(std::io::stdin(), rustix::process::getpgrp()).unwrap();
    assert_eq!(dimensions().unwrap(), TerminalSize { rows: 48, columns: 120 });
    let before = configuration(tcgetattr(std::io::stdin()).unwrap());
    let flags = fcntl_getfl(std::io::stdin()).unwrap();
    let (mut terminal, io) = Terminal::stdio(true, true).unwrap();
    assert!(!tcgetattr(std::io::stdin())
        .unwrap()
        .local_modes
        .intersects(LocalModes::ICANON | LocalModes::ECHO));
    match mode.as_str() {
        "normal" => terminal.finish().unwrap(),
        "cancel" => drop(terminal),
        "error" => {
            let (completed, _written) = oneshot::channel();
            io.output
                .sender
                .send(Output {
                    stream: OutputStream::Stdin,
                    bytes: vec![1],
                    completed,
                })
                .await
                .unwrap();
            if let Some(wake) = &io.output.wake {
                wake();
            }
            assert!(io.completion.await.unwrap().is_err());
            assert!(terminal.finish().is_err());
        }
        _ => unreachable!(),
    }
    assert_eq!(configuration(tcgetattr(std::io::stdin()).unwrap()), before);
    assert_eq!(fcntl_getfl(std::io::stdin()).unwrap(), flags);
    use std::io::{Read, Write};
    println!("PTY_RESTORED");
    std::io::stdout().flush().unwrap();
    let mut ack = [0];
    std::io::stdin().read_exact(&mut ack).unwrap();
    assert_eq!(ack, [b'y']);
}

fn configuration(mut termios: Termios) -> String {
    // PENDIN is queued-input state, not a configured terminal mode. macOS
    // sets it when ICANON is restored, even for an empty queue. Do not flush
    // user input or change production restoration to erase this kernel state.
    // https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/tty.c
    termios.local_modes.remove(LocalModes::PENDIN);
    format!("{termios:?}")
}
