//! The eight tools `dex-mcp` advertises, and how each maps to a daemon command
//! (docs/prd.md §10.2).
//!
//! **Descriptions are written for a model, not for an API reference.** Each one
//! says when to reach for the tool and shows an example, because a tool a model
//! does not understand when to call is a tool it never calls. `note_append` is
//! the one that makes the whole feature work, so its description asks for
//! frequent use outright.

use serde_json::{Value, json};

/// One advertised tool.
pub struct Tool {
    /// The name the model calls.
    pub name: &'static str,
    /// When to use it, in the model's terms.
    pub description: &'static str,
    /// The daemon command it proxies to.
    pub command: &'static str,
}

/// Every tool, in the order a model meets them.
pub const TOOLS: [Tool; 8] = [
    Tool {
        name: "note_append",
        command: "context.note",
        description: "\
Record something the other agents in this workspace would want to know. Use it \
often and in one line: after a decision, a discovery, a rename, a broken \
assumption, or when you finish a piece of work. Other agents see your notes \
automatically at their next turn, so a note is how you avoid two agents solving \
the same problem twice. Example: note_append(body: \"switched the auth module \
to JWT; tokens expire after 15 minutes\").",
    },
    Tool {
        name: "context_read",
        command: "context.read",
        description: "\
Read one shared fact by its key. Use it when another agent's note or the \
workspace digest mentioned a key and you need its current value, or before \
overwriting a key so you can merge rather than clobber. Returns the value and \
its version; pass that version back to context_write to write safely. Example: \
context_read(key: \"auth/jwt\").",
    },
    Tool {
        name: "context_write",
        command: "context.write",
        description: "\
Store a durable fact under a key. Use it for things that stay true — a \
decision, a convention, a configured value — and note_append for things that \
merely happened. Keys are lowercase and use \"/\" for namespaces. If another \
agent may be editing the same key, read it first and \
pass expected_version; the write then fails instead of silently overwriting \
their change, and tells you the current value so you can merge. Example: \
context_write(key: \"auth/jwt\", value: \"tokens expire after 15 minutes\", \
expected_version: 3).",
    },
    Tool {
        name: "context_search",
        command: "context.search",
        description: "\
Full-text search across everything agents have stored in this workspace. Use it \
before asking a human, and before assuming something has not been decided yet — \
another agent may have written it down hours ago. Example: \
context_search(query: \"token expiry\").",
    },
    Tool {
        name: "context_list",
        command: "context.list",
        description: "\
List the keys stored in this workspace, with who wrote each and when, but \
without the values. Cheap: use it to see what is known before deciding what to \
read. Example: context_list(tag: \"api\").",
    },
    Tool {
        name: "agents_list",
        command: "agent.list",
        description: "\
Show the other agents working in this workspace: what each is doing and whether \
it is running, idle, or waiting on a human. Use it before sending a message, \
and when deciding whether to take on work someone else may already be doing.",
    },
    Tool {
        name: "message_send",
        command: "context.message_send",
        description: "\
Send a note to one specific agent rather than to everyone. Use it when the \
information only matters to them — a file you changed that they are editing, or \
an answer to something they asked. Name them by their label from agents_list. \
Example: message_send(target_agent: \"backend\", body: \"I moved the schema to \
db/schema.sql\").",
    },
    Tool {
        name: "message_inbox",
        command: "context.inbox",
        description: "\
Read the messages other agents addressed to you. Your workspace digest tells \
you how many are waiting but never their contents, so this is the only way to \
see them. Reading them clears them. Use it as soon as a digest says messages \
are waiting for you.",
    },
];

/// The JSON Schema advertised for a tool's arguments.
pub fn schema(name: &str) -> Value {
    match name {
        "note_append" => object(
            json!({
                "body": string("What happened, in one or two sentences."),
                "tags": string("Optional comma-separated tags, such as \"api,auth\"."),
            }),
            &["body"],
        ),
        "context_read" => object(
            json!({ "key": string("The key to read, such as \"auth/jwt\".") }),
            &["key"],
        ),
        "context_write" => object(
            json!({
                "key": string("Lowercase key; \"/\" separates namespaces."),
                "value": string("The fact to store."),
                "tags": string("Optional comma-separated tags."),
                "expected_version": {
                    "type": "integer",
                    "description": "The version you read. Omit for last-write-wins; 0 to create only.",
                },
            }),
            &["key", "value"],
        ),
        "context_search" => object(
            json!({
                "query": string("Words to look for."),
                "limit": { "type": "integer", "description": "Most hits to return (default 10)." },
            }),
            &["query"],
        ),
        "context_list" => object(
            json!({ "tag": string("Only entries carrying this tag.") }),
            &[],
        ),
        "agents_list" => object(json!({}), &[]),
        "message_send" => object(
            json!({
                "target_agent": string(
                    "The recipient's label, from agents_list. Labels name agents in your own workspace; an id reaches anyone.",
                ),
                "body": string("The message."),
            }),
            &["target_agent", "body"],
        ),
        "message_inbox" => object(json!({}), &[]),
        _ => object(json!({}), &[]),
    }
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

/// The tool by that name, if there is one.
pub fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|tool| tool.name == name)
}

/// Every tool as `tools/list` advertises it.
pub fn advertised() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": schema(tool.name),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_schema_and_a_description_that_teaches_it() {
        assert_eq!(TOOLS.len(), 8);
        for tool in &TOOLS {
            assert!(tool.description.len() > 80, "{} is too terse", tool.name);
            assert!(
                tool.description.contains("Use it") || tool.description.contains("use it"),
                "{} never says when to use it",
                tool.name
            );
            let schema = schema(tool.name);
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert!(schema["properties"].is_object(), "{}", tool.name);
        }
    }

    #[test]
    fn the_tools_a_model_calls_most_carry_an_example() {
        for name in [
            "note_append",
            "context_read",
            "context_write",
            "context_search",
            "context_list",
            "message_send",
        ] {
            let tool = find(name).unwrap();
            assert!(tool.description.contains("Example:"), "{name}");
        }
    }

    #[test]
    fn names_are_unique_and_resolvable() {
        for tool in &TOOLS {
            assert!(find(tool.name).is_some());
        }
        assert!(find("no_such_tool").is_none());
        let mut names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate tool name");
    }

    #[test]
    fn required_arguments_are_declared() {
        assert_eq!(schema("context_write")["required"], json!(["key", "value"]));
        assert_eq!(schema("message_inbox")["required"], json!([]));
    }
}
