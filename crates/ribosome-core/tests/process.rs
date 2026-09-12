#![cfg(unix)]

use std::{
    collections::BTreeMap,
    os::unix::process::CommandExt,
    process::Command,
    time::{Duration, Instant},
};

#[test]
#[ignore = "subprocess fixture for inherited pipe cleanup"]
#[expect(
    clippy::zombie_processes,
    reason = "This fixture must exit before its child; the outer test kills the reported PID, which the OS then reaps."
)]
fn escaped_pipe_holder() {
    let child = Command::new("/bin/sleep")
        .arg("3")
        .process_group(0)
        .spawn()
        .unwrap();
    println!("pipe-holder-pid={}", child.id());
}

#[test]
fn inherited_pipes_do_not_extend_command_completion() {
    let directory = tempfile::tempdir().unwrap();
    let start = Instant::now();
    let result = ribosome_core::process::execute(
        &std::env::current_exe().unwrap(),
        &[
            "--ignored".into(),
            "--exact".into(),
            "escaped_pipe_holder".into(),
            "--nocapture".into(),
        ],
        directory.path(),
        Some(vec![0; 1_000_000]),
        &BTreeMap::new(),
        Duration::from_millis(500),
        None,
    )
    .unwrap();
    let elapsed = start.elapsed();
    // The fixture deliberately escaped the command's group. Clean up only its
    // reported process; process groups are not an OS boundary for hostile code.
    let pid: i32 = result
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("pipe-holder-pid="))
        .unwrap()
        .parse()
        .unwrap();
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
    assert!(result.success, "{}", result.stderr);
    assert!(
        elapsed < Duration::from_secs(2),
        "inherited pipe delayed completion by {elapsed:?}"
    );
}
