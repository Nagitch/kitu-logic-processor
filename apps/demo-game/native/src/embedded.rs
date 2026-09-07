//! The native owner advances one reusable host; loopback tooling only queues work.

use std::{
    io::Write,
    net::{SocketAddr, TcpListener},
    path::PathBuf,
};

use kitu_demo_game::{
    arena::package::{LoadedPackage, PackageIdentity},
    host::{ArenaHost, HostOptions},
    DemoRuntime,
};
use kitu_osc_ir::{OscArg, OscBundle};
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
    package: Option<PackageIdentity>,
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
        timeline_directory: Option<PathBuf>,
        package: Option<LoadedPackage>,
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
            ("timelineDirectory", &timeline_directory),
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
        let timeline_source = timeline_directory
            .or_else(|| storage_directory.as_ref().map(|dir| dir.join("timelines")));
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
                        .write_all(selected_source(
                            package.as_ref(),
                            "arena.tmd",
                            include_bytes!("../../content/arena.tmd"),
                        ))
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
                        .write_all(selected_source(
                            package.as_ref(),
                            "boss.rhai",
                            include_bytes!("../../content/boss.rhai"),
                        ))
                        .map_err(|error| format!("seed Arena script: {error}"))?,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(format!("create Arena script: {error}")),
                }
            }
        }
        if let (Some(directory), Some(source)) = (&storage_directory, &timeline_source) {
            if source == &directory.join("timelines") {
                std::fs::create_dir_all(source)
                    .map_err(|error| format!("create Arena timelines: {error}"))?;
                for (name, bytes) in [
                    (
                        "boss-telegraph.tsq",
                        selected_source(
                            package.as_ref(),
                            "timelines/boss-telegraph.tsq",
                            include_bytes!("../../content/timelines/boss-telegraph.tsq"),
                        ),
                    ),
                    (
                        "floor-transition.tsq",
                        selected_source(
                            package.as_ref(),
                            "timelines/floor-transition.tsq",
                            include_bytes!("../../content/timelines/floor-transition.tsq"),
                        ),
                    ),
                ] {
                    match std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(source.join(name))
                    {
                        Ok(mut file) => file
                            .write_all(bytes)
                            .map_err(|error| format!("seed Arena timeline {name}: {error}"))?,
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(format!("create Arena timeline {name}: {error}")),
                    }
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
            timeline_directory: timeline_source,
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
            package: package.map(|package| package.identity),
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

// A validated package owns every allowlisted source. An explicit package never
// falls back to compiled bytes, including when seeding later authoring copies.
fn selected_source<'a>(
    package: Option<&'a LoadedPackage>,
    path: &str,
    default: &'static [u8],
) -> &'a [u8] {
    match package {
        Some(package) => package
            .source_bytes(path)
            .expect("validated package source allowlist"),
        None => default,
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
        let mut output = self
            .host
            .inspect_host()
            .map_err(|error| error.to_string())?;
        for message in output.iter_mut().flat_map(|bundle| &mut bundle.messages) {
            if message.address == "/host/arena/status" {
                let [OscArg::Str(json)] = message.args.as_mut_slice() else {
                    return Err("native host status must contain one JSON string".into());
                };
                let mut status: serde_json::Value =
                    serde_json::from_str(json).map_err(|error| error.to_string())?;
                let object = status
                    .as_object_mut()
                    .ok_or("native host status must be an object")?;
                object.insert(
                    "package".into(),
                    serde_json::to_value(&self.package).map_err(|error| error.to_string())?,
                );
                *json = serde_json::to_string(&status).map_err(|error| error.to_string())?;
            }
        }
        Ok(output)
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
