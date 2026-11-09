# Pivot Studio — Comprehensive Product & Implementation Plan (Phased)

> **Status:** in progress — **Phase 0** (`9cfb9fe`, `013a2f9`), **Phase 1** (`971b5ad`,`c303259`,`960184c`,`ed241db`,`729b471`), **Phase 2 engine/model/UI + tests/grid** (`a6510d9`,`32cc369`,`f1632d9`,`c52d288`,`b555d9b`,`b6ee3b5`,`dd925a9`,`a6bb218`,`5d5d0f7`,`1900ecf`,`95b9620`), **Phase 3 polish/differentiators** (`699a9c9`), **Phase 4 table themes/presets** (`dc4ba5c`), **Phase 6 export foundation/polish** (`9b7ed07`,`6900282`,`5083c0b`,`f329fce`), **Phase 5 SQLite/MySQL pushdown+credentials foundations/UI** (`5a8cce4`,`6b9507b`,`baf8ca9`). Verified on Windows (`cargo test --workspace` green: pivot-core 13, pivot-data 16 + 1 ignored MySQL integration, pivot-export 5, pivot-theme 4; AC2.4 50k recompute ~307ms debug). macOS/Linux build + full manual QA pending. **Date drafted:** 2026-11-08 (Asia/Jakarta).
> **Decision:** Option 1 — keep the current contents of this folder as **UI/reference material**, then build a new desktop pivot application **from scratch**. No functionality from the current codebase is carried over except *design patterns* (layout, menu, color theme) studied as reference.  
> **UI stack (decision #1):** **Rust + `egui/eframe` 0.31** (Phase 0 baseline; DesignCraft reference uses egui 0.37 fork — API differences handled), using **DesignCraft** as the UI reference for theme/brightness tokens, menu/chrome structure, and docking feel.  
> **Reference codebase:** preserved at `reference/designcraft/` (git-excluded, not part of Pivot Studio build). Key UI reference files: `reference/designcraft/crates/ui-egui/src/theme.rs`, `reference/designcraft/crates/ui-egui/src/menus.rs`, `reference/designcraft/crates/ui-egui/src/app.rs`, `reference/designcraft/apps/designcraft/src/main.rs`, `reference/designcraft/crates/ui-egui/src/chrome/*`, `reference/designcraft/crates/ui-egui/src/dock.rs`.

---

## 1. Product vision

Build a **desktop pivot-table application** that ingests data from heterogeneous sources, lets users define pivot layouts (rows / columns / values / filters) like Excel, and produces **polished, presentation-ready tables**.

**Differentiators vs. Excel pivot tables**
- **Text in header column 1 (row-header corner / stub-head).** A first-class, editable label (e.g. “Region / Metric”, “Produk”) in the top-left header cell — an area Excel’s pivot layout does not treat as freely editable report text.
- **Multiple table footers.** More than one footer band, e.g. Subtotal, Grand Total, Average, Count, and a **meta/notes footer** (active filters, data source, generated time) — each distinct and stylable.
- **Predefined, diverse table themes** applied consistently to screen and export (header, body, zebra, totals, typography, spacing, borders).
- **Automatic, dynamic titles.** Table title, column headers, and footer text derived from selected variables and active filters (with user override / pin).
- **Roadmap headroom.** Conditional formatting, sparklines, calculated fields, “show values as”, annotations — additive polished features.

**Non-goals (v1)** — not a full spreadsheet/BI suite; no arbitrary free-grid formulas; no real-time streaming; no web/WASM (core pivot engine remains UI-agnostic to keep the door open).

---

## 2. Target platforms & scale

| Dimension | Decision |
|---|---|
| Platforms | Desktop: **Windows, macOS, Linux** (v1). |
| Small data | **Tens of thousands of rows in memory** — fast local pivot recompute. |
| Large data | **Pushdown to the source database** (aggregate in SQL/source engine); only result sets (and necessary keys) travel to the client. Hybrid/client fallback only when a grouped result is small and the source cannot express a measure. |
| Outputs | In-app interactive view **plus export** to **XLSX, PDF, HTML** with the **same theme** as on screen. |

---

## 3. Architecture & standards

### 3.1 Layering (design rule)
```
Data Connectors (file/db) + schema/capabilities
        -> Pivot Engine (PivotSpec -> result), in-memory or pushdown
        -> Themed Table Model (cells, bands, resolved styles, titles/footers)
        ->  UI (egui/eframe, renders model)  AND  Exporters (XLSX/PDF/HTML, render same model)
```
- **Parity invariant:** the UI never computes export styling ad hoc; **export = render the themed table model**.
- **Responsiveness invariant:** parsing, queries, pivot recompute, and export run on background workers; the egui frame never blocks on I/O or large compute.

### 3.2 Planned crates (new workspace)
- `pivot-core` — `PivotSpec`, aggregation/sort/filter, auto/dynamic titles & footer generation, `ThemedTableModel` (canonical, UI-agnostic).
- `pivot-data` — connectors + capability descriptors (pushdown support, aggregates, type mapping); materialized table or pushdown plan.
- `pivot-theme` — theme tokens/presets (colors, typography, borders, banding, number/date formats) + serialization; role resolution.
- `pivot-export` — `export_xlsx`, `export_pdf`, `export_html` consuming the themed table model.
- `pivot-app` — `eframe` desktop shell (menus, panels, dialogs, pickers) wiring core/data/theme/export.

### 3.3 Standardized technology decisions (locked)
| # | Area | Decision |
|---|---|---|
| 1 | UI | **`egui/eframe`** (Rust). DesignCraft is the UI reference (brightness/theme tokens, menu/chrome, docking). Port *patterns*, not the document engine. |
| 2 | Data crates (standardized) | **CSV/TSV:** `csv` (MIT/Unlicense). **XLSX read:** `calamine` (MIT) — reads values + basic types; formulas read as cached values in v1. **DBF:** `dbase` (MIT). **SQLite:** `rusqlite` (MIT, bundled where appropriate). **MySQL:** `mysql` (MIT/Apache-2.0) for sync queries executed off the UI thread. **XLSX export (Phase 6):** `rust_xlsxwriter` (MIT/Apache-2.0). *All chosen for maturity and permissive licensing; confirm exact versions/feature flags in Phase 0.* |
| 3 | PDF export | **Pure-Rust layout**. Standardized approach: **`printpdf`** (MIT/Apache-2.0) for PDF primitives + a **custom deterministic table paginator/layout engine in `pivot-export`** over the `ThemedTableModel` (controls pagination, repeated header rows, theme fidelity). No HTML-to-PDF dependency. |
| 4 | MySQL credentials | **Two options, user-selectable per connection:** **(a) session-only** (held in memory, never persisted) and **(b) OS keychain** (persisted via the platform credential store). The workspace/connection profile stores *which mode* and non-secret metadata, never the secret when keychain is used. |
| 5 | Workspace save format | **Zip container saved with the `.pivot` extension** (e.g. `report.pivot`). Contains a versioned manifest + serialized `PivotSpec`, theme reference/customization, and connection profile metadata (per decision #4 — secrets never written to disk; keychain entries referenced by id, session-only re-prompts). Optionally caches small source previews/snapshots by policy (documented), never large raw datasets by default. |

---

## 4. Repository restructuring (Option 1, before Phase 0 coding)

Perform as explicit, reviewed steps (branch/PR). Verify working tree and remote state before moving files.

1. **Inventory & freeze reference.** Record the current commit as the DesignCraft UI reference point.
2. **Create a reference area.** `git mv` existing workspace contents into `reference/designcraft/`, preserving licenses/notices inside that folder.
3. **Bootstrap the new app** at the workspace root as a fresh Rust workspace (`Cargo.toml` + `pivot-core`, `pivot-data`, `pivot-theme`, `pivot-export`, `pivot-app`). Do not inherit DesignCraft engine crates, CI wiring, or `xtask` gates unless deliberately reintroduced/adapted.
4. **Port design patterns, not code blobs.** Re-implement the theme-token/brightness and menu/chrome patterns against the egui/eframe versions chosen for Pivot Studio; treat `reference/designcraft/` as read-only inspiration. Resolve licensing/attribution before copying any text/assets (fonts/icons) — default to original assets.
5. **Housekeeping.** New top-level `README.md` for Pivot Studio; document `reference/designcraft/` as “UI reference only, not built as part of Pivot Studio”.
6. **Validate early.** Deliver Phase 0 (runnable themed shell) before deep feature work.

---

## 5. Phased plan with clear acceptance criteria

**How to read acceptance criteria:** each phase is **done only when every listed criterion passes**. Criteria are objective (builds/tests/observable behavior). Manual checks are called out explicitly. A phase may be split into PRs, but the phase is not accepted until all criteria hold on the integrated result.

---

### Phase 0 — Repository restructure & egui shell (DesignCraft-pattern UI) — *bootstrapped*

**Goal**  
Establish a clean, buildable Pivot Studio workspace and validate the GUI direction (egui + DesignCraft theme/menu feel) before any data/pivot logic.

**Delivered** (`9cfb9fe`, `013a2f9`)
- `reference/designcraft/` holds the preserved prior tree (git-excluded via `.gitignore` `/reference/`). Root Cargo workspace: `pivot-core`, `pivot-data`, `pivot-theme`, `pivot-export`, `pivot-app` (resolver 2, edition 2021, `MIT OR Apache-2.0`).
- `crates/pivot-theme/src/lib.rs`: DesignCraft-pattern `Brightness` (Dark/MediumDark/MediumLight/Light/HighContrast, stable ids/labels/parse), `Tokens::for_brightness`, `apply(ctx,&Tokens)` installs Visuals/text styles/spacing and stores tokens; `Tokens::get`; High Contrast legible (black/white, yellow accent, cyan focus). Tests: `brightness_has_stable_ids_and_roundtrip`, `tokens_resolve_for_all_modes`.
- `crates/pivot-app/src/{main,app,menus,chrome,dock}.rs`: frameless/custom chrome (no generic OS title bar) with Home icon (vector paint) → main menu, drag/double-click maximize, caption buttons; data-driven menu tree (File/Edit/Data/Pivot/Format/View/Help) + View→Brightness; Home page (open `.pivot`/import CSV/XLSX/DBF/connect DB) and Worksheet dock (left Sources/Fields, center grid/preview, right Layout/Properties, bottom Filters/Settings), all resizable.
- Pinned: `eframe`/`egui` 0.31, `serde` 1, `serde_json` 1, `thiserror` 2.

**Acceptance criteria**
- [x] **AC0.1 Structure:** `reference/designcraft/` preserved (notices inside); root workspace builds `pivot-app` + stubs. *Verified: Windows build/test. macOS/Linux: CI or documented local builds still pending.*
- [x] **AC0.2 Launch:** `pivot-app` opens/closes cleanly (`cargo run -p pivot-app` verified on Windows). *Full interactive/manual smoke across platforms pending.*
- [x] **AC0.3 Brightness:** View → Brightness switches all 5 modes at runtime (session state); High Contrast tokens defined as legible. *Manual legibility check recorded informally; formal cross-platform check pending.*
- [~] **AC0.4 Token discipline:** core chrome/dock/theme widgets use `Tokens::get`/theme API. *Exception to clean up: close-button hover red `Color32::from_rgb(196,43,28)` in `crates/pivot-app/src/chrome.rs` — move to a token if desired; otherwise acceptable platform-idiom literal.*
- [x] **AC0.5 Menus/chrome:** data-driven domain menus, Brightness dispatch, custom app bar + status bar visible.
- [x] **AC0.6 Layout:** center area + left/right/bottom resizable dock regions at default window size.
- [x] **AC0.7 Reference hygiene:** `README.md` states `reference/designcraft/` is UI reference only and excluded from the build.

**Dependencies** — none.  
**Notes** — No assets/fonts copied from DesignCraft; Home icon is original vector paint. LF/CRLF warnings on Windows commits are benign.

---

### Phase 1 — In-memory data ingest & field model (file sources) — *foundation landed*

**Goal**  
Load file-based sources into a typed in-memory table and expose schema/fields to the UI, using the standardized crates (decision #2).

**Delivered** (`971b5ad`, `c303259`, `960184c`, `ed241db`, `729b471`)
- **Crates (confirmed in workspace/Cargo.lock context):** `csv="1"`, `calamine="0.32"` (0.26.1 failed to compile in this toolchain: `E0599 no method named read_event_into` in `calamine-0.26.1/src/xlsx/mod.rs:827:27`), `dbase="0.6"`, `encoding_rs="0.8"`, `chrono="0.4"` (default-features off, `std`), `rfd="0.15"`. Dev: `rust_xlsxwriter="0.79"`, `tempfile="3"`. XLSX writing deferred to Phase 6 (`rust_xlsxwriter`).
- `crates/pivot-core/src/lib.rs`: `DataType` (String,Integer,Decimal,Boolean,Date,DateTime), `Value` (`as_display`), `Column {name,inferred,override_type,nullable,effective_type}`, `DataTable {columns,rows,source,row_count,column_count,get,reinterpret_column,schema_summary}`, `coerce_value`. Test `coerce_and_override_reinterpret`.
- `crates/pivot-data/src/{lib,csv_import,infer,xlsx_import,dbf_import}.rs`: `FileKind`, `load_file`, `DataError` (Io/Csv/Xlsx/Dbf/Invalid/Unsupported, `thiserror`). CSV/TSV: BOM strip, `csv::ReaderBuilder` (delimiter/has_headers/flexible/trim), encoding via `encoding_rs` label or UTF-8 lossy, infer+parse. XLSX: `calamine::open_workbook_auto`/first sheet, cached formula values read as values (`#[allow(unreachable_patterns)]` on non-exhaustive fallback). DBF: `dbase` 0.6 `Reader`/field mapping (`#[allow(unreachable_patterns)]` fallback). Type inference: per-cell parse, `promote` (int+dec→dec, mixed date/non-date→string, mixed bool→string, datetime dominates date).
- `crates/pivot-app/src/{app,chrome,dock}.rs`: `LoadMsg/LoadState/LoadedTable`, background `std::thread` + `mpsc` load, `rfd::FileDialog` (`file.import.csv|xlsx|dbf`), progress/cancel (`Arc<AtomicBool>`), `poll_loading` + 100ms repaint while loading, row/col counts in status bar, Home import actions, Worksheet Sources/Fields with `DataType` override (`reinterpret_column`) and data preview grid (first 200 rows).

**Acceptance criteria**
- [x] **AC1.1 CSV/TSV:** header + TSV tests pass (`csv_header_and_delimiter`,`tsv_parses`,`no_header_generates_columns`); delimiter/encoding/header handled by API. Malformed CSV path is panic-free/deterministic (`malformed_reports_error_without_panic`; flexible recovery means some malformed input may parse — test asserts no panic and bounded/deterministic result). *Manual UI import smoke pending.*
- [x] **AC1.2 XLSX:** fixture test `xlsx_happy_path_reads_values_and_types` (generated via `rust_xlsxwriter`) reads values/basic types; formula cached values mapped in loader. *Manual sheet-selection UI not yet exposed (loads first sheet); multi-sheet picker pending.*
- [x] **AC1.3 DBF:** fixture test `dbf_happy_path_reads_rows` (generated via `dbase` 0.6) maps types and loads rows without panic. *Manual representative-file smoke pending.*
- [x] **AC1.4 Typing:** inference shown/editable in UI; `reinterpret_column`/`coerce_value` unit-tested (`coerce_and_override_reinterpret`, inference tests). *Rule is deterministic; document edge cases (bool/date coercion) in code/docs as needed.*
- [~] **AC1.5 Scale/UX:** synthetic **50,000-row** load test passes (`loads_50k_rows_synthetic`, 5 cols, inferred types asserted). Load runs on a background thread with progress/spinner/cancel + periodic repaint (UI thread not blocked by the worker). *Manual responsiveness and true mid-parse cancellation (worker checks around load, not inside every parse) should be verified/polished.*
- [x] **AC1.6 Tests:** `cargo test --workspace` (Windows) exit 0 — `pivot-core` 1, `pivot-data` 10, `pivot-theme` 2, others 0. *CI/macOS/Linux pending.*

**Dependencies** — Phase 0.  
**Next (Phase 1 polish / Phase 2 start):** manual QA with real CSV/XLSX/DBF files; generic file picker/TSV auto-detect; XLSX sheet chooser; richer progress/cancel granularity and error details; document parsing/type rules. Then **Phase 2 — Pivot Engine & `ThemedTableModel`** (rows/cols/values/aggs, stub-head, subtotals/grand total, canonical model). Keep `reference/designcraft/` excluded.

---

### Phase 2 — Pivot engine (in-memory) & basic grid — *foundation landed*

**Goal**  
Define and compute pivots in memory from the loaded table and render a correct, interactive grid.

**Delivered** (`a6510d9`, `32cc369`, `f1632d9`, `c52d288`, `b555d9b`)
- `crates/pivot-core/src/{spec,model,pivot}.rs`: `PivotSpec` (rows/columns/values/filters, `SubtotalConfig`, editable `stub_head`, `title_override`), aggregates `Sum,Count,CountDistinct,Average,Min,Max` (`ValueField`/`FieldRef`), filter ops (Include/Exclude/Equals/NotEquals). Canonical UI-agnostic `ThemedTableModel` (title/title_pinned, `row_header_cols`, header/body rows of `Cell{role,colspan,rowspan,indent}`, ordered `FooterBand`s incl. GrandTotal+Notes). `pivot::compute` groups in-memory, computes row/col pivots, row subtotals per prefix (with column-pivot prefix subtotals), grand total, auto title (`vals by rows [by cols]`) with pin, notes footer (`Source|Rows|Filters`). Null handling: Count counts rows; CountDistinct counts distinct display values; Sum/Avg/Min/Max ignore null/non-numeric (Avg over non-null numeric count).
- `crates/pivot-core/src/lib.rs`: exports modules/types. Tests (11): `ac2_1a_single_row_sum`, `ac2_1b_row_column_pivot`, `ac2_1c_multiple_values`, `ac2_1d_filter`, `ac2_1e_aggregates_and_nulls` (West Qty `[Null,5]`: Count=2,Avg=5,Min=5,Max=5), `ac2_1e_count_distinct`, `ac2_2_multi_level_subtotals`, `ac2_4_50k_recompute_budget` (~307ms Windows debug), `ac3_1_stub_head_and_3_3_title`, `ac3_2_footer_bands`, plus `coerce_and_override_reinterpret`.
- `crates/pivot-app/src/{app,chrome,dock,menus}.rs`: `pivot_spec`/`pivot_model` state, background recompute thread + `mpsc`/cancel (`request_pivot_recompute`, needs >=1 value), field-well actions (+Row/+Col/Sum/Count/Avg/Min/Max) and type override, Layout panel (Rows/Columns/Values with remove/reorder, Clear/Recompute, row-subtotals/grand-total toggles, stub-head edit triggers recompute), status shows computing spinner/cancel.
- `crates/pivot-app/src/pivot_grid.rs` (`b555d9b`): custom-painted pivot grid over `ThemedTableModel` (replaces `egui::Grid` for the model view) — sticky header strip with colspan/rowspan occupancy, body+footer in `ScrollArea::both`, column resize handles, drag selection, TSV copy (selection/all) + Ctrl+C toolbar, role/indent styling via `pivot_theme::Tokens`. `crates/pivot-app/src/dock.rs` renders via `pivot_grid::show`.
- `crates/pivot-app/src/{app,dock}.rs` (priority 1 QA): bottom-panel **Filters** UI — field/op/value picker (distinct values from loaded table, capped) adds/removes/clears `PivotSpec.filters` and triggers recompute; Notes/footer and auto title reflect active filters (exercises AC3.3).

**Acceptance criteria (all must pass)**
- [~] **AC2.1 Correctness:** golden/unit tests cover (a) single row+sum, (b) row+column pivot (2 dimensions), (c) multiple values, (d) filter, (e) count/count-distinct/avg/min/max incl. null behavior (`ac2_1c_multiple_values`). *Pending: recorded multi-dimension/multi-level subtotal fixture set.*
- [~] **AC2.2 Subtotals/totals:** engine computes row subtotals per enabled level and grand total; UI toggles trigger recompute. Golden `ac2_2_multi_level_subtotals` verifies multi-level row subtotals (East=220/West=200) and grand total (420) with row-header suppression. *Pending: column-subtotal edge cases and broader rendered-order/multi-value-subtotal fixtures.*
- [x] **AC2.3 Nulls/types:** null/empty handling explicit in `Accum` and tested (`ac2_1e_aggregates_and_nulls`); no panic on missing/mixed data (filters/value access use fallbacks). *Document Count-vs-Sum/Avg semantics in UI help as needed.*
- [~] **AC2.4 Interactivity:** recompute runs off the UI thread. Measured Windows debug (`cargo test -p pivot-core ac2_4_50k -- --nocapture`, 50k rows, 2 row/col dims, 2 values, subtotals+grand total): ~307ms body rows=4/footer bands=2 (well within interactive budget; release should be faster). *Pending: in-app manual responsiveness/50k UI load check.*
- [~] **AC2.5 Grid UX:** custom painter renders `ThemedTableModel` with **sticky header strip** (colspan/rowspan occupancy), scrollable body+footer, **column resize** handles, **drag selection**, and **copy as TSV** (selection/all, Ctrl+C) — `crates/pivot-app/src/pivot_grid.rs` (`b555d9b`). *Pending: manual UX polish (header/body horizontal-scroll sync feel, keyboard nav, sticky/pinned footer bands, copy fidelity for complex spans), broader cross-platform/manual QA.*

**Dependencies** — Phase 1.  
**Next:** AC2.5 manual polish (header/body x-scroll sync feel, keyboard nav, pinned footer bands, complex-span copy) + cross-platform/manual QA; AC2.2 column-subtotal/multi-value-subtotal goldens; AC2.4 in-app 50k manual check; drag-drop field wells/filters UI; then **Phase 3/4** polish (theme presets + dynamic title pin UI + multi-footer band config, AC3.1-AC4.5) and **Phase 6** export parity (XLSX/PDF/HTML from `ThemedTableModel`, decision #3).

---

### Phase 3 — Polished table model (differentiators) — *core delivered*

**Goal**  
Turn the raw pivot result into a presentation-ready `ThemedTableModel` with the product’s key polished behaviors.

**Delivered** (`699a9c9`)
- `crates/pivot-core/src/spec.rs`: `FooterBandSpec { kind, label, aggregate, field, enabled }` with helpers (`grand_total/notes/average/count/effective_label`), `SubtotalConfig.grand_total` legacy when explicit bands present, `PivotSpec.footer_bands` + `effective_footer_bands()` (explicit filtered-by-enabled, else legacy GrandTotal? + Notes). Exported from `crates/pivot-core/src/lib.rs`.
- `crates/pivot-core/src/pivot.rs`: ordered configurable footer emission from `effective_footer_bands()` (GrandTotal/Average/Count/Custom/Subtotal/Notes), Notes renders `Source|Rows|Filters` (incl. filter field:op), value bands computed over filtered rows (per-column-key when column pivot). Tests `ac3_2_configurable_multi_footer` (GrandTotal+Average+Notes; Average 125 over all 4 rows) and `ac3_2_footer_reorder_and_filter_affects_values` (filter-aware, order preserved).
- `crates/pivot-app/src/{app,dock}.rs`: stub-head edit/clear in Layout/Options; title pin checkbox + editable pinned title/reset-to-auto + auto preview (`set_title_pinned`); footer band add (Grand Total/Average/Count/Notes), remove/reorder, recompute wired; `clear_layout` preserves polished config. (Title pin UI calls recompute/reset through spec.)

**Acceptance criteria (all must pass)**
- [~] **AC3.1 Stub-head:** editable in Layout/Options, stored in `PivotSpec.stub_head`/model header (`ac3_1_stub_head_and_3_3_title` covers model), clear/reset supported. *Pending: manual grid display smoke on a pivot with row headers.*
- [~] **AC3.2 Multi-footer:** >=3 distinct bands configurable simultaneously (incl. GrandTotal+Notes), ordered display, correct filtered values, add/remove/reorder in UI; model tests cover config/order/filter. *Pending: manual UI verification across layouts.*
- [~] **AC3.3 Dynamic text:** auto title updates from `PivotSpec`/filters; `title_override` pin preserved across recompute (`ac3_1_stub_head_and_3_3_title` model behavior + UI pin/reset wired). Filters UI added (bottom panel: field/op Equals/NotEquals/Include/Exclude + distinct value, add/remove/clear -> recompute) so active filters appear in Notes and feed dynamic/pinned behavior. *Pending: manual filter/field-change + pin-survives-recompute verification.*
- [x] **AC3.4 Model integrity:** `ThemedTableModel` fully describes title/stub-head/body/ordered footer bands independent of egui; unit tests cover stub-head, multi-footer (incl. config/order/filter) and dynamic-title/pin.
- [x] **AC3.5 No regression:** Phase 2/pivot-core/pivot-data/pivot-theme tests still pass (`cargo test --workspace` green on Windows).

**Dependencies** — Phase 2.  
**Next:** manual QA (AC3.1-3.3, filter UI once added); consider per-footer field/aggregate editor and label templates.

---

### Phase 4 — Theme system & predefined presets (on-screen parity) — *core delivered*

**Goal**  
Apply a diverse set of polished themes consistently to the pivot view, coherent with app brightness.

**Delivered** (`dc4ba5c`)
- `crates/pivot-theme/src/table.rs` (new, `pub mod table` from `pivot-theme`): `TableThemeId` with 6 stable-id presets — `ClassicGrid`, `Ledger` (zebra/totals), `Minimal`, `Executive` (accent), `CompactDense`, `HighContrast` — plus `RoleStyle{fill,text,strong,grid_line}` and `TableStyle::resolve(id,brightness,tokens)` resolving roles for title/stub_head/col_header/row_header/body/body_zebra/subtotal/grand_total/footer/notes/selection/border, `zebra`, `row_height/header_height`. High Contrast table/brightness forced legible (black/white, yellow totals, cyan notes). Tests `theme_ids_stable`, `resolve_all_themes_and_brightnesses` (all ids × all brightness modes).
- `crates/pivot-app/src/menus.rs`: `Format > Table Theme` submenu (6 presets, `format.theme` id params, checked state). `crates/pivot-app/src/app.rs`: `table_theme: TableThemeId` in session state, `format.theme` dispatch, menu check, `clear_layout` preserves theme; about/status updated. Theme stored independently of `PivotSpec`/`pivot_model` (switching theme does not trigger recompute).
- `crates/pivot-app/src/pivot_grid.rs` + `crates/pivot-app/src/dock.rs`: grid resolves `TableStyle` per render from `(brightness, table_theme, Tokens)` and paints title/headers/body/zebra/subtotals/grand total/footer/notes/selection/border via role styles; Notes band styled distinctly; row/header heights come from theme (Compact Dense tighter). `reference/designcraft/` remains excluded.

**Acceptance criteria (all must pass)**
- [~] **AC4.1 Gallery:** 6 predefined themes selectable from `Format > Table Theme`; switching updates the rendered table live without recomputing pivot data (theme is app/grid state, not recompute input). *Pending: manual visual sweep of all 6 themes on a representative pivot.*
- [~] **AC4.2 Role coverage:** grid paints resolved roles for stub-head, column/row headers, body/zebra, subtotal, grand total, footer/notes bands and selection/border. *Pending: code review/visual check that every `ThemedTableModel` band/role is covered (including Notes span) and no band-specific literals leak outside theme definitions/grid role mapping.*
- [~] **AC4.3 Brightness coherence:** themes resolved against all `Brightness` modes in tests; High Contrast forced legible. *Pending: manual contrast/legibility check under Light/Dark/High Contrast (record).*
- [~] **AC4.4 Tokenized rendering:** pivot grid consumes `TableStyle`/`Tokens` roles (`role_style_for` -> resolved style) rather than per-cell hard-coded colors. *Pending: grep/review pass; minor literals may remain for resize affordances/copy status — acceptable only if outside band styling.*
- [x] **AC4.5 Persistence in-session:** selected `table_theme` retained across pivot recomputes, spec changes, and layout clear within the session.

**Dependencies** — Phase 3.  
**Next:** manual QA (AC4.1-4.4), optional font weight/number-format theming, and move into **Phase 5** (SQLite/MySQL pushdown + credentials, decisions #2/#4) or **Phase 6** export parity (`rust_xlsxwriter`/`printpdf`+custom paginator/HTML over `ThemedTableModel`, decision #3) per priority.

---

### Phase 5 — Database sources & pushdown (SQLite, MySQL) + credentials

**Goal**  
Support large data via database connectors with pushdown aggregation, and implement MySQL credential handling (decision #4).

**Delivered (foundations + UI, `5a8cce4`,`6b9507b`,`baf8ca9`)**
- Deps (decision #2/#4): `rusqlite 0.32` (bundled), `mysql 26`, `keyring 3` (apple-native/windows-native/linux-native). Workspace + `crates/pivot-data/Cargo.toml`.
- `crates/pivot-data/src/db_import.rs`: `CredentialMode { SessionOnly, OsKeychain }`; `SqliteProfile`/`MySqlProfile`/`DbProfile`; `MySqlProfile::sanitized_for_storage()` and no plaintext secret persistence. Session secrets in process memory map (`SESSION_SECRETS`); OS keychain via `keyring` service `pivot-studio`; `store_secret/load_secret/remove_secret`, `switch_credential_mode` (AC5.7). SQL safety: `validate_identifier`, quoted identifiers, `build_group_by_sql` produces parameterized `?` values for filters, `GROUP BY` dims, aggregate aliases; no untrusted SQL interpolation (AC5.3). SQLite (rusqlite): `list_sqlite_tables`, `connect_and_preview_sqlite`, `execute_pivot_sqlite` (pushdown aggregate query -> conformance model via `pivot_core::pivot::compute` on aggregate result). MySQL (mysql 26): `build_mysql_url` (encoded user/pass/db), `preview_mysql`, `execute_pivot_mysql`; generic `collect_mysql_result` for Text/Binary `QueryResult` protocols.
- `crates/pivot-data/src/lib.rs`: exports DB API + `DataError::{Database,Credentials,Cancelled}`.
- App/UI: `crates/pivot-app/src/app.rs` `DbSource::{Sqlite,MySql}`, DB panel state, SQLite file/table picker + preview/pushdown source, MySQL host/port/db/user/table/password + SSL flag + credential mode radio (session-only never persisted / OS keychain non-secret metadata) + preview; `request_pivot_recompute` uses DB pushdown when `db_source` set (SQL `GROUP BY`), else in-memory `compute`; file import clears `db_source`; cancellable via existing cancel plumbing (worker checks cancel; AC5.4 foundations). `crates/pivot-app/src/dock.rs` adds Database button/panel in Filters/Data area + worksheet.
- Tests (`pivot-data` 16 + 1 ignored): `ac5_1_sqlite_pushdown_matches_in_memory` (conformance vs in-memory engine), `ac5_3_identifiers_validated_and_queries_parameterized`, `ac5_4_cancel_is_respected`, `ac5_5_session_secret_not_persisted_by_helper` (sanitized profile contains no secret), `ac5_7_mode_switch_session_to_keychain_fallback_path` (graceful when keychain unavailable), `ac5_2_mysql_pushdown_sql_generation_unit`. `#[ignore] ac5_2_mysql_pushdown_integration` requires `PIVOT_MYSQL_URL` and `sales` table (`cargo test -p pivot-data ac5_2_mysql_pushdown_integration -- --ignored`).

**Acceptance criteria (all must pass)**
- [~] **AC5.1 SQLite pushdown:** conformance test passes (`ac5_1_sqlite_pushdown_matches_in_memory`); UI sets SQLite `db_source` and Recompute uses pushdown path. Manual QA with large table/threshold UX pending.
- [~] **AC5.2 MySQL pushdown:** parameterized SQL generation unit-tested; live pushdown path implemented in `execute_pivot_mysql`. Integration test harnessed as `#[ignore]` (needs MySQL service + `PIVOT_MYSQL_URL`/`sales` table). Manual/connect QA pending.
- [~] **AC5.3 Safety:** parameterized filter values, validated/quoted identifiers, unit tests; malformed input returns clear errors (no panic by construction; fuzz/hostile-input expansion pending).
- [~] **AC5.4 UX & responsiveness:** connect/preview/pushdown run off UI thread for recompute; cancel flag supported; DB preview currently synchronous in UI action (small 500-row preview) — move to background worker/progress if needed. Progress indicators present for recompute/export/load.
- [~] **AC5.5 Session-only:** session store used when mode session-only; `MySqlProfile::sanitized_for_storage()` strips secret; `.pivot` workspace persistence not yet implemented (Phase 7) — invariant enforced by helper/API.
- [~] **AC5.6 Keychain:** keyring backend wired; graceful error when unavailable/denied reported in status (test tolerates headless). Cross-platform/manual keychain QA pending.
- [~] **AC5.7 Mode switch:** `switch_credential_mode` implemented/tested for fallback bookkeeping; UI mode selection stores secret in chosen mode on preview/apply. Persisted-profile switch UX arrives with Phase 7 `.pivot`.

**Status:** foundations/UI landed; run MySQL integration test when service available; move DB preview to background worker and complete Phase 7 profile persistence before release.
## Phase 6 — Export parity (XLSX, PDF, HTML) — *foundation landed*

**Goal**  
Export the pivot to XLSX, PDF, and HTML with the **same theme** and full polished structure (decision #3 for PDF).

**Delivered (foundation, `9b7ed07`)**
- `crates/pivot-export/src/{lib,html,xlsx,pdf}.rs`: exporters consume canonical `ThemedTableModel` (AC6 parity invariant) and resolve roles via `pivot_theme::table::TableStyle` (`ExportStyle { table_theme, brightness }`).
- **HTML:** semantic `<table>` with `<caption>`/title, `<thead>`, `<tbody>`, `<tfoot>` footer bands, `data-role`/`data-band` attributes, inline role styles (fills/text/borders/indent) derived from the resolved theme (AC6.1/6.2 foundation).
- **XLSX:** `rust_xlsxwriter` styled/merged cells for title/headers/body/footer bands (fills/fonts/borders/align/indent, numeric writes when parseable), freeze panes, column widths (AC6.1/6.2 foundation).
- **PDF (decision #3, pure Rust):** `printpdf 0.7` + deterministic custom table paginator in `pivot-export` (A4 landscape, computed column widths, role fills/borders/text, title, repeated header rows, footer bands kept with pagination checks). No HTML-to-PDF (AC6.3 foundation; visual fidelity/pagination polish/manual QA pending).
- **App/UI:** `File > Export` submenu (HTML/XLSX/PDF) in `crates/pivot-app/src/menus.rs`; `PivotApp::export_pivot` in `crates/pivot-app/src/app.rs` uses `rfd` save dialog and exports current `pivot_model` with active `table_theme`/`brightness`.
- Tests (`pivot-export`): HTML structure/role assertions, XLSX non-empty write, PDF `%PDF-` smoke, extension dispatch for `.html/.xlsx/.pdf`.

**Acceptance criteria (all must pass)**
- [~] **AC6.1 Parity of structure:** HTML exporter emits title/stub-head/header hierarchy/body/footer bands in order (`<caption>/<thead>/<tbody>/<tfoot>` + `data-role`/band attrs, unit test). XLSX/PDF render the same `ThemedTableModel` sections; fixture-based cross-format structural assertions and manual comparison still needed.
- [~] **AC6.2 Theme fidelity (XLSX/HTML):** role fills/text/borders/weight/indent/total styling derived from resolved `TableStyle` are emitted (HTML inline styles; XLSX `Format` colors/bold/borders/indent). Automated style-attribute fixture assertions still needed.
- [~] **AC6.3 PDF (pure-Rust):** uses `printpdf` + custom paginator (no HTML-to-PDF). Repeats header rows on page breaks and renders title/footer bands (foundation). Multi-page reference export and manual visual QA (no clipping, header repeat, band placement, span/Unicode fidelity) still required.
- [~] **AC6.4 Dynamic content:** export consumes the current resolved `pivot_model` (which reflects spec/filter/theme/title state). End-to-end “change filter/spec → export differs” test pending.
- [~] **AC6.5 UX:** export is initiated from UI (file dialog) and reports success/failure in status. Runs on UI action (model already computed); background/progress/cancel + temp-then-move safe write for long exports not yet implemented.
- [~] **AC6.6 Tests:** automated tests validate HTML structure/roles and produce valid non-empty XLSX/PDF for a reference fixture (`pivot-export` 4 tests). Broader reference-fixture assertions pending.

**Dependencies** — Phases 3–4 (polished model + themes). Phase 5 not required.
### Phase 7 — Workspace format (`.pivot` zip), hardening & packaging

**Goal**  
Persist and reopen workspaces safely (decision #5) and ship a hardened desktop release.

**Scope / deliverables** (decision #5)
- **Workspace file:** a **Zip container with `.pivot` extension** (e.g. `report.pivot`). Versioned contents: `manifest` (format version, app version, created/modified), serialized `PivotSpec`, theme reference/custom theme data, data-source/connection profile metadata, and UI state needed to restore the view (e.g. active theme, layout prefs) — **no secrets** (honors decision #4: session-only re-prompts; keychain referenced by id/mode only).
- Save/Open/Recent Workspaces; migration path for manifest version bumps.
- Hardening: robust error messages (no panics on bad/corrupt `.pivot`, missing source, or failed DB connect), large-data safeguards (estimates, caps, virtualization/paging as needed), performance passes on recompute/export, accessibility pass on High Contrast and keyboard/focus basics.
- Packaging/installers for **Windows, macOS, Linux** (format chosen per platform conventions) with smoke-tested launch.

**Acceptance criteria (all must pass)**
- [ ] **AC7.1 Save/reopen:** saving a workspace (source ref/profile + `PivotSpec` + theme + stub-head + multi-footer config + pinned overrides) to `name.pivot` and reopening it restores the equivalent pivot view and settings.
- [ ] **AC7.2 Zip/extension:** the `.pivot` file is a valid Zip container; the manifest is versioned and readable; an unsupported/future manifest version fails with a clear, non-panicking message.
- [ ] **AC7.3 No secrets on disk:** for a MySQL connection in **session-only** mode, the `.pivot` contains no password/secret; for **keychain** mode, it contains only the mode/reference metadata and no plaintext secret (verified by inspecting the archive contents).
- [ ] **AC7.4 Corruption handling:** opening a truncated/corrupt/non-zip `.pivot` or one with a missing/malformed manifest reports a clear error and leaves the app usable (no crash, no state corruption).
- [ ] **AC7.5 Robustness:** invalid inputs across supported sources (bad file, dropped DB, cancelled query) surface user-facing errors; no panic paths in normal/error flows (validated by tests/fuzz-style cases for parsers and load paths where practical).
- [ ] **AC7.6 Packaging:** installable/runnable artifacts produced for Windows, macOS, and Linux; each launches, opens the sample/fixture workspace flow, and performs a basic pivot+export smoke test (recorded).

**Dependencies** — Phases 1–6 (save/load spans the full feature set).

---

## 6. GUI reference map (DesignCraft, `egui`) — use during implementation

Read these under `reference/designcraft/` after restructuring (paths below are current, pre-move). Port **patterns**, not the document engine.

| Area | Reference file(s) | Pattern to port into Pivot Studio |
|---|---|---|
| Brightness + tokens + `apply` | `crates/ui-egui/src/theme.rs` | `Brightness` (incl. High Contrast), central `Tokens::for_brightness`, `apply(ctx,tokens)` sets `Visuals`/text styles/spacing; widgets never hard-code colors. |
| Menu data + dispatch | `crates/ui-egui/src/menus.rs` | Data-driven `Item::{Sep,Sub,Cmd}`/menu tree + command dispatch; adapt to File/Edit/Data/Pivot/Format/View/Help. |
| App shell/state wiring | `crates/ui-egui/src/app.rs` | App state, frame/update flow, integration of menus/theme/panels. |
| Chrome | `crates/ui-egui/src/chrome/*` | App bar, status bar, control surfaces. |
| Docking/panels | `crates/ui-egui/src/dock.rs` | Resizable/dockable regions for fields/grid/filters/settings. |
| Entry point | `apps/designcraft/src/main.rs` | `eframe` startup/bootstrap pattern (adapted; ignore DesignCraft-specific control/gpu details not needed by Pivot Studio). |

**GUI acceptance (cross-cutting, verified in Phase 0 and preserved):** all 5 brightness modes work; High Contrast legible; centralized tokens (no scattered color literals); data-driven menus; docked/resizable workspace panels.

---

## 7. Cross-cutting quality requirements (apply to every phase)

- **No UI blocking:** heavy work (parse/load/query/pivot/export) on background workers; progress + cancel where long-running.
- **No panics on bad input:** malformed files, corrupt `.pivot`, bad connections, and hostile/unexpected data return `Result`/clear errors.
- **Security:** parameterized SQL only; identifiers validated; **secrets never written to disk** (session-only in memory; keychain mode stores only references; `.pivot` contains no plaintext secrets). No secrets in logs.
- **Tests per phase:** each phase adds/keeps passing tests for its acceptance criteria (unit + fixture/conformance as specified).
- **Licensing/assets:** confirm crate licenses are permissive/compatible (decisions in §3.3); do not copy DesignCraft assets/text without resolving attribution — prefer original assets.

---

## 8. Decision log (resolved)

1. **UI:** `egui/eframe` (DesignCraft UI reference). — *resolved.*
2. **Crates:** `csv`, `calamine` (xlsx read), `dbase`, `rusqlite`, `mysql`; `rust_xlsxwriter` (xlsx export). — *resolved, confirm versions/features in Phase 0.*
3. **PDF:** pure-Rust layout via `printpdf` + custom `pivot-export` table paginator. — *resolved.*
4. **MySQL credentials:** two options, per connection — **session-only** and **OS keychain**. — *resolved.*
5. **Workspace:** Zip container, **`.pivot`** extension, versioned manifest; no secrets persisted. — *resolved.*

**Remaining (confirm early, not blockers to Phase 0):** exact `egui/eframe` versions; Linux keychain backend choice/abstraction; PDF font embedding/standard-font strategy details; whether to snapshot small previews inside `.pivot`; optional PostgreSQL/Parquet post-v1.
