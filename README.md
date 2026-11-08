# Pivot Studio

A desktop pivot-table application (Windows / macOS / Linux) built with **Rust + egui/eframe**.

Pivot Studio ingests data from heterogeneous sources (CSV/XLSX/DBF/SQLite/MySQL), computes pivot layouts (rows / columns / values / filters), and produces **polished, presentation-ready tables** — editable header column 1 (stub-head), multiple footer bands, diverse table themes, and dynamic titles — exportable to **XLSX/PDF/HTML with the same theme**.

## Status

**Phase 0 (in progress):** repository restructure and egui shell. See [PIVOT_STUDIO_PLAN.md](PIVOT_STUDIO_PLAN.md) for the phased plan and acceptance criteria.

## Workspace

- `crates/pivot-core` — pivot spec/engine and the canonical themed table model (UI-agnostic)
- `crates/pivot-data` — data connectors and schema/capability negotiation
- `crates/pivot-theme` — theme tokens, presets, and role resolution
- `crates/pivot-export` — XLSX/PDF/HTML exporters consuming the themed table model
- `crates/pivot-app` — egui/eframe desktop shell

## Reference material

`reference/designcraft/` contains a preserved copy of the **DesignCraft** tree used as **UI reference only** (theme/brightness tokens, menu/chrome, docking patterns). It is **not** part of the Pivot Studio build and is excluded from this repository (`.gitignore`).

## Build

```bash
cargo run -p pivot-app
```

## License

MIT OR Apache-2.0 (see workspace metadata). Confirm third-party crate and asset licenses before redistribution.
