//! Data-driven menu tree and dispatch (DesignCraft `menus.rs` pattern, original code).
//! Commands are simple id+params for Phase 0; View->Brightness dispatches in the app.

use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub enum Item {
    Sep,
    Sub(String, Vec<Item>),
    Cmd {
        label: String,
        id: String,
        params: Value,
        shortcut: Option<&'static str>,
    },
}

fn cmd(label: &str, id: &str) -> Item {
    Item::Cmd {
        label: label.to_string(),
        id: id.to_string(),
        params: Value::Null,
        shortcut: None,
    }
}

fn cmd_p(label: &str, id: &str, params: Value) -> Item {
    Item::Cmd {
        label: label.to_string(),
        id: id.to_string(),
        params,
        shortcut: None,
    }
}

pub fn menu_tree() -> Vec<(&'static str, Vec<Item>)> {
    vec![
        (
            "File",
            vec![
                cmd("New Workspace", "file.new"),
                cmd("Open Workspace...", "file.open"),
                Item::Sep,
                cmd("Import Data...", "file.import"),
                Item::Sep,
                cmd("Export...", "file.export"),
                Item::Sep,
                cmd("Exit", "app.exit"),
            ],
        ),
        (
            "Edit",
            vec![
                cmd("Undo", "edit.undo"),
                cmd("Redo", "edit.redo"),
                Item::Sep,
                cmd("Copy", "edit.copy"),
            ],
        ),
        (
            "Data",
            vec![
                cmd("Refresh", "data.refresh"),
                cmd("Field Types...", "data.types"),
            ],
        ),
        (
            "Pivot",
            vec![
                cmd("Recompute", "pivot.recompute"),
                cmd("Clear Layout", "pivot.clear"),
            ],
        ),
        (
            "Format",
            vec![
                cmd("Table Theme...", "format.theme"),
                cmd("Number Format...", "format.number"),
            ],
        ),
        (
            "View",
            vec![
                Item::Sub(
                    "Brightness".to_string(),
                    vec![
                        cmd_p("Dark", "view.brightness", json!({"mode": "dark"})),
                        cmd_p("Medium Dark", "view.brightness", json!({"mode": "mediumDark"})),
                        cmd_p("Medium Light", "view.brightness", json!({"mode": "mediumLight"})),
                        cmd_p("Light", "view.brightness", json!({"mode": "light"})),
                        cmd_p("High Contrast", "view.brightness", json!({"mode": "highContrast"})),
                    ],
                ),
                Item::Sep,
                cmd("Reset Layout", "view.reset_layout"),
            ],
        ),
        (
            "Help",
            vec![cmd("About Pivot Studio", "help.about")],
        ),
    ]
}
