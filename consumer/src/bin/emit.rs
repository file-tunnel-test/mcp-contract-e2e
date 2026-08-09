use std::env;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

fn parse_count(value: Option<String>, name: &str) -> usize {
    value
        .unwrap_or_else(|| panic!("missing {name}"))
        .parse::<usize>()
        .unwrap_or_else(|error| panic!("invalid {name}: {error}"))
}

fn write_repeated(mut writer: impl Write, byte: u8, mut remaining: usize) -> io::Result<()> {
    let chunk = [byte; 4096];
    while remaining > 0 {
        let count = remaining.min(chunk.len());
        writer.write_all(&chunk[..count])?;
        remaining -= count;
    }
    writer.flush()
}

fn main() -> io::Result<()> {
    let mut arguments = env::args().skip(1);
    let stdout_bytes = parse_count(arguments.next(), "stdout byte count");
    let stderr_bytes = parse_count(arguments.next(), "stderr byte count");
    let sleep_millis = parse_count(arguments.next(), "sleep milliseconds");
    assert!(arguments.next().is_none(), "unexpected extra argument");

    write_repeated(io::stdout().lock(), b'o', stdout_bytes)?;
    write_repeated(io::stderr().lock(), b'e', stderr_bytes)?;

    if sleep_millis > 0 {
        thread::sleep(Duration::from_millis(sleep_millis as u64));
    }
    Ok(())
}
