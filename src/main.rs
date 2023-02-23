use std::{thread, time::Duration, io::Read, io::Write};

use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

fn main() -> Result<()> {
    let nu_path = "/home/reilly/bin/nu";
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            // Not all systems support pixel_width, pixel_height,
            // but it is good practice to set it to something
            // that matches the size of the selected font.  That
            // is more complex than can be shown here in this
            // brief example though!
            pixel_width: 0,
            pixel_height: 0,
        })?;

    let command = CommandBuilder::new(nu_path);
    let mut child = pair.slave.spawn_command(command)?;
    let mut killer = child.clone_killer();
    
    // Read and print output from the pty
    // TODO: how does `reader` relate to the nu process's stdout and stderr? Are they multiplexed into 1 stream?
    let reader = pair.master.try_clone_reader()?;
    let mut stdout = std::io::stdout();
    std::thread::spawn(move || {
        for b in reader.bytes() {
            let b = b.unwrap();
            stdout.write(&[b]).unwrap();
        }
    });
    
    // kill Nu after 1s to avoid getting stuck
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        eprintln!("Goodbye!");
        killer.kill().unwrap();
    });
    
    // try to get Nu to run the `version` command
    let mut writer = pair.master.take_writer()?;    // print out everything from Nu
    writeln!(writer, "version").unwrap();

    child.wait().unwrap();
    
    Ok(())
}
