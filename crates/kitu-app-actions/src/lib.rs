//! Shared application action catalog definitions.
//!
//! # Responsibilities
//! - Describe Kitu general and project-specific runtime actions in one catalog model.
//! - Validate action inputs and materialize action executions into OSC-IR messages.
//! - Load project action manifests that can be owned by an application runtime.
//!
//! # Integration
//! `kitu-runtime` owns the merged catalog at runtime. Tools such as the web admin
//! and `kitu-cli` consume the same definitions instead of redefining commands.

use std::collections::HashMap;

use kitu_osc_ir::{OscArg, OscMessage};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Result alias for app action catalog operations.
pub type AppActionResult<T> = std::result::Result<T, AppActionError>;

/// Errors returned while loading, validating, or materializing app actions.
#[derive(Debug, Error, PartialEq)]
pub enum AppActionError {
    /// The requested action id does not exist in the catalog.
    #[error("unknown app action: {0}")]
    UnknownAction(String),
    /// The action definition or manifest is invalid.
    #[error("invalid app action definition: {0}")]
    InvalidDefinition(String),
    /// A required action input is missing.
    #[error("missing input `{0}`")]
    MissingInput(String),
    /// An action input has the wrong type or cannot be parsed.
    #[error("invalid input `{name}`: {message}")]
    InvalidInput {
        /// Input field name.
        name: String,
        /// Human-readable validation detail.
        message: String,
    },
    /// A manifest could not be parsed.
    #[error("invalid app action manifest: {0}")]
    Manifest(String),
}

/// A merged set of Kitu general and project-specific action definitions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppActionCatalog {
    /// Ordered action definitions.
    pub actions: Vec<AppActionDefinition>,
}

impl AppActionCatalog {
    /// Creates an empty action catalog.
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    /// Creates a catalog from the provided ordered actions.
    pub fn with_actions(actions: Vec<AppActionDefinition>) -> Self {
        Self { actions }
    }

    /// Adds one action definition after validating its shape.
    pub fn add_action(&mut self, action: AppActionDefinition) -> AppActionResult<()> {
        action.validate()?;
        if self.actions.iter().any(|existing| existing.id == action.id) {
            return Err(AppActionError::InvalidDefinition(format!(
                "duplicate action id `{}`",
                action.id
            )));
        }
        self.actions.push(action);
        Ok(())
    }

    /// Adds multiple action definitions.
    pub fn extend_actions(
        &mut self,
        actions: impl IntoIterator<Item = AppActionDefinition>,
    ) -> AppActionResult<()> {
        for action in actions {
            self.add_action(action)?;
        }
        Ok(())
    }

    /// Finds an action definition by id.
    pub fn action(&self, id: &str) -> Option<&AppActionDefinition> {
        self.actions.iter().find(|action| action.id == id)
    }

    /// Materializes an action execution into an OSC-IR message.
    pub fn materialize_message(
        &self,
        id: &str,
        inputs: &HashMap<String, ActionValue>,
    ) -> AppActionResult<OscMessage> {
        let action = self
            .action(id)
            .ok_or_else(|| AppActionError::UnknownAction(id.to_string()))?;
        action.materialize_message(inputs)
    }
}

/// Defines one runtime action that can be surfaced in UI and CLI tools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppActionDefinition {
    /// Stable action id, such as `spawn-object` or `enemy.spawn`.
    pub id: String,
    /// Whether the action is built into Kitu or supplied by a project.
    pub scope: AppActionScope,
    /// Human-readable display name.
    pub label: String,
    /// Optional longer help text.
    pub description: Option<String>,
    /// CLI presentation metadata.
    pub cli: CliCommandSpec,
    /// UI rendering metadata.
    pub ui: UiActionSpec,
    /// Ordered input schema.
    pub inputs: Vec<ActionInputSpec>,
    /// OSC-IR output template.
    pub output: OscTemplate,
}

impl AppActionDefinition {
    /// Validates the definition shape.
    pub fn validate(&self) -> AppActionResult<()> {
        if self.id.trim().is_empty() {
            return Err(AppActionError::InvalidDefinition(
                "action id must not be empty".to_string(),
            ));
        }
        if self.label.trim().is_empty() {
            return Err(AppActionError::InvalidDefinition(format!(
                "action `{}` label must not be empty",
                self.id
            )));
        }
        if self.output.address.trim().is_empty() {
            return Err(AppActionError::InvalidDefinition(format!(
                "action `{}` output address must not be empty",
                self.id
            )));
        }
        Ok(())
    }

