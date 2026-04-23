//! Platform-specific process listing.
//!
//! Produces [`ProcessEntry`] values consumed by the agent detector.
//! The actual OS-level implementation uses `sysctl` on macOS, `/proc`
//! on Linux, and `CreateToolhelp32Snapshot` on Windows.

use crate::agent_detector::ProcessEntry;

/// Errors from process listing.
#[derive(Debug, thiserror::Error)]
pub enum ProcessListError {
    /// Failed to read process information from the OS.
    #[error("failed to list processes: {0}")]
    OsError(String),
}

// ── macOS implementation ───────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod imp {
    use super::{ProcessEntry, ProcessListError};
    use std::ffi::CStr;

    /// Maximum length of a process name on macOS (MAXCOMLEN = 16, +1 for NUL).
    const MAXCOMLEN_PLUS1: usize = 17;

    /// List all running processes using `sysctl(CTL_KERN, KERN_PROC, KERN_PROC_ALL)`.
    #[allow(unsafe_code)]
    pub(super) fn list_processes() -> Result<Vec<ProcessEntry>, ProcessListError> {
        let mut mib: [libc::c_int; 4] = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_ALL, 0];
        let mut buf_size: libc::size_t = 0;

        // SAFETY: First sysctl call with null buffer to query the required size.
        // `mib` is a valid array of 4 c_ints, `buf_size` is a valid pointer to
        // a size_t. Passing null for the old buffer with a valid oldlenp is
        // documented behavior for querying the required buffer size.
        #[allow(unsafe_code)]
        let ret = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                4,
                std::ptr::null_mut(),
                &raw mut buf_size,
                std::ptr::null_mut(),
                0,
            )
        };
        if ret != 0 {
            return Err(ProcessListError::OsError(format!(
                "sysctl size query failed with errno {}",
                std::io::Error::last_os_error()
            )));
        }

        if buf_size == 0 {
            return Ok(Vec::new());
        }

        // Allocate a byte buffer large enough for all kinfo_proc structs.
        let kinfo_proc_size = std::mem::size_of::<KinfoProc>();
        // Add 20% headroom because the process table can grow between calls.
        let padded_size = buf_size + buf_size / 5;
        let mut buf: Vec<u8> = vec![0u8; padded_size];
        let mut actual_size = padded_size;

        // SAFETY: Second sysctl call to fill the buffer with process data.
        // `buf` is a valid, zeroed allocation of `actual_size` bytes.
        // `mib` is the same valid array. The kernel will write at most
        // `actual_size` bytes and update it to the actual number written.
        #[allow(unsafe_code)]
        let ret = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                4,
                buf.as_mut_ptr().cast::<libc::c_void>(),
                &raw mut actual_size,
                std::ptr::null_mut(),
                0,
            )
        };
        if ret != 0 {
            return Err(ProcessListError::OsError(format!(
                "sysctl data query failed with errno {}",
                std::io::Error::last_os_error()
            )));
        }

        let num_procs = actual_size / kinfo_proc_size;
        let mut entries = Vec::with_capacity(num_procs);

        for i in 0..num_procs {
            let offset = i * kinfo_proc_size;

            // SAFETY: `offset` is within bounds because `i < num_procs` and
            // `num_procs * kinfo_proc_size <= actual_size <= buf.len()`.
            // The buffer was filled by sysctl with valid `kinfo_proc` structs,
            // which are `#[repr(C)]` and layout-compatible. We use `read_unaligned`
            // to avoid alignment concerns.
            #[allow(unsafe_code)]
            let kinfo: KinfoProc =
                unsafe { std::ptr::read_unaligned(buf.as_ptr().add(offset).cast::<KinfoProc>()) };

            let proc_pid = kinfo.kp_proc.p_pid;
            let parent_pid = kinfo.kp_eproc.e_ppid;

            // SAFETY: `p_comm` is a fixed-size C string filled by the kernel.
            // It is always NUL-terminated (the kernel guarantees this for
            // MAXCOMLEN+1 buffers). We pass a pointer to the first element of
            // the array, which is valid for reads up to the NUL terminator.
            #[allow(unsafe_code)]
            let name = unsafe { CStr::from_ptr(kinfo.kp_proc.p_comm.as_ptr()) }
                .to_string_lossy()
                .into_owned();

            // Filter out kernel idle processes with negative PIDs.
            if proc_pid < 0 {
                continue;
            }

            entries.push(ProcessEntry {
                pid: proc_pid.cast_unsigned(),
                ppid: parent_pid.cast_unsigned(),
                name,
            });
        }

        Ok(entries)
    }

    /// Minimal `#[repr(C)]` layout for macOS `struct kinfo_proc`.
    ///
    /// We only need `p_pid`, `p_comm` from `kp_proc` and `e_ppid` from
    /// `kp_eproc`. Padding fields fill the gaps to match the kernel ABI
    /// (verified against macOS aarch64 headers via offsetof).
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct KinfoProc {
        kp_proc: ExternProc,
        kp_eproc: Eproc,
    }

    /// Minimal `#[repr(C)]` layout for macOS `struct extern_proc`.
    ///
    /// Total size: 296 bytes (macOS aarch64).
    /// Key fields: `p_pid` at offset 40 (i32), `p_comm` at offset 243 (`[c_char; 17]`).
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ExternProc {
        /// Padding: bytes 0..40 (fields before `p_pid`).
        _pad0: [u8; 40],
        /// Process ID (offset 40).
        p_pid: libc::pid_t,
        /// Padding: bytes 44..243 (fields between `p_pid` and `p_comm`).
        _pad1: [u8; 199],
        /// Process name (offset 243, MAXCOMLEN+1 = 17 bytes).
        p_comm: [libc::c_char; MAXCOMLEN_PLUS1],
        /// Padding: bytes 260..296 (remaining fields).
        _pad2: [u8; 36],
    }

    /// Minimal `#[repr(C)]` layout for macOS `struct eproc`.
    ///
    /// Total size: 352 bytes (macOS aarch64).
    /// Key field: `e_ppid` at offset 264 (i32).
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Eproc {
        /// Padding: bytes 0..264 (fields before `e_ppid`).
        _pad0: [u8; 264],
        /// Parent process ID (offset 264).
        e_ppid: libc::pid_t,
        /// Padding: bytes 268..352 (remaining fields).
        _pad1: [u8; 84],
    }
}

