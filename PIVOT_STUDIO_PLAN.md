# Pivot Studio — Comprehensive Product & Implementation Plan (Phased)

> **Status:** in progress — **Phase 0 bootstrapped** (`9cfb9fe`, `013a2f9`), **Phase 1 data ingest foundation + UI landed** (`971b5ad`, `c303259`, `960184c`, `ed241db`, `729b471`). Verified on Windows (build/tests/run). macOS/Linux build + full manual QA pending. **Date drafted:** 2026-11-08 (Asia/Jakarta).  
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

### Phase 2 — Pivot engine (in-memory) & basic grid

**Goal**  
Define and compute pivots in memory from the loaded table and render a correct, interactive grid.

**Scope / deliverables**
- `PivotSpec` in `pivot-core`: **Rows** (multi-level, order), **Columns** (0..n, pivoted), **Values** (field + aggregate + display name/format), **Filters** (include/exclude, basic operators). Aggregates: Sum, Count, Count Distinct, Average, Min, Max.
- In-memory pivot computation → result structure (row/column headers, body values) with subtotals per row level and a grand total (configurable on/off).
- Field well UI (assign to Rows/Columns/Values/Filters) + pivot recompute on change (off the UI thread for non-trivial work).
- Basic grid: frozen header rows/cols, scroll, resizable columns, selection/copy.

**Acceptance criteria (all must pass)**
- [ ] **AC2.1 Correctness:** for a fixed fixture dataset, computed results for (a) single row + sum, (b) row+column pivot (2 dimensions), (c) multiple values, (d) filter applied, and (e) count/count-distinct/avg/min/max match independently verified expected outputs (golden/unit tests).
- [ ] **AC2.2 Subtotals/totals:** subtotals per enabled row level and grand total render and are numerically correct for the filtered domain; toggling them updates the result.
- [ ] **AC2.3 Nulls/types:** null/empty handling per aggregate is explicit and tested (e.g. sum ignores nulls, count behavior documented) with no panic on mixed/missing data.
- [ ] **AC2.4 Interactivity:** changing a field assignment or filter recomputes and updates the grid; on the 50k-row in-memory dataset a representative pivot recomputes within an acceptable interactive budget (measured and recorded, target sub-second to a few seconds; never freezes the UI).
- [ ] **AC2.5 Grid UX:** headers remain visible while scrolling (frozen); columns are resizable; a selection can be copied as text/TSV.

**Dependencies** — Phase 1.

---

### Phase 3 — Polished table model (differentiators)

**Goal**  
Turn the raw pivot result into a presentation-ready `ThemedTableModel` with the product’s key polished behaviors.

**Scope / deliverables**
- **Header column 1 / stub-head cell**: explicit, editable text (first-class field), independent of generated row headers.
- **Multiple footer bands** (ordered, configurable): e.g. Subtotal, Grand Total, Average, Count, and a **meta/notes** band (data source, active filter summary, row count, generated timestamp). Each band has its own aggregate/label and can be added/removed/reordered.
- **Automatic, dynamic titles**: table title, column header labels, and footer text derived from `PivotSpec` + active filters (template-driven), **recomputed on change**, with **user override/pin** that survives until reset.
- All of the above represented in the canonical `ThemedTableModel` (consumed later by UI theming and exporters).

**Acceptance criteria (all must pass)**
- [ ] **AC3.1 Stub-head:** the user can set arbitrary text in header column 1; it displays in the grid and is stored in the model/spec state; clearing/reset restores the default/empty state. (Verified on a pivot where row headers exist.)
- [ ] **AC3.2 Multi-footer:** the user can enable at least **three** distinct footer bands simultaneously (including grand total and a meta/notes band); bands display in configured order; each shows correct values/labels for the current filtered result; reordering/removing updates the model and view.
- [ ] **AC3.3 Dynamic text:** table title and relevant header/footer labels update automatically when a field assignment or filter changes; a manually overridden/pinned label remains unchanged across a subsequent recompute until the user resets it.
- [ ] **AC3.4 Model integrity:** the `ThemedTableModel` fully describes title, header bands (incl. stub-head), body, and ordered footer bands (structure + values) independent of egui; unit tests assert model contents for stub-head, multi-footer, and dynamic-title/pin behavior.
- [ ] **AC3.5 No regression:** Phase 2 acceptance tests still pass.

