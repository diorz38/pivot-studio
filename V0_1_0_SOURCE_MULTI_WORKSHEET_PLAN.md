# Mini Plan — Source Management + Multi-Worksheet (v0.1.0)

Grounded in current architecture (`pivot-core` spec/model, `pivot-data` workspace/loaders, `pivot-app` single-sheet state, `pivot-export` single-`ThemedTableModel` renderers) and the parity invariant: **exports render `ThemedTableModel`**.

## Assumptions / decisions to lock
- **Project scope:** 1 project = 1 shared data source + 1 shared column selection + N worksheets (each worksheet has its own `PivotSpec`/view + cached model). Per-worksheet sources are out of scope.
- **Column selection semantics:** “Processed in pivot” means the column is available to field pickers, filters/distinct discovery, and pivot computation for all worksheets in the project. Default = all columns selected (v1 compat).
- **Workspace evolution:** `.pivot` format changes require `FORMAT_VERSION` bump + backward-compatible load of v1 single-spec workspaces. **No secrets** persisted (keep existing MySQL sanitization rules).
- **DB sources:** Model/UX should be source-kind agnostic; optimize pushdown for selected columns later if needed. Respect current UI state (MySQL panel temporarily disabled).
- **Stale spec policy:** If a worksheet `PivotSpec` references an unselected/missing column, warn and block recompute for that worksheet until resolved (no silent data loss / no silent auto-prune).
- **PDF (v0.1.0):** Multi-sheet PDF exports the **active worksheet only** (documented). XLSX/HTML get full multi-sheet/tab support.
- **Excel sheet names:** sanitize/trim, max 31 chars, replace forbidden `[]:*?/\\`, dedupe case-insensitively.
- **HTML tabs:** one semantic table per worksheet with tab control; preserve `data-role`/footer bands per sheet.

## Dependency overview
`Phase 0 (design/contracts)` → `Phase 1 (workspace/project model)` → `Phase 2 (multi-worksheet core + exports)` and `Phase 3 (source page + column projection)` in parallel after Phase 1 → `Phase 4 (integration, validation, migration tests, parity)`.

---

## Phase 0 — Design lock & contracts
**Goal:** Freeze data/workspace/export contracts so implementation doesn’t churn. **Status: implemented (foundation).**

**Scope / tasks**
- Define `ProjectState` / `Worksheet` structs (app-level) and the workspace v2 on-disk shape.
- Define export API evolution: keep single-sheet convenience + add multi-sheet entrypoints.
- Define column-selection behavior for stale `PivotSpec` references (warn + block recompute until resolved).
- Define Excel sheet naming rules and HTML tab/print behavior; PDF = active sheet only (v0.1.0).

**Key areas**
- `crates/pivot-data/src/workspace.rs` (format design)
- `crates/pivot-export/src/lib.rs`, `html.rs`, `xlsx.rs`, `pdf.rs` (API design)
- `crates/pivot-app/src/app.rs` (state shape)

**Acceptance criteria**
- [x] A short design note (this plan) records: project/worksheet model, shared source+selection decision, workspace v2 fields, v1→v2 migration rules, stale-spec policy, Excel/HTML/PDF multi-sheet behavior.
- [x] `FORMAT_VERSION` bump plan is explicit; all new/changed persisted fields have serde defaults/compat strategy (`FORMAT_VERSION=2`, serde defaults, v1 compat entries on save).
- [x] Export API signatures are agreed (single + multi) without breaking existing single-sheet call sites unnecessarily (`export_*` + `export_*_sheets`/`write_sheets`/`render_sheets`/`sanitize_sheet_names`).

---

## Phase 1 — Workspace v2 & project model foundation
**Goal:** Persist/load multi-worksheet projects and column selection with full v1 compatibility.

**Scope / tasks**
- Extend workspace types in `crates/pivot-data/src/workspace.rs`:
  - Add worksheet entries (e.g., `worksheets: Vec<WorkspaceWorksheet { id/name, spec: PivotSpec, view: WorkspaceView }>`), `active_worksheet` index/id.
  - Add shared column selection (`selected_columns: Option<Vec<String>>`, `None` = all columns) and keep `source: WorkspaceSource`.
  - Bump `FORMAT_VERSION`; implement `load_workspace` migration: v1 (`spec` + `view` / `pivot_spec.json`) → single worksheet named “Sheet 1”.
- Preserve/extend `resolved_theme_ids()`-style compat helpers and MySQL source sanitization on load/save.
- Update/add unit tests for roundtrip, v1 compat load, corruption handling, and sanitization.

**Key areas**
- `crates/pivot-data/src/workspace.rs`

**Acceptance criteria**
- [x] Saving a multi-worksheet project and loading it back restores: source, column selection, every worksheet’s `name/spec/view`, and active worksheet (`WorkspaceFile` v2 + `project.json`/`worksheets.json`; app save/open wired).
- [x] Loading a **v1** `.pivot` succeeds and yields exactly 1 worksheet with equivalent spec/view; no data loss (`load_workspace` migrates `pivot_spec.json` -> `WorkspaceFile::from_v1`, "Sheet 01", `selected_columns: None`).
- [x] `selected_columns` missing defaults to “all columns” behavior (serde default/`None`).
- [x] MySQL profiles remain sanitized (no secrets written/loaded as trusted secrets; `WorkspaceSource::sanitized`/load+save sanitization retained).
- [x] Corrupt/unknown-format files still return clear `Workspace` errors (existing tests).
- [x] `cargo test -p pivot-data` passes.

---

