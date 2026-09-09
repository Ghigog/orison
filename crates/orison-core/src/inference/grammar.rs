//! JSON-Schema-to-GBNF conversion for [`super::llamacpp::LlamaCppBackend`].
//!
//! Ollama's `/api/chat` accepts a JSON Schema directly for `format` and
//! constrains its own sampler with it. `llama-cpp-2`'s grammar sampler
//! (`LlamaSampler::grammar`) instead takes a GBNF grammar string, so
//! `LlamaCppBackend` needs its own JSON-Schema-to-GBNF step to honour the
//! same [`super::types::ResponseFormat`] contract the trait promises.
//!
//! **Scope.** This covers the subset of JSON Schema that
//! `crate::inference::schema::root_schema` actually produces for this
//! codebase's response types: `object` (fixed property set, arbitrary
//! order), `array`, `string`/`integer`/`number`/`boolean`, string enums, and
//! `Option<T>` nullability (`"type": [T, "null"]`). It does not resolve
//! `$ref`/`definitions` — `root_schema` inlines subschemas specifically so
//! this converter never has to — and it does not support `oneOf`/`anyOf`
//! unions beyond the nullable case, or numeric `minimum`/`maximum` (advisory
//! bounds are not enforced by the grammar; see the doc comment on
//! `EmotionalUpdate` in `crate::prompt::schemas`).
//!
//! **Object fields are not optional in the grammar.** JSON object key order
//! is not meaningful to a JSON deserializer, so this converter is free to
//! emit properties in a fixed order; it takes advantage of that by *always*
//! emitting every property (using `null` for an absent `Option<T>`) rather
//! than modelling true optionality, which keeps the generated grammar linear
//! instead of combinatorial in the property count.

use serde_json::Value;

/// A GBNF grammar plus the name of its root rule.
#[derive(Debug, Clone)]
pub struct Grammar {
    pub root_rule: String,
    text: String,
}

impl Grammar {
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

const BASE_RULES: &str = r#"ws ::= | " " | "\n" ws
gbnf-string ::= "\"" ( [^"\\\x7F\x00-\x1F] | "\\" (["\\bfnrt] | "u" [0-9a-fA-F]{4}) )* "\""
gbnf-number ::= "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)? ([eE] [-+]? [0-9]+)?
gbnf-integer ::= "-"? ("0" | [1-9] [0-9]*)
gbnf-boolean ::= "true" | "false"
gbnf-null ::= "null"
gbnf-value ::= gbnf-object | gbnf-array | gbnf-string | gbnf-number | gbnf-boolean | gbnf-null
gbnf-object ::= "{" ws (gbnf-string ws ":" ws gbnf-value ("," ws gbnf-string ws ":" ws gbnf-value)*)? ws "}"
gbnf-array ::= "[" ws (gbnf-value ("," ws gbnf-value)*)? ws "]"
"#;

struct Builder {
    rules: Vec<(String, String)>,
    counter: usize,
}

impl Builder {
    fn fresh_name(&mut self, hint: &str) -> String {
        self.counter += 1;
        let base = hint
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>();
        format!("r-{base}-{}", self.counter)
    }

    fn define(&mut self, hint: &str, body: String) -> String {
        let name = self.fresh_name(hint);
        self.rules.push((name.clone(), body));
        name
    }

    /// Convert one schema node into a rule reference (either an existing
    /// primitive like `gbnf-string`, or a freshly defined named rule).
    fn convert(&mut self, schema: &Value, hint: &str) -> String {
        if let Some(literal) = schema.get("const") {
            return json_literal(literal);
        }
        if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
            let alts: Vec<String> = enum_values.iter().map(json_literal).collect();
            return self.define(hint, format!("({})", alts.join(" | ")));
        }

