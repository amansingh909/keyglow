//! Asks the kernel for a short time slice, so keyglowd gets a CPU soon after it wakes up even
//! while other programs keep every CPU busy.
//!
//! Sending one frame wakes keyglowd about twenty times inside the keyboard driver. While the
//! computer is busy, each of those wake-ups waits for the running program to use up its slice
//! (about 3 ms), so frames and key presses come late. Nice levels don't help: they only rank
//! processes inside one cgroup, and system76-scheduler sets its own anyway. A task with a
//! shorter slice may go first when it wakes (Linux 6.12 and later). It needs no privileges and
//! takes no CPU time from anything else: it changes when keyglowd runs, not how much.

use nix::libc;
use std::io;

/// The shortest slice the kernel allows, 0.1 ms. keyglowd's work after each wake-up is shorter.
pub const SLICE_NS: u64 = 100_000;

/// `struct sched_attr` from the kernel's uapi headers, up to the utilization clamps.
#[repr(C)]
#[derive(Default)]
struct SchedAttr {
    size: u32,
    sched_policy: u32,
    sched_flags: u64,
    sched_nice: i32,
    sched_priority: u32,
    sched_runtime: u64,
    sched_deadline: u64,
    sched_period: u64,
    sched_util_min: u32,
    sched_util_max: u32,
}

fn get() -> io::Result<SchedAttr> {
    let mut attr = SchedAttr::default();
    let size = std::mem::size_of::<SchedAttr>() as libc::c_uint;
    // SAFETY: the kernel writes at most `size` bytes into `attr`, which is that big.
    let r = unsafe { libc::syscall(libc::SYS_sched_getattr, 0, &mut attr as *mut SchedAttr, size, 0) };
    if r == 0 {
        Ok(attr)
    } else {
        Err(io::Error::last_os_error())
    }
}

/// The calling thread's slice in nanoseconds, as the kernel reports it.
pub fn current() -> io::Result<u64> {
    get().map(|a| a.sched_runtime)
}

/// Asks for a `SLICE_NS` slice for the calling thread (keyglowd has only one), keeping its
/// scheduling policy and nice level. Returns whether the kernel now reports that slice: older
/// kernels accept the call and ignore it. Only normal tasks are changed.
pub fn request() -> io::Result<bool> {
    let mut attr = get()?;
    if attr.sched_policy != libc::SCHED_OTHER as u32 {
        return Ok(false);
    }
    attr.size = std::mem::size_of::<SchedAttr>() as u32;
    attr.sched_flags = 0;
    attr.sched_runtime = SLICE_NS;
    // SAFETY: the kernel reads `attr.size` bytes from `attr`, which is that big.
    let r = unsafe { libc::syscall(libc::SYS_sched_setattr, 0, &attr as *const SchedAttr, 0) };
    if r != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(current()? == SLICE_NS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_for_a_short_slice_and_keeps_the_nice_level() {
        // A thread of its own, so the slice doesn't stay on a thread other tests run on.
        std::thread::spawn(|| {
            let before = get().unwrap();
            assert!(request().unwrap(), "this kernel ignored the slice (it needs Linux 6.12 or later)");
            let after = get().unwrap();
            assert_eq!(after.sched_runtime, SLICE_NS);
            assert_eq!(after.sched_policy, before.sched_policy);
            assert_eq!(after.sched_nice, before.sched_nice);
            // Asking again is fine.
            assert!(request().unwrap());
        })
        .join()
        .unwrap();
    }
}
