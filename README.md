# Pivot Studio

A desktop pivot-table application (Windows / macOS / Linux) built with **Rust + egui/eframe**.

Pivot Studio ingests data from heterogeneous sources (CSV/XLSX/DBF/SQLite; MySQL profile restore is currently disabled in this build), computes pivot layouts (rows / columns / values / filters), and produces **polished, presentation-ready tables** — editable header column 1 (stub-head), multiple footer bands, diverse table themes, and dynamic titles — exportable to **XLSX/PDF/HTML with the same theme**.

## Status

**v0.2.1** (on `feature/multi-source-relations`): multi-source data models with **lookup relations** (dataset + relation manager on the Source Data page, `.pivot` workspace **v3** with v1/v2 migration), source-data column selection, multi-worksheet projects, XLSX multi-sheet export, HTML tabbed export, PDF export of the active sheet (recompute between sheets as needed), themes/brightness, and dev-mode terminal instrumentation (`[pivot-dev]`). See [MULTI_SOURCE_RELATIONS_PLAN.md](MULTI_SOURCE_RELATIONS_PLAN.md), [PIVOT_STUDIO_PLAN.md](PIVOT_STUDIO_PLAN.md) and [V0_1_0_SOURCE_MULTI_WORKSHEET_PLAN.md](V0_1_0_SOURCE_MULTI_WORKSHEET_PLAN.md).

## Multi-source projects & lookup relations (MVP)

One project can combine several datasets — CSV/TSV, XLSX (per-sheet selection), DBF, and SQLite tables — into a single analysis table:

- **Data Model page** (`View → Source Data`, command id `nav.source`) lists datasets and relations, previews each dataset's schema (normalized column name, preserved original header, type, nullable, sampled rows), picks the **base** dataset, exposes a **projection** (which resolved fields pivots may use), and hosts the relation editor.
- **CSV delimiter auto-detection**: CSV imports sniff the delimiter (`,`, `;`, tab, `|`) from the first non-empty line (counts outside double quotes), so semicolon-separated files load correctly; TSV stays explicit tab.
- **Lookup relations** replace a code/id in the base (processed) data with a meaningful label coming from a related table, e.g. `orders.customer_id → customers.id` appending `customers.customer_name`.
  - **Row count and sums of the base data never change**: every base row is kept, no inner-join row filtering, and the relation must not multiply rows.
  - **Duplicate non-null keys in the lookup dataset are rejected** with an actionable error (deduplicate the lookup table or use a unique key) instead of fanning rows out.
  - Any number of related columns can be chosen per relation and they are appended in the related table's schema order (for example main CSV `kegiatan_id` → XLSX `id` exposing `nama`, `fungsi`, `kode_kegiatan`); unmatched/null keys produce **null values** plus a warning, never dropped rows.
  - **Key matching** compares values by canonical display (text trimmed; integer-valued decimals rendered as integers), so a text key `" 42 "` matches numeric `42`; null keys never match. Unsupported mismatches therefore yield null labels instead of silently wrong joins.
  - Legacy `inner` relations are refused because they would filter base rows.
- **Column identity**: resolved fields are exposed as `alias.column`; legacy unqualified/original-header refs still resolve and are migrated to the canonical name after a resolve.
- **Column normalization** (required for XLSX, applied to file datasets): headers are normalized to portable SQLite-compatible identifiers — whitespace/symbols → `_`, non-ASCII and otherwise unsafe characters → `_`, digit-leading names get a `_` prefix, empties become `column_<n>`, collisions get deterministic `_2`/`_3` suffixes, and SQLite keywords are suffixed (`select` → `select_col`). The original header is preserved as column metadata and shown in the schema preview.
- **Workspaces**: `.pivot` v3 stores datasets, relations, base, projection and worksheets; v1/v2 projects migrate automatically to a single dataset with no relations. MySQL dataset loading is not enabled in this build (kept attributable).

Known MVP limits: the relation editor targets lookup/enrichment only (no fact-to-fact joins), schema preview is sampled and capped (`row_limit`/`preview_rows`, loaded on a worker thread with a cancel button), projection and relation edits require **Resolve Data Model** to take effect, and large-data materialization is bounded by load caps rather than a dedicated stress-tested path.

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
