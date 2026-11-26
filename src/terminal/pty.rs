use portable_pty::{Child, CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, sync_channel};
use std::thread;

/// Chunks (64 KiB each) the reader may run ahead of the UI: about 4 MiB.
const PTY_CHUNKS: usize = 64;

pub struct Pty {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    /// ConPTY keeps its output pipe open after the child exits, so EOF alone
    /// cannot signal termination on Windows. Polling the child covers both OSes.
    child: Box<dyn Child + Send + Sync>,
    pub rx: Receiver<Vec<u8>>,
}

impl Pty {
    pub fn spawn(cols: u16, rows: u16, shell: &str, cwd: Option<&str>) -> anyhow::Result<Self> {
        let pair = native_pty_system().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(shell);
        // Le shell interattive e molti programmi si aspettano TERM impostato.
        if cmd.get_env("TERM").is_none() {
            cmd.env("TERM", "xterm-256color");
        }

        // A new pane or tab starts where the previous one was, when that
        // directory exists here (a path reported over SSH usually does not).
        if let Some(dir) = cwd.filter(|d| std::path::Path::new(d).is_dir()) {
            cmd.cwd(dir);
        }
        super::integration::apply(&mut cmd, shell);

        let child = pair.slave.spawn_command(cmd)?;
        // On Windows ConPTY, keeping the slave open in the parent may prevent EOF.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        // Bounded: when the UI falls behind, the reader blocks and the child
        // is slowed by the pty instead of buffering unbounded output.
        let (tx, rx) = sync_channel(PTY_CHUNKS);
        thread::spawn(move || {
            // Large buffer: fewer syscalls and channel messages.
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(Self {
            writer,
            master: pair.master,
            child,
            rx,
        })
    }

    /// The shell's exit status once it has terminated.
    pub fn exit_status(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    pub fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()
    }

    /// Updates the PTY winsize: the kernel sends SIGWINCH to the shell.
    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[::core::prelude::v1::test]
    fn reports_the_exit_of_cmd_exe() {
        let mut pty = Pty::spawn(80, 24, "cmd.exe", None).unwrap();
        assert!(pty.exit_status().is_none());
        thread::sleep(Duration::from_millis(500)); // let cmd.exe show its prompt
        pty.write(b"exit\r\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut out = Vec::new();
        while Instant::now() < deadline && pty.exit_status().is_none() {
            if let Ok(chunk) = pty.rx.recv_timeout(Duration::from_millis(20)) {
                out.extend_from_slice(&chunk);
                // ConPTY asks for the cursor position before showing the prompt.
                if chunk.windows(4).any(|w| w == b"\x1b[6n") {
                    pty.write(b"\x1b[1;1R").unwrap();
                }
            }
        }
        assert!(
            pty.exit_status().is_some_and(|status| status.success()),
            "cmd.exe exit was not detected; output: {:?}",
            String::from_utf8_lossy(&out)
        );
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Everything the shell prints until `needle` shows up (or 5 s pass).
    fn read_until(pty: &Pty, needle: &str) -> String {
        let (mut out, deadline) = (String::new(), Instant::now() + Duration::from_secs(5));
        while Instant::now() < deadline && !out.contains(needle) {
            if let Ok(chunk) = pty.rx.recv_timeout(Duration::from_millis(100)) {
                out.push_str(&String::from_utf8_lossy(&chunk));
            }
        }
        out
    }

    #[::core::prelude::v1::test]
    fn a_new_shell_starts_in_the_given_directory() {
        let dir = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let mut pty = Pty::spawn(80, 24, "/bin/sh", Some(dir.to_str().unwrap())).unwrap();
        pty.write(b"echo cwd=$(pwd)\n").unwrap();
        let out = read_until(&pty, &format!("cwd={}", dir.display()));
        assert!(
            out.contains(&format!("cwd={}", dir.display())),
            "got: {out:?}"
        );
        // A missing directory is ignored instead of failing the spawn.
        assert!(Pty::spawn(80, 24, "/bin/sh", Some("/no/such/dir")).is_ok());
    }

    #[::core::prelude::v1::test]
    fn the_pty_channel_closes_when_the_shell_exits() {
        let mut pty = Pty::spawn(80, 24, "/bin/sh", None).unwrap();
        pty.write(b"exit\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match pty.rx.recv_timeout(Duration::from_millis(100)) {
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                _ if Instant::now() > deadline => panic!("channel never closed"),
                _ => {}
            }
        }
    }
}
