use anyhow::Result;
use std::os::unix::net::UnixStream;
use sysinfo::Pid;
use sysinfo::System;

// macOS-specific constants for LOCAL_PEERPID
#[cfg(target_os = "macos")]
const SOL_LOCAL: libc::c_int = 0; // Protocol level for local sockets
#[cfg(target_os = "macos")]
const LOCAL_PEERPID: libc::c_int = 0x002; // Get peer PID

pub(crate) fn verify_client_is_descendant(stream: &UnixStream, ancestor_pid: u32) -> Result<()> {
    // Option 1: Using unix-cred to get peer PID (cross-platform)
    #[cfg(target_os = "linux")]
    {
        verify_client_is_descendant_linux(stream, ancestor_pid)
    }

    #[cfg(target_os = "macos")]
    {
        verify_client_is_descendant_macos(stream, ancestor_pid)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        compile_error!("Unsupported OS")
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn verify_client_is_descendant_macos(
    stream: &UnixStream,
    ancestor_pid: u32,
) -> Result<(), anyhow::Error> {
    use std::mem;
    use std::os::unix::io::AsRawFd;

    let mut pid: libc::pid_t = 0;
    let mut len = mem::size_of::<libc::pid_t>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            SOL_LOCAL,
            LOCAL_PEERPID,
            &mut pid as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if result != 0 {
        anyhow::bail!(
            "Failed to get peer PID: {}",
            std::io::Error::last_os_error()
        );
    }
    if is_process_descendant(pid as u32, ancestor_pid)? {
        return Ok(());
    }
    anyhow::bail!("Client process is not a descendant of the server");
}

#[cfg(target_os = "linux")]
pub(crate) fn verify_client_is_descendant_linux(
    stream: &UnixStream,
    ancestor_pid: u32,
) -> Result<(), anyhow::Error> {
    let (peer_pid, _, _) =
        unix_cred::get_peer_pid_ids(stream).context("Failed to get peer credentials")?;
    if let Some(pid) = peer_pid {
        if is_process_descendant(pid as u32, ancestor_pid)? {
            return Ok(());
        }
    }
    anyhow::bail!("Client process is not a descendant of the server");
}

macro_rules! unwrapOrReturnFalse {
    ($v:expr) => {
        match $v {
            Some(v) => v,
            _ => {
                return Ok(false);
            }
        }
    };
}

pub(crate) fn is_process_descendant(child_pid: u32, ancestor_pid: u32) -> Result<bool> {
    use sysinfo::ProcessesToUpdate;

    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let mut current_pid = Pid::from(child_pid as usize);

    loop {
        if current_pid.as_u32() == ancestor_pid {
            return Ok(true);
        }

        let process = unwrapOrReturnFalse!(system.process(current_pid));
        let parent_pid = unwrapOrReturnFalse!(process.parent());
        if parent_pid.as_u32() <= 1 {
            return Ok(false);
        }
        current_pid = parent_pid;
    }
}
