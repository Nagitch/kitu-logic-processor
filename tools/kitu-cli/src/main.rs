//! Terminal client for the running host's shared command endpoint.
use anyhow::{Context, Result};
use kitu_shell::{CommandRequest, CommandResponse, HostCatalog, COMMAND_VERSION};
use std::{
    env,
    io::{self, BufRead, IsTerminal, Write},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Client {
    agent: ureq::Agent,
    endpoint: String,
    catalog: HostCatalog,
    producer: String,
    next: u64,
}
impl Client {
    fn execute(&mut self, args: Vec<String>) -> Result<bool> {
        kitu_shell::resolve_command(&args).map_err(anyhow::Error::msg)?;
        let request = CommandRequest {
            version: COMMAND_VERSION,
            session_id: self.catalog.session_id.clone(),
            client_id: self.producer.clone(),
            id: self.next,
            args,
        };
        self.next = self.next.checked_add(1).context("command IDs exhausted")?;
        eprintln!(
            "session={} client={} id={}",
            request.session_id, request.client_id, request.id
        );
        let result: CommandResponse = self
            .agent
            .post(format!("{}/shell/execute", self.endpoint))
            .send_json(&request)
            .context(
                "request outcome is uncertain; retry with the same --session, --client and --id",
            )?
            .body_mut()
            .read_json()
            .context("read command response (inspect state before sending a new identity)")?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        Ok(result.ok)
    }
}
fn main() -> Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("kitu-cli [--endpoint http://127.0.0.1:8787] [--session ID] [--client NAME --id N] <command ...>\nkitu-cli [options] shell\n\nRun `help` to fetch the active host's command definitions. `shell` reads one command per line; `exit` ends it.\nCommands act on the running host; no local simulation is created. JSON results go to stdout; refusal returns a nonzero exit code.");
        return Ok(());
    }
    let mut endpoint =
        env::var("KITU_RUNTIME_URL").unwrap_or_else(|_| "http://127.0.0.1:8787".into());
    let mut producer = format!(
        "cli-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let mut next = 1;
    let mut expected_session = None;
    while args.first().is_some_and(|a| a.starts_with("--")) {
        anyhow::ensure!(args.len() >= 2, "option requires a value");
        let flag = args.remove(0);
        let value = args.remove(0);
        match flag.as_str() {
            "--endpoint" => endpoint = value,
            "--client" => producer = value,
            "--id" => next = value.parse().context("id must be a positive integer")?,
            "--session" => expected_session = Some(value),
            _ => anyhow::bail!("unknown option {flag}"),
        }
    }
    endpoint = endpoint.trim_end_matches('/').to_owned();
    anyhow::ensure!(endpoint.starts_with("http://"),"this local development client uses HTTP; production TLS/remote authentication is outside this stage");
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(90)))
            .build(),
    );
    let catalog: HostCatalog = agent
        .get(format!("{endpoint}/shell/catalog"))
        .call()
        .context("connect to running Kitu host")?
        .body_mut()
        .read_json()?;
    anyhow::ensure!(
        catalog.version == COMMAND_VERSION,
        "incompatible host command version"
    );
    if let Some(expected) = expected_session {
        anyhow::ensure!(
            catalog.session_id == expected,
            "host session changed; refusing to replay a command against a different run"
        );
    }
    let mut client = Client {
        agent,
        endpoint,
        catalog,
        producer,
        next,
    };
    if args == ["shell"] {
        let interactive = io::stdin().is_terminal();
        eprintln!(
            "Connected to {}. Type help or exit.",
            client.catalog.session_id
        );
        let mut succeeded = true;
        if interactive {
            eprint!("kitu> ");
            io::stderr().flush()?;
        }
        for line in io::stdin().lock().lines() {
            let line = line?;
            if line.trim() == "exit" {
                break;
            }
            if !line.trim().is_empty() {
                match kitu_shell::parse_line(&line)
                    .map_err(anyhow::Error::msg)
                    .and_then(|args| client.execute(args))
                {
                    Ok(ok) => succeeded &= ok,
                    Err(error) => {
                        eprintln!("{error:#}");
                        succeeded = false;
                    }
                }
            }
            if interactive {
                eprint!("kitu> ");
                io::stderr().flush()?;
            }
        }
        if !succeeded {
            std::process::exit(1);
        }
    } else {
        if args.is_empty() {
            args.push("help".into());
        }
        if !client.execute(args)? {
            std::process::exit(1);
        }
    }
    Ok(())
}