    /// Materializes this action into one OSC-IR message.
    pub fn materialize_message(
        &self,
        inputs: &HashMap<String, ActionValue>,
    ) -> AppActionResult<OscMessage> {
        self.validate()?;
        let mut normalized = HashMap::new();
        for input in &self.inputs {
            if let Some(value) = inputs.get(&input.name) {
                normalized.insert(input.name.clone(), input.coerce(value)?);
            } else if let Some(default) = &input.default {
                normalized.insert(input.name.clone(), input.coerce(default)?);
            } else if input.required {
                return Err(AppActionError::MissingInput(input.name.clone()));
            }
        }

        let mut message = OscMessage::new(self.output.address.clone());
        for template_arg in &self.output.args {
            let value = match template_arg {
                OscTemplateArg::Input { name } => normalized
                    .get(name)
                    .ok_or_else(|| AppActionError::MissingInput(name.clone()))?
                    .clone(),
                OscTemplateArg::Literal { value } => value.clone(),
            };
            message.push_arg(value.into_osc_arg());
        }
        Ok(message)
    }
}

/// Action ownership and grouping metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AppActionScope {
    /// A framework-provided action available to all Kitu applications.
    KituGeneral,
    /// An application-specific action loaded by the runtime.
    Project {
        /// Owning application id.
        #[serde(rename = "appId")]
        app_id: String,
    },
}

/// Metadata for exposing an action through `kitu-cli`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliCommandSpec {
    /// Suggested command phrase.
    pub command: String,
}

/// Metadata for rendering an action in web admin UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiActionSpec {
    /// Preferred UI treatment.
    pub kind: UiActionKind,
    /// Submit button label.
    pub submit_label: String,
    /// Whether the action should be styled as destructive.
    pub destructive: bool,
}

/// High-level action UI presentation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiActionKind {
    /// Render a standard form.
    Form,
    /// Render a compact button when no inputs are needed.
    Button,
}

/// Declares one input accepted by an app action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionInputSpec {
    /// Stable input name.
    pub name: String,
    /// Human-readable field label.
    pub label: String,
    /// Expected input value type.
    pub value_type: ActionInputType,
    /// Whether callers must provide this value when no default exists.
    pub required: bool,
    /// Optional default value.
    pub default: Option<ActionValue>,
}

impl ActionInputSpec {
    fn coerce(&self, value: &ActionValue) -> AppActionResult<ActionValue> {
        match (&self.value_type, value) {
            (ActionInputType::String, ActionValue::String(value)) => {
                Ok(ActionValue::String(value.clone()))
            }
            (ActionInputType::Float, ActionValue::Float(value)) => Ok(ActionValue::Float(*value)),
            (ActionInputType::Float, ActionValue::Int(value)) => {
                Ok(ActionValue::Float(*value as f32))
            }
            (ActionInputType::Int, ActionValue::Int(value)) => Ok(ActionValue::Int(*value)),
            (ActionInputType::Bool, ActionValue::Bool(value)) => Ok(ActionValue::Bool(*value)),
            (expected, _) => Err(AppActionError::InvalidInput {
                name: self.name.clone(),
                message: format!("expected {expected:?}"),
            }),
        }
    }
}

/// Supported action input value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionInputType {
    /// UTF-8 text.
    String,
    /// 32-bit floating point number.
    Float,
    /// 32-bit signed integer.
    Int,
    /// Boolean value.
    Bool,
}

/// A runtime action input or literal value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum ActionValue {
    /// Text value.
    String(String),
    /// 32-bit floating point value.
    Float(f32),
    /// 32-bit signed integer value.
    Int(i32),
    /// Boolean value.
    Bool(bool),
}

impl ActionValue {
    /// Parses a CLI `key=value` string value according to the expected type.
    pub fn parse_cli_value(
        name: impl Into<String>,
        value: &str,
        value_type: ActionInputType,
    ) -> AppActionResult<Self> {
        let name = name.into();
        match value_type {
            ActionInputType::String => Ok(Self::String(value.to_string())),
            ActionInputType::Float => {
                value
                    .parse::<f32>()
                    .map(Self::Float)
                    .map_err(|_| AppActionError::InvalidInput {
                        name,
                        message: "expected float".to_string(),
                    })
            }
            ActionInputType::Int => {
                value
                    .parse::<i32>()
                    .map(Self::Int)
                    .map_err(|_| AppActionError::InvalidInput {
                        name,
                        message: "expected integer".to_string(),
                    })
            }
            ActionInputType::Bool => {
                value
                    .parse::<bool>()
                    .map(Self::Bool)
                    .map_err(|_| AppActionError::InvalidInput {
                        name,
                        message: "expected bool".to_string(),
                    })
            }
        }
    }

