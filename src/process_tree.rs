use anyhow::{Context, Result};
use std::os::unix::net::UnixStream;
use sysinfo::Pid;
use sysinfo::System;

#[cfg(target_os = "macos")]
const SOL_LOCAL: libc::c_int = 0;
#[cfg(target_os = "macos")]
const LOCAL_PEERPID: libc::c_int = 0x002;

pub fn client_is_descendant(stream: &UnixStream, ancestor_pid: u32) -> Result<bool> {
    let peer_id = get_peer_id(stream)?;
    Ok(is_process_descendant(peer_id, ancestor_pid))
}

fn get_peer_id(stream: &UnixStream) -> Result<u32> {
    #[cfg(target_os = "linux")]
    {
        get_peer_id_linux(stream)
    }

    #[cfg(target_os = "macos")]
    {
        get_peer_id_macos(stream)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        compile_error!("Unsupported OS")
    }
}

#[cfg(target_os = "macos")]
fn get_peer_id_macos(stream: &UnixStream) -> Result<u32> {
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
    Ok(pid as u32)
}

#[cfg(target_os = "linux")]
pub(crate) fn get_peer_id_linux(stream: &UnixStream) -> Result<u32> {
    use anyhow::anyhow;

    let (peer_pid, _, _) =
        unix_cred::get_peer_pid_ids(stream).context("Failed to get peer credentials")?;
    Ok(peer_pid
        .ok_or_else(|| anyhow!("Could not get peer pid"))?
        .try_into()
        .unwrap())
}

macro_rules! unwrapOrReturnFalse {
    ($v:expr) => {
        match $v {
            Some(v) => v,
            _ => {
                return false;
            }
        }
    };
}

fn is_process_descendant(child_pid: u32, ancestor_pid: u32) -> bool {
    use sysinfo::ProcessesToUpdate;

    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let mut current_pid = Pid::from(child_pid as usize);

    loop {
        if current_pid.as_u32() == ancestor_pid {
            return true;
        }

        let process = unwrapOrReturnFalse!(system.process(current_pid));
        let parent_pid = unwrapOrReturnFalse!(process.parent());
        if parent_pid.as_u32() <= 1 {
            return false;
        }
        current_pid = parent_pid;
    }
}
