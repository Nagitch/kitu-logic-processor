//! Entry point for the kitu-cli binary.

use std::{collections::HashMap, env};

use anyhow::{Context, Result};
use kitu_app_actions::{ActionInputType, ActionValue, AppActionCatalog};
use kitu_runtime::build_runtime;
use kitu_transport::LocalChannel;

fn main() -> Result<()> {
    run(env::args().skip(1).collect())
}

fn run(args: Vec<String>) -> Result<()> {
    let mut runtime = build_runtime(LocalChannel::connected());
    runtime
        .load_project_app_actions_from_toml(
            "demo-game",
            include_str!("../../../apps/demo-game/kitu-app-actions.toml"),
        )
        .context("load demo project app actions")?;
    let catalog = runtime.app_action_catalog().clone();

    match args.as_slice() {
        [] => {
            print_help();
        }
        [single] if single == "--help" || single == "-h" => {
            print_help();
        }
        [scope, command, rest @ ..] if scope == "app" && command == "action" => {
            run_app_action_command(&catalog, rest)?;
        }
        [scope, command, rest @ ..] if scope == "world" => {
            run_world_command(&catalog, command, rest)?;
        }
        _ => {
            print_help();
            anyhow::bail!("unsupported command");
        }
    }
    Ok(())
}

fn run_app_action_command(catalog: &AppActionCatalog, args: &[String]) -> Result<()> {
    match args {
        [command] if command == "list" => {
            for action in &catalog.actions {
                println!("{}\t{}\t{}", action.id, action.label, action.cli.command);
            }
        }
        [command, action_id] if command == "describe" => {
            let action = catalog
                .action(action_id)
                .with_context(|| format!("unknown app action `{action_id}`"))?;
            println!("id: {}", action.id);
            println!("label: {}", action.label);
            if let Some(description) = &action.description {
                println!("description: {description}");
            }
            println!("cli: {}", action.cli.command);
            println!("osc: {}", action.output.address);
            for input in &action.inputs {
                println!(
                    "input: {} ({:?}) required={}",
                    input.name, input.value_type, input.required
                );
            }
        }
        [command, action_id, rest @ ..] if command == "run" => {
            let inputs = parse_action_args(catalog, action_id, rest)?;
            let message = catalog
                .materialize_message(action_id, &inputs)
                .with_context(|| format!("materialize app action `{action_id}`"))?;
            println!("{}", message.to_debug_string()?);
        }
        _ => anyhow::bail!("expected `app action list`, `describe <id>`, or `run <id>`"),
    }
    Ok(())
}

fn run_world_command(catalog: &AppActionCatalog, command: &str, args: &[String]) -> Result<()> {
    let (action_id, aliases): (&str, &[(&str, &str, ActionInputType)]) = match command {
        "spawn" => (
            "spawn-object",
            &[
                ("kind", "kind", ActionInputType::String),
                ("x", "x", ActionInputType::Float),
                ("y", "y", ActionInputType::Float),
                ("z", "z", ActionInputType::Float),
            ],
        ),
        "move" => (
            "move-object",
            &[
                ("id", "id", ActionInputType::String),
                ("x", "x", ActionInputType::Float),
                ("y", "y", ActionInputType::Float),
                ("z", "z", ActionInputType::Float),
            ],
        ),
        "reset" => ("reset-world", &[]),
        _ => anyhow::bail!("unsupported world command `{command}`"),
    };
    let mut inputs = HashMap::new();
    for (flag, name, value_type) in aliases {
        if let Some(raw) = flag_value(args, flag) {
            inputs.insert(
                (*name).to_string(),
                ActionValue::parse_cli_value(*name, raw, *value_type)?,
            );
        }
    }
    let message = catalog
        .materialize_message(action_id, &inputs)
        .with_context(|| format!("materialize app action `{action_id}`"))?;
    println!("{}", message.to_debug_string()?);
    Ok(())
}

fn parse_action_args(
    catalog: &AppActionCatalog,
    action_id: &str,
    args: &[String],
) -> Result<HashMap<String, ActionValue>> {
    let action = catalog
        .action(action_id)
        .with_context(|| format!("unknown app action `{action_id}`"))?;
    let mut inputs = HashMap::new();
    for raw in normalized_action_args(args)? {
        let (name, value) = raw
            .split_once('=')
            .with_context(|| format!("expected key=value argument, got `{raw}`"))?;
        let spec = action
            .inputs
            .iter()
            .find(|input| input.name == name)
            .with_context(|| format!("unknown input `{name}` for action `{action_id}`"))?;
        inputs.insert(
            name.to_string(),
            ActionValue::parse_cli_value(name, value, spec.value_type)?,
        );
    }
    Ok(inputs)
}

fn normalized_action_args(args: &[String]) -> Result<Vec<String>> {
    let mut normalized = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--arg" {
            let value = iter
                .next()
                .with_context(|| "`--arg` expects key=value after it")?;
            normalized.push(value.clone());
        } else if let Some(value) = arg.strip_prefix("--arg=") {
            normalized.push(value.to_string());
        } else {
            normalized.push(arg.clone());
        }
    }
    Ok(normalized)
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let needle = format!("--{flag}");
    args.windows(2)
        .find_map(|pair| (pair[0] == needle).then_some(pair[1].as_str()))
        .or_else(|| {
            let prefix = format!("{needle}=");
            args.iter()
                .find_map(|arg| arg.strip_prefix(&prefix).map(str::trim))
        })
}

fn print_help() {
    println!("kitu-cli app action list");
    println!("kitu-cli app action describe <action-id>");
    println!("kitu-cli app action run <action-id> --arg key=value");
    println!("kitu-cli world spawn --kind enemy --x 1 --y 0 --z 2");
    println!("kitu-cli world move --id obj-1 --x 4 --y 0 --z 6");
    println!("kitu-cli world reset");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_value_action_args() {
        let catalog = kitu_app_actions::kitu_general_catalog();
        let inputs = parse_action_args(
            &catalog,
            "spawn-object",
            &["kind=enemy".to_string(), "x=1".to_string()],
        )
        .unwrap();

        assert_eq!(
            inputs.get("kind"),
            Some(&ActionValue::String("enemy".to_string()))
        );
        assert_eq!(inputs.get("x"), Some(&ActionValue::Float(1.0)));
    }
}