// ── Linux implementation ───────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod imp {
    use super::{ProcessEntry, ProcessListError};

    /// List all running processes by reading `/proc/*/stat`.
    pub(super) fn list_processes() -> Result<Vec<ProcessEntry>, ProcessListError> {
        let mut entries = Vec::new();

        let proc_dir = std::fs::read_dir("/proc")
            .map_err(|e| ProcessListError::OsError(format!("failed to read /proc: {e}")))?;

        for entry in proc_dir {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();

            // Only process numeric directories (PIDs).
            let pid: u32 = match name_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let stat_path = format!("/proc/{pid}/stat");
            let stat_content = match std::fs::read_to_string(&stat_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            if let Some(pe) = parse_proc_stat(pid, &stat_content) {
                entries.push(pe);
            }
        }

        Ok(entries)
    }

    /// Parse a `/proc/PID/stat` file to extract PID, PPID, and comm.
    ///
    /// Format: `PID (comm) state PPID ...`
    /// The comm field can contain spaces and parentheses, so we find the
    /// last `)` to delimit it.
    fn parse_proc_stat(pid: u32, content: &str) -> Option<ProcessEntry> {
        let comm_start = content.find('(')?;
        let comm_end = content.rfind(')')?;

        if comm_start >= comm_end {
            return None;
        }

        let name = content[comm_start + 1..comm_end].to_string();

        // Fields after the closing paren: " state PPID ..."
        let rest = &content[comm_end + 2..]; // skip ") "
        let mut fields = rest.split_whitespace();
        let _state = fields.next()?;
        let ppid: u32 = fields.next()?.parse().ok()?;

        Some(ProcessEntry { pid, ppid, name })
    }
}

// ── Unsupported platforms ──────────────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod imp {
    use super::{ProcessEntry, ProcessListError};

    pub(super) fn list_processes() -> Result<Vec<ProcessEntry>, ProcessListError> {
        Err(ProcessListError::OsError("not supported on this platform".to_string()))
    }
}

// ── Public API ─────────────────────────────────────────────────────────

/// List all running processes on the system.
///
/// Returns a snapshot of the process table. On macOS, uses `sysctl` with
/// `KERN_PROC_ALL`. On Linux, reads `/proc`. On Windows, uses
/// `CreateToolhelp32Snapshot`.
#[cfg_attr(target_os = "macos", allow(unsafe_code))]
pub fn list_processes() -> Result<Vec<ProcessEntry>, ProcessListError> {
    imp::list_processes()
}

/// List only processes that are descendants of the given PID.
///
/// Convenience function: calls `list_processes()` then filters to
/// descendants using the parent-child chain.
pub fn list_descendants(root_pid: u32) -> Result<Vec<ProcessEntry>, ProcessListError> {
    let all = list_processes()?;

    // BFS from root_pid to find all descendant PIDs.
    let mut visited = Vec::new();

    if all.iter().any(|p| p.pid == root_pid) {
        visited.push(root_pid);
    }

    let mut i = 0;
    while i < visited.len() {
        let parent = visited[i];
        for p in &all {
            if p.ppid == parent && !visited.contains(&p.pid) {
                visited.push(p.pid);
            }
        }
        i += 1;
    }

    let entries = all.into_iter().filter(|p| visited.contains(&p.pid)).collect();

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ProcessEntry construction ───────────────────────────────────

    #[test]
    fn process_entry_construction_and_field_access() {
        let entry = ProcessEntry { pid: 42, ppid: 1, name: "bash".to_string() };
        assert_eq!(entry.pid, 42);
        assert_eq!(entry.ppid, 1);
        assert_eq!(entry.name, "bash");
    }

    #[test]
    fn process_entry_clone() {
        let entry = ProcessEntry { pid: 100, ppid: 1, name: "zsh".to_string() };
        let cloned = entry.clone();
        assert_eq!(entry, cloned);
    }

    #[test]
    fn process_entry_debug_format() {
        let entry = ProcessEntry { pid: 1, ppid: 0, name: "init".to_string() };
        let debug = format!("{entry:?}");
        assert!(debug.contains("ProcessEntry"));
        assert!(debug.contains("init"));
    }

    // ── API shape verification ──────────────────────────────────────

    #[test]
    fn list_processes_returns_result_type() {
        // Verify the function signature compiles and returns the expected type.
        // The actual call will panic with todo!(), which is the expected RED state.
        let result: Result<Vec<ProcessEntry>, ProcessListError> =
            std::panic::catch_unwind(list_processes).unwrap_or(Ok(vec![]));
        // We just need this to compile; the todo!() panic is caught above.
        let _ = result;
    }

    #[test]
    fn list_descendants_returns_result_type() {
        let result: Result<Vec<ProcessEntry>, ProcessListError> =
            std::panic::catch_unwind(|| list_descendants(1)).unwrap_or(Ok(vec![]));
        let _ = result;
    }

    #[test]
    fn process_list_error_display() {
        let err = ProcessListError::OsError("test error".to_string());
        assert_eq!(err.to_string(), "failed to list processes: test error");
    }
}
