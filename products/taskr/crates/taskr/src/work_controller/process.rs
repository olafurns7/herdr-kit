//! Bounded subprocess capture. Every exit path cleans up the owned process group.
use sha2::{Digest, Sha256};
use std::{
    io::{self, Read},
    os::unix::process::{CommandExt, ExitStatusExt},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use taskr_core::store::work::OutputDigest;

pub(super) const LIMIT: usize = 1024 * 1024;
#[derive(Default)]
pub(super) struct Capture {
    pub bytes: Vec<u8>,
    total: u64,
    hash: Sha256,
}
impl Capture {
    fn add(&mut self, bytes: &[u8]) {
        self.total += bytes.len() as u64;
        self.hash.update(bytes);
        self.bytes
            .extend_from_slice(&bytes[..bytes.len().min(LIMIT - self.bytes.len())]);
    }
    pub fn digest(&self) -> OutputDigest {
        OutputDigest {
            sha256: format!("{:x}", self.hash.clone().finalize()),
            bytes: self.total,
            truncated: self.total > self.bytes.len() as u64,
        }
    }
}
pub(super) struct Output {
    pub exit: Option<i32>,
    pub signal: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub stdout: Capture,
    pub stderr: Capture,
}
struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        taskr_core::store::terminate_group(self.0.id());
        let _ = self.0.wait();
    }
}
fn nonblock(fd: &impl std::os::fd::AsFd) -> io::Result<()> {
    let flags = rustix::fs::fcntl_getfl(fd)?;
    rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK)?;
    Ok(())
}
fn drain(reader: &mut impl Read, out: &mut Capture) -> io::Result<bool> {
    let mut buf = [0; 8192];
    // A producer that never yields cannot starve the deadline/cancellation checks.
    for _ in 0..16 {
        match reader.read(&mut buf) {
            Ok(0) => return Ok(true),
            Ok(n) => out.add(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(false)
}
pub(super) fn run(mut cmd: Command, timeout: Duration, cancel: &AtomicBool) -> io::Result<Output> {
    let mut child = Owned(
        cmd.process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let mut stdout = child.0.stdout.take().expect("stdout pipe");
    let mut stderr = child.0.stderr.take().expect("stderr pipe");
    nonblock(&stdout)?;
    nonblock(&stderr)?;
    let start = Instant::now();
    let mut out = Output {
        exit: None,
        signal: None,
        timed_out: false,
        cancelled: false,
        stdout: Capture::default(),
        stderr: Capture::default(),
    };
    loop {
        let a = drain(&mut stdout, &mut out.stdout)?;
        let b = drain(&mut stderr, &mut out.stderr)?;
        out.cancelled = cancel.load(Ordering::SeqCst);
        out.timed_out = start.elapsed() >= timeout;
        if out.cancelled || out.timed_out {
            taskr_core::store::terminate_group(child.0.id());
            let status = child.0.wait()?;
            out.exit = status.code();
            out.signal = status.signal();
            // Drain only bytes already in the pipes after killing the whole group.
            for _ in 0..16 {
                if drain(&mut stdout, &mut out.stdout)? && drain(&mut stderr, &mut out.stderr)? {
                    break;
                }
            }
            return Ok(out);
        }
        if let Some(status) = child.0.try_wait()? {
            out.exit = status.code();
            out.signal = status.signal();
            // Kill descendants remaining in this group, even if they closed its pipes.
            taskr_core::store::terminate_group(child.0.id());
            let until = Instant::now() + Duration::from_secs(1);
            let (mut a, mut b) = (a, b);
            while !(a && b) && Instant::now() < until {
                a = drain(&mut stdout, &mut out.stdout)?;
                b = drain(&mut stderr, &mut out.stderr)?;
            }
            if !(a && b) {
                return Err(io::Error::other(
                    "check pipes did not close after group cleanup",
                ));
            }
            return Ok(out);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
