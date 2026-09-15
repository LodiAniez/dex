//! Printing: aligned tables for people, JSON for scripts (docs/prd.md §11).

use dex_protocol::ErrorBody;
use serde::Serialize;

/// How the user asked for output.
#[derive(Debug, Clone, Copy)]
pub struct Format {
    /// `--json`: machine-readable output.
    pub json: bool,
    /// Tables get a header row (off with `--no-header`).
    pub header: bool,
}

/// Prints `value` as pretty JSON on stdout.
pub fn json(value: &impl Serialize) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(err) => eprintln!("error: cannot print JSON: {err}"),
    }
}

/// Prints rows as left-aligned columns, with a header row unless `--no-header`.
pub fn table(format: Format, headers: &[&str], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let render = |cells: &[&str]| -> String {
        let last = cells.len().saturating_sub(1);
        let padded: Vec<String> = cells
            .iter()
            .zip(&widths)
            .enumerate()
            .map(|(i, (cell, width))| {
                if i == last {
                    (*cell).to_owned()
                } else {
                    format!("{cell:<width$}")
                }
            })
            .collect();
        padded.join("  ").trim_end().to_owned()
    };
    if format.header {
        println!("{}", render(headers));
    }
    for row in rows {
        let cells: Vec<&str> = row.iter().map(String::as_str).collect();
        println!("{}", render(&cells));
    }
}

/// Prints an error with its repair: to stderr for people, as JSON for scripts.
pub fn print_error(err: &ErrorBody, format: Format) {
    if format.json {
        json(&serde_json::json!({ "ok": false, "error": err }));
    } else {
        eprintln!("error: {}", err.message);
        eprintln!("  fix: {}", err.repair);
    }
}
