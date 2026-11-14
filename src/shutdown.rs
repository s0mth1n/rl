//! Wake the event loop on SIGINT or SIGTERM without doing I/O cleanup in a signal handler.
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicI32, Ordering};

static WRITE_FD: AtomicI32 = AtomicI32::new(-1);

extern "C" fn notify(_: libc::c_int) {
    let fd = WRITE_FD.load(Ordering::Relaxed);
    if fd >= 0 {
        // write is async-signal-safe; a full pipe already wakes the event loop.
        unsafe {
            libc::write(fd, [1u8].as_ptr().cast(), 1);
        }
    }
}

pub struct Shutdown {
    reader: OwnedFd,
    _writer: OwnedFd,
    previous: [libc::sigaction; 2],
}

impl Shutdown {
    pub fn new() -> std::io::Result<Self> {
        let mut pipe = [-1; 2];
        if unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let reader = unsafe { OwnedFd::from_raw_fd(pipe[0]) };
        let writer = unsafe { OwnedFd::from_raw_fd(pipe[1]) };
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = notify as *const () as usize;
        unsafe { libc::sigemptyset(&mut action.sa_mask) };
        let mut previous = unsafe { std::mem::zeroed::<[libc::sigaction; 2]>() };
        WRITE_FD.store(writer.as_raw_fd(), Ordering::Relaxed);
        for (index, signal) in [libc::SIGINT, libc::SIGTERM].into_iter().enumerate() {
            if unsafe { libc::sigaction(signal, &action, &mut previous[index]) } < 0 {
                let error = std::io::Error::last_os_error();
                WRITE_FD.store(-1, Ordering::Relaxed);
                if index == 1 {
                    unsafe { libc::sigaction(libc::SIGINT, &previous[0], std::ptr::null_mut()) };
                }
                return Err(error);
            }
        }
        Ok(Self {
            reader,
            _writer: writer,
            previous,
        })
    }

    pub fn fd(&self) -> i32 {
        self.reader.as_raw_fd()
    }
}

impl Drop for Shutdown {
    fn drop(&mut self) {
        WRITE_FD.store(-1, Ordering::Relaxed);
        for (signal, action) in [libc::SIGINT, libc::SIGTERM]
            .into_iter()
            .zip(&self.previous)
        {
            unsafe { libc::sigaction(signal, action, std::ptr::null_mut()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn termination_signals_wake_the_event_loop() {
        let shutdown = Shutdown::new().unwrap();
        for signal in [libc::SIGINT, libc::SIGTERM] {
            assert_eq!(unsafe { libc::raise(signal) }, 0);
            let mut poll = libc::pollfd {
                fd: shutdown.fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            assert_eq!(unsafe { libc::poll(&mut poll, 1, 100) }, 1);
            assert_eq!(poll.revents, libc::POLLIN);
            let mut byte = [0u8];
            assert_eq!(
                unsafe { libc::read(shutdown.fd(), byte.as_mut_ptr().cast(), 1) },
                1
            );
            assert_eq!(byte, [1]);
        }
    }
}
