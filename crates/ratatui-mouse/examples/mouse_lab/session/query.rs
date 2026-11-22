//! Bounded raw replies, without a background reader competing with crossterm.
#[cfg(unix)]
use std::{
    io::{self, Read},
    os::fd::AsFd,
    time::{Duration, Instant},
};

pub const MODES: [u16; 8] = [9, 1000, 1002, 1003, 1005, 1006, 1015, 1016];

pub fn parse_mode_replies(bytes: &[u8]) -> Vec<(u16, u8)> {
    String::from_utf8_lossy(bytes)
        .split("\x1b[?")
        .skip(1)
        .filter_map(|suffix| {
            let (parameters, _) = suffix.split_once("$y")?;
            let (mode, state) = parameters.split_once(';')?;
            let mode = mode.parse().ok()?;
            let state = state.parse().ok()?;
            (MODES.contains(&mode) && state <= 4).then_some((mode, state))
        })
        .collect()
}

#[cfg(unix)]
pub fn collect_reply<R: Read + AsFd>(reader: &mut R, timeout: Duration) -> io::Result<Vec<u8>> {
    use rustix::event::{PollFd, PollFlags, Timespec, poll};
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        if remaining.is_zero() {
            break;
        }
        let timeout = Timespec::try_from(remaining).map_err(io::Error::other)?;
        let mut descriptors = [PollFd::new(&*reader, PollFlags::IN)];
        if poll(&mut descriptors, Some(&timeout))? == 0 {
            break;
        }
        let mut buffer = [0; 1024];
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => bytes.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
        let replies = parse_mode_replies(&bytes);
        if MODES
            .iter()
            .all(|mode| replies.iter().any(|(replied, _)| replied == mode))
        {
            break;
        }
        // A noisy or malicious terminal cannot make this observation unbounded.
        if bytes.len() >= 16_384 {
            break;
        }
    }
    Ok(bytes)
}

#[cfg(unix)]
pub fn read_terminal_replies(timeout: Duration) -> io::Result<Vec<u8>> {
    use rustix::fs::{Mode, OFlags, open};
    let descriptor = open(
        "/dev/tty",
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let mut terminal = std::fs::File::from(descriptor);
    collect_reply(&mut terminal, timeout)
}
