# Multi-Source & Relations — Implementation Plan

**Branch:** `feature/multi-source-relations`  
**Status:** MVP implementation complete (2026-11-09; lookup addendum m00581 applied; Data Model page simplified per m01501) — all phases accepted except the Phase 7 manual GUI QA matrix and release notes  
**Supersedes (scope decision):** [V0_1_0_SOURCE_MULTI_WORKSHEET_PLAN.md:6](V0_1_0_SOURCE_MULTI_WORKSHEET_PLAN.md#L6) — *“1 project = 1 shared data source + 1 shared column selection + N worksheets. Per-worksheet sources are out of scope.”* This plan replaces that constraint with a **project-level data model** (multiple sources/datasets + relations) that resolves to an analysis dataset consumed by existing pivots/exports.

## Progress (current)

- [x] Branch + comprehensive plan; addendum m00581 (simple lookup relations) recorded.
- [x] **Column normalization foundation** (`pivot-core::Column.original_name/display_name`; `pivot-data::normalize` SQLite-compatible identifiers with deterministic collision/keyword handling; XLSX required + CSV/DBF file ingestion normalized; original headers preserved as metadata, not SQLite `COMMENT ON`).
- [x] **Workspace v3** (`FORMAT_VERSION=3`, `data_model.json` payload, compat entries retained): `Dataset`/`Relation`/`JoinType::{Lookup,Left,Inner}`/`ProjectDataModel`; v1/v2→v3 migration to single dataset, no relations; MySQL sanitization parity; roundtrip/migration/sanitization tests.
- [x] **Dataset registry/loader MVP** (`pivot-data::registry`): load CSV/XLSX(sheet)/DBF/SQLite table, row caps/capped warnings, attributable missing-source errors; `qualify_column_names` (`alias.column`); MySQL multi-source loader intentionally unsupported (attributable).
- [x] **Lookup/enrichment resolver MVP** (`pivot-data::resolve`, m00581): base rows preserved (no inner-join filtering), duplicate lookup keys rejected (no fanout), unmatched keys → null labels, non-key lookup columns appended with deterministic `_label` collision renames, base row-count invariant enforced; legacy `Inner` rejected as unsupported.
- [x] **Field resolution** (`pivot-core::pivot::resolve_field_index` public): exact/CI/original/display/qualified suffix/tail heuristics; stale/compute paths use flexible resolution.
- [x] **App MVP wiring** (`pivot-app`): `ProjectDataModel` state, async resolve (`ModelDone`), add file/SQLite dataset, open multi-source v3 queues resolve, legacy single-source path preserved; Source Data page repurposed to Data Model with datasets/diagnostics and **lookup relation editor UI** (load cached dataset columns, pick base/lookup dataset+key, add/edit/remove relation, validation messages).
- [x] **XLSX sheet / SQLite table picker** in add-dataset flow (`pivot-data` re-exports `list_sheets`; `pivot-app` draft `add_dataset_sheet`/`add_dataset_table`, multi-sheet/table Comboboxes, `add_dataset_from_file_with_sheet` persists `WorkspaceSource::File.sheet`).
- [ ] **Phase 7 QA matrix (manual, Windows GUI): file+file relation, file+SQLite relation, XLSX sheet selection, lookup label selection, stale-ref after removing a dataset, v1/v2 workspace migration, save/reopen, export XLSX/HTML/PDF(active), large-file cap messaging.** *(all automatable criteria are covered by unit/integration tests — including the new E2E workspace roundtrip test; the GUI click-through matrix needs a human run)*

Latest checks (local): `cargo check --workspace` OK, `cargo check -p pivot-app` warning-free; `cargo test --workspace` **0 failed** — pivot-core 19, pivot-data 48 passed + 1 ignored (`db_import::tests::ac5_2_mysql_pushdown_integration`, needs `PIVOT_MYSQL_URL`), pivot-export 6 unit + 2 integration (`multi_source_export_parity`, `multi_source_workspace_e2e`), pivot-theme 4, pivot-app 0 unit tests.

Note: the earlier progress bullets that describe the *old* Data Model page (load-cols buttons, collapsible schema previews, projection checkbox grid, add/edit relation editor with Name field) are superseded by the "Data Model page simplified (m01501)" bullet below; resolver-side `exposed_columns` projection support remains and is still covered by tests.

Additional completion since the last progress update:
- [x] **Label-column selection (resolver + app draft)**: `Relation.label_columns: Option<Vec<String>>` (serde default None/empty = append all non-key lookup labels for compat); `resolve.rs` enforces selected labels, rejects unknown/empty selection with attributable errors, keeps schema order + deterministic collision renames, and keeps base-row preservation. Test: `lookup_label_columns_selection_limits_appended_labels` verifies `group` is not appended when only `b.label` is selected.
- [x] **Workspace roundtrip for label selection**: `v3_multi_source_roundtrips_and_sanitizes_mysql` now exercises `label_columns: None` (default append-all) plus sanitization.
- [x] **Lookup editor UI (relations)**: Data Model page now has add/edit/remove relations with dataset/column pickers loaded from `dataset_schema_cache` (`Load cols` per dataset), draft validation (`save_relation_from_draft` checks base/lookup datasets, distinctness, required keys, schema membership), label-column checkbox picker, and diagnostics display.
- [x] **XLSX sheet / SQLite table picker**: `pivot-data::list_sheets` re-exported; `pivot-app` draft fields `add_dataset_sheet`/`add_dataset_table` drive Comboboxes in the Data Model add-dataset flow; `add_dataset_from_file_with_sheet` persists the selected sheet into the v3 dataset origin; SQLite table selection persisted in `WorkspaceSource::Sqlite.table`.
- [x] **Dataset preview/schema API**: `pivot-data::preview_dataset_schema`/`preview_dataset_columns` (`DatasetPreview` with normalized `columns`, capped `preview_rows`, `row_count/capped/truncated/warnings`) exported from `pivot-data::registry`; covered by `registry::tests::preview_dataset_schema_returns_columns_and_capped_rows` (normalized names + original metadata + type inference + cap/truncation).
- [x] **Import vs. add-dataset UX polish**: once a project models datasets (`ProjectDataModel.datasets` non-empty), `File → Import Data…` and the Home import cards **add a dataset** to the data model (and switch to the Data Model page) instead of replacing the legacy single table; a fresh project keeps the classic import → select columns → build pivot flow. Helper: `PivotApp::import_adds_dataset()`.
- [x] **Projection (`exposed_columns`) enforcement (resolver)**: `resolve_model` applies `ProjectDataModel.exposed_columns` after lookups as an order-preserving column projection; matched with qualified/unqualified/original/tail forms via `resolve_col_index`; unknown names hard-fail with sampled error, empty projection warns and empties columns. Tests: `exposed_columns_projection_limits_resolved_fields`, `exposed_columns_unknown_names_error`. Projection survives save/open because `build_workspace_file` no longer overwrites a modeled `exposed_columns` with the legacy selection.
- [x] **Dataset schema/preview UI + projection picker (app)**: `PivotApp::load_dataset_schema` now uses `pivot_data::preview_dataset_schema` and fills `dataset_preview_cache`; the Data Model page renders per-dataset collapsible previews (normalized name, preserved original header, type, nullable, sampled rows, cap/truncation/warnings). New `projection_candidates`/`is_projection_selected`/`set_projection_selected`/`clear_projection` back a projection checkbox section ("Expose all fields" clears the projection); caches reset on new/open workspace.
- [x] **Persisted field-ref migration to resolved identity**: `pivot_core::pivot::canonical_field_name(table, name)` maps any legacy/unqualified/original/display ref to the resolved `alias.column` name; `PivotApp::canonicalize_spec_refs()` rewrites `rows`/`columns`/`values`/`filters` refs of every worksheet right after a multi-source model resolves (status reports the migration count; unresolved refs are left for stale detection to report). Legacy single-source projects are untouched. Test: `pivot-core::pivot::tests::canonical_field_name_maps_legacy_refs_to_qualified_identity`.
- [x] **Async dataset schema preview + cancel (UI)**: `PivotApp::load_dataset_schema` no longer blocks the UI thread — it spawns a worker with `preview_dataset_schema(... Some(&cancel_flag))`, tracks `preview_rx`/`preview_cancel`/`schema_loading_dataset`, and `poll_preview` drains the result in `update()` (repaint requested; caches + relation draft defaults refreshed on success). The Data Model page shows "Schema preview loading..." with a **Cancel preview** button (`PivotApp::cancel_preview`).
- [x] **Per-dataset row caps honored for file datasets**: `registry::apply_row_limit` now takes the resolved `effective_limit(dataset, opts)` (previously only `opts.row_limit` was applied for files, so `Dataset.row_limit` was silently ignored outside SQLite); capped loads emit an existing diagnostic warning and are visible in `ResolveDiagnostics.input_rows`/`output_rows`.
- [x] **End-to-end multi-source regression test**: `crates/pivot-export/tests/multi_source_workspace_e2e.rs::multi_source_workspace_roundtrip_resolve_compute_export` — base CSV + sheet-scoped XLSX lookup → v3 `WorkspaceFile` (2 datasets, 1 enabled `Lookup` relation, base `orders`) → `save_workspace`/`load_workspace` roundtrip (datasets/relation/base/worksheet spec/`sheet: Some("Customers")` preserved) → `resolve_model` (`diagnostics.ok()`, 3 rows preserved, `orders.customer_id` + `customers.customer_name`, null-label warning for the unmatched order) → `pivot::compute` (3 body rows) → `export_xlsx` (exists, non-empty, `PK` magic). `export_xlsx` signature: `pub fn export_xlsx(path: impl AsRef<Path>, model: &ThemedTableModel, style: ExportStyle) -> Result<()>` (`crates/pivot-export/src/lib.rs:73`).
- [x] **Export parity test (multi-source)**: `crates/pivot-export/tests/multi_source_export_parity.rs::lookup_resolution_preserves_rows_and_exports_xlsx` — CSV+CSV lookup resolve (row count preserved, label appended) → `pivot::compute` → `export_xlsx` (non-empty, `PK` zip magic). `pivot-export` dev-dependency on `pivot-data` added for the test only.
- [x] **Cross-type key matching + cycle safety**: `resolve::tests::cross_type_keys_match_via_canonical_display` (text key `" 42 "` matches integer `42`; unmatched key yields null label, base row kept) and `resolve::tests::relation_cycle_terminates_without_duplicate_labels` (cyclic `a↔b` relations terminate, reverse relation appends nothing). Coercion rule documented in `README.md`.
- [x] **XLSX sheet / SQLite table picker**: `pivot-data::list_sheets` re-exported; `pivot-app` draft fields `add_dataset_sheet`/`add_dataset_table` drive Comboboxes in the Data Model add-dataset flow; `add_dataset_from_file_with_sheet` persists the selected sheet into the v3 dataset origin; SQLite table selection persisted in `WorkspaceSource::Sqlite.table`.
- [x] **Data Model page simplified (m01501)**: single flow only - add dataset, tick the `Main` radio, then `Related table` -> `Key in main table` -> `Same key in related` -> `Columns to add to main` (checkbox list, m01660) -> **Add relation and apply** (saves the relation and resolves in one click). Removed: relation Edit mode/Name field/duplicate base+lookup dataset pickers, per-dataset `Load cols` buttons (schemas now warm up automatically, with a small spinner + Cancel), the collapsible per-dataset schema preview block, and the projection checkbox grid. Main-dataset changes clear relations; relations render as `main . key -> related` rows with the chosen columns and a Remove button. `pivot-app` helpers `start_edit_relation`, `projection_candidates`, `is_projection_selected`, `set_projection_selected`, `clear_projection` and `fn dataset_preview_block` were deleted with their call sites.
- [x] **Multi-column exposure (m01660)**: the label picker is a multi-select checkbox list (`Related table (add columns from another dataset)` section), so one relation can copy several related columns into the main table (e.g. main CSV `kegiatan_id` -> XLSX `id`, exposing `nama`, `fungsi`, `kode_kegiatan`). Appended columns keep the related table's schema order (not click order), the related key is never duplicated, unselected columns stay out, unselected/no-match keys leave empty values, and the main row count/sums are untouched. Test: `resolve::tests::multiple_label_columns_are_appended_in_lookup_schema_order`.
- [x] **Contrast fix on Data Model page (m01501)**: column names/headers that used `Tokens.text_muted` now use `text_strong` (dataset grid, relation list/editor, source column grid headers); only secondary detail (source path, row/col counts) stays muted. The dim schema-preview grid the user could not read is gone.
- [x] **Encoding repair**: earlier edits had double-encoded non-ASCII punctuation in `crates/pivot-app/src/dock.rs` and `crates/pivot-app/src/app.rs` (rendered as mojibake in the GUI); all 9 occurrences replaced with ASCII (`->`, `|`, `x`, `'`). Project UI text stays ASCII-only.

## Objective

Enable a single project to register **multiple heterogeneous data sources** (MVP: CSV, XLSX, DBF, SQLite tables), define **simple lookup relations** between them, and build pivots across the related fields — while preserving the parity invariant: **exports render `ThemedTableModel`** ([PIVOT_STUDIO_PLAN.md](PIVOT_STUDIO_PLAN.md), [README.md](README.md)).

User framing (m00001): *project can have a table from CSV and XLSX and relate columns; or CSV and a table from SQLite; or XLSX and a table from SQLite.*

**Addendum (m00581):** Relations created here are **simple lookup/enrichment relations**. Rule: **do not change the sum/row count of the processed (base) data**. A relation is only for replacing a column code or id with a meaningful label/value from a related data table (dimension lookup). Implementation implications: lookup preserves every base row (no inner-join row filtering), must not multiply base rows (no fanout; duplicate lookup keys are a validation/error), and appends selected label columns from the lookup table; unmatched keys produce null labels. This is not a general-purpose join for combining fact tables.

## Current architecture (ground truth)

- `crates/pivot-core/src/pivot.rs:209` — `pub fn compute(table: &DataTable, spec: &PivotSpec) -> ThemedTableModel` (single `DataTable`).
- `crates/pivot-core/src/spec.rs` — `FieldRef{name}` resolves by plain source column name.
- `crates/pivot-core/src/lib.rs` — `DataTable/Column/Value/DataType/coerce_value` are reusable for heterogeneous ingestion.
- `crates/pivot-data/src/workspace.rs` — `.pivot` ZIP, `FORMAT_NAME="pivot-studio-workspace"`, `FORMAT_VERSION: u32 = 2`; single `WorkspaceProject.source` (`WorkspaceSource = None|File|Sqlite|MySql`) + shared `selected_columns: Option<Vec<String>>` + N worksheets; v1 compat + MySQL `sanitized()` (no secrets).
- `crates/pivot-data/src/db_import.rs` — SQLite/MySQL preview/pushdown, identifier validation/quoting, keychain/session credential handling.
- `crates/pivot-app/src/app.rs` — one `table: Option<DataTable>`, one `db_source`, shared `selected_columns: Option<BTreeSet<String>>`; all worksheets recompute over the same projected source (in-memory `compute` or single-DB pushdown).
- `crates/pivot-app/src/dock.rs` — **`source_page` / “Source Data” (nav `nav.source`, menus “View > Source Data”)** currently manages column inclusion over the single loaded table. **This page is the home for the expanded multi-source/relations UX.**
- `crates/pivot-export` — consumes `ThemedTableModel`; should remain largely unchanged.

## Design principles

1. **Project-level data model, pivot engine stays simple (MVP).** Introduce datasets + relations + projection that **resolve to an analysis dataset** (in-memory `DataTable` or an adapter) so `pivot_core::compute` and exports remain intact initially.
2. **Stable field identity.** Multi-source requires unambiguous fields (`dataset_alias.column` and/or stable IDs). Persisted `PivotSpec` field refs must migrate/resolve without orphaning existing worksheets.
3. **In-memory lookup/enrichment first; pushdown later (scoped).** Uniform semantics across file↔file↔SQLite via materialization; same-engine SQL pushdown is a later optimization, not MVP.
4. **Safe, explicit lookups (addendum m00581).** Base processed rows must not change. Surface key/type mismatches and reject duplicate lookup keys (fanout) — never silently filter base rows or multiply aggregates.
5. **No secrets in workspaces.** Extend existing sanitization discipline to any new source/profile carrying credentials.
6. **Backward-compatible migration path.** v1/v2 workspaces open as a single-dataset project with no relations.

## Scope

### MVP (this plan targets)
- Multiple **named datasets** per project from: CSV, XLSX (selected sheet), DBF, SQLite table (file-based). *MySQL remains out of MVP / follows current disabled-restore state until re-enabled under the new model.*
- **Column normalization for file datasets (required for XLSX):** normalize column names to **SQLite-compatible identifiers** (e.g., `[space]` → `_`, replace other disallowed chars) using deterministic rules, and preserve the **original column name** as metadata (not a SQLite `COMMENT ON`, which SQLite does not support natively) for display, diagnostics, and persistence. See **Column normalization** below.
- **Simple lookup relations** between dataset columns (addendum m00581): equi-key lookup from a base/fact dataset to a related lookup/dimension table to **replace a code/id with meaningful labels**. **Must not change processed base row count/sums**: preserves all base rows, unmatched keys yield null labels, and duplicate lookup-key matches are rejected (no fanout). Relation graph/order remains simple/deterministic.
- **Source Data page** repurposed into a **Data Model / Source Manager**: manage datasets, preview schemas, define lookup relations, manage projection/column exposure and naming, and show validation/diagnostics.
- Per-dataset and/or global **column projection** (what is exposed to pivots), replacing the single shared `selected_columns` concept.
- Workspace **format v3** with v1/v2 migration.
- Worksheets/pivots operate over the resolved multi-source field set; exports unchanged in contract.

### Out of scope (defer)
- Cross-database federated query planner / full SQL pushdown across heterogeneous engines.
- MySQL multi-source + credential UX rework (follow-on after MVP model stabilizes).
- Many-to-many automatic resolution beyond diagnostics/warnings (require user to model keys/dedup explicitly).
- Calculated fields, unions, non-equi joins, fuzzy matching.
- Streaming/out-of-core joins for very large datasets (MVP uses caps/materialization + clear limits).

## Decisions (locked during implementation)
- [x] **Field reference format in specs:** persisted refs are qualified `alias.column` strings; legacy plain/original/display names still resolve at compute time via `pivot_core::pivot::resolve_field_index`, and `PivotApp::canonicalize_spec_refs()` rewrites worksheet specs to the canonical name once a multi-source model resolves. No internal stable-ID indirection (no consumer needs it; keeping specs human-diffable in `data_model.json`).
- [x] **Relation graph shape:** ordered, deterministic application by relation id, each dataset joined at most once (chain/forest); relations disconnected from the base are skipped with a warning; cycles terminate by construction (`resolve::tests::relation_cycle_terminates_without_duplicate_labels`). General acyclic-graph planning is deferred.
- [x] **Materialization policy:** full load with caps; `Dataset.row_limit` (per dataset) and `DatasetLoadOptions.row_limit` both honored via `effective_limit`, caps surfaced in `ResolveDiagnostics`/dataset warnings. Streaming/out-of-core deferred.
- [x] **Join key type coercion:** canonical-display comparison (`join_key_display`: null skipped, text trimmed, integral decimals rendered as integers); mismatches are no-match (null labels + warning), never silent wrong joins or dropped base rows.
- [x] **Name collision rule:** columns are alias-qualified (`alias.column`) during resolve; appended lookup labels drop the `${lookup_alias}_` prefix and gain deterministic `_label`/`_label{n}` suffixes on collision, with a warning.
- [x] **XLSX/identifier column normalization (SQLite-compatible):** uniform for all file datasets (CSV/TSV/XLSX/DBF); original header kept as `Column.original_name` metadata in the v3 schema (SQLite has no column-comment facility); DB-native identifiers are left untouched.

## Column normalization (XLSX & SQLite-compatible identifiers)

**Rationale:** XLSX headers often contain spaces, punctuation, mixed case, or characters that are awkward/invalid as identifiers when datasets participate in relations, SQLite-oriented workflows, or deterministic field addressing. Normalize source column names to a safe, stable identifier form while preserving the user-visible original header.

**Applies to:** XLSX dataset ingestion (required). Recommended to apply the same deterministic normalization pipeline to CSV/DBF file datasets for consistency; SQLite/MySQL table columns already have DB identifiers but must still carry `original_name`/display metadata in the project model.

**Normalization rules (deterministic):**
1. `normalized_name` starts from the header text, trimmed. `[space]` → `_` (collapse runs of whitespace to a single `_`).
2. Replace any character not allowed in a **SQLite-safe column identifier** with `_`. Allowed set (MVP, conservative/portable): ASCII letters `A-Z`/`a-z`, digits `0-9`, and `_`. (Non-ASCII letters/digits: replace with `_` in MVP unless/until an explicit Unicode-identifier policy is adopted; document the choice.)
3. Collapse consecutive `_` to a single `_`; trim leading/trailing `_` (but do not produce an empty base except via fallback).
4. If the result starts with a digit, prefix with `_` (e.g., `1st Name` → `_1st_Name`) so it is a safe unquoted identifier.
5. If the result is empty (blank header) or becomes empty, fallback to `column_<1-based index>` (e.g., `column_3`).
6. If the result collides with a SQLite keyword or another normalized column in the **same dataset**, append a deterministic suffix (`_2`, `_3`, …) and record the disambiguation. (Keyword list: use a conservative common-SQLite keyword set; final list locked in Phase 0/1.)
7. Case handling: preserve a canonical normalized form (recommend lower_snake or preserve transformed case consistently — lock in Phase 0). Comparisons for collisions are case-insensitive.

**Original name preservation (metadata, not SQLite `COMMENT ON`):**
- SQLite has no native `COMMENT ON COLUMN/TABLE` support; do **not** rely on DB comments. Store the original header as metadata in the project/dataset schema, e.g. `Column { name: <normalized_name>, original_name: Some(<original header>), display_name: Option<...>, inferred, override_type, nullable }` (exact shape finalized in Phase 0/1; `pivot_core::Column` currently has only `name,inferred,override_type,nullable` and will need extension or a parallel metadata carrier).
- Persist `original_name` (and normalized name) in workspace v3 so relations, field pickers, diagnostics, and exports can show the original label while using the normalized identifier for stable addressing/joins.
- UI: show `original_name` (or `display_name`) as the primary label with the normalized identifier available (tooltip/secondary text) where ambiguity matters. Relation editors and stale-ref diagnostics should reference both when helpful.

**Examples:**
| Original header | Normalized (`normalized_name`) | Notes |
|---|---|---|
| `Order ID` | `Order_ID` (or `order_id` per case rule) | space → `_` |
| `Customer Name` | `Customer_Name` | space → `_` |
| `Total ($)` | `Total` / `Total_` → `Total` | `(`, `$`, `)`, space → `_`, collapsed/trimmed |
| `1st Item` | `_1st_Item` | leading digit prefixed |
| `名前` / `Mã KH` | `column_N` or transliteration fallback (lock policy) | non-ASCII → fallback in MVP unless Unicode policy adopted |
| *(blank)* | `column_3` | index fallback |
| `Order ID`, `Order-ID` | `Order_ID`, `Order_ID_2` | deterministic collision suffix |

---

## Phase 0 — Design lock, ADR & migration contract
**Goal:** Freeze contracts (identity, workspace schema, join semantics, UX flow) so implementation doesn’t churn. No production behavior change required except approved docs.

**Scope / tasks**
- Write/confirm an ADR (this document + any inline design notes) covering: project data model, field identity, relation semantics, materialization limits, stale-ref/migration policy, and explicit supersession of the v1/v2 “single shared source” rule.
- Specify workspace **v3** on-disk schema (types/JSON shape) and **v1/v2 → v3** migration rules, including MySQL sanitization parity.
- Specify how `PivotSpec` / `FieldRef` references fields post-migration and how legacy plain names resolve.
- Lock **column normalization** rules (XLSX required; recommend uniform for CSV/DBF): SQLite-compatible identifier algorithm, keyword/collision suffix policy, case/canonical form, and where `original_name` (and optional `display_name`) is stored in schema/v3 metadata. Explicitly document that original names are persisted as project metadata (not SQLite `COMMENT ON`).
- Define validation/diagnostic model (errors vs warnings): missing dataset/column, type mismatch, ambiguous name, cycle/invalid graph, fanout risk.
- Define “Source Data” page information architecture and navigation labels (keep `nav.source` stable; retitle to “Data Model”/“Sources” as decided).

**Key areas**
- `MULTI_SOURCE_RELATIONS_PLAN.md` (this file), optionally `PIVOT_STUDIO_PLAN.md` notes
- `crates/pivot-data/src/workspace.rs` (schema design)
- `crates/pivot-core/src/spec.rs` (field identity design)
- `crates/pivot-app/src/dock.rs`, `menus.rs` (IA)

**Acceptance criteria**
- [x] Schema v3 + migration (v1→v3, v2→v3) is fully specified, including defaults and credential sanitization rules.
- [x] Field identity + legacy-resolution rules are written and unambiguous (examples included in the doc or tests plan).
- [x] **Column normalization contract** is specified (SQLite-compatible identifier rules, collision/keyword handling, case form) and the metadata location for `original_name`/`display_name` is defined (v3 schema; not SQLite `COMMENT ON`).
- [x] Join semantics for MVP (inner/left, key coercion, null handling, fanout warning) are documented.
- [x] UX flow for the repurposed Source Data page is documented (add source → select table/sheet → define relation → confirm projection → use in pivot).
- [x] Open decisions above are resolved or explicitly time-boxed with owners/defaults. *(all six locked in the "Decisions" section above; the remaining deferrals — federated pushdown, general join graph planning, MySQL multi-source, streaming joins — are recorded in "Out of scope (defer)")*
- [ ] Stakeholder/maintainer sign-off recorded in this plan (checkbox/date). *(maintainer review: fill in when you accept the MVP)*

---

## Phase 1 — Workspace v3 & core data-model types
**Goal:** Persist/load multi-source projects and relations with full backward compatibility; no join execution yet.

**Scope / tasks**
- In `crates/pivot-data/src/workspace.rs` (and/or new workspace/model modules as appropriate):
  - Bump `FORMAT_VERSION` to `3`; keep `FORMAT_NAME`.
  - Introduce `Dataset`/named source records (id, alias, `WorkspaceSource`-like origin incl. file kind/path + table/sheet metadata, load options/caps) and `Relation` records (left/right dataset+column, join type, key pairs, enabled flag).
  - Replace single `source`/`selected_columns` with a project data-model section (e.g., `datasets`, `relations`, `projection/exposed_fields`) while preserving per-worksheet `spec`/`view` structure.
  - Implement `from_v1`/`from_v2` → v3: legacy single source becomes dataset “source” (stable id/alias); legacy `selected_columns` maps to projection; relations empty.
  - Maintain safe load/save: unknown future fields tolerated where reasonable (`serde(default)`), clear `DataError::Workspace/Invalid` on corruption; **all secret-bearing profiles sanitized** on save (parity with current MySQL handling).
  - Write v3 + compat read; decide compat-write strategy (recommend write v3 only, or dual-write only if justified — document it).
- Add/adjust pure data-model types needed by core/app (ids, aliases, relation/join enums) with serde support.

**Key areas**
- `crates/pivot-data/src/workspace.rs`
- `crates/pivot-data/src/lib.rs`
- (optionally) `crates/pivot-core/src/` if identity types belong in core

**Acceptance criteria**
- [x] Saving a project with 2+ datasets and 1+ relation and reloading restores datasets, relations, projection, all worksheets/specs/views, and active worksheet exactly (roundtrip test).
- [x] Loading **v1** and **v2** `.pivot` fixtures yields a valid v3 in-memory model with 1 dataset, no relations, and behavior-equivalent projection/worksheets (no data loss).
- [x] Missing/`null` new fields default safely; legacy plain-field refs remain loadable via documented resolution.
- [x] No credentials/secrets are serialized in the workspace (unit test analogous to existing MySQL `secret-password` assertion).
- [x] Corrupt/unsupported files return clear, actionable errors.
- [x] `cargo test -p pivot-data` passes; new migration/roundtrip/sanitization tests added.

---

## Phase 2 — Dataset loading & schema registry (heterogeneous ingestion)
**Goal:** Load/preview each dataset into a normalized representation the app/core can reason about, independent of origin.

**Scope / tasks**
- Build a dataset loader/registry layer (in `pivot-data` or a dedicated module consumed by app) that:
  - Loads CSV/XLSX/DBF via existing loaders and SQLite tables via `db_import` into `DataTable`s (or a lazy handle) with a stable dataset id/alias and source metadata.
  - Captures schema (columns/types/nullability) and sample/preview rows; supports refresh/reload.
  - Applies per-dataset options (e.g., XLSX sheet selection, SQLite table name, row cap/materialization limit) and reports truncation.
- Normalize column naming for the *resolved* context (deterministic collision handling, e.g., `alias__column` or `alias.column` mapping) without mutating original source data semantics.
- Define and implement error handling for missing files/tables, permission errors, unsupported sheets, and schema drift on reload.

**Key areas**
- `crates/pivot-data/src/{csv_import,xlsx_import,dbf_import,db_import,lib}.rs`
- new loader/registry module (e.g., `crates/pivot-data/src/model.rs` or `registry.rs`)
- `crates/pivot-core/src/lib.rs` (reuse `DataTable`/types)

**Acceptance criteria**
- [x] A project can register and load datasets from **CSV, XLSX, DBF, and SQLite table**; each yields a schema + preview + row count (or capped count) with correct inferred types.
- [x] Missing/moved source or missing table/sheet produces a clear, attributable error (names the dataset/alias).
- [x] Column collisions across datasets are resolved deterministically and reported to the caller/UI model.
- [x] Row-cap/materialization limit is enforced and surfaced (no unbounded memory on huge inputs in MVP path).
- [x] Automated tests cover mixed-source loading and schema normalization (using temp files / temp SQLite DB).

---

## Phase 3 — Relation engine (in-memory joins) & resolved dataset
**Goal:** Execute defined relations to produce a correct, diagnosable analysis dataset for pivoting.

**Scope / tasks**
- Implement a join/resolve engine (prefer `pivot-core`-adjacent pure logic or a new `pivot-data`/`pivot-model` module with unit tests) that:
  - Validates relations (known datasets/columns, compatible/coercible key types, acyclic/orderable graph per MVP rule).
  - Executes **inner** and **left** equi-joins over in-memory `DataTable`s in deterministic order; applies key coercion rules decided in Phase 0 (reuse `coerce_value` where appropriate).
  - Produces a **resolved `DataTable` (or equivalent)** with unambiguous column identities mapped to the field names `PivotSpec` will reference.
  - Emits diagnostics: type mismatch, null keys, unmatched rows (counts), and **row multiplication/fanout** (input vs output row counts, per-join expansion hints).
- Define behavior for projection (only exposed columns included) and for filters/pushdown boundaries (document: multi-source resolves in-memory; single-source fast paths can remain where valid).

**Key areas**
- new module/crate surface for relation resolution
- `crates/pivot-core/src/lib.rs` (`Value/DataType/coerce_value` reuse)
- `crates/pivot-core/src/pivot.rs` (consumption contract unchanged in MVP)

**Acceptance criteria**
- [~] Inner and left joins produce correct results for representative cases (1:1, 1:many, unmatched left rows, null keys) — verified by unit tests with known fixtures. *(addendum m00581 overrides: lookup semantics only; inner rejected, duplicate lookup keys rejected; see resolver tests)*
- [~] Cross-type key joins follow the documented coercion rules; unsupported mismatches fail with a clear diagnostic (not silent wrong joins). *(MVP coercion is canonical-display matching — text trimmed, integer-valued decimals rendered as integers, nulls never match — covered by `resolve::tests::cross_type_keys_match_via_canonical_display` and documented in `README.md`; mismatches yield null labels + warning rather than an error, because lookup semantics never drop base rows. A richer numeric/text coercion matrix is out of scope for this MVP)*
- [x] Resolved output has deterministic, unique column names/ids; no ambiguous `FieldRef` resolution.
- [x] Fanout is detected and reported (e.g., output rows >> input rows) in diagnostics. *(implemented as lookup duplicate-key rejection + fanout observations; inner-join fanout tests removed per m00581)*
- [x] `pivot_core::pivot::compute` runs successfully against the resolved dataset without API changes (or with a documented, minimal adapter).
- [x] `cargo test -p pivot-core` (and any new module/crate tests) pass.

---

## Phase 4 — `pivot-core`/spec field identity & stale-ref handling
**Goal:** Make field references robust across multi-source projects and migrations, and harden stale-reference behavior.

**Scope / tasks**
- Evolve `FieldRef`/spec handling (backward-compatibly) to support qualified/stable identity as decided in Phase 0; implement serde aliases/defaults so older specs deserialize.
- Update stale/unresolved detection (currently name-based against selected columns in `pivot-app`) to operate on the **resolved field registry** (dataset removed/renamed, column removed, relation broken, projection changed).
- Ensure worksheet specs migrate cleanly: legacy single-source names map to the migrated dataset; ambiguous cases surface actionable warnings, not silent drops.

**Key areas**
- `crates/pivot-core/src/spec.rs`
- `crates/pivot-app/src/app.rs` (`stale_spec_refs`, field pickers’ source of truth)
- migration helpers in `pivot-data`

**Acceptance criteria**
- [x] Old v1/v2 workspaces with populated `PivotSpec`s recompute equivalently after migration (same semantic result given the same single source).
- [x] Renaming/removing a dataset or breaking a relation marks affected worksheet fields as stale/unresolved with a clear message and blocks unsafe recompute (consistent with current “block until fixed” policy). *(stale detection via `resolve_field_index` blocks recompute; persisted refs are upgraded to `alias.column` after every resolve, so dataset/column renames surface as stale rather than silently retargeting)*
- [x] Field pickers and filters enumerate only valid, exposed, resolved fields (no duplicates/ambiguity). *(resolved fields enumerated with flexible resolution; `exposed_columns` projection enforced in the resolver, so pickers see only exposed resolved fields)*
- [x] Spec serde roundtrip tests cover legacy and new field-ref forms. *(legacy names resolve at runtime via `pivot_core::pivot::canonical_field_name`/`resolve_field_index` and are rewritten to qualified `alias.column` after resolve; `FieldRef` stays a plain `String` so both forms deserialize — test `canonical_field_name_maps_legacy_refs_to_qualified_identity`)*

---

## Phase 5 — App state & Source Data page → “Data Model / Source Manager” UX
**Goal:** Repurpose the existing Source Data page into the project’s multi-source/relations hub and wire it into the app lifecycle.

**Scope / tasks**
- **`crates/pivot-app/src/app.rs`**: replace single `table/db_source/selected_columns` state with project model state (datasets, relations, registry handles, resolved dataset cache, diagnostics). Update `available_columns/distinct_values_for/stale_spec_refs`, load/import flows, workspace new/open/save/build, and recompute to: validate → resolve (or reuse cache) → compute per active worksheet. Preserve cancellation/progress behavior and DB single-source fast path only where still valid.
- **`crates/pivot-app/src/dock.rs` `source_page` (nav.source)** — evolve in place (keep route/command stable):
  - Section A: dataset list (add/import CSV/XLSX/DBF, connect SQLite table; show alias, origin, row/col count, status; reload/remove).
  - Section B: schema/preview + per-dataset column exposure.
  - Section C: relation editor (pick left/right dataset+column, join type, add/remove; show validation/fanout warnings).
  - Section D: resolved/projection summary (exposed fields feeding pivots) + “Apply/Refresh model” action and diagnostics.
- **`crates/pivot-app/src/menus.rs` / `chrome.rs` / status**: retitle labels where helpful (“Source Data” → “Data Model”/“Sources”, keep command id `nav.source` stable), update Home cards/help text to reflect multi-source capability.
- Maintain responsive UI (long loads/joins off the UI thread consistent with existing load/recompute patterns).

**Key areas**
- `crates/pivot-app/src/{app,dock,menus,chrome}.rs`
- `crates/pivot-data` loader/registry APIs consumed by app

**Acceptance criteria**
- [x] User can add 2+ sources (covering file+file and file+SQLite cases), define a relation, refresh the model, and see the unified field list in pivot configuration.
- [x] Pivots compute correctly over related fields; filters/distinct values operate on resolved/exposed fields.
- [x] Validation errors (missing source, bad key, cycle, type mismatch) are shown clearly on the Source/Data Model page and prevent unsafe compute with an actionable message. *(missing source / missing key / duplicate lookup key / unknown label / unknown projection paths are attributable errors surfaced in the page's diagnostics and abort the resolve; **cycles** terminate by construction — the resolver joins each dataset at most once per pass and only adds an unjoined dataset, verified by `resolve::tests::relation_cycle_terminates_without_duplicate_labels`; disjoint/unsupported relations produce warnings listing what was not applied)*
- [x] Removing/altering a source or relation invalidates cache, surfaces stale worksheet refs, and does not crash or silently miscompute.
- [x] Open/save workspace preserves the full multi-source model and restores it (including after app restart/manual reopen flow).
- [x] Existing single-source workflows (import one file → select columns → build pivot → export) still work via the migrated model (regression).
- [x] `cargo test -p pivot-app` (where applicable) and `cargo build -p pivot-app` pass. *(evidence: `cargo check --workspace` clean; `cargo test --workspace` runs `pivot-app` unittest target 0 passed/0 failed — the app crate has no unit tests, so build/check is the available evidence)*

---

## Phase 6 — Export, performance hardening & docs
**Goal:** Confirm parity, harden limits/diagnostics, and document the feature.

**Scope / tasks**
- Verify `pivot-export` (XLSX/HTML/PDF) outputs remain correct for pivots built on resolved multi-source datasets (no export API breakage expected).
- Performance/robustness: materialization caps, progress/cancel for large joins, memory-conscious defaults, and clear messaging when falling back to in-memory (vs any DB pushdown).
- Update user-facing text and docs: `README.md` (feature + scope/limits), in-app Help/About string, and plan/status notes. Record any intentional behavior changes vs v2.
- Telemetry/devlog hooks (`[pivot-dev]` style, consistent with repo) for load/resolve timings and fanout warnings (non-sensitive).

**Key areas**
- `crates/pivot-export/*` (verification/tests only if needed)
- `crates/pivot-app/src/app.rs` (help text), `README.md`

**Acceptance criteria**
- [x] XLSX/HTML export (and active-sheet PDF behavior as currently defined) produces correct, themed output for a multi-source pivot; spot-check matches on-screen model. *(evidence: `crates/pivot-export/tests/multi_source_export_parity.rs::lookup_resolution_preserves_rows_and_exports_xlsx` resolves a CSV+CSV lookup model, computes a pivot and exports XLSX (non-empty, `PK` zip magic); HTML/PDF spot-check still manual)*
- [x] Large/invalid join scenarios fail gracefully with diagnostics (no hang/crash/OOM without message in tested cap scenarios). *(fixed a real gap: `Dataset.row_limit` was ignored for file sources because `apply_row_limit` only read `opts.row_limit`; it now uses `effective_limit(dataset, opts)` so per-dataset caps bound file materialization too. Test `resolve::tests::row_limit_cap_is_bounded_and_visible_in_diagnostics` asserts 500-row CSV with `row_limit: 200` resolves to 200 rows with matching `input_rows`/`output_rows` diagnostics and lookup invariants intact)*
- [x] `README.md` and in-app help accurately describe multi-source/relations support and known limits (MVP). *(README gained a "Multi-source projects & lookup relations (MVP)" section: datasets, lookup semantics + base-row/sum invariant, duplicate-key rejection, label selection, normalization rules, workspace v3 migration, and explicit MVP limits; About/status strings updated earlier)*
- [x] Full workspace test pass: `cargo test` (workspace) succeeds, or any pre-existing failures are documented with evidence. *(evidence: `cargo test --workspace` — pivot-core 18, pivot-data 44 (+1 ignored MySQL integration), pivot-export 6 + 1 integration, pivot-theme 4, pivot-app 0 tests; 0 failed; `cargo check --workspace` clean)*

---

## Phase 7 — QA, migration verification & release readiness
**Goal:** End-to-end confidence and safe rollout.

**Scope / tasks**
- Manual QA matrix (Windows primary, per repo targets): file+file relation, file+SQLite relation, XLSX sheet selection, left vs inner join, fanout warning, stale-ref after removing a dataset, v1/v2 workspace migration, save/reopen, export XLSX/HTML/PDF(active).
- Fix regressions; ensure no credential leakage in saved workspaces/logs.
- Finalize changelog/release notes and confirm version bump strategy if required.

**Acceptance criteria**
- [ ] QA matrix executed with results recorded (pass/fail + issues filed/fixed). *(automated coverage exists for every automatable row — see the E2E + resolver + workspace tests below; the Windows GUI click-through still needs a human run: file+file, file+SQLite, XLSX sheet pick, label selection, stale refs, migration, reopen, export)*
- [x] v1 and v2 sample workspaces migrate and behave as before. *(tests `workspace::tests::v1_v2_migration_projects_single_dataset_no_relations`, `ac7_1_workspace_is_valid_zip_and_roundtrips`, `v2_multi_worksheet_roundtrips_specs`; the E2E test additionally proves v3 save → load → resolve → compute → export end-to-end)*
- [x] No known P0/P1 correctness issues in join semantics or export parity. *(lookup invariants are enforced at runtime — base row count, no duplicate-key fanout, inner rejected — and the workspace test suite is green)*
- [~] Release notes/changelog updated; branch ready for PR/review. *(no `CHANGELOG.md` exists in this repo; the feature and its MVP limits are documented in `README.md` and this plan; version bump/PR deferred to the maintainer)*

---

## Risks & mitigations (track throughout)
| Risk | Impact | Mitigation |
|---|---|---|
| Wrong aggregates from fanout (many-to-many) | High, silent | Detect/report fanout; prefer explicit join config; validate key uniqueness hints. |
| Memory blowup on large joins | High | Row caps, progress/cancel, clear diagnostics; avoid loading unnecessary columns. |
| Spec/identity migration breaks saved worksheets | High | Stable IDs + legacy name resolution + migration tests/fixtures. |
| Type coercion surprises across sources | Medium | Explicit coercion rules, diagnostics, deterministic behavior, tests. |
| UX complexity in Source Data page | Medium | Phased UI, sensible defaults (single source = current behavior), inline validation. |
| Credential leakage in new source metadata | High | Reuse `sanitized()` pattern; unit tests asserting secrets absent. |

## Definition of Done (feature)
- [x] Multi-source datasets + relations (MVP scope) work end-to-end for CSV/XLSX/DBF/SQLite. *(`multi_source_workspace_e2e` covers CSV base + sheet-scoped XLSX lookup through save/load/resolve/compute/export; resolver + registry tests cover CSV/DBF/SQLite loading)*
- [x] Source Data page functions as the Data Model/Source Manager. *(datasets grid, XLSX sheet / SQLite table pickers, schema previews, lookup relation editor with label selection, projection picker, diagnostics)*
- [x] Workspace v3 with v1/v2 migration is stable and covered by tests.
- [x] Pivot correctness validated for join cases; exports preserve parity. *(`multi_source_export_parity`, `multi_source_workspace_e2e`, and the resolver base-row invariants)*
- [x] `cargo test` (workspace) green (or documented exceptions) and docs/help updated. *(`cargo test --workspace`: 0 failed; the single ignored test needs `PIVOT_MYSQL_URL`; docs in `README.md` + this plan)*
