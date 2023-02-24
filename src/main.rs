use std::{io::Read, io::Write, sync::mpsc::channel, thread, time::Duration};

use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

fn main() -> Result<()> {
    let nu_path = "/home/reilly/bin/nu";
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        // FIXME: set realistic values for pixel_*
        pixel_width: 0,
        pixel_height: 0,
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
    
    // watch the child's output, responding to escape codes and writing all output to disk
    let cloned_stdin_tx = stdin_tx.clone();
    thread::spawn(move || -> Result<()> {
        let mut recording = std::fs::File::create("output.txt")?;
        let mut buf = [0u8; 8192];
        loop {
            let size = reader.read(&mut buf)?;
            if size == 0 {
                break;
            }
            let bytes = buf[0..size].to_vec();
            const QUERY_CURSOR_POSITION: &[u8] = "[6n".as_bytes();

            // https://stackoverflow.com/a/35907071/854694
            fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
                haystack
                    .windows(needle.len())
                    .position(|window| window == needle)
            }

            // we gotta respond to Query Cursor Position messages or Reedline will hang
            // FIXME: an escape sequence *could* be split across multiple reads... handle that someday
            if find_subsequence(&bytes, QUERY_CURSOR_POSITION).is_some() {
                // response format is <ESC>[{ROW};{COLUMN}R
                // hardcoding 20;20 for now
                let cursor_position_msg = b"\x1B[20;20R".to_vec();
                cloned_stdin_tx.send(cursor_position_msg)?;
            }
            recording.write_all(&bytes)?;
        }
        Ok(())
    });

    // Kill Nu after a few seconds in case it gets stuck
    thread::spawn(move || -> Result<()> {
        thread::sleep(Duration::from_secs(2));
        killer.kill()?;
        Ok(())
    });

    // Seems like we need to wait for the prompt to be ready before Nu will listen
    // TODO: figure out a way to remove this sleep
    thread::sleep(Duration::from_millis(100));
    
    // try to get Nu to run a command
    stdin_tx.send("lsb_release -a\r\n".as_bytes().to_vec())?;

    child.wait().unwrap();

    Ok(())
}
