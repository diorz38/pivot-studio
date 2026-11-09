//! Dev-mode terminal logging for tracing worksheet/project state bugs.
//!
//! Enable with `PIVOT_DEVLOG=1` (or any non-empty value) when running the app.
//! Example: `PIVOT_DEVLOG=1 cargo run -p pivot-app`

use std::sync::OnceLock;

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("PIVOT_DEVLOG")
            .map(|v| !v.trim().is_empty() && v.trim() != "0")
            .unwrap_or(cfg!(debug_assertions))
    })
}

pub fn log(args: std::fmt::Arguments<'_>) {
    if enabled() {
        eprintln!("[pivot-dev] {}", args);
    }
}

#[macro_export]
macro_rules! devlog {
    ($($arg:tt)*) => {
        $crate::devlog::log(format_args!($($arg)*))
    };
}