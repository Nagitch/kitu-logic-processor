//! Transport-independent command definitions shared by terminal and browser clients.
use kitu_osc_ir::{OscArg, OscMessage};
use serde::{Deserialize, Serialize};

/// Wire version for live command requests and catalogs.
pub const COMMAND_VERSION: u32 = 1;

/// A human-readable definition served by the active host.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpec {
    /// Exact leading words used to dispatch this command.
    pub prefix: Vec<String>,
    /// Argument syntax shown by both clients.
    pub usage: String,
    /// Purpose and observable effect.
    pub description: String,
    /// Whether execution can change the active host.
    pub mutates: bool,
}

/// Standard commands; application actions/scenarios are supplied by the host.
///
/// # Examples
/// ```
/// assert!(kitu_shell::command_catalog().iter().any(|s| s.usage == "replay seek <tick>"));
/// ```
pub fn command_catalog() -> Vec<CommandSpec> {
    [
        (
            "help",
            "help",
            "Show these shared command definitions",
            false,
        ),
        (
            "inspect",
            "inspect <application|world|content|recording|replay>",
            "Inspect the active host and observed run",
            false,
        ),
        (
            "osc send",
            "osc send <address> [i:32-bit-int h:64-bit-int f:float s:string b:bool ...]",
            "Apply typed OSC at the host's next tick and return its outcome",
            true,
        ),
        (
            "app action list",
            "app action list",
            "List runtime-owned action definitions",
            false,
        ),
        (
            "app action describe",
            "app action describe <id>",
            "Inspect one runtime action",
            false,
        ),
        (
            "app action run",
            "app action run <id> [name=value ...]",
            "Apply an action through ordinary input admission",
            true,
        ),
        (
            "content validate",
            "content validate",
            "Evaluate the authoring file without changing the current run",
            true,
        ),
        (
            "content stage",
            "content stage <hash> <source-sha256>",
            "Stage the reviewed candidate for the next run",
            true,
        ),
        (
            "replay list",
            "replay list",
            "List saved TSQ1 content IDs",
            false,
        ),
        (
            "replay save",
            "replay save",
            "Save the current live recording",
            true,
        ),
        (
            "replay verify",
            "replay verify <id>",
            "Verify all saved state/event proofs",
            false,
        ),
        (
            "replay load",
            "replay load <id>",
            "Verify a recording and enter paused replay",
            true,
        ),
        (
            "replay play",
            "replay play",
            "Play the loaded recording at 60 Hz",
            true,
        ),
        ("replay pause", "replay pause", "Pause replay", true),
        (
            "replay step",
            "replay step",
            "Advance one recorded tick",
            true,
        ),
        (
            "replay stop",
            "replay stop",
            "Return replay to tick -1",
            true,
        ),
        (
            "replay live",
            "replay live",
            "Restore the paused live run",
            true,
        ),
        (
            "replay seek",
            "replay seek <tick>",
            "Re-execute to an exact tick and pause",
            true,
        ),
        (
            "scenario list",
            "scenario list",
            "List application-owned live scenarios",
            false,
        ),
        (
            "scenario run",
            "scenario run <id>",
            "Run a bounded action scenario on the live Runtime",
            true,
        ),
    ]
    .into_iter()
    .map(|(prefix, usage, description, mutates)| CommandSpec {
        prefix: prefix.split_whitespace().map(str::to_owned).collect(),
        usage: usage.into(),
        description: description.into(),
        mutates,
    })
    .collect()
}

/// Catalog and connection identity obtained from the running host.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostCatalog {
    /// Protocol version required by the host.
    pub version: u32,
    /// Unique host lifetime; stale commands must not target a restarted game.
    pub session_id: String,
    /// Commands supported by this host.
    pub commands: Vec<CommandSpec>,
}

/// Idempotent logical command; duplicate identities retain their original result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandRequest {
    /// Shared command wire version.
    pub version: u32,
    /// Runtime lifetime from the catalog.
    pub session_id: String,
    /// Stable producer name for this terminal or browser session.
    pub client_id: String,
    /// Positive monotonically increasing request ID from this producer.
    pub id: u64,
    /// Argument vector; a shell process is never invoked by the server.
    pub args: Vec<String>,
}

/// Applied result or actionable refusal, shared by CLI and browser Shell.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResponse {
    /// Original producer request ID.
    pub id: u64,
    /// True only when command execution completed successfully.
    pub ok: bool,
    /// Structured inspection or execution outcome.
    pub data: serde_json::Value,
    /// Diagnostic when execution was refused or failed.
    pub error: Option<String>,
}

/// Parses quoting without expansion, substitution or operating-system execution.
///
/// # Examples
/// ```
/// assert_eq!(kitu_shell::parse_line("osc send /example 's:two words'").unwrap()[3], "s:two words");
/// assert!(kitu_shell::parse_line("'unfinished").is_err());
/// ```
pub fn parse_line(line: &str) -> Result<Vec<String>, String> {
    if line.len() > 16384 {
        return Err("command line exceeds 16 KiB".into());
    }
    let args = shlex::split(line).ok_or("unclosed quote or escape")?;
    validate_args(&args)?;
    Ok(args)
}

/// Resolves the longest catalog prefix and validates bounded argument input.
///
/// # Examples
/// ```
/// let args = kitu_shell::parse_line("replay seek 42").unwrap();
/// let (spec, rest) = kitu_shell::resolve_command(&args).unwrap();
/// assert_eq!(spec.usage, "replay seek <tick>");
/// assert_eq!(rest, ["42"]);
/// ```
pub fn resolve_command(args: &[String]) -> Result<(CommandSpec, &[String]), String> {
    validate_args(args)?;
    let spec = command_catalog()
        .into_iter()
        .filter(|s| args.starts_with(&s.prefix))
        .max_by_key(|s| s.prefix.len())
        .ok_or("unknown command; run help")?;
    let n = spec.prefix.len();
    Ok((spec, &args[n..]))
}
fn validate_args(args: &[String]) -> Result<(), String> {
    if args.is_empty() || args.len() > 128 || args.iter().map(String::len).sum::<usize>() > 16384 {
        return Err("command requires 1..128 arguments totaling at most 16 KiB".into());
    }
    Ok(())
}

/// Converts explicit type-prefixed arguments without guessing or narrowing types.
///
/// # Examples
/// ```
/// let message = kitu_shell::parse_osc("/example", &["h:9007199254740993".into(), "b:true".into()]).unwrap();
/// assert!(matches!(message.args[0], kitu_osc_ir::OscArg::Int64(9007199254740993)));
/// assert!(kitu_shell::parse_osc("/example", &["f:NaN".into()]).is_err());
/// ```
pub fn parse_osc(address: &str, args: &[String]) -> Result<OscMessage, String> {
    if !address.starts_with('/') || address.len() > 256 {
        return Err("OSC address must start with / and fit 256 bytes".into());
    }
    let mut message = OscMessage::new(address);
    for raw in args {
        let (tag, value) = raw
            .split_once(':')
            .ok_or("use explicit i:, h:, f:, s: or b: OSC types")?;
        let error = || format!("invalid {tag}: argument {value:?}");
        message.push_arg(match tag {
            "i" => OscArg::Int(value.parse().map_err(|_| error())?),
            "h" => OscArg::Int64(value.parse().map_err(|_| error())?),
            "f" => {
                let number: f32 = value.parse().map_err(|_| error())?;
                if !number.is_finite() {
                    return Err(error());
                }
                OscArg::Float(number)
            }
            "s" => OscArg::Str(value.into()),
            "b" => OscArg::Bool(value.parse().map_err(|_| error())?),
            _ => return Err("unknown OSC type; use i, h, f, s or b".into()),
        });
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_arguments_are_literal_and_types_are_lossless() {
        let args =
            parse_line("osc send /x 's:$(echo untouched)' h:9223372036854775807 f:-0.0 b:false")
                .unwrap();
        let message = parse_osc(&args[2], &args[3..]).unwrap();
        assert_eq!(message.args[0], OscArg::Str("$(echo untouched)".into()));
        assert_eq!(message.args[1], OscArg::Int64(i64::MAX));
        assert!(matches!(message.args[2], OscArg::Float(v) if v.to_bits() == (-0.0_f32).to_bits()));
        assert!(parse_osc("/x", &["i:2147483648".into()]).is_err());
        assert!(parse_osc("/x", &["b:1".into()]).is_err());
        assert!(resolve_command(&["replay".into(), "bogus".into()]).is_err());
    }
}