    /// Converts this value into an OSC-IR argument.
    pub fn into_osc_arg(self) -> OscArg {
        match self {
            Self::String(value) => OscArg::Str(value),
            Self::Float(value) => OscArg::Float(value),
            Self::Int(value) => OscArg::Int(value),
            Self::Bool(value) => OscArg::Bool(value),
        }
    }
}

/// OSC-IR message template emitted by an action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OscTemplate {
    /// OSC address emitted by the action.
    pub address: String,
    /// Ordered OSC argument template.
    pub args: Vec<OscTemplateArg>,
}

/// One OSC argument template item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum OscTemplateArg {
    /// Substitute the named action input.
    Input {
        /// Input name to substitute.
        name: String,
    },
    /// Emit a literal value.
    Literal {
        /// Literal argument value.
        value: ActionValue,
    },
}

/// Returns the framework-provided Kitu general actions.
pub fn kitu_general_actions() -> Vec<AppActionDefinition> {
    vec![
        spawn_object_action(),
        move_object_action(),
        reset_world_action(),
    ]
}

/// Returns a catalog containing only framework-provided Kitu general actions.
pub fn kitu_general_catalog() -> AppActionCatalog {
    AppActionCatalog::with_actions(kitu_general_actions())
}

/// Loads project actions from a TOML manifest string.
pub fn load_project_actions_from_toml(
    app_id: &str,
    source: &str,
) -> AppActionResult<Vec<AppActionDefinition>> {
    let builders = parse_manifest_builders(source)?;
    if builders.is_empty() {
        return Err(AppActionError::Manifest(
            "missing [[actions]] array".to_string(),
        ));
    }
    builders
        .into_iter()
        .map(|builder| builder.build(app_id))
        .collect()
}

#[derive(Debug, Default)]
struct ManifestActionBuilder {
    id: Option<String>,
    label: Option<String>,
    description: Option<String>,
    command: Option<String>,
    inputs: Vec<ManifestInputBuilder>,
    output_address: Option<String>,
    output_args: Vec<ManifestValue>,
}

impl ManifestActionBuilder {
    fn build(self, app_id: &str) -> AppActionResult<AppActionDefinition> {
        let id = required_manifest_string(self.id, "id")?;
        let label = required_manifest_string(self.label, "label")?;
        let command = required_manifest_string(self.command, "cli.command")?;
        let address = required_manifest_string(self.output_address, "output.address")?;
        let inputs = self
            .inputs
            .into_iter()
            .map(|input| input.build(&id))
            .collect::<AppActionResult<Vec<_>>>()?;
        let args = self
            .output_args
            .iter()
            .map(parse_manifest_template_arg)
            .collect::<AppActionResult<Vec<_>>>()?;
        let action = AppActionDefinition {
            id,
            scope: AppActionScope::Project {
                app_id: app_id.to_string(),
            },
            label,
            description: self.description,
            cli: CliCommandSpec { command },
            ui: UiActionSpec {
                kind: if inputs.is_empty() {
                    UiActionKind::Button
                } else {
                    UiActionKind::Form
                },
                submit_label: "Run".to_string(),
                destructive: false,
            },
            inputs,
            output: OscTemplate { address, args },
        };
        action.validate()?;
        Ok(action)
    }
}

#[derive(Debug, Default)]
struct ManifestInputBuilder {
    name: Option<String>,
    label: Option<String>,
    type_name: Option<String>,
    required: Option<bool>,
    default: Option<ManifestValue>,
}

