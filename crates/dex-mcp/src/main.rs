//! `dex-mcp`: a stdio MCP server exposing the workspace context store to
//! Claude Code. Holds no state; proxies every tool call to the daemon over the
//! pipe. Outside a Dex pane it advertises no tools (docs/prd.md §10.2).
#![forbid(unsafe_code)]

fn main() {}
