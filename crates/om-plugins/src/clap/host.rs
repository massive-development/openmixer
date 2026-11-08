//! The host side of the CLAP calls: what the plugin may ask of us. Callbacks only set flags or
//! record requests; the worker (control thread) acts on them, and nothing here blocks or allocates
//! on the audio thread.

use clack_extensions::gui::{GuiSize, HostGui, HostGuiImpl};
use clack_extensions::latency::{HostLatency, HostLatencyImpl};
use clack_extensions::log::{HostLog, HostLogImpl, LogSeverity};
use clack_extensions::params::{
    HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags, ParamRescanFlags,
};
use clack_extensions::posix_fd::{FdFlags, HostPosixFd, HostPosixFdImpl};
use clack_extensions::tail::{HostTail, HostTailImpl};
use clack_extensions::thread_check::{HostThreadCheck, HostThreadCheckImpl};
use clack_extensions::timer::{HostTimer, HostTimerImpl, TimerId};
use clack_host::prelude::*;
use std::cell::{Cell, RefCell};
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

thread_local! {
    // Const-initialised: reading them never allocates, so the plugin may ask from `process`.
    static IS_MAIN: Cell<bool> = const { Cell::new(false) };
    static IS_AUDIO: Cell<bool> = const { Cell::new(false) };
}

/// Marks the calling thread as the plugin's main (control) thread.
pub(super) fn mark_main_thread() {
    IS_MAIN.with(|f| f.set(true));
}

/// Marks the calling thread as the audio thread. Cheap, so `process` does it on every block.
pub(super) fn mark_audio_thread() {
    IS_AUDIO.with(|f| f.set(true));
}

/// Requests the plugin made, read by the worker and the stage. All atomics.
#[derive(Default)]
pub(super) struct Signals {
    pub restart: AtomicBool,
    pub process: AtomicBool,
    pub callback: AtomicBool,
    pub latency_changed: AtomicBool,
    /// The latest latency the control thread read, in frames (display only).
    pub latency: AtomicU32,
    pub tail_changed: AtomicBool,
    pub params_rescan: AtomicBool,
    pub gui_resize_pending: AtomicBool,
    pub gui_resize: AtomicU64,
    pub gui_closed: AtomicBool,
}

/// Marker type tying the handler types together for `PluginInstance<Host>`.
pub(super) struct Host;

pub(super) struct HostShared {
    pub signals: Arc<Signals>,
    pub plugin: String,
}

pub(super) struct HostMain<'a> {
    pub shared: &'a HostShared,
    pub timers: RefCell<Vec<Timer>>,
    pub fds: RefCell<Vec<(RawFd, FdFlags)>>,
    next_timer: Cell<u32>,
}

pub(super) struct Timer {
    pub id: TimerId,
    pub period: Duration,
    pub due: Instant,
}

pub(super) struct HostAudio {
    signals: Arc<Signals>,
}

impl HostMain<'_> {
    pub(super) fn new(shared: &HostShared) -> HostMain<'_> {
        HostMain {
            shared,
            timers: RefCell::new(Vec::new()),
            fds: RefCell::new(Vec::new()),
            next_timer: Cell::new(1),
        }
    }
}

impl HostAudio {
    pub(super) fn new(shared: &HostShared) -> Self {
        Self {
            signals: Arc::clone(&shared.signals),
        }
    }
}

impl HostHandlers for Host {
    type Shared<'a> = HostShared;
    type MainThread<'a> = HostMain<'a>;
    type AudioProcessor<'a> = HostAudio;

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &HostShared) {
        builder
            .register::<HostLog>()
            .register::<HostThreadCheck>()
            .register::<HostParams>()
            .register::<HostLatency>()
            .register::<HostTail>()
            .register::<HostTimer>()
            .register::<HostPosixFd>()
            .register::<HostGui>();
    }
}

impl<'a> SharedHandler<'a> for HostShared {
    fn request_restart(&self) {
        self.signals.restart.store(true, Ordering::Release);
    }

    fn request_process(&self) {
        self.signals.process.store(true, Ordering::Release);
    }

    fn request_callback(&self) {
        self.signals.callback.store(true, Ordering::Release);
    }
}

impl<'a> MainThreadHandler<'a> for HostMain<'a> {}

impl AudioProcessorHandler<'_> for HostAudio {}

