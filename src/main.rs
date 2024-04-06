use std::{io::Read, io::Write, sync::mpsc::channel, thread, time::Duration};

use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use termwiz::escape::{csi::Cursor, Action, CSI};

fn main() -> Result<()> {
    let nu_path = "/home/linuxbrew/.linuxbrew/bin/nu";
    let pty_system = native_pty_system();
    let size = crossterm::terminal::window_size()?;
    let pair = pty_system.openpty(PtySize {
        rows: size.rows,
        cols: size.columns,
        // FIXME: set realistic values for pixel_*
        pixel_width: size.width,
        pixel_height: size.height,
    })?;

    let command = CommandBuilder::new(nu_path);
    // command.arg("--log-level=info");
    let mut child = pair.slave.spawn_command(command)?;
    let mut killer = child.clone_killer();

    // This reads output (stderr and stdout multiplexed into 1 stream) from Nu
    let mut reader = pair.master.try_clone_reader()?;
    
    // This writes to Nu's stdin. We put it behind a channel because it doesn't implement Sync
    let mut stdin_writer = pair.master.take_writer()?;
    let (stdin_tx, stdin_rx) = channel::<Vec<u8>>();
    thread::spawn(move || -> Result<()> {
        for msg in stdin_rx {
            stdin_writer.write_all(&msg)?;
        }
        Ok(())
    });

    crossterm::terminal::enable_raw_mode()?;
    
    // watch the child's output, responding to escape codes and writing all output to disk
    let cloned_stdin_tx = stdin_tx.clone();
    thread::spawn(move || -> Result<()> {

        let mut parser = termwiz::escape::parser::Parser::new();
        let mut recording = std::fs::File::create("output.txt")?;
        let mut buf = [0u8; 8192];
        loop {
            let size = reader.read(&mut buf)?;
            if size == 0 {
                break;
            }
            let bytes = buf[0..size].to_vec();

            let actions = parser.parse_as_vec(&bytes);

            for action in actions {
                // we gotta respond to Query Cursor Position messages or Reedline will hang
                if matches!(action, Action::CSI(CSI::Cursor(Cursor::RequestActivePositionReport))) {
                    // response format is <ESC>[{ROW};{COLUMN}R
                    // hardcoding 20;20 for now
                    let cursor_position_msg = b"\x1B[20;20R".to_vec();
                    cloned_stdin_tx.send(cursor_position_msg)?;
                }
            }

            std::io::stdout().write_all(&bytes)?;
            recording.write_all(&bytes)?;
        }
        Ok(())
    });

    // Kill Nu after a few seconds in case it gets stuck
    thread::spawn(move || -> Result<()> {
        thread::sleep(Duration::from_secs(1));
        killer.kill()?;
        Ok(())
    });

    // Seems like we need to wait for the prompt to be ready before Nu will listen
    // TODO: figure out a way to remove this sleep
    thread::sleep(Duration::from_millis(100));

    // try to get Nu to run a command
    stdin_tx.send(b"lsb_release -a\r".to_vec())?;

    child.wait().unwrap();

    crossterm::terminal::disable_raw_mode()?;

    println!("Done!");

    Ok(())
}