**Dependencies** — Phase 2.

---

### Phase 4 — Theme system & predefined presets (on-screen parity)

**Goal**  
Apply a diverse set of polished themes consistently to the pivot view, coherent with app brightness.

**Scope / deliverables**
- `pivot-theme`: role-based theme (table title, stub-head, column/row headers by level, body, zebra, subtotal, grand total, extra footer bands, notes, gridlines/borders) with fill, text color, weight/size, alignment, padding, borders, number/date format defaults.
- **Predefined, diverse theme gallery** (v1 set, >= 6 themes spanning looks such as classic grid, ledger/zebra+totals, minimal, executive/accented, compact dense, and a **High Contrast** table theme) — serialized/preset data.
- Theme picker with live preview; grid paints from resolved theme roles (no ad-hoc styling).
- Coherence with app Brightness modes (Light/Dark/High Contrast) and accessible contrast for text.

**Acceptance criteria (all must pass)**
- [ ] **AC4.1 Gallery:** >= 6 predefined themes are selectable; switching themes updates the entire table (title, headers, stub-head, body/zebra, all footer bands) live without recomputing the pivot data.
- [ ] **AC4.2 Role coverage:** every `ThemedTableModel` band/role (incl. stub-head and multiple footer bands) is visibly styled by the active theme; no band falls back to an unstyled/hard-coded default.
- [ ] **AC4.3 Brightness coherence:** each theme remains legible under app Light and Dark modes; the High Contrast app mode (and/or High Contrast table theme) meets clear contrast (manual check recorded; no low-contrast text on totals/headers).
- [ ] **AC4.4 Tokenized rendering:** the grid/table rendering path consumes resolved theme roles/tokens (code review/grep confirms no band-specific color literals outside theme definitions).
- [ ] **AC4.5 Persistence in-session:** the selected theme is retained across pivot recomputes and spec changes within the session.

**Dependencies** — Phase 3.

---

### Phase 5 — Database sources & pushdown (SQLite, MySQL) + credentials

