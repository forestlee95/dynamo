// SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

//! Shape repairs for `/v1/chat/completions` request bodies.
//!
//! Some clients send a chat completions request using the spelling of a neighbouring API
//! (the Responses API, the Anthropic Messages API) or leave out a field that has only one
//! possible value. The typed request rejects those bodies as a whole. The repairs here
//! rewrite such a body into the standard form.
//!
//! Rules for adding a repair:
//!
//! - It runs only after the standard parse has failed, so a conforming request never
//!   reaches this module.
//! - It must not change what the client asked for. A spelling with no exact standard
//!   equivalent stays rejected: a tool type other than `function`, a content part of an
//!   unsupported kind, an unknown role.
//! - It applies only when every key of the object is accounted for. An object carrying
//!   keys the rewrite would have to drop is left alone.

use serde_json::{Map, Value};

/// Flat function tool (Responses API spelling) wrapped into `function`.
const FLAT_TOOL: &str = "flat_tool";
/// Missing tool `type` filled in as `function`.
const TOOL_TYPE: &str = "tool_type";
/// Flat assistant tool call wrapped into `function`.
const FLAT_TOOL_CALL: &str = "flat_tool_call";
/// Object or foreign spelling of `tool_choice` rewritten to the standard one.
const TOOL_CHOICE: &str = "tool_choice";
/// `content: null` on a user or tool message replaced by an empty string.
const NULL_CONTENT: &str = "null_content";
/// `input_text` content part (Responses API spelling) renamed to `text`.
const INPUT_TEXT_PART: &str = "input_text_part";
/// Tool call `type: "tool_call"` renamed to `function`.
const TOOL_CALL_TYPE: &str = "tool_call_type";
/// Empty `response_format` object removed.
const EMPTY_RESPONSE_FORMAT: &str = "empty_response_format";
/// `response_format` given as a bare type name, or as `{"type":"json"}`.
const RESPONSE_FORMAT: &str = "response_format";
/// `reasoning_effort: "mid"` renamed to `medium`.
const REASONING_EFFORT: &str = "reasoning_effort";
/// `seed` above `i64::MAX` folded into the supported range.
const SEED_RANGE: &str = "seed_range";

/// Names of the repairs applied to one request, in first-use order, without repeats.
#[derive(Default)]
struct Applied(Vec<&'static str>);

impl Applied {
    fn note(&mut self, repair: &'static str) {
        if !self.0.contains(&repair) {
            self.0.push(repair);
        }
    }
}

/// Rewrites the non-standard spellings listed in this module into the standard form.
///
/// Returns the names of the repairs applied, empty when the body was left unchanged.
pub(crate) fn repair_chat_request(request: &mut Value) -> Vec<&'static str> {
    let mut applied = Applied::default();
    let Some(request) = request.as_object_mut() else {
        return applied.0;
    };

    if let Some(tools) = request.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            repair_tool(tool, &mut applied);
        }
    }

    if let Some(tool_choice) = request.get_mut("tool_choice") {
        repair_tool_choice(tool_choice, &mut applied);
    }

    if let Some(messages) = request.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            repair_message(message, &mut applied);
        }
    }

    repair_response_format(request, &mut applied);

    // The scale has no `mid`; the level between `low` and `high` is `medium`.
    if request.get("reasoning_effort").and_then(Value::as_str) == Some("mid") {
        request.insert("reasoning_effort".to_owned(), Value::from("medium"));
        applied.note(REASONING_EFFORT);
    }

    // `seed` only selects a random stream, so any deterministic mapping keeps "same seed,
    // same output". Clearing the top bit keeps the value non-negative.
    if let Some(seed) = request.get("seed").and_then(Value::as_u64)
        && seed > i64::MAX as u64
    {
        request.insert("seed".to_owned(), Value::from(seed & i64::MAX as u64));
        applied.note(SEED_RANGE);
    }

    applied.0
}

