//! Resident memory from `/proc/self/status` (design D3): no `unsafe`, no
//! libc, Linux only. Elsewhere every reading is `None`, reported as
//! "unavailable".

use std::fs;

/// One reading of the process's memory, in KiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    /// Resident set now (`VmRSS`).
    pub rss_kib: Option<u64>,
    /// Peak resident set since the process started (`VmHWM`).
    pub hwm_kib: Option<u64>,
}

impl Memory {
    /// Reads the current process's memory.
    pub fn now() -> Self {
        let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
        Self {
            rss_kib: field(&status, "VmRSS:"),
            hwm_kib: field(&status, "VmHWM:"),
        }
    }
}

/// Parses a `Name:   1234 kB` line of `/proc/<pid>/status`.
fn field(status: &str, name: &str) -> Option<u64> {
    let line = status.lines().find(|l| l.starts_with(name))?;
    let mut words = line[name.len()..].split_whitespace();
    let value = words.next()?.parse().ok()?;
    (words.next() == Some("kB")).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_lines() {
        let status = "Name:\tspike\nVmHWM:\t  204800 kB\nVmRSS:\t   10240 kB\n";
        assert_eq!(field(status, "VmHWM:"), Some(204_800));
        assert_eq!(field(status, "VmRSS:"), Some(10_240));
        assert_eq!(field(status, "VmSwap:"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reads_this_process() {
        let memory = Memory::now();
        assert!(memory.rss_kib.is_some_and(|rss| rss > 0));
        assert!(memory.hwm_kib >= memory.rss_kib);
    }
}