**Goal**  
Support large data via database connectors with pushdown aggregation, and implement MySQL credential handling (decision #4).

**Scope / deliverables** (decisions #2, #4)
- Connectors: **SQLite via `rusqlite`**, **MySQL via `mysql`**. Connection UI for MySQL (host/port/db/user, SSL option as applicable) and file picker for SQLite.
- **Capability negotiation:** each connector declares supported aggregates/operators/pushdown; engine generates parameterized aggregate queries (`GROUP BY`) for pushdown; identifiers validated/sanitized; **no SQL string interpolation of untrusted values**.
- Execution mode: in-memory (small) vs pushdown (large) with a row/size estimate and user prompt/override; bounded preview; cancellable queries off the UI thread.
- **MySQL credential modes (decision #4), user-selectable per connection:**
  - **(a) Session-only:** secret held in memory for the app session, never written to disk.
  - **(b) OS keychain:** secret stored/retrieved via the platform credential store (Windows Credential Manager / macOS Keychain / Linux Secret Service or a documented equivalent abstraction); the connection profile persists only a key/reference and non-secret metadata.
- Connection profiles store the chosen mode and never persist a secret in plain form.

**Acceptance criteria (all must pass)**
- [ ] **AC5.1 SQLite pushdown:** running a pivot against a SQLite table larger than the in-memory comfort threshold executes via pushdown (`GROUP BY` aggregate query) and returns correct aggregates matching the in-memory engine on an equivalent dataset (conformance test).
- [ ] **AC5.2 MySQL pushdown:** same conformance for MySQL (integration test against a test instance/container, or a clearly documented, repeatable local setup; the test may be marked `ignored` if no service is available but the harness and SQL generation unit tests must pass).
- [ ] **AC5.3 Safety:** all query values are parameterized; identifiers are validated/whitelisted; a test/code review confirms no untrusted string interpolation into SQL. Malformed connection input fails with a clear error, no panic.
- [ ] **AC5.4 UX:** long-running connect/query shows progress and can be **cancelled**; the UI remains responsive.
- [ ] **AC5.5 Credentials — session-only:** with mode (a), connecting works for the session; restarting the app requires re-entry; no credential is written to the `.pivot` workspace file or other app config on disk (verified by inspecting saved output/config).
- [ ] **AC5.6 Credentials — keychain:** with mode (b), the secret is saved to and retrieved from the OS keychain; the stored connection profile contains no plaintext secret; a reconnect after restart succeeds without re-entry (on a platform where the keychain is available; behavior is gracefully reported where unavailable).
- [ ] **AC5.7 Mode switch:** the user can change a connection between session-only and keychain; switching updates storage accordingly (e.g. moving to keychain stores it, moving to session-only removes the keychain entry/reference as appropriate).

**Dependencies** — Phases 1–2 (engine), Phase 3/4 optional for display but model should exist.

---

### Phase 6 — Export parity (XLSX, PDF, HTML)

**Goal**  
Export the pivot to XLSX, PDF, and HTML with the **same theme** and full polished structure (decision #3 for PDF).

**Scope / deliverables** (decisions #2 xlsx write, #3)
- **XLSX export via `rust_xlsxwriter`:** styled cells (merged header spans where needed, fills, fonts, borders, banding, number formats), typed numbers/dates where possible, stub-head text, and **multiple footer bands** as real styled rows.
- **PDF export (decision #3, pure-Rust layout):** `printpdf` + custom deterministic table paginator in `pivot-export` over the `ThemedTableModel`. Supports pagination, **repeated header rows**, table title, and theme styling (fills/borders/text/number formatting as faithfully as the stack allows). No HTML-to-PDF dependency.
- **HTML export:** semantic `<table>` (`<caption>`/title, `<thead>`, `<tbody>`, footer section(s) representing multiple footer bands) with inline/embedded CSS derived from the active theme.
- All exporters consume the **`ThemedTableModel`** (parity invariant); run off the UI thread with progress/cancel.

**Acceptance criteria (all must pass)**
- [ ] **AC6.1 Parity of structure:** for a reference pivot (nested headers, stub-head text set, >= 2 footer bands, active filters reflected in title/meta), all three exports contain the same logical structure — title, stub-head, header hierarchy, body values, and each footer band in order.
- [ ] **AC6.2 Theme fidelity (XLSX/HTML):** key theme roles (header fill/font weight, zebra, total/footer styling, stub-head) are present in the XLSX styles and HTML/CSS for the reference pivot (fixture-based assertions on generated files/markup).
- [ ] **AC6.3 PDF (pure-Rust):** PDF is generated using the `printpdf` + custom paginator path (no HTML-to-PDF). A multi-page reference export paginates without clipping content and **repeats header rows** on subsequent pages; title and footer bands appear per the model (manual visual check recorded).
- [ ] **AC6.4 Dynamic content:** changing a filter/spec regenerates exports whose dynamic title/column/footer text matches the on-screen model.
- [ ] **AC6.5 UX:** export runs off the UI thread with progress and cancel; cancel leaves no corrupted partial file at the target path (writes to temp then moves, or equivalent safe behavior).
- [ ] **AC6.6 Tests:** automated tests validate XLSX/HTML structure (and selected style attributes) for the reference fixture; PDF generation smoke-tests produce a valid, non-empty PDF for the fixture.

**Dependencies** — Phases 3–4 (polished model + themes). Phase 5 not required.

---

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
