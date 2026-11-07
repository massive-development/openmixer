//! The control thread of one plugin instance. A CLAP instance has a "main thread" and clack's
//! `PluginInstance` is not `Send`, so each instance lives on a thread of its own: every call that
//! must run there (init, activate, deactivate, destroy, parameter queries, GUI, timers) is a job sent
//! to it. This is also the message boundary an out-of-process host would replace (ADR 0007, decision 1).
//!
//! Teardown rule: the audio processor is handed back and deactivated here before the instance is
//! dropped. Dropping an instance with a live processor leaks it in clack, so [`Worker::drop`] asserts
//! the order by construction (the stage returns its processor first).

use super::host::{self, Host, HostAudio, HostMain, HostShared, Signals};
use crate::PluginError;
use clack_host::entry::PluginEntry;
use clack_host::prelude::*;
use std::ffi::CString;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How often the worker looks at timers and plugin requests while idle.
const TICK: Duration = Duration::from_millis(15);

/// Everything a job on the control thread can reach.
pub(super) struct Ctx {
    pub instance: PluginInstance<Host>,
    pub signals: Arc<Signals>,
}

type Job = Box<dyn FnOnce(&mut Ctx) + Send>;

enum Cmd {
    Run(Job),
    Stop,
}

pub(super) struct Worker {
    tx: Sender<Cmd>,
    join: Option<JoinHandle<()>>,
    pub signals: Arc<Signals>,
}

fn host_info() -> Result<HostInfo, PluginError> {
    HostInfo::new(
        "OpenMixer",
        "OpenMixer",
        "https://github.com/massive-development/openmixer",
        env!("CARGO_PKG_VERSION"),
    )
    .map_err(|e| PluginError::Rejected(format!("host info: {e}")))
}

impl Worker {
    /// Creates (and initialises) the plugin `plugin_id` from `entry` on a new control thread.
    pub(super) fn spawn(
        entry: PluginEntry,
        plugin_id: CString,
        label: &str,
    ) -> Result<Self, PluginError> {
        let signals = Arc::new(Signals::default());
        let (tx, rx) = mpsc::channel::<Cmd>();
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);
        let info = host_info()?;
        let thread_signals = Arc::clone(&signals);
        let label = label.to_owned();
        let join = std::thread::Builder::new()
            .name(format!("clap-{label}"))
            .spawn(move || {
                host::mark_main_thread();
                let made = PluginInstance::<Host>::new(
                    |_| HostShared {
                        signals: Arc::clone(&thread_signals),
                        plugin: label.clone(),
                    },
                    |shared| HostMain::new(shared),
                    &entry,
                    &plugin_id,
                    &info,
                );
                match made {
                    Ok(instance) => {
                        let _ = ready_tx.send(Ok(()));
                        run(
                            Ctx {
                                instance,
                                signals: thread_signals,
                            },
                            &rx,
                        );
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                    }
                }
            })
            .map_err(|e| PluginError::Rejected(format!("cannot start plugin thread: {e}")))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                tx,
                join: Some(join),
                signals,
            }),
            Ok(Err(reason)) => {
                let _ = join.join();
                Err(PluginError::InvalidLibrary {
                    path: String::new(),
                    reason: format!("cannot create the plugin: {reason}"),
                })
            }
            Err(_) => {
                let _ = join.join();
                Err(PluginError::Rejected("plugin thread died on start".into()))
            }
        }
    }

    /// Runs `job` on the control thread and waits for its result. `None` when the thread is gone
    /// (the plugin panicked on its main thread).
    pub(super) fn call<R: Send + 'static>(
        &self,
        job: impl FnOnce(&mut Ctx) -> R + Send + 'static,
    ) -> Option<R> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let job: Job = Box::new(move |ctx| {
            let _ = reply_tx.send(job(ctx));
        });
        self.tx.send(Cmd::Run(job)).ok()?;
        reply_rx.recv().ok()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run(mut ctx: Ctx, rx: &mpsc::Receiver<Cmd>) {
    loop {
        match rx.recv_timeout(TICK) {
            Ok(Cmd::Run(job)) => {
                // A panic in plugin code on its main thread ends this instance, not the process.
                if catch_unwind(AssertUnwindSafe(|| job(&mut ctx))).is_err() {
                    tracing::error!(
                        "a CLAP plugin panicked on its control thread; instance dropped"
                    );
                    break;
                }
            }
            Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if catch_unwind(AssertUnwindSafe(|| ctx.service())).is_err() {
            tracing::error!("a CLAP plugin panicked in a timer or callback; instance dropped");
            break;
        }
    }
}

impl Ctx {
    /// Plugin requests and due timers.
    fn service(&mut self) {
        if self.signals.callback.swap(false, Ordering::AcqRel) {
            self.instance.call_on_main_thread_callback();
        }
        self.run_due_timers();
    }

    fn run_due_timers(&mut self) {
        let now = Instant::now();
        let due: Vec<_> = self.instance.access_handler(|h| {
            let mut timers = h.timers.borrow_mut();
            let mut due = Vec::new();
            for t in timers.iter_mut().filter(|t| t.due <= now) {
                t.due = now + t.period;
                due.push(t.id);
            }
            due
        });
        if due.is_empty() {
            return;
        }
        let mut handle = self.instance.plugin_handle();
        if let Some(ext) = handle.get_extension::<clack_extensions::timer::PluginTimer>() {
            for id in due {
                ext.on_timer(&mut handle, id);
            }
        }
    }

    /// Activates the plugin and returns its stopped audio processor.
    pub(super) fn activate(
        &mut self,
        config: PluginAudioConfiguration,
    ) -> Result<PluginAudioProcessor<Host>, PluginError> {
        self.instance
            .activate(|shared, _| HostAudio::new(shared), config)
            .map(PluginAudioProcessor::Stopped)
            .map_err(|e| PluginError::Rejected(format!("activate failed: {e}")))
    }

    /// Deactivates the instance with the processor handed back by the audio side.
    pub(super) fn deactivate(&mut self, processor: PluginAudioProcessor<Host>) {
        let stopped = match processor {
            PluginAudioProcessor::Started(started) => started.stop_processing(),
            PluginAudioProcessor::Stopped(stopped) => stopped,
        };
        self.instance.deactivate(stopped);
    }
}
