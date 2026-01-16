use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct CommandContext {
    pub session_id: Option<String>,
    pub args: Vec<String>,
    pub raw: String,
}

pub type CommandResult = Result<String, String>;
pub type CommandHandler = Arc<dyn Fn(CommandContext) -> CommandResult + Send + Sync>;

#[derive(Clone)]
pub struct Command {
    pub name: String,
    pub description: String,
    pub handler: CommandHandler,
}

impl Command {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        handler: CommandHandler,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            handler,
        }
    }
}

#[derive(Default)]
pub struct CommandRegistry {
    commands: Mutex<HashMap<String, Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, command: Command) -> Result<(), String> {
        let mut commands = self.commands.lock().expect("command registry mutex");
        if commands.contains_key(&command.name) {
            return Err(format!("command already registered: {}", command.name));
        }
        commands.insert(command.name.clone(), command);
        Ok(())
    }

    pub fn list(&self) -> Vec<Command> {
        let commands = self.commands.lock().expect("command registry mutex");
        commands.values().cloned().collect()
    }

    pub fn execute(&self, name: &str, ctx: CommandContext) -> CommandResult {
        let command = {
            let commands = self.commands.lock().expect("command registry mutex");
            commands.get(name).cloned()
        };

        match command {
            Some(command) => (command.handler)(ctx),
            None => Err(format!("command not found: {name}")),
        }
    }
}
