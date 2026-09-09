//! Schema generation shared by both backends.
//!
//! Schemas are generated with subschemas inlined rather than left as
//! `$ref`/`definitions` pairs. Ollama's structured-output support handles
//! `$ref` fine, but `LlamaCppBackend`'s JSON-Schema-to-GBNF grammar
//! converter (`super::grammar`) does not resolve references, so both
//! backends are given the same self-contained shape rather than maintaining
//! two schema-generation paths that could drift.

use schemars::gen::SchemaSettings;
use schemars::schema::RootSchema;
use schemars::JsonSchema;

pub fn root_schema<T: JsonSchema>() -> RootSchema {
    let settings = SchemaSettings::draft07().with(|s| {
        s.inline_subschemas = true;
    });
    settings.into_generator().into_root_schema_for::<T>()
}