        match schema.get("type") {
            Some(Value::Array(types)) => {
                let alts: Vec<String> = types
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|t| self.primitive(t, schema, hint))
                    .collect();
                self.define(hint, format!("({})", alts.join(" | ")))
            }
            Some(Value::String(t)) => self.primitive(t, schema, hint),
            _ => "gbnf-value".to_string(),
        }
    }

    fn primitive(&mut self, ty: &str, schema: &Value, hint: &str) -> String {
        match ty {
            "null" => "gbnf-null".to_string(),
            "boolean" => "gbnf-boolean".to_string(),
            "integer" => "gbnf-integer".to_string(),
            "number" => "gbnf-number".to_string(),
            "string" => "gbnf-string".to_string(),
            "object" => self.object(schema, hint),
            "array" => self.array(schema, hint),
            _ => "gbnf-value".to_string(),
        }
    }

    fn object(&mut self, schema: &Value, hint: &str) -> String {
        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            let mut parts = Vec::new();
            for (key, sub) in properties {
                let value_rule = self.convert(sub, &format!("{hint}-{key}"));
                parts.push(format!("\"\\\"{key}\\\"\" ws \":\" ws {value_rule}"));
            }
            let body = if parts.is_empty() {
                "\"{\" ws \"}\"".to_string()
            } else {
                format!("\"{{\" ws {} ws \"}}\"", parts.join(" \",\" ws "))
            };
            return self.define(hint, body);
        }

        // A map (`HashMap<String, V>`): schemars emits `additionalProperties`
        // with no `properties`. Grammar-generate a repeatable string-keyed
        // list of that value type rather than fixed keys.
        if let Some(additional) = schema.get("additionalProperties") {
            if let Some(sub) = additional.as_object() {
                let value_rule =
                    self.convert(&Value::Object(sub.clone()), &format!("{hint}-entry"));
                let pair = self.define(
                    &format!("{hint}-pair"),
                    format!("gbnf-string ws \":\" ws {value_rule}"),
                );
                return self.define(
                    hint,
                    format!("\"{{\" ws ({pair} (\",\" ws {pair})*)? ws \"}}\""),
                );
            }
        }

        "gbnf-object".to_string()
    }

    fn array(&mut self, schema: &Value, hint: &str) -> String {
        let Some(items) = schema.get("items") else {
            return "gbnf-array".to_string();
        };
        let item_rule = self.convert(items, &format!("{hint}-item"));
        self.define(
            hint,
            format!("\"[\" ws ({item_rule} (\",\" ws {item_rule})*)? ws \"]\""),
        )
    }
}

fn json_literal(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"\\\"{s}\\\"\""),
        Value::Bool(b) => format!("\"{b}\""),
        Value::Null => "\"null\"".to_string(),
        other => format!("\"{other}\""),
    }
}

/// Convert a `schemars` root schema (as produced by
/// `crate::inference::schema::root_schema`) into a GBNF grammar whose root
/// rule matches exactly the shape `T` deserializes.
pub fn from_schema(schema: &Value, type_name: &str) -> Grammar {
    let mut builder = Builder {
        rules: Vec::new(),
        counter: 0,
    };
    let root_ref = builder.convert(schema, type_name);

    let mut text = String::from(BASE_RULES);
    for (name, body) in &builder.rules {
        text.push_str(&format!("{name} ::= {body}\n"));
    }
    text.push_str(&format!("root ::= {root_ref}\n"));

    Grammar {
        root_rule: "root".to_string(),
        text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    // Local, test-only types standing in for the response structs
    // `crate::prompt::schemas` defines in a later task (§2.4). They exercise
    // the same shapes — nested object, string enum, array of objects, and a
    // `HashMap<String, bool>` map — without this module depending on work
    // that comes after it.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    enum TestEmotion {
        Serenity,
        Anger,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    struct TestNested {
        emotion: TestEmotion,
        intensity: f32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    struct TestChoice {
        text: String,
        #[serde(rename = "type")]
        kind: TestEmotion,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    struct TestResponse {
        narration: String,
        nested: TestNested,
        choices: Vec<TestChoice>,
        flags: HashMap<String, bool>,
    }

    #[test]
    fn nested_response_grammar_has_a_root_rule_and_references_are_defined() {
        let schema = crate::inference::schema::root_schema::<TestResponse>();
        let value = serde_json::to_value(schema).unwrap();
        let grammar = from_schema(&value, "TestResponse");
        assert!(grammar.as_str().contains("root ::="));
        assert_no_dangling_references(grammar.as_str());
    }

    #[test]
    fn hashmap_field_produces_a_map_rule_not_untyped_fallback() {
        let schema = crate::inference::schema::root_schema::<TestResponse>();
        let value = serde_json::to_value(schema).unwrap();
        let grammar = from_schema(&value, "TestResponse");
        // `flags: HashMap<String, bool>` must produce a map rule, not fall
        // back to the untyped `gbnf-object` escape hatch.
        assert!(grammar
            .as_str()
            .contains("gbnf-string ws \":\" ws gbnf-boolean"));
        assert_no_dangling_references(grammar.as_str());
    }

    /// Every rule name mentioned in a rule body must be defined somewhere
    /// (either a `gbnf-*` primitive or a rule this grammar itself defines).
    /// A dangling reference would mean the grammar cannot compile in
    /// llama.cpp at all, which is a stronger failure than "schema drift" —
    /// it is not "less than 100% valid", it is zero.
    fn assert_no_dangling_references(grammar: &str) {
        let mut defined = std::collections::HashSet::new();
        for line in grammar.lines() {
            if let Some((name, _)) = line.split_once("::=") {
                defined.insert(name.trim().to_string());
            }
        }
        for line in grammar.lines() {
            let Some((_, body)) = line.split_once("::=") else {
                continue;
            };
            for word in body.split_whitespace() {
                let candidate = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
                if candidate.starts_with("r-") || candidate.starts_with("gbnf-") {
                    assert!(
                        defined.contains(candidate),
                        "rule '{candidate}' referenced but never defined in:\n{grammar}"
                    );
                }
            }
        }
    }
}
