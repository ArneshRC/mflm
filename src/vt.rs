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

pub enum Event {
    Byte(u8),
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
    pub fn next_byte(&mut self) -> Option<u8> {
        if self.pending.is_empty() && !self.fill() {
            return None;
        }
        self.pending.pop_front()
    }

    pub fn next_event(&mut self) -> Event {
        if let Some(b) = self.pending.pop_front() {
            return Event::Byte(b);
        }
        let wake_rd = match self.wake_rd {
            Some(fd) => fd,
            None => {
                return self.next_byte().map_or(Event::Eof, Event::Byte)
            }
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
            return self.next_byte().map_or(Event::Eof, Event::Byte);
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
