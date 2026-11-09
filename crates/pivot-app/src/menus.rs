//! Data-driven menu tree and dispatch (DesignCraft `menus.rs` pattern, original code).

use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub enum Item {
    Sep,
    Sub(String, Vec<Item>),
    Cmd { label: String, id: String, params: Value, shortcut: Option<&'static str> },
}

fn cmd(label: &str, id: &str) -> Item {
    Item::Cmd { label: label.to_string(), id: id.to_string(), params: Value::Null, shortcut: None }
}
fn cmd_p(label: &str, id: &str, params: Value) -> Item {
    Item::Cmd { label: label.to_string(), id: id.to_string(), params, shortcut: None }
}

pub fn menu_tree() -> Vec<(&'static str, Vec<Item>)> {
    vec![
        ("File", vec![
            cmd("New Workspace", "file.new"),
            cmd("Open Workspace...", "file.open"),
            cmd("Save Workspace", "file.save"),
            cmd("Save Workspace As...", "file.save_as"),
            Item::Sep,
            cmd("Import Data...", "file.import"),
            cmd("Connect SQLite...", "data.connect"),
            Item::Sep,
            Item::Sub("Export".to_string(), vec![
                cmd_p("Export as HTML...", "file.export", json!({"format":"html"})),
                cmd_p("Export as XLSX...", "file.export", json!({"format":"xlsx"})),
                cmd_p("Export as PDF (active sheet)...", "file.export", json!({"format":"pdf"})),
            ]),
            Item::Sep,
            cmd("Exit", "app.exit"),
        ]),
        ("Edit", vec![cmd("Undo", "edit.undo"), cmd("Redo", "edit.redo"), Item::Sep, cmd("Copy", "edit.copy")]),
        ("Data", vec![cmd("Refresh", "data.refresh"), cmd("Field Types...", "data.types")]),
        ("Pivot", vec![cmd("Add Worksheet", "pivot.add_sheet"), cmd("Recompute", "pivot.recompute"), cmd("Clear Layout", "pivot.clear")]),
        ("Format", vec![
            Item::Sub("Color Theme".to_string(), vec![
                cmd_p("Classic Grid", "format.color_theme", json!({"id":"classicGrid"})),
                cmd_p("Ledger / Zebra", "format.color_theme", json!({"id":"ledger"})),
                cmd_p("Minimal", "format.color_theme", json!({"id":"minimal"})),
                cmd_p("Executive Accent", "format.color_theme", json!({"id":"executive"})),
                cmd_p("Compact Dense", "format.color_theme", json!({"id":"compactDense"})),
                cmd_p("High Contrast", "format.color_theme", json!({"id":"highContrast"})),
            ]),
            Item::Sub("Border Theme".to_string(), vec![
                cmd_p("Full Border", "format.border_theme", json!({"id":"full"})),
                cmd_p("Horizontal Rules (header/footer)", "format.border_theme", json!({"id":"horizontalRules"})),
                cmd_p("No Border", "format.border_theme", json!({"id":"none"})),
            ]),
            cmd("Number Format...", "format.number"),
        ]),
        ("View", vec![
            cmd("Home", "nav.home"),
            cmd("Source Data", "nav.source"),
            cmd("Worksheet", "nav.worksheet"),
            Item::Sep,
            Item::Sub("Brightness".to_string(), vec![
                cmd_p("Dark", "view.brightness", json!({"mode": "dark"})),
                cmd_p("Medium Dark", "view.brightness", json!({"mode": "mediumDark"})),
                cmd_p("Medium Light", "view.brightness", json!({"mode": "mediumLight"})),
                cmd_p("Light", "view.brightness", json!({"mode": "light"})),
                cmd_p("High Contrast", "view.brightness", json!({"mode": "highContrast"})),
            ]),
            Item::Sep,
            cmd("Reset Layout", "view.reset_layout"),
        ]),
        ("Help", vec![cmd("About Pivot Studio", "help.about")]),
    ]
}
