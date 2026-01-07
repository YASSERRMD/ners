//! CPU Affinity for NERS Phase 2
//!
//! Provides cross-platform CPU core pinning.

use std::io;

/// Pin the current thread to a specific CPU core
#[cfg(target_os = "linux")]
pub fn pin_to_core(core_id: usize) -> io::Result<()> {
    use nix::sched::{sched_setaffinity, CpuSet};
    use nix::unistd::Pid;
    
    let mut cpu_set = CpuSet::new();
    cpu_set.set(core_id).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    
    sched_setaffinity(Pid::from_raw(0), &cpu_set)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    
    Ok(())
}

/// Pin the current thread to a specific CPU core (macOS - no-op for now)
#[cfg(target_os = "macos")]
pub fn pin_to_core(core_id: usize) -> io::Result<()> {
    // macOS doesn't support traditional CPU affinity
    // Thread affinity tags are available but require different API
    log::debug!("CPU affinity not supported on macOS, thread {} requested core {}", 
                std::thread::current().name().unwrap_or("unnamed"), core_id);
    Ok(())
}

/// Get the number of available CPU cores
pub fn available_cores() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_cores() {
        let cores = available_cores();
        assert!(cores >= 1);
    }

    #[test]
    fn test_pin_to_core() {
        // Should not panic
        let _ = pin_to_core(0);
    }
}