/// Moves `keys` of `object` into a new nested `function` object.
///
/// Does nothing unless `object` has a string `name`, has no `function`, and has no keys
/// other than `keys` and `kept`.
fn wrap_into_function(object: &mut Map<String, Value>, keys: &[&str], kept: &[&str]) -> bool {
    let wrappable = !object.contains_key("function")
        && object.get("name").is_some_and(Value::is_string)
        && object
            .keys()
            .all(|key| keys.contains(&key.as_str()) || kept.contains(&key.as_str()));
    if !wrappable {
        return false;
    }

    let mut function = Map::new();
    for key in keys {
        if let Some(value) = object.remove(*key) {
            function.insert((*key).to_owned(), value);
        }
    }
    object.insert("function".to_owned(), Value::Object(function));
    true
}

fn repair_response_format(request: &mut Map<String, Value>, applied: &mut Applied) {
    let repaired = match request.get("response_format") {
        // `{}` names no format; the default (`text`) is what an absent field means.
        Some(Value::Object(format)) if format.is_empty() => {
            request.remove("response_format");
            applied.note(EMPTY_RESPONSE_FORMAT);
            return;
        }
        // `json` is not a type; JSON mode is `json_object`. With any other key next to it
        // (a schema, say) the intent is no longer clear, so that is left alone.
        Some(Value::Object(format))
            if format.len() == 1 && format.get("type").and_then(Value::as_str) == Some("json") =>
        {
            "json_object"
        }
        // A bare type name, for the types that need no other field.
        Some(Value::String(kind)) => match kind.as_str() {
            "text" => "text",
            "json_object" | "json" => "json_object",
            _ => return,
        },
        _ => return,
    };
    request.insert(
        "response_format".to_owned(),
        serde_json::json!({"type": repaired}),
    );
    applied.note(RESPONSE_FORMAT);
}

fn repair_tool(tool: &mut Value, applied: &mut Applied) {
    let Some(tool) = tool.as_object_mut() else {
        return;
    };

    match tool.get("type") {
        // `{"type":"function","name":…,"parameters":…}`: the Responses API spelling.
        Some(Value::String(kind)) if kind == "function" => {
            if wrap_into_function(
                tool,
                &["name", "description", "parameters", "strict"],
                &["type"],
            ) {
                applied.note(FLAT_TOOL);
            }
        }
        // `function` is the only tool type this endpoint has, so a tool that carries a
        // `function` object and no `type` can only mean that one.
        None if tool.get("function").is_some_and(Value::is_object) => {
            tool.insert("type".to_owned(), Value::from("function"));
            applied.note(TOOL_TYPE);
        }
        _ => {}
    }
}

fn repair_tool_choice(tool_choice: &mut Value, applied: &mut Applied) {
    let repaired = match &*tool_choice {
        // Anthropic's name for "must call some tool".
        Value::String(mode) if mode == "any" => Some(Value::from("required")),
        Value::Object(choice) => {
            let kind = choice.get("type").and_then(Value::as_str);
            let name = choice.get("name").and_then(Value::as_str);
            match (kind, name, choice.len()) {
                (Some(mode @ ("auto" | "none" | "required")), None, 1) => Some(Value::from(mode)),
                (Some("any"), None, 1) => Some(Value::from("required")),
                // `{"type":"function","name":…}` is the Responses API spelling of a named
                // choice, `{"type":"tool","name":…}` the Anthropic one.
                (Some("function" | "tool"), Some(name), 2) => Some(serde_json::json!({
                    "type": "function",
                    "function": {"name": name},
                })),
                _ => None,
            }
        }
        _ => None,
    };

    if let Some(repaired) = repaired {
        *tool_choice = repaired;
        applied.note(TOOL_CHOICE);
    }
}

