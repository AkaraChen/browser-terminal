use std::{
    env,
    io::{self, Read, Write},
    sync::mpsc as std_mpsc,
    thread,
};

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tokio::sync::mpsc;

pub(crate) struct PtyProcess {
    pub(crate) control: PtyControl,
    pub(crate) reader: Box<dyn Read + Send>,
    pub(crate) writer: Box<dyn Write + Send>,
}

pub(crate) struct PtyControl {
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl PtyProcess {
    pub(crate) fn spawn() -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut command = CommandBuilder::new(shell);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        if let Some(home) = dirs::home_dir() {
            command.cwd(home);
        }

        let child = pair
            .slave
            .spawn_command(command)
            .context("failed to spawn shell")?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        Ok(Self {
            control: PtyControl {
                master: pair.master,
                child,
            },
            reader,
            writer,
        })
    }
}

impl PtyControl {
    pub(crate) fn resize(&mut self, size: PtySize) -> Result<()> {
        self.master.resize(size).context("failed to resize pty")
    }

    pub(crate) fn kill_child(&mut self) -> Result<()> {
        self.child.kill().context("failed to kill child process")
    }
}

pub(crate) fn spawn_pty_reader(mut reader: Box<dyn Read + Send>, output_tx: mpsc::Sender<Vec<u8>>) {
    thread::spawn(move || {
        let mut buf = [0_u8; 8192];

        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if output_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
    });
}

pub(crate) fn spawn_pty_writer(
    mut writer: Box<dyn Write + Send>,
    input_rx: std_mpsc::Receiver<Vec<u8>>,
) {
    thread::spawn(move || {
        while let Ok(input) = input_rx.recv() {
            if writer.write_all(&input).is_err() {
                break;
            }
            let _ = writer.flush();
        }
    });
}
