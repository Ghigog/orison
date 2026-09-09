//! Native tool calling (§2.6).
//!
//! This replaces the hand-rolled ReAct loop at `GameLoopController.gd:773`,
//! which asked the model to emit `{"thought", "action", "args"}` as prose
//! JSON and hoped the action name matched a `match` arm. Here, a tool's
//! arguments are a Rust type; `schemars` derives the schema handed to the
//! backend from that same type, so a model call and the code that handles it
//! cannot drift the way hand-parsed prose can.
//!
//! **Scope note.** Phase 2 builds the typed calling *mechanism* only: tool
//! definitions, argument types, and a dispatch trait. Wiring dispatch to real
//! campaign state is game logic (`turn`, Phase 4) and knowledge-graph
//! lookups (`knowledge`/`retrieval`, Phase 3), neither of which exists yet.
//!
//! The original Director loop exposed four tools. `search_knowledge_graph`
//! is deliberately not one of the three below: the handoff calls for it to
//! become a single deterministic pre-pass in Phase 3 rather than a tool
//! call, which is also what gets the pre-narration call count down. That
//! leaves exactly the tools suited to genuinely unpredictable lookups.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::error::InferenceError;
use super::types::ToolDefinition;

/// A tool's argument type. `NAME` and `DESCRIPTION` are what the backend
/// sees; the schema it constrains against is derived from `Self`.
pub trait ToolArgs: DeserializeOwned + Serialize + JsonSchema + Send + Sync {
    const NAME: &'static str;
    const DESCRIPTION: &'static str;
}

/// Build the [`ToolDefinition`] advertised to a backend for `T`.
pub fn tool_definition<T: ToolArgs>() -> ToolDefinition {
    let schema = super::schema::root_schema::<T>();
    ToolDefinition {
        name: T::NAME.to_string(),
        description: T::DESCRIPTION.to_string(),
        parameters: serde_json::to_value(schema)
            .expect("schemars root schema is always valid JSON"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetCharacterProfileArgs {
    /// Name of the character to look up.
    pub character_name: String,
}

impl ToolArgs for GetCharacterProfileArgs {
    const NAME: &'static str = "get_character_profile";
    const DESCRIPTION: &'static str = "Retrieve the detailed profile for a character node.";
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetLocationDetailArgs {
    /// Name of the location to look up.
    pub location_name: String,
}

impl ToolArgs for GetLocationDetailArgs {
    const NAME: &'static str = "get_location_detail";
    const DESCRIPTION: &'static str = "Retrieve the details and description for a location node.";
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetRelationshipArgs {
    /// Name or ID of the first entity.
    pub entity_a: String,
    /// Name or ID of the second entity.
    pub entity_b: String,
}

impl ToolArgs for GetRelationshipArgs {
    const NAME: &'static str = "get_relationship";
    const DESCRIPTION: &'static str = "Retrieve relationship edges connecting two entities.";
}

/// The tool set offered to the Director model. Kept to three: small models
/// select tools far less reliably than they format the calls, and the
/// handoff caps this at 3-5.
pub fn director_tools() -> Vec<ToolDefinition> {
    vec![
        tool_definition::<GetCharacterProfileArgs>(),
        tool_definition::<GetLocationDetailArgs>(),
        tool_definition::<GetRelationshipArgs>(),
    ]
}

/// Executes a tool call and returns the observation text fed back to the
/// model as a `Role::Tool` message. Implemented against real campaign state
/// in Phase 3/4; `orison-core`'s inference layer only defines the contract.
#[async_trait]
pub trait ToolDispatcher: Send + Sync {
    async fn get_character_profile(&self, args: GetCharacterProfileArgs) -> String;
    async fn get_location_detail(&self, args: GetLocationDetailArgs) -> String;
    async fn get_relationship(&self, args: GetRelationshipArgs) -> String;
}

/// Decode a tool call's raw JSON arguments into `T`, returning a typed error
/// rather than a silent `parsing_failed` flag if the model's call doesn't
/// match what `T::NAME` promised.
pub fn decode_args<T: ToolArgs>(raw: &serde_json::Value) -> Result<T, InferenceError> {
    serde_json::from_value(raw.clone()).map_err(Into::into)
}

/// Dispatch a single [`super::types::ToolCall`] by name, decoding its
/// arguments into the matching typed struct. Returns `None` for a name none
/// of the three tools declare, so an unknown call is a value the caller
/// handles rather than a `match` fallthrough silently doing nothing.
pub async fn dispatch(
    dispatcher: &dyn ToolDispatcher,
    call: &super::types::ToolCall,
) -> Option<Result<String, InferenceError>> {
    match call.name.as_str() {
        GetCharacterProfileArgs::NAME => {
            match decode_args::<GetCharacterProfileArgs>(&call.arguments) {
                Ok(args) => Some(Ok(dispatcher.get_character_profile(args).await)),
                Err(e) => Some(Err(e)),
            }
        }
        GetLocationDetailArgs::NAME => {
            match decode_args::<GetLocationDetailArgs>(&call.arguments) {
                Ok(args) => Some(Ok(dispatcher.get_location_detail(args).await)),
                Err(e) => Some(Err(e)),
            }
        }
        GetRelationshipArgs::NAME => match decode_args::<GetRelationshipArgs>(&call.arguments) {
            Ok(args) => Some(Ok(dispatcher.get_relationship(args).await)),
            Err(e) => Some(Err(e)),
        },
        _ => None,
    }
}