impl ManifestInputBuilder {
    fn build(self, action_id: &str) -> AppActionResult<ActionInputSpec> {
        let name = required_manifest_string(self.name, "input.name")?;
        let label = self.label.unwrap_or_else(|| humanize_label(&name));
        let type_name = required_manifest_string(self.type_name, "input.type")?;
        let value_type = match type_name.as_str() {
            "string" => ActionInputType::String,
            "float" => ActionInputType::Float,
            "int" | "integer" => ActionInputType::Int,
            "bool" | "boolean" => ActionInputType::Bool,
            _ => {
                return Err(AppActionError::Manifest(format!(
                    "action `{action_id}` input `{name}` has unsupported type `{type_name}`"
                )));
            }
        };
        let default = self
            .default
            .as_ref()
            .map(|value| parse_manifest_value(&name, value, value_type))
            .transpose()?;
        Ok(ActionInputSpec {
            name,
            label,
            value_type,
            required: self.required.unwrap_or(true),
            default,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ManifestValue {
    String(String),
    Float(f64),
    Integer(i64),
    Boolean(bool),
    Array(Vec<ManifestValue>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestSection {
    Action,
    Cli,
    Input,
    Output,
}

fn parse_manifest_builders(source: &str) -> AppActionResult<Vec<ManifestActionBuilder>> {
    let mut actions = Vec::new();
    let mut current: Option<ManifestActionBuilder> = None;
    let mut section = ManifestSection::Action;

    for (line_index, raw_line) in source.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        match line {
            "[[actions]]" => {
                if let Some(action) = current.take() {
                    actions.push(action);
                }
                current = Some(ManifestActionBuilder::default());
                section = ManifestSection::Action;
            }
            "[actions.cli]" => section = ManifestSection::Cli,
            "[[actions.inputs]]" => {
                let action = current.as_mut().ok_or_else(|| {
                    manifest_line_error(line_index, "[[actions.inputs]] before [[actions]]")
                })?;
                action.inputs.push(ManifestInputBuilder::default());
                section = ManifestSection::Input;
            }
            "[actions.output]" => section = ManifestSection::Output,
            _ => {
                let (key, value) = line.split_once('=').ok_or_else(|| {
                    manifest_line_error(line_index, "expected key = value assignment")
                })?;
                let key = key.trim();
                let value = parse_manifest_scalar(value.trim()).map_err(|error| {
                    manifest_line_error(line_index, &format!("invalid value: {error}"))
                })?;
                assign_manifest_value(
                    current.as_mut().ok_or_else(|| {
                        manifest_line_error(line_index, "assignment before [[actions]]")
                    })?,
                    section,
                    key,
                    value,
                )?;
            }
        }
    }

    if let Some(action) = current.take() {
        actions.push(action);
    }
    Ok(actions)
}

fn assign_manifest_value(
    action: &mut ManifestActionBuilder,
    section: ManifestSection,
    key: &str,
    value: ManifestValue,
) -> AppActionResult<()> {
    match section {
        ManifestSection::Action => match key {
            "id" => action.id = Some(expect_manifest_string(key, value)?),
            "label" => action.label = Some(expect_manifest_string(key, value)?),
            "description" => action.description = Some(expect_manifest_string(key, value)?),
            _ => {
                return Err(AppActionError::Manifest(format!(
                    "unknown action field `{key}`"
                )))
            }
        },
        ManifestSection::Cli => match key {
            "command" => action.command = Some(expect_manifest_string(key, value)?),
            _ => {
                return Err(AppActionError::Manifest(format!(
                    "unknown cli field `{key}`"
                )))
            }
        },
        ManifestSection::Input => {
            let input = action.inputs.last_mut().ok_or_else(|| {
                AppActionError::Manifest("input field before [[actions.inputs]]".to_string())
            })?;
            match key {
                "name" => input.name = Some(expect_manifest_string(key, value)?),
                "label" => input.label = Some(expect_manifest_string(key, value)?),
                "type" => input.type_name = Some(expect_manifest_string(key, value)?),
                "required" => input.required = Some(expect_manifest_bool(key, value)?),
                "default" => input.default = Some(value),
                _ => {
                    return Err(AppActionError::Manifest(format!(
                        "unknown input field `{key}`"
                    )))
                }
            }
        }
        ManifestSection::Output => match key {
            "address" => action.output_address = Some(expect_manifest_string(key, value)?),
            "args" => {
                action.output_args = match value {
                    ManifestValue::Array(values) => values,
                    _ => {
                        return Err(AppActionError::Manifest(
                            "output.args must be an array".to_string(),
                        ));
                    }
                }
            }
            _ => {
                return Err(AppActionError::Manifest(format!(
                    "unknown output field `{key}`"
                )))
            }
        },
    }
    Ok(())
}

fn parse_manifest_scalar(source: &str) -> std::result::Result<ManifestValue, String> {
    let source = source.trim();
    if source.starts_with('"') && source.ends_with('"') {
        return Ok(ManifestValue::String(
            source.trim_matches('"').replace("\\\"", "\""),
        ));
    }
    if source.starts_with('[') && source.ends_with(']') {
        let inner = source.trim_start_matches('[').trim_end_matches(']');
        if inner.trim().is_empty() {
            return Ok(ManifestValue::Array(Vec::new()));
        }
        return inner
            .split(',')
            .map(parse_manifest_scalar)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(ManifestValue::Array);
    }
    if source == "true" {
        return Ok(ManifestValue::Boolean(true));
    }
    if source == "false" {
        return Ok(ManifestValue::Boolean(false));
    }
    if source.contains('.') {
        return source
            .parse::<f64>()
            .map(ManifestValue::Float)
            .map_err(|error| error.to_string());
    }
    source
        .parse::<i64>()
        .map(ManifestValue::Integer)
        .map_err(|error| error.to_string())
}

fn parse_manifest_template_arg(value: &ManifestValue) -> AppActionResult<OscTemplateArg> {
    match value {
        ManifestValue::String(value) if value.starts_with('$') => Ok(OscTemplateArg::Input {
            name: value.trim_start_matches('$').to_string(),
        }),
        ManifestValue::String(value) => Ok(OscTemplateArg::Literal {
            value: ActionValue::String(value.clone()),
        }),
        ManifestValue::Float(value) => Ok(OscTemplateArg::Literal {
            value: ActionValue::Float(*value as f32),
        }),
        ManifestValue::Integer(value) => {
            let value = i32::try_from(*value).map_err(|_| {
                AppActionError::Manifest("integer OSC template arg is out of range".to_string())
            })?;
            Ok(OscTemplateArg::Literal {
                value: ActionValue::Int(value),
            })
        }
        ManifestValue::Boolean(value) => Ok(OscTemplateArg::Literal {
            value: ActionValue::Bool(*value),
        }),
        ManifestValue::Array(_) => Err(AppActionError::Manifest(
            "nested OSC template arrays are not supported".to_string(),
        )),
    }
}

fn parse_manifest_value(
    name: &str,
    value: &ManifestValue,
    value_type: ActionInputType,
) -> AppActionResult<ActionValue> {
    match (value_type, value) {
        (ActionInputType::String, ManifestValue::String(value)) => {
            Ok(ActionValue::String(value.clone()))
        }
        (ActionInputType::Float, ManifestValue::Float(value)) => {
            Ok(ActionValue::Float(*value as f32))
        }
        (ActionInputType::Float, ManifestValue::Integer(value)) => {
            Ok(ActionValue::Float(*value as f32))
        }
        (ActionInputType::Int, ManifestValue::Integer(value)) => {
            let value = i32::try_from(*value).map_err(|_| AppActionError::InvalidInput {
                name: name.to_string(),
                message: "integer default is out of range".to_string(),
            })?;
            Ok(ActionValue::Int(value))
        }
        (ActionInputType::Bool, ManifestValue::Boolean(value)) => Ok(ActionValue::Bool(*value)),
        _ => Err(AppActionError::InvalidInput {
            name: name.to_string(),
            message: "default value type mismatch".to_string(),
        }),
    }
}

fn required_manifest_string(value: Option<String>, field: &str) -> AppActionResult<String> {
    value.ok_or_else(|| AppActionError::Manifest(format!("missing string field `{field}`")))
}

fn expect_manifest_string(key: &str, value: ManifestValue) -> AppActionResult<String> {
    match value {
        ManifestValue::String(value) => Ok(value),
        _ => Err(AppActionError::Manifest(format!(
            "`{key}` must be a string"
        ))),
    }
}

fn expect_manifest_bool(key: &str, value: ManifestValue) -> AppActionResult<bool> {
    match value {
        ManifestValue::Boolean(value) => Ok(value),
        _ => Err(AppActionError::Manifest(format!("`{key}` must be a bool"))),
    }
}

fn manifest_line_error(line_index: usize, message: &str) -> AppActionError {
    AppActionError::Manifest(format!("line {}: {message}", line_index + 1))
}

fn humanize_label(name: &str) -> String {
    let mut chars = name.replace('_', " ").chars().collect::<Vec<_>>();
    if let Some(first) = chars.first_mut() {
        first.make_ascii_uppercase();
    }
    chars.into_iter().collect()
}

fn spawn_object_action() -> AppActionDefinition {
    AppActionDefinition {
        id: "spawn-object".to_string(),
        scope: AppActionScope::KituGeneral,
        label: "Spawn Object".to_string(),
        description: Some("Create an object in the runtime world.".to_string()),
        cli: CliCommandSpec {
            command: "world spawn".to_string(),
        },
        ui: UiActionSpec {
            kind: UiActionKind::Form,
            submit_label: "Spawn".to_string(),
            destructive: false,
        },
        inputs: vec![
            string_input("kind", "Kind", true, Some("marker")),
            float_input("x", "X", true, Some(0.0)),
            float_input("y", "Y", true, Some(0.0)),
            float_input("z", "Z", true, Some(0.0)),
        ],
        output: OscTemplate {
            address: "/admin/world/spawn".to_string(),
            args: vec![
                input_arg("kind"),
                input_arg("x"),
                input_arg("y"),
                input_arg("z"),
            ],
        },
    }
}

fn move_object_action() -> AppActionDefinition {
    AppActionDefinition {
        id: "move-object".to_string(),
        scope: AppActionScope::KituGeneral,
        label: "Move Object".to_string(),
        description: Some("Move an existing runtime world object.".to_string()),
        cli: CliCommandSpec {
            command: "world move".to_string(),
        },
        ui: UiActionSpec {
            kind: UiActionKind::Form,
            submit_label: "Move".to_string(),
            destructive: false,
        },
        inputs: vec![
            string_input("id", "Object", true, None),
            float_input("x", "X", true, None),
            float_input("y", "Y", true, None),
            float_input("z", "Z", true, None),
        ],
        output: OscTemplate {
            address: "/admin/world/move".to_string(),
            args: vec![
                input_arg("id"),
                input_arg("x"),
                input_arg("y"),
                input_arg("z"),
            ],
        },
    }
}

fn reset_world_action() -> AppActionDefinition {
    AppActionDefinition {
        id: "reset-world".to_string(),
        scope: AppActionScope::KituGeneral,
        label: "Reset World".to_string(),
        description: Some("Clear all runtime world objects.".to_string()),
        cli: CliCommandSpec {
            command: "world reset".to_string(),
        },
        ui: UiActionSpec {
            kind: UiActionKind::Button,
            submit_label: "Reset".to_string(),
            destructive: true,
        },
        inputs: Vec::new(),
        output: OscTemplate {
            address: "/admin/world/reset".to_string(),
            args: Vec::new(),
        },
    }
}

fn string_input(name: &str, label: &str, required: bool, default: Option<&str>) -> ActionInputSpec {
    ActionInputSpec {
        name: name.to_string(),
        label: label.to_string(),
        value_type: ActionInputType::String,
        required,
        default: default.map(|value| ActionValue::String(value.to_string())),
    }
}

fn float_input(name: &str, label: &str, required: bool, default: Option<f32>) -> ActionInputSpec {
    ActionInputSpec {
        name: name.to_string(),
        label: label.to_string(),
        value_type: ActionInputType::Float,
        required,
        default: default.map(ActionValue::Float),
    }
}

fn input_arg(name: &str) -> OscTemplateArg {
    OscTemplateArg::Input {
        name: name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_spawn_materializes_osc_message() {
        let catalog = kitu_general_catalog();
        let inputs = HashMap::from([
            ("kind".to_string(), ActionValue::String("enemy".to_string())),
            ("x".to_string(), ActionValue::Float(1.0)),
            ("y".to_string(), ActionValue::Float(0.0)),
            ("z".to_string(), ActionValue::Float(2.0)),
        ]);

        let message = catalog
            .materialize_message("spawn-object", &inputs)
            .unwrap();

        assert_eq!(message.address, "/admin/world/spawn");
        assert_eq!(
            message.args,
            vec![
                OscArg::Str("enemy".to_string()),
                OscArg::Float(1.0),
                OscArg::Float(0.0),
                OscArg::Float(2.0),
            ]
        );
    }

    #[test]
    fn project_manifest_loads_action_template() {
        let source = r#"
            [[actions]]
            id = "enemy.spawn"
            label = "Enemy Spawn"

            [actions.cli]
            command = "enemy spawn"

            [[actions.inputs]]
            name = "enemy_type"
            type = "string"
            required = true

            [[actions.inputs]]
            name = "x"
            type = "float"
            required = true

            [actions.output]
            address = "/game/enemy/spawn"
            args = ["$enemy_type", "$x"]
        "#;

        let actions = load_project_actions_from_toml("demo", source).unwrap();

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].id, "enemy.spawn");
        assert_eq!(
            actions[0].scope,
            AppActionScope::Project {
                app_id: "demo".to_string()
            }
        );
    }
}
