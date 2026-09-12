use crate::error::{Error, Result};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub struct ProcessOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub cancelled: bool,
    pub elapsed_ms: u64,
}

pub fn execute(
    program: &Path,
    args: &[String],
    workspace: &Path,
    input: Option<Vec<u8>>,
    environment: &BTreeMap<String, String>,
    timeout: Duration,
    cancellation: Option<&std::sync::atomic::AtomicBool>,
) -> Result<ProcessOutput> {
    if cancellation.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::SeqCst)) {
        return Err(Error::denied("command cancelled before dispatch"));
    }
    if timeout.is_zero() {
        return Err(Error::exhausted("command deadline reached"));
    }
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(workspace)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .envs(environment)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    let stdin = child.stdin.take();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    #[cfg(unix)]
    if let Err(error) = (|| {
        nonblocking(&stdout)?;
        nonblocking(&stderr)?;
        if let Some(stdin) = &stdin {
            nonblocking(stdin)?;
        }
        Ok::<_, std::io::Error>(())
    })() {
        terminate(&mut child);
        return Err(error.into());
    }
    let stopped = Arc::new(AtomicBool::new(false));
    let input_writer = input.map(|input| {
        let stopped = stopped.clone();
        std::thread::spawn(move || write_input(stdin.unwrap(), &input, &stopped))
    });
    let output_stop = stopped.clone();
    let error_stop = stopped.clone();
    let output_reader = std::thread::spawn(move || drain(stdout, &output_stop));
    let error_reader = std::thread::spawn(move || drain(stderr, &error_stop));
    let started = Instant::now();
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok((status.success(), false)),
            Ok(None) => {}
            Err(error) => break Err(error),
        }
        if cancellation.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::SeqCst)) {
            break Ok((false, false));
        }
        if started.elapsed() >= timeout {
            break Ok((false, true));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    terminate(&mut child);
    // A descendant can retain a pipe after the leader has exited or leave its
    // group entirely. Pipe readers/writers must not extend the host deadline.
    stopped.store(true, Ordering::SeqCst);
    if let Some(writer) = input_writer {
        let _ = writer.join();
    }
    let stdout = output_reader
        .join()
        .map_err(|_| Error::internal("stdout reader panicked"))??;
    let stderr = error_reader
        .join()
        .map_err(|_| Error::internal("stderr reader panicked"))??;
    let (success, timed_out) = outcome?;
    Ok(ProcessOutput {
        success: success && !timed_out,
        cancelled: cancellation.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::SeqCst)),
        stdout,
        stderr,
        timed_out,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

fn terminate(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
    // Recheck after reaping the leader to catch a fork that was in progress
    // when the first group signal was sent.
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
}

#[cfg(unix)]
fn nonblocking(pipe: &impl std::os::fd::AsRawFd) -> std::io::Result<()> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

fn write_input(
    mut pipe: impl Write,
    mut input: &[u8],
    stopped: &AtomicBool,
) -> std::io::Result<()> {
    while !input.is_empty() && !stopped.load(Ordering::SeqCst) {
        match pipe.write(input) {
            Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
            Ok(n) => input = &input[n..],
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn drain(mut pipe: impl Read, stopped: &AtomicBool) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        if stopped.load(Ordering::SeqCst) && bytes.len() >= 24000 {
            break;
        }
        let n = match pipe.read(&mut buffer) {
            Ok(n) => n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if stopped.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(error) => return Err(error),
        };
        if n == 0 {
            break;
        }
        let keep = n.min(24000_usize.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..keep]);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
