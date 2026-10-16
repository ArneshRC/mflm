// The kernel ignores VT switches while a console is in KD_GRAPHICS and
// VT_AUTO mode, so we take over switching with VT_PROCESS: the kernel signals
// us on release/acquire and we acknowledge. On acquire the screen must be
// redrawn, so the handler wakes the input loop through a self-pipe.

use std::{
    collections::VecDeque,
    os::unix::io::RawFd,
    sync::atomic::{AtomicI32, Ordering}
};

use log::{info, warn};
use nix::{
    errno::Errno,
    fcntl::OFlag,
    libc,
    poll::{poll, PollFd, PollFlags},
    sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal},
    unistd
};

// from linux/vt.h
const VT_SETMODE: libc::c_ulong = 0x5602;
const VT_RELDISP: libc::c_ulong = 0x5605;
const VT_AUTO: libc::c_char = 0;
const VT_PROCESS: libc::c_char = 1;
const VT_ACKACQ: libc::c_int = 2;

const TTY_FD: RawFd = 0;

// How long to wait for the rest of an escape sequence before treating a lone
// ESC byte as the Esc key.
const ESC_TIMEOUT_MS: i32 = 25;

#[repr(C)]
struct VtMode {
    mode: libc::c_char,
    waitv: libc::c_char,
    relsig: libc::c_short,
    acqsig: libc::c_short,
    frsig: libc::c_short
}

static WAKE_FD: AtomicI32 = AtomicI32::new(-1);

extern "C" fn on_vt_signal(sig: libc::c_int) {
    // only async-signal-safe calls in here
    unsafe {
        if sig == libc::SIGUSR1 {
            libc::ioctl(TTY_FD, VT_RELDISP as _, 1 as libc::c_int);
        } else if sig == libc::SIGUSR2 {
            libc::ioctl(TTY_FD, VT_RELDISP as _, VT_ACKACQ);
            let fd = WAKE_FD.load(Ordering::Relaxed);
            if fd >= 0 {
                libc::write(fd, b"a".as_ptr().cast(), 1);
            }
        }
    }
}

fn set_vt_mode(mode: libc::c_char) -> nix::Result<()> {
    let vt_mode = VtMode {
        mode,
        waitv: 0,
        relsig: if mode == VT_PROCESS { libc::SIGUSR1 as _ } else { 0 },
        acqsig: if mode == VT_PROCESS { libc::SIGUSR2 as _ } else { 0 },
        frsig: 0
    };
    let res = unsafe { libc::ioctl(TTY_FD, VT_SETMODE as _, &vt_mode) };
    Errno::result(res).map(drop)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    // Ctrl+letter, as the lowercase letter
    Ctrl(char),
    Enter,
    Tab,
    Backspace,
    Esc,
    Up,
    Down,
    Left,
    Right,
    F(u8),
    Unknown
}

// Decodes one key starting at `first`. `next` yields the following bytes of
// an escape sequence, or None if none arrive in time.
fn decode(first: u8, mut next: impl FnMut() -> Option<u8>) -> Key {
    match first {
        b'\r' => Key::Enter,
        b'\t' => Key::Tab,
        0x7F => Key::Backspace,
        0x1B => match next() {
            None => Key::Esc,
            Some(b'[') => decode_csi(next),
            // xterm-style F1-F4
            Some(b'O') => match next() {
                Some(b @ b'P'..=b'S') => Key::F(b - b'P' + 1),
                _ => Key::Unknown
            },
            // Alt+key
            Some(_) => Key::Unknown
        },
        0x01..=0x1A => Key::Ctrl((first + 0x60) as char),
        0x00..=0x1F => Key::Unknown,
        b => Key::Char(b as char)
    }
}

fn decode_csi(mut next: impl FnMut() -> Option<u8>) -> Key {
    let mut num: Option<u32> = None;
    loop {
        match next() {
            // Linux console F1-F5: ESC [ [ A..E
            Some(b'[') if num.is_none() => {
                return match next() {
                    Some(b @ b'A'..=b'E') => Key::F(b - b'A' + 1),
                    _ => Key::Unknown
                };
            }
            Some(b @ b'0'..=b'9') => {
                let d = (b - b'0') as u32;
                num = Some(num.unwrap_or(0).saturating_mul(10).saturating_add(d));
            }
            // modifier parameters, ignored
            Some(b';') => {}
            Some(b'A') => return Key::Up,
            Some(b'B') => return Key::Down,
            Some(b'C') => return Key::Right,
            Some(b'D') => return Key::Left,
            Some(b'~') => {
                return match num {
                    Some(n @ 11..=15) => Key::F((n - 10) as u8),
                    Some(n @ 17..=21) => Key::F((n - 11) as u8),
                    Some(n @ 23..=24) => Key::F((n - 12) as u8),
                    _ => Key::Unknown
                };
            }
            // any other final byte ends an unrecognized sequence
            Some(0x40..=0x7E) | None => return Key::Unknown,
            Some(_) => {}
        }
    }
}

pub enum Event {
    Key(Key),
    // we got the VT back and must repaint everything
    Redraw,
    Eof
}

pub struct Tty {
    wake_rd: Option<RawFd>,
    pending: VecDeque<u8>
}

impl Tty {
    pub fn new() -> Self {
        let mut tty = Tty {
            wake_rd: None,
            pending: VecDeque::new()
        };
        match tty.enable_vt_switching() {
            Ok(()) => info!("VT switching enabled"),
            Err(e) => warn!("Unable to enable VT switching: {e}")
        }
        tty
    }

