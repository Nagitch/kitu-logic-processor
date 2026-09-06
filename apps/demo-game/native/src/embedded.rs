//! The native owner advances one reusable host; loopback tooling only queues work.

use std::{
    io::Write,
    net::{SocketAddr, TcpListener},
    path::PathBuf,
};

use kitu_demo_game::{
    host::{ArenaHost, HostOptions},
    DemoRuntime,
};
use kitu_osc_ir::OscBundle;
use kitu_runtime::InputMetadata;
use kitu_unity_ffi::application::ApplicationDriver;
use serde::Deserialize;
use tokio::{runtime::Runtime, sync::oneshot, task::JoinHandle};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BridgeConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    address: Option<String>,
}

pub(super) struct EmbeddedDriver {
    host: ArenaHost,
    io_runtime: Option<Runtime>,
    stop: Option<oneshot::Sender<()>>,
    server: Option<JoinHandle<Result<(), std::io::Error>>>,
}

impl EmbeddedDriver {
    pub(super) fn new(
        runtime: DemoRuntime,
        bridge: BridgeConfig,
        storage_directory: Option<PathBuf>,
        content_path: Option<PathBuf>,
        script_path: Option<PathBuf>,
    ) -> Result<Self, String> {
        // Parse and bind before creating files. Only literal loopback addresses
        // are allowed; this development surface does not offer remote auth.
        let address: SocketAddr = bridge
            .address
            .as_deref()
            .unwrap_or("127.0.0.1:8789")
            .parse()
            .map_err(|error| format!("invalid native bridge address: {error}"))?;
        if !address.ip().is_loopback() {
            return Err("native bridge address must be loopback".into());
        }
        for (name, path) in [
            ("storageDirectory", &storage_directory),
            ("contentPath", &content_path),
            ("scriptPath", &script_path),
        ] {
            if path.as_ref().is_some_and(|path| !path.is_absolute()) {
                return Err(format!("{name} must be an absolute path"));
            }
        }
        let listener = if bridge.enabled {
            let listener = TcpListener::bind(address).map_err(|error| {
                format!("cannot bind native development bridge {address}: {error}")
            })?;
            listener
                .set_nonblocking(true)
                .map_err(|error| error.to_string())?;
            Some(listener)
        } else {
            None
        };
        let endpoint = listener
            .as_ref()
            .map(|listener| {
                listener
                    .local_addr()
                    .map(|address| format!("http://{address}"))
            })
            .transpose()
            .map_err(|error| error.to_string())?;
        let source =
            content_path.or_else(|| storage_directory.as_ref().map(|dir| dir.join("arena.tmd")));
        let script_source =
            script_path.or_else(|| storage_directory.as_ref().map(|dir| dir.join("boss.rhai")));
        if let Some(directory) = &storage_directory {
            std::fs::create_dir_all(directory)
                .map_err(|error| format!("create Arena storage: {error}"))?;
        }
        // A separately supplied contentPath belongs to the caller and must not
        // be replaced. The default editable copy is seeded only when absent.
        if let (Some(directory), Some(source)) = (&storage_directory, &source) {
            if source == &directory.join("arena.tmd") {
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(source)
                {
                    Ok(mut file) => file
                        .write_all(include_bytes!("../../content/arena.tmd"))
                        .map_err(|error| format!("seed Arena TMD: {error}"))?,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(format!("create Arena TMD: {error}")),
                }
            }
        }
        if let (Some(directory), Some(source)) = (&storage_directory, &script_source) {
            if source == &directory.join("boss.rhai") {
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(source)
                {
                    Ok(mut file) => file
                        .write_all(include_bytes!("../../content/boss.rhai"))
                        .map_err(|error| format!("seed Arena script: {error}"))?,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(format!("create Arena script: {error}")),
                }
            }
        }
        let io_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("arena-native-io")
            .enable_all()
            .build()
            .map_err(|error| format!("create native I/O runtime: {error}"))?;
        let options = HostOptions {
            external_controller: true,
            content_path: source.unwrap_or_default(),
            script_path: script_source,
            run_directory: storage_directory
                .as_ref()
                .map(|dir| dir.join("runs"))
                .unwrap_or_default(),
            recording_directory: storage_directory
                .as_ref()
                .map(|dir| dir.join("recordings"))
                .unwrap_or_default(),
            persist_runs: storage_directory.is_some(),
            io_runtime: Some(io_runtime.handle().clone()),
            bridge_endpoint: endpoint,
        };
        let host = match ArenaHost::new(runtime, options) {
            Ok(host) => host,
            Err(error) => {
                // Creation may be called from an existing async host. Tokio's
                // blocking destructor belongs on a plain thread in that case.
                std::thread::spawn(move || drop(io_runtime))
                    .join()
                    .map_err(|_| "native I/O shutdown panicked".to_string())?;
                return Err(format!("create embedded Arena host: {error:#}"));
            }
        };
        let mut driver = Self {
            host,
            io_runtime: Some(io_runtime),
            stop: None,
            server: None,
        };
        if let Some(listener) = listener {
            let router = driver.host.router();
            let (stop, stopped) = oneshot::channel();
            let rt = driver.io_runtime.as_ref().expect("new runtime");
            let listener = {
                let _entered = rt.enter();
                tokio::net::TcpListener::from_std(listener).map_err(|error| error.to_string())?
            };
            driver.server = Some(rt.spawn(async move {
                axum::serve(listener, router)
                    .with_graceful_shutdown(async {
                        let _ = stopped.await;
                    })
                    .await
            }));
            driver.stop = Some(stop);
        }
        Ok(driver)
    }
}

impl ApplicationDriver for EmbeddedDriver {
    fn submit(
        &mut self,
        bundle: OscBundle,
        metadata: Option<InputMetadata>,
    ) -> Result<u64, String> {
        self.host
            .submit(bundle, metadata)
            .map_err(|error| error.to_string())
    }
    fn tick(&mut self) -> Result<Vec<OscBundle>, String> {
        self.host.tick().map_err(|error| error.to_string())
    }
    fn inspect(&self) -> Result<Vec<OscBundle>, String> {
        self.host.inspect().map_err(|error| error.to_string())
    }
    fn inspect_host(&self) -> Result<Vec<OscBundle>, String> {
        self.host.inspect_host().map_err(|error| error.to_string())
    }
}

impl Drop for EmbeddedDriver {
    fn drop(&mut self) {
        self.host.begin_shutdown();
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let Some(runtime) = self.io_runtime.take() else {
            return;
        };
        let server = self.server.take();
        let host = &self.host;
        // Always join, including blocking content/replay workers, before returning
        // to a caller that may unload this library. Never abandon a timed-out task.
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    runtime.block_on(async {
                        host.wait_shutdown().await;
                        if let Some(server) = server {
                            // A peer may hold an incomplete HTTP request outside
                            // host Work tracking. Graceful HTTP draining would wait
                            // forever for its body. All admitted host jobs are now
                            // joined; cancel the listener future and let Runtime's
                            // destructor cancel/join remaining connection tasks.
                            server.abort();
                            let _ = server.await;
                        }
                    });
                    drop(runtime);
                })
                .join()
                .expect("native I/O shutdown panicked");
        });
    }
}
