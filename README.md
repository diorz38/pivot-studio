# Pivot Studio

A desktop pivot-table application (Windows / macOS / Linux) built with **Rust + egui/eframe**.

Pivot Studio ingests data from heterogeneous sources (CSV/XLSX/DBF/SQLite; MySQL profile restore is currently disabled in this build), computes pivot layouts (rows / columns / values / filters), and produces **polished, presentation-ready tables** — editable header column 1 (stub-head), multiple footer bands, diverse table themes, and dynamic titles — exportable to **XLSX/PDF/HTML with the same theme**.

## Status

**v0.2.1** (on `feature/source-mgmt-multi-worksheet`): source-data column selection (pre-worksheet management page), multi-worksheet projects (`.pivot` workspace v2 with v1 migration), XLSX multi-sheet export, HTML tabbed export, PDF export of the active sheet (recompute between sheets as needed), themes/brightness, and dev-mode terminal instrumentation (`[pivot-dev]`). See [PIVOT_STUDIO_PLAN.md](PIVOT_STUDIO_PLAN.md) and [V0_1_0_SOURCE_MULTI_WORKSHEET_PLAN.md](V0_1_0_SOURCE_MULTI_WORKSHEET_PLAN.md).

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