    fn enable_vt_switching(&mut self) -> nix::Result<()> {
        let (rd, wr) = unistd::pipe2(OFlag::O_NONBLOCK | OFlag::O_CLOEXEC)?;
        WAKE_FD.store(wr, Ordering::Relaxed);
        self.wake_rd = Some(rd);

        let action = SigAction::new(
            SigHandler::Handler(on_vt_signal),
            SaFlags::SA_RESTART,
            SigSet::empty()
        );
        unsafe {
            sigaction(Signal::SIGUSR1, &action)?;
            sigaction(Signal::SIGUSR2, &action)?;
        }
        set_vt_mode(VT_PROCESS)
    }

    fn fill(&mut self) -> bool {
        let mut buf = [0u8; 64];
        loop {
            match unistd::read(TTY_FD, &mut buf) {
                Ok(0) => return false,
                Ok(n) => {
                    self.pending.extend(&buf[..n]);
                    return true;
                }
                Err(Errno::EINTR) => continue,
                Err(e) => {
                    warn!("Failed to read from tty: {e}");
                    return false;
                }
            }
        }
    }

    // next input byte, ignoring redraw requests (they stay queued in the pipe)
    fn next_byte(&mut self) -> Option<u8> {
        if self.pending.is_empty() && !self.fill() {
            return None;
        }
        self.pending.pop_front()
    }

    // like next_byte, but gives up if nothing arrives within timeout_ms
    fn next_byte_timeout(&mut self, timeout_ms: i32) -> Option<u8> {
        if self.pending.is_empty() {
            let mut fds = [PollFd::new(TTY_FD, PollFlags::POLLIN)];
            loop {
                match poll(&mut fds, timeout_ms) {
                    Ok(0) => return None,
                    Ok(_) => break,
                    Err(Errno::EINTR) => continue,
                    Err(_) => return None
                }
            }
            if !self.fill() {
                return None;
            }
        }
        self.pending.pop_front()
    }

    fn next_key(&mut self) -> Event {
        match self.next_byte() {
            Some(b) => Event::Key(decode(b, || {
                self.next_byte_timeout(ESC_TIMEOUT_MS)
            })),
            None => Event::Eof
        }
    }

    pub fn next_event(&mut self) -> Event {
        if !self.pending.is_empty() {
            return self.next_key();
        }
        let wake_rd = match self.wake_rd {
            Some(fd) => fd,
            None => return self.next_key()
        };

        let mut fds = [
            PollFd::new(TTY_FD, PollFlags::POLLIN),
            PollFd::new(wake_rd, PollFlags::POLLIN)
        ];
        loop {
            match poll(&mut fds, -1) {
                Ok(_) => break,
                Err(Errno::EINTR) => continue,
                Err(e) => {
                    warn!("Failed to poll tty: {e}");
                    return Event::Eof;
                }
            }
        }

        let ready = |fd: &PollFd| {
            fd.revents().is_some_and(|r| !r.is_empty())
        };
        if ready(&fds[1]) {
            let mut buf = [0u8; 16];
            while let Ok(n) = unistd::read(wake_rd, &mut buf) {
                if n == 0 {
                    break;
                }
            }
            return Event::Redraw;
        }
        if ready(&fds[0]) {
            return self.next_key();
        }
        Event::Redraw
    }
}

impl Drop for Tty {
    fn drop(&mut self) {
        if let Some(rd) = self.wake_rd.take() {
            if let Err(e) = set_vt_mode(VT_AUTO) {
                warn!("Failed to restore VT_AUTO mode: {e}");
            }
            let wr = WAKE_FD.swap(-1, Ordering::Relaxed);
            let _ = unistd::close(wr);
            let _ = unistd::close(rd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(bytes: &[u8]) -> Key {
        let mut rest = bytes[1..].iter().copied();
        decode(bytes[0], || rest.next())
    }

    #[test]
    fn plain_keys() {
        assert_eq!(key(b"a"), Key::Char('a'));
        assert_eq!(key(b"\r"), Key::Enter);
        assert_eq!(key(b"\t"), Key::Tab);
        assert_eq!(key(b"\x7f"), Key::Backspace);
        assert_eq!(key(b"\x15"), Key::Ctrl('u'));
        assert_eq!(key(b"\x03"), Key::Ctrl('c'));
    }

    #[test]
    fn lone_esc() {
        assert_eq!(key(b"\x1b"), Key::Esc);
    }

    #[test]
    fn arrows() {
        assert_eq!(key(b"\x1b[A"), Key::Up);
        assert_eq!(key(b"\x1b[B"), Key::Down);
        assert_eq!(key(b"\x1b[C"), Key::Right);
        assert_eq!(key(b"\x1b[D"), Key::Left);
        assert_eq!(key(b"\x1b[1;5C"), Key::Right);
    }

    #[test]
    fn linux_console_fkeys() {
        assert_eq!(key(b"\x1b[[A"), Key::F(1));
        assert_eq!(key(b"\x1b[[E"), Key::F(5));
        assert_eq!(key(b"\x1b[17~"), Key::F(6));
        assert_eq!(key(b"\x1b[21~"), Key::F(10));
        assert_eq!(key(b"\x1b[23~"), Key::F(11));
        assert_eq!(key(b"\x1b[24~"), Key::F(12));
    }

    #[test]
    fn xterm_fkeys() {
        assert_eq!(key(b"\x1bOP"), Key::F(1));
        assert_eq!(key(b"\x1bOS"), Key::F(4));
        assert_eq!(key(b"\x1b[15~"), Key::F(5));
    }

    #[test]
    fn unknown_sequences() {
        assert_eq!(key(b"\x1b[3~"), Key::Unknown);
        assert_eq!(key(b"\x1b[H"), Key::Unknown);
        assert_eq!(key(b"\x1bx"), Key::Unknown);
        assert_eq!(key(b"\x1b["), Key::Unknown);
    }
}
