//! Services the file descriptors a plugin registered with `posix-fd` (X11 plugin GUIs pump their
//! event loop through them) on the control thread. Polled without waiting on each worker tick.

#![allow(unsafe_code)]

use super::worker::Ctx;
use clack_extensions::posix_fd::{FdFlags, PluginPosixFd};
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use std::os::fd::{BorrowedFd, RawFd};

fn wanted(flags: FdFlags) -> PollFlags {
    let mut out = PollFlags::empty();
    if flags.contains(FdFlags::READ) {
        out |= PollFlags::IN;
    }
    if flags.contains(FdFlags::WRITE) {
        out |= PollFlags::OUT;
    }
    out
}

fn reported(events: PollFlags) -> FdFlags {
    let mut out = FdFlags::empty();
    if events.contains(PollFlags::IN) {
        out |= FdFlags::READ;
    }
    if events.contains(PollFlags::OUT) {
        out |= FdFlags::WRITE;
    }
    if events.intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL) {
        out |= FdFlags::ERROR;
    }
    out
}

/// Polls the registered descriptors once and calls the plugin's `on_fd` for each that is ready.
pub(super) fn poll_ready(ctx: &mut Ctx) {
    let registered: Vec<(RawFd, FdFlags)> = ctx.instance.access_handler(|h| h.fds.borrow().clone());
    if registered.is_empty() {
        return;
    }
    let ready = ready_fds(&registered);
    if ready.is_empty() {
        return;
    }
    let handle = ctx.instance.plugin_handle();
    if let Some(ext) = handle.get_extension::<PluginPosixFd>() {
        for (fd, flags) in ready {
            ext.on_fd(&handle, fd, flags);
        }
    }
}

fn ready_fds(registered: &[(RawFd, FdFlags)]) -> Vec<(RawFd, FdFlags)> {
    // SAFETY: each descriptor was registered by the plugin through `register_fd` and stays open until
    // it calls `unregister_fd`, which only runs on this thread between ticks; the borrow ends before
    // this function returns.
    let borrowed: Vec<BorrowedFd<'_>> = registered
        .iter()
        .map(|&(fd, _)| unsafe { BorrowedFd::borrow_raw(fd) })
        .collect();
    let mut polls: Vec<PollFd<'_>> = borrowed
        .iter()
        .zip(registered)
        .map(|(fd, &(_, flags))| PollFd::new(fd, wanted(flags)))
        .collect();
    let now = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    match poll(&mut polls, Some(&now)) {
        Ok(0) | Err(_) => Vec::new(),
        Ok(_) => registered
            .iter()
            .zip(&polls)
            .map(|(&(fd, _), p)| (fd, reported(p.revents())))
            .filter(|(_, flags)| !flags.is_empty())
            .collect(),
    }
}
