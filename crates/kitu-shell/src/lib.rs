//! CLI shell skeleton for driving the runtime.

use std::collections::HashMap;

use kitu_core::{KituError, Result};

/// Represents a shell command handler.
pub trait CommandHandler: Send + Sync {
    /// Executes the command with provided arguments.
    fn execute(&self, args: &[String]) -> Result<String>;
}

/// Simple shell registry and dispatcher.
#[derive(Default)]
pub struct Shell {
    commands: HashMap<String, Box<dyn CommandHandler>>,
}

impl Shell {
    /// Registers a command by name.
    pub fn register_command<C: CommandHandler + 'static>(
        &mut self,
        name: impl Into<String>,
        handler: C,
    ) {
        self.commands.insert(name.into(), Box::new(handler));
    }

    /// Executes a command, returning its string response.
    pub fn run(&self, command: &str, args: &[String]) -> Result<String> {
        let handler = self
            .commands
            .get(command)
            .ok_or(KituError::InvalidInput("unknown command"))?;
        handler.execute(args)
    }
}

/// A basic echo command useful for testing.
#[derive(Default)]
pub struct EchoCommand;

impl CommandHandler for EchoCommand {
    fn execute(&self, args: &[String]) -> Result<String> {
        Ok(args.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_registered_command_returns_output() {
        let mut shell = Shell::default();
        shell.register_command("echo", EchoCommand);
        let out = shell
            .run("echo", &["hello".into(), "world".into()])
            .unwrap();
        assert_eq!(out, "hello world");
    }

    #[test]
    fn running_unknown_command_errors() {
        let shell = Shell::default();
        let err = shell.run("missing", &[]).unwrap_err();
        assert!(matches!(err, KituError::InvalidInput("unknown command")));
    }
}