## Phase 2 — Multi-worksheet app core, tabs UI, and exports
**Goal:** Users can work with multiple worksheets in one project; XLSX exports multiple sheets; HTML exports tabbed sheets. Parity preserved per sheet.

**Scope / tasks**
- **App state** (`crates/pivot-app/src/app.rs`): refactor from single `pivot_spec/pivot_model` to `worksheets + active index`; per-worksheet cached `ThemedTableModel` and compute status/cancel channel. Commands/dispatch operate on the active worksheet.
- **UI** (`crates/pivot-app/src/dock.rs`, menus/chrome): tab strip for add/rename/duplicate/delete/reorder/select worksheet; worksheet view binds to active sheet’s spec/model.
- **Workspace integration:** new/open/save/save-as build/restore `WorkspaceFile` v2.
- **Exports** (`crates/pivot-export/src/{lib,xlsx,html,pdf}.rs`):
  - Multi-sheet XLSX renders N sheets from N `ThemedTableModel`s with sheet-name sanitization/dedup.
  - Multi-tab HTML renders N semantic tables with tab control; preserve roles/thead/tbody/tfoot/footer bands per sheet.
  - PDF exports **active sheet only** (v0.1.0).

**Key areas**
- `crates/pivot-app/src/{app,dock,menus}.rs`
- `crates/pivot-export/src/{lib,xlsx,html,pdf}.rs`

**Acceptance criteria**
- [~] User can add, switch, and remove worksheet tabs; active sheet edits do not overwrite another sheet’s spec/model. Rename/duplicate/reorder deferred (v0.1.0 cut keeps add/switch/close).
- [x] Each worksheet computes/renders its own `ThemedTableModel` (per-sheet state + `sheet_id` tagged compute).
- [x] **XLSX export** produces one `.xlsx` with one Excel sheet per worksheet tab, correct sanitized/deduped names, content matching each sheet’s `ThemedTableModel` (`export_xlsx_sheets`/`write_sheets`/`sanitize_sheet_names`).
- [x] **HTML export** produces one `.html` with a tab control switching between sheets; semantic structure/`data-role`/footer bands preserved per sheet (`export_html_sheets`/`render_sheets`).
- [x] **PDF export** exports active sheet only (documented/consistent; menu label "active sheet").
- [x] Save → close → open `.pivot` restores all tabs, names, specs, active tab (workspace v2 wired in app).
- [x] Existing single-sheet workflows still work (wrappers preserved; v1 migration).
- [~] `cargo test --workspace` passes; multi-sheet export APIs implemented and single-model tests remain green. Add dedicated multi-sheet XLSX/HTML + golden v1 fixture tests as follow-up hardening.

---

## Phase 3 — Source management page & column selection
**Goal:** Before entering the worksheet, users manage the loaded source and choose which columns are processed in pivots.

**Scope / tasks**
- **Navigation/UX** (`crates/pivot-app/src/{app,dock}.rs`): add Source/Manage Source page reached after selecting/opening a source. Show source box/card (kind, path/name, counts) and column list with **name + effective/inferred type + checkbox**.
- **Selection state:** shared `selected_columns`; default all checked. Bulk all/none actions.
- **Projection enforcement:** field pickers, `distinct_values_for`, and compute input use only selected columns.
- **Spec validation:** detect `PivotSpec` fields referencing unselected/missing columns; clear UI errors and block invalid recompute.
- **Persistence:** selection in workspace v2; roundtrip.

**Key areas**
- `crates/pivot-app/src/{app,dock}.rs`
- `crates/pivot-data/src/workspace.rs`

**Acceptance criteria**
- [x] Source management view shows column **name + type + checkbox** for each column (`nav.source`, Source Data page).
- [x] Only checked columns appear in Pivot Layout/Filters and are used for computation/distinct values (`available_columns` projection).
- [x] Unchecking a column referenced by any worksheet’s spec produces a clear validation message; no silent incorrect compute (`stale_spec_refs` warn in UI + block recompute).
- [x] Default for new imports and legacy projects is all columns selected.
- [x] Column selection persists in `.pivot` and restores on open (`selected_columns` in v2 workspace).
- [x] `cargo test --workspace` passes.

---

## Phase 4 — Integration hardening, migration, and parity/QA
**Goal:** Ship-ready behavior across old projects, exports, and edge cases.

**Scope / tasks**
- End-to-end checks: v1 open → v2 save → reopen; multi-sheet export parity; source selection across worksheets.
- Edge cases: empty selection, duplicate/sanitized sheet names, wide column lists, renamed/removed source file, stale specs.
- Update docs: `README.md`, `PIVOT_STUDIO_PLAN.md`. Run full tests.

**Acceptance criteria**
- [x] v1 `.pivot` migrates with no user-facing regression (logic covered in workspace load path; golden fixture recommended).
- [x] Multi-worksheet XLSX/HTML outputs derived from each worksheet’s `ThemedTableModel` (parity per sheet).
- [x] No secrets written to `.pivot`; workspace errors remain actionable.
- [x] `cargo test --workspace` green on Windows (pivot-core 14, pivot-data 20+1 ignored, pivot-export 6, pivot-theme 4).
- [x] Known limitations documented (PDF active-sheet only; rename/duplicate/reorder tabs, load-time column pruning, and dedicated multi-sheet/golden-fixture tests deferred).

## Suggested v0.1.0 cut line
- **Must:** Phase 1 + Phase 2 core + Phase 3 MVP + Phase 4 migration/tests.
- **Defer:** load-time column pruning, advanced bulk selection, per-worksheet themes/sources, full multi-sheet PDF.