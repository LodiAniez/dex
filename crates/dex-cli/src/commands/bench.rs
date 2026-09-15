//! Hidden `dex bench-ping`: one connect, one request line, one response line
//! over a named pipe. `scripts/bench-hooks.ps1` times it to measure what a hook
//! round trip costs (M0). The real client, with the handshake, replaces it in M4.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};

use anyhow::{Context, bail};

/// Sends one request to `\\.\pipe\<pipe>` and waits for one response line.
pub fn ping(pipe: &str) -> anyhow::Result<()> {
    // A Windows named pipe opens like a file, so the synchronous std API is
    // enough here: no async runtime to start, which keeps hook latency down.
    let path = format!(r"\\.\pipe\{pipe}");
    let mut conn = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("cannot open {path}"))?;

    conn.write_all(b"{\"id\":\"bench\",\"cmd\":\"bench.ping\",\"args\":{}}\n")
        .context("cannot send request")?;

    let mut line = String::new();
    BufReader::new(&conn)
        .read_line(&mut line)
        .context("cannot read response")?;
    if line.is_empty() {
        bail!("server closed the pipe without responding");
    }
    Ok(())
}
