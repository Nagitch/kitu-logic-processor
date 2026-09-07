//! Per-host lifetime tracking; no detached library code may outlive native destruction.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct Work {
    closing: AtomicBool,
    active: Mutex<usize>,
    changed: tokio::sync::Notify,
    shutdown: tokio::sync::watch::Sender<bool>,
}

impl Default for Work {
    fn default() -> Self {
        Self {
            closing: AtomicBool::new(false),
            active: Mutex::new(0),
            changed: tokio::sync::Notify::new(),
            shutdown: tokio::sync::watch::channel(false).0,
        }
    }
}

pub(super) struct Guard(Arc<Work>);
impl Drop for Guard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.0.active.lock() {
            *active -= 1;
        }
        self.0.changed.notify_waiters();
    }
}

impl Work {
    pub(super) fn is_closing(&self) -> bool {
        self.closing.load(Ordering::Acquire)
    }
    pub(super) fn check(&self) -> Result<()> {
        anyhow::ensure!(!self.is_closing(), "host is shutting down");
        Ok(())
    }
    pub(super) fn enter(self: &Arc<Self>) -> Result<Guard> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("work lock poisoned"))?;
        self.check()?;
        *active += 1;
        Ok(Guard(self.clone()))
    }
    pub(super) fn subscribe(&self) -> tokio::sync::watch::Receiver<bool> {
        self.shutdown.subscribe()
    }
    pub(super) fn close(&self) {
        if let Ok(_active) = self.active.lock() {
            self.closing.store(true, Ordering::Release);
        }
        self.shutdown.send_replace(true);
        self.changed.notify_waiters();
    }
    pub(super) async fn wait(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            // Register before inspecting the count so even concurrent waiters
            // cannot miss the final worker's completion notification.
            changed.as_mut().enable();
            if self.active.lock().map(|n| *n == 0).unwrap_or(true) {
                return;
            }
            changed.await;
        }
    }
}

impl AppState {
    fn io_handle(&self) -> Result<tokio::runtime::Handle> {
        self.options
            .io_runtime
            .clone()
            .map(Ok)
            .unwrap_or_else(|| tokio::runtime::Handle::try_current().map_err(Into::into))
    }

    pub(super) fn spawn<T: Send + 'static>(
        &self,
        future: impl std::future::Future<Output = T> + Send + 'static,
    ) -> Result<tokio::task::JoinHandle<T>> {
        let guard = self.work.enter()?;
        Ok(self.io_handle()?.spawn(async move {
            let _guard = guard;
            future.await
        }))
    }

    pub(super) fn spawn_blocking<T: Send + 'static>(
        &self,
        job: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> Result<tokio::task::JoinHandle<Result<T>>> {
        let guard = self.work.enter()?;
        let work = self.work.clone();
        Ok(self.io_handle()?.spawn_blocking(move || {
            let _guard = guard;
            work.check()?;
            job()
        }))
    }
}