impl HostLogImpl for HostShared {
    fn log(&self, severity: LogSeverity, message: &str) {
        let plugin = self.plugin.as_str();
        match severity {
            LogSeverity::Debug => tracing::debug!(plugin, "{message}"),
            LogSeverity::Info => tracing::info!(plugin, "{message}"),
            LogSeverity::Warning => tracing::warn!(plugin, "{message}"),
            _ => tracing::error!(plugin, ?severity, "{message}"),
        }
    }
}

impl HostThreadCheckImpl for HostShared {
    fn is_main_thread(&self) -> bool {
        IS_MAIN.with(Cell::get)
    }

    fn is_audio_thread(&self) -> bool {
        IS_AUDIO.with(Cell::get)
    }
}

impl HostParamsImplShared for HostShared {
    fn request_flush(&self) {
        // Parameters flow through `process`; a flush while idle is not needed (ADR 0007, step 5).
        self.signals.process.store(true, Ordering::Release);
    }
}

impl HostParamsImplMainThread for HostMain<'_> {
    fn rescan(&self, _flags: ParamRescanFlags) {
        self.shared
            .signals
            .params_rescan
            .store(true, Ordering::Release);
    }

    fn clear(&self, _param_id: ClapId, _flags: ParamClearFlags) {}
}

impl HostLatencyImpl for HostMain<'_> {
    fn changed(&self) {
        self.shared
            .signals
            .latency_changed
            .store(true, Ordering::Release);
    }
}

impl HostTailImpl for HostAudio {
    fn changed(&mut self) {
        self.signals.tail_changed.store(true, Ordering::Release);
    }
}

impl HostGuiImpl for HostShared {
    fn resize_hints_changed(&self) {}

    fn request_resize(&self, new_size: GuiSize) -> Result<(), HostError> {
        self.signals
            .gui_resize
            .store(new_size.pack_to_u64(), Ordering::Release);
        self.signals
            .gui_resize_pending
            .store(true, Ordering::Release);
        Ok(())
    }

    fn request_show(&self) -> Result<(), HostError> {
        Ok(())
    }

    fn request_hide(&self) -> Result<(), HostError> {
        Ok(())
    }

    fn closed(&self, _was_destroyed: bool) {
        self.signals.gui_closed.store(true, Ordering::Release);
    }
}

/// The shortest timer period the host grants; a plugin asking for less gets this.
const MIN_TIMER_PERIOD: Duration = Duration::from_millis(15);

impl HostTimerImpl for HostMain<'_> {
    fn register_timer(&self, period_ms: u32) -> Result<TimerId, HostError> {
        let id = TimerId(self.next_timer.get());
        self.next_timer.set(id.0.wrapping_add(1).max(1));
        let period = Duration::from_millis(u64::from(period_ms)).max(MIN_TIMER_PERIOD);
        self.timers.borrow_mut().push(Timer {
            id,
            period,
            due: Instant::now() + period,
        });
        Ok(id)
    }

    fn unregister_timer(&self, timer_id: TimerId) -> Result<(), HostError> {
        let mut timers = self.timers.borrow_mut();
        let before = timers.len();
        timers.retain(|t| t.id != timer_id);
        if timers.len() == before {
            return Err(HostError::Message("unknown timer id"));
        }
        Ok(())
    }
}

impl HostPosixFdImpl for HostMain<'_> {
    fn register_fd(&self, fd: RawFd, flags: FdFlags) -> Result<(), HostError> {
        let mut fds = self.fds.borrow_mut();
        if fds.iter().any(|(f, _)| *f == fd) {
            return Err(HostError::Message("fd already registered"));
        }
        fds.push((fd, flags));
        Ok(())
    }

    fn modify_fd(&self, fd: RawFd, flags: FdFlags) -> Result<(), HostError> {
        let mut fds = self.fds.borrow_mut();
        let slot = fds.iter_mut().find(|(f, _)| *f == fd);
        slot.map(|s| s.1 = flags)
            .ok_or(HostError::Message("unknown fd"))
    }

    fn unregister_fd(&self, fd: RawFd) -> Result<(), HostError> {
        let mut fds = self.fds.borrow_mut();
        let before = fds.len();
        fds.retain(|(f, _)| *f != fd);
        if fds.len() == before {
            return Err(HostError::Message("unknown fd"));
        }
        Ok(())
    }
}