fn repair_message(message: &mut Value, applied: &mut Applied) {
    let Some(message) = message.as_object_mut() else {
        return;
    };

    // An assistant message may already carry `content: null`, and a system message has
    // its own rule for a missing `content`; neither is touched.
    let is_user_or_tool = matches!(
        message.get("role").and_then(Value::as_str),
        Some("user" | "tool")
    );
    if is_user_or_tool && message.get("content").is_some_and(Value::is_null) {
        message.insert("content".to_owned(), Value::from(""));
        applied.note(NULL_CONTENT);
    }

    if let Some(parts) = message.get_mut("content").and_then(Value::as_array_mut) {
        for part in parts {
            if let Some(part) = part.as_object_mut()
                && part.get("type").and_then(Value::as_str) == Some("input_text")
                && part.get("text").is_some_and(Value::is_string)
            {
                part.insert("type".to_owned(), Value::from("text"));
                applied.note(INPUT_TEXT_PART);
            }
        }
    }

    if let Some(tool_calls) = message.get_mut("tool_calls").and_then(Value::as_array_mut) {
        for tool_call in tool_calls {
            let Some(tool_call) = tool_call.as_object_mut() else {
                continue;
            };
            // A tool call has one type; `tool_call` is that type under another name.
            if tool_call.get("type").and_then(Value::as_str) == Some("tool_call") {
                tool_call.insert("type".to_owned(), Value::from("function"));
                applied.note(TOOL_CALL_TYPE);
            }
            let is_function = match tool_call.get("type") {
                None => true,
                Some(kind) => kind.as_str() == Some("function"),
            };
            if is_function
                && wrap_into_function(tool_call, &["name", "arguments"], &["id", "type", "index"])
            {
                applied.note(FLAT_TOOL_CALL);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn repaired(mut request: Value) -> (Value, Vec<&'static str>) {
        let applied = repair_chat_request(&mut request);
        (request, applied)
    }

    #[test]
    fn standard_request_is_left_alone() {
        let request = json!({
            "model": "m",
            "seed": -7,
            "messages": [
                {"role": "system", "content": "s"},
                {"role": "user", "content": [{"type": "text", "text": "hi"}]},
                {"role": "assistant", "content": null, "tool_calls": [
                    {"id": "c1", "type": "function", "function": {"name": "f", "arguments": "{}"}}
                ]},
                {"role": "tool", "tool_call_id": "c1", "content": "ok"}
            ],
            "tools": [{"type": "function", "function": {"name": "f", "parameters": {"type": "object"}}}],
            "tool_choice": {"type": "function", "function": {"name": "f"}},
            "response_format": {"type": "json_object"}
        });
        let (after, applied) = repaired(request.clone());
        assert!(applied.is_empty(), "{applied:?}");
        assert_eq!(after, request);
    }

    #[test]
    fn flat_tool_is_wrapped() {
        let (after, applied) = repaired(json!({"tools": [{
            "type": "function",
            "name": "get_weather",
            "description": "d",
            "parameters": {"type": "object", "properties": {"b": {}, "a": {}}},
            "strict": true
        }]}));
        assert_eq!(applied, [FLAT_TOOL]);
        assert_eq!(
            after["tools"][0],
            json!({"type": "function", "function": {
                "name": "get_weather",
                "description": "d",
                "parameters": {"type": "object", "properties": {"b": {}, "a": {}}},
                "strict": true
            }})
        );
    }

    #[test]
    fn flat_tool_with_unaccounted_keys_is_left_alone() {
        // Anthropic spelling: wrapping it would silently drop the parameter schema.
        for tool in [
            json!({"name": "f", "description": "d", "input_schema": {"type": "object"}}),
            json!({"type": "function", "name": "f", "input_schema": {"type": "object"}}),
            json!({"type": "function", "description": "no name"}),
        ] {
            let (after, applied) = repaired(json!({"tools": [tool.clone()]}));
            assert!(applied.is_empty(), "{tool}");
            assert_eq!(after["tools"][0], tool);
        }
    }

    #[test]
    fn other_tool_types_are_left_alone() {
        for tool in [
            json!({"type": "openrouter:web_search"}),
            json!({"type": "web_search", "name": "search"}),
            json!({"type": "custom", "custom": {"name": "c"}}),
        ] {
            let (after, applied) = repaired(json!({"tools": [tool.clone()]}));
            assert!(applied.is_empty(), "{tool}");
            assert_eq!(after["tools"][0], tool);
        }
    }

    #[test]
    fn missing_tool_type_is_filled_in() {
        let (after, applied) = repaired(json!({"tools": [{"function": {"name": "f"}}]}));
        assert_eq!(applied, [TOOL_TYPE]);
        assert_eq!(
            after["tools"][0],
            json!({"function": {"name": "f"}, "type": "function"})
        );
    }

    #[test]
    fn flat_tool_call_is_wrapped() {
        let (after, applied) = repaired(json!({"messages": [{
            "role": "assistant",
            "content": null,
            "tool_calls": [
                {"id": "c1", "type": "function", "name": "f", "arguments": "{\"a\":1}"},
                {"id": "c2", "name": "g", "arguments": "{}"}
            ]
        }]}));
        assert_eq!(applied, [FLAT_TOOL_CALL]);
        assert_eq!(
            after["messages"][0]["tool_calls"],
            json!([
                {"id": "c1", "type": "function", "function": {"name": "f", "arguments": "{\"a\":1}"}},
                {"id": "c2", "function": {"name": "g", "arguments": "{}"}}
            ])
        );
    }

    #[test]
    fn flat_tool_call_with_unaccounted_keys_is_left_alone() {
        for tool_call in [
            json!({"type": "function_call", "call_id": "c1", "name": "f", "arguments": "{}"}),
            json!({"id": "c1", "type": "function", "name": "f", "arguments": "{}", "output": "x"}),
            json!({"id": "c1", "type": "custom", "name": "f", "arguments": "{}"}),
        ] {
            let (after, applied) = repaired(json!({"messages": [
                {"role": "assistant", "content": null, "tool_calls": [tool_call.clone()]}
            ]}));
            assert!(applied.is_empty(), "{tool_call}");
            assert_eq!(after["messages"][0]["tool_calls"][0], tool_call);
        }
    }

    #[test]
    fn tool_choice_spellings_are_rewritten() {
        let named = json!({"type": "function", "function": {"name": "f"}});
        for (before, expected) in [
            (json!({"type": "auto"}), json!("auto")),
            (json!({"type": "none"}), json!("none")),
            (json!({"type": "required"}), json!("required")),
            (json!({"type": "any"}), json!("required")),
            (json!("any"), json!("required")),
            (json!({"type": "tool", "name": "f"}), named.clone()),
            (json!({"type": "function", "name": "f"}), named.clone()),
        ] {
            let (after, applied) = repaired(json!({"tool_choice": before.clone()}));
            assert_eq!(applied, [TOOL_CHOICE], "{before}");
            assert_eq!(after["tool_choice"], expected, "{before}");
        }
    }

    #[test]
    fn tool_choice_without_exact_equivalent_is_left_alone() {
        for tool_choice in [
            json!("auto"),
            json!({"type": "function", "function": {"name": "f"}}),
            json!({"type": "function"}),
            json!({"function": {"name": "f"}}),
            json!({"type": "auto", "disable_parallel_tool_use": true}),
            json!({"type": "tool", "name": "f", "disable_parallel_tool_use": true}),
            json!({"type": "allowed_tools", "allowed_tools": {"mode": "auto", "tools": []}}),
            json!("whatever"),
        ] {
            let (after, applied) = repaired(json!({"tool_choice": tool_choice.clone()}));
            assert!(applied.is_empty(), "{tool_choice}");
            assert_eq!(after["tool_choice"], tool_choice);
        }
    }

    #[test]
    fn null_content_becomes_empty_for_user_and_tool_only() {
        let (after, applied) = repaired(json!({"messages": [
            {"role": "user", "content": null},
            {"role": "tool", "tool_call_id": "c1", "content": null},
            {"role": "assistant", "content": null},
            {"role": "system", "content": null},
            {"role": "user"}
        ]}));
        assert_eq!(applied, [NULL_CONTENT]);
        assert_eq!(
            after["messages"],
            json!([
                {"role": "user", "content": ""},
                {"role": "tool", "tool_call_id": "c1", "content": ""},
                {"role": "assistant", "content": null},
                {"role": "system", "content": null},
                {"role": "user"}
            ])
        );
    }

    #[test]
    fn input_text_part_is_renamed_and_other_parts_are_left_alone() {
        let (after, applied) = repaired(json!({"messages": [{"role": "user", "content": [
            {"type": "input_text", "text": "hi"},
            {"type": "file", "file": {"file_id": "f1"}},
            {"type": "input_text"},
            {"text": "no type"}
        ]}]}));
        assert_eq!(applied, [INPUT_TEXT_PART]);
        assert_eq!(
            after["messages"][0]["content"],
            json!([
                {"type": "text", "text": "hi"},
                {"type": "file", "file": {"file_id": "f1"}},
                {"type": "input_text"},
                {"text": "no type"}
            ])
        );
    }

    #[test]
    fn tool_call_type_is_renamed() {
        let (after, applied) = repaired(json!({"messages": [{
            "role": "assistant",
            "content": null,
            "tool_calls": [
                {"id": "c1", "type": "tool_call", "function": {"name": "f", "arguments": "{}"}},
                {"id": "c2", "type": "tool_call", "name": "g", "arguments": "{}"},
                {"id": "c3", "type": "custom", "custom": {"name": "h", "input": "x"}}
            ]
        }]}));
        assert_eq!(applied, [TOOL_CALL_TYPE, FLAT_TOOL_CALL]);
        assert_eq!(
            after["messages"][0]["tool_calls"],
            json!([
                {"id": "c1", "type": "function", "function": {"name": "f", "arguments": "{}"}},
                {"id": "c2", "type": "function", "function": {"name": "g", "arguments": "{}"}},
                {"id": "c3", "type": "custom", "custom": {"name": "h", "input": "x"}}
            ])
        );
    }

    #[test]
    fn response_format_spellings_are_rewritten() {
        let (after, applied) = repaired(json!({"model": "m", "response_format": {}}));
        assert_eq!(applied, [EMPTY_RESPONSE_FORMAT]);
        assert_eq!(after, json!({"model": "m"}));

        for (before, expected) in [
            (json!("json_object"), "json_object"),
            (json!("json"), "json_object"),
            (json!("text"), "text"),
            (json!({"type": "json"}), "json_object"),
        ] {
            let (after, applied) = repaired(json!({"response_format": before.clone()}));
            assert_eq!(applied, [RESPONSE_FORMAT], "{before}");
            assert_eq!(
                after["response_format"],
                json!({"type": expected}),
                "{before}"
            );
        }

        for response_format in [
            json!("json_schema"),
            json!({"type": "json", "schema": {"type": "object"}}),
            json!({"type": "{\"type\":\"object\"}"}),
            json!({"type": "json_object"}),
            json!(null),
        ] {
            let (after, applied) = repaired(json!({"response_format": response_format.clone()}));
            assert!(applied.is_empty(), "{response_format}");
            assert_eq!(after["response_format"], response_format);
        }
    }

    #[test]
    fn reasoning_effort_mid_is_renamed() {
        let (after, applied) = repaired(json!({"reasoning_effort": "mid"}));
        assert_eq!(applied, [REASONING_EFFORT]);
        assert_eq!(after["reasoning_effort"], "medium");

        for effort in [json!("medium"), json!("ultra"), json!(3), json!(null)] {
            let (after, applied) = repaired(json!({"reasoning_effort": effort.clone()}));
            assert!(applied.is_empty(), "{effort}");
            assert_eq!(after["reasoning_effort"], effort);
        }
    }

    #[test]
    fn seed_above_i64_is_folded() {
        let (after, applied) = repaired(json!({"seed": 10_000_000_000_000_000_000_u64}));
        assert_eq!(applied, [SEED_RANGE]);
        assert_eq!(
            after["seed"],
            json!(10_000_000_000_000_000_000_u64 - (1_u64 << 63))
        );
        assert!(after["seed"].as_i64().is_some());

        for seed in [
            json!(i64::MAX),
            json!(-1),
            json!(0),
            json!(1.5),
            json!("7"),
            json!(1e30),
        ] {
            let (after, applied) = repaired(json!({"seed": seed.clone()}));
            assert!(applied.is_empty(), "{seed}");
            assert_eq!(after["seed"], seed);
        }
    }

    #[test]
    fn several_repairs_are_reported_once_each_in_order() {
        let (_, applied) = repaired(json!({
            "seed": u64::MAX,
            "tool_choice": {"type": "auto"},
            "tools": [
                {"type": "function", "name": "f"},
                {"type": "function", "name": "g"}
            ],
            "messages": [{"role": "user", "content": null}, {"role": "user", "content": null}]
        }));
        assert_eq!(applied, [FLAT_TOOL, TOOL_CHOICE, NULL_CONTENT, SEED_RANGE]);
    }

    #[test]
    fn non_object_bodies_are_left_alone() {
        for body in [
            json!([]),
            json!("x"),
            json!(null),
            json!({"tools": "x", "messages": 3}),
        ] {
            let (after, applied) = repaired(body.clone());
            assert!(applied.is_empty());
            assert_eq!(after, body);
        }
    }
}
