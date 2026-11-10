# Pivot Studio — DERIVA GUI & SVG Icons Implementation Plan

**Target Version**: v0.3.0  
**Status**: Planned / Ready for Execution  
**Author**: Antigravity Pair Programmer & Pivot Studio Team  
**Design Reference**: DERIVA v1.0 Compiler Studio (`Screenshot 2026-11-10 115940.png`)  
**Target Platform**: Rust + `egui` / `eframe` (Desktop: Windows, macOS, Linux)

---

## 1. Executive Summary & Design Vision

### 1.1 The Concept: "The Analytical Data Studio"
**DERIVA v1.0 Compiler Studio** is an educational, interactive development environment that visualizes the end-to-end compilation pipeline (Grammars → Semantics → IR/TAC → CFG → Optimization → Codegen/RegAlloc → VM).

Pivot Studio is structurally an **analytical data compiler**:
1. **Raw Sources (Grammars/Lexer)**: Ingests heterogeneous files (CSV, XLSX, DBF, SQLite).
2. **Schema & Normalization (AST/Parser)**: Normalizes headers to safe identifiers.
3. **Lookup Relations (Semantic Analysis & Type Checking)**: Validates foreign key mappings, detects duplicate or orphan keys.
4. **Pivot Dimensional Aggregation (IR & Code Lowering)**: Multi-axis groupings (Rows, Columns, Values, Filters).
5. **Role-Based Theme Styling (Optimizer & Passes)**: Assigns semantic visual roles (Header, Stub, Totals, Footers).
6. **Output Generation (Codegen & VM)**: Compiles formatted artifacts to screen, XLSX, PDF, and HTML.

By adopting the **DERIVA Studio aesthetic and layout**, Pivot Studio transitions from a page-flipping utility into an integrated, professional workbench.

---

## 2. Visual Architecture & Layout Mapping

```
+-----------------------------------------------------------------------------------------------------------------------------+
| Top Global Header: [P] PIVOT STUDIO  | Presets ˅ | Deriva Dark ˅ | Engine: In-Memory | [▶ RECOMPUTE] [● 0 ERRORS] | [ - □ ✕ ]     |
+-----------------------------------------------------------------------------------------------------------------------------+
| Stage Ribbon: [ 1. Data Sources ] -> [ 2. Relations & Model ] -> [ 3. Pivot Layout ] -> [ 4. Table Styling ] -> [ 5. Export ] |
+------------+-----------------------------------+----------------------------------------------------------------------------+
| Activity   | Left Panel: Pivot Layout & Fields | Center/Right Workspace: Active Worksheet & Canvas                          |
| Bar (44px) | (Moved from right to left!)       | Tabs: [ Sheet 01 • ] [ Regional Summary ] [+]                              |
|            |                                   |                                                                            |
| 📁 Files   | ::: PIVOT LAYOUT       Stage PIVOT| ┌───────────────┬─────────────────────────────┬───────────┐                |
| 🔗 Model   |                                   | │ Branch Name   │ Q1 2026   Q2 2026   Q3 2026 │ Total     │                |
| 📊 Layout  | • FIELD WELLS (K-COLORS)          | ├───────────────┼─────────────────────────────┼───────────┤                |
| 🎨 Themes  |   - Rows:    [R0] branch_name     | │ North Hub     │ 12,400    14,200    11,900  │ 38,500    │                |
| ⇡ Export   |   - Cols:    [C0] fiscal_year     | └───────────────┴─────────────────────────────┴───────────┘                |
| ───        |   - Vals:    [V0] SUM(revenue)    |                                                                            |
| ⚙ Settings | • STATS & SUMMARY                 +----------------------------------------------------------------------------+
|            |   - Rows:    4,210 groups         | Center/Right-Bottom: Data Quality & Diagnostics Drawer (Filter: All/Err)  |
|            |   - Cols:    4 columns            | ┌────────────────────────────────────────────────────────────────────────┐ |
|            | • OPTIONS & FOOTER BAND           | │ ⊗ RELATION   orders.customer_id -> customers.id           [Copy] [Fix] │ |
|            |   [✓] Row Subtotals               | │ 34 rows contain unmatched keys in base data (resolves to NULL labels). │ |
|            |   [✓] Column Totals               | │ ┌────────────────────────────────────────────────────────────────────┐ │ |
|            |                                   | │ │ 💡 Tip: Trim whitespace or inspect key format in 'customers.xlsx'. │ │ |
|            |                                   | │ └────────────────────────────────────────────────────────────────────┘ │ |
|            |                                   | └────────────────────────────────────────────────────────────────────────┘ |
+------------+-----------------------------------+----------------------------------------------------------------------------+
| Bottom Status Bar: Cell [R4, C2] | 48,210 Rows x 14 Cols | Compute: 12ms | Engine: SQLite Pushdown | ● Clean                 |
+-----------------------------------------------------------------------------------------------------------------------------+
```

### 2.1 Key Layout Refinements:
1. **Left Activity Bar (Icon Menu)**: Replaces traditional top text menus with a sleek 44px vertical icon strip:
   - `📁 Files`: File management, workspace open/save, recent projects popup.
   - `🔗 Model`: Data sources, schema preview, and lookup relations manager.
   - `📊 Layout`: Pivot fields builder, row/column/value assignments.
   - `🎨 Themes`: Role-based table palettes and border themes.
   - `⇡ Export`: Multi-sheet XLSX, PDF, and HTML export actions.
   - `⚙ Settings`: Brightness mode (Deriva Dark), output language, preferences.
2. **Left Docked Inspector (`dock_left`)**:
   - The Pivot Layout / Field Wells panel is moved from the right side to the **left side** (adjacent to the Activity Bar), perfectly mirroring the position of the Explorer / Pipeline panel in DERIVA.
3. **Spacious Center-Right Canvas**:
   - The central area houses the primary **Themed Pivot Grid Canvas** on top and the **Compiler Diagnostics & Problems Drawer** on the bottom.

---

## 3. SVG Icon System Specification

### 3.1 Library Choice: VS Code Codicons & Lucide Icons (MIT / Permissive)
To match DERIVA’s exact visual language, we borrow 14 vector SVGs from **VS Code Codicons** (by Microsoft) and **Lucide Icons**, licensed under MIT/ISC.

### 3.2 Icon Inventory

| Filename | Source Library | Role in UI | Visual Target in DERIVA |
| :--- | :--- | :--- | :--- |
| `app-logo.svg` | Custom SVG | Brand mark top-left | `[D]` square logo |
| `database.svg` | Lucide (MIT) | Stage 1: Data Sources & Ingestion | Data storage cylinder |
| `git-merge.svg` | Codicons (MIT) | Stage 2: Relations & Lookup Model | Join / branching node |
| `table.svg` | Lucide (MIT) | Stage 3: Pivot Grid & Layout | Matrix table icon |
| `palette.svg` | Lucide (MIT) | Stage 4: Table Styling & Roles | Color palette |
| `download.svg` | Lucide (MIT) | Stage 5: Export (XLSX, PDF, HTML) | Export download arrow |
| `files.svg` | Codicons (MIT) | Activity Bar: Files & Datasets | Explorer overlapping sheets |
| `play.svg` | Codicons (MIT) | Top Bar: Recompute execution button | Green `▶ RUN` triangle |
| `alert-circle.svg` | Codicons (MIT) | Diagnostic Card: Error badge | Red `(!)` error indicator |
| `alert-triangle.svg`| Codicons (MIT) | Diagnostic Card: Warning badge | Amber `▲` warning triangle |
| `lightbulb.svg` | Lucide (MIT) | Diagnostic Card: Fix tip badge | Blue `💡 Tip` container |
| `copy.svg` | Codicons (MIT) | Diagnostic Card: Copy error text | Clean document copy icon |
| `grip-vertical.svg`| Codicons (MIT) | Panel Titlebars: 6-dot drag handle | `:::` drag dots |
| `settings.svg` | Codicons (MIT) | Activity Bar: Preferences | Gear wheel |

### 3.3 Integration Engine: `egui_extras` Native SVG Loader
Add `egui_extras` to `crates/pivot-app/Cargo.toml`:
```toml
egui_extras = { version = "0.31", features = ["svg"] }
```
During startup in `PivotApp::new(cc)`:
```rust
egui_extras::install_image_loaders(&cc.egui_ctx);
```
All SVGs are rendered with **dynamic token tinting**, ensuring they automatically recolor during theme switching:
```rust
ui.add(
    egui::Image::new(egui::include_image!("../assets/icons/database.svg"))
        .max_size(egui::vec2(16.0, 16.0))
        .tint(t.accent_cyan)
);
```

---

## 4. Phased Implementation Roadmap

### Phase 1: Theme & Visual Tokens (`crates/pivot-theme`)
- [x] Add `Brightness::DerivaDark` to the theme registry.
- [x] Add cyber dark palette tokens:
  - Deep midnight canvas: `#0a0e17`
  - Studio panels: `#131926`
  - Subtle borders: `#202b3e`
  - Luminous blue accent: `#3b82f6`
  - Cyan glow: `#38bdf8`
  - Diagnostic error container: `#291216` with border `#822735` and text `#f87171`
  - Diagnostic warning container: `#2a1e0f` with border `#855018` and text `#fbbf24`
  - Tip container: `#0f1c30` with border `#203a60` and text `#93c5fd`
- [x] Run and update unit test suites in `pivot-theme`.

---

### Phase 2: SVG Icon Assets & Loader Integration (`crates/pivot-app`)
- [ ] Create directory `crates/pivot-app/assets/icons/`.
- [ ] Add the 14 approved MIT/ISC SVG vector assets.
- [ ] Update `crates/pivot-app/Cargo.toml` with `egui_extras = { version = "0.31", features = ["svg"] }`.
- [ ] Call `egui_extras::install_image_loaders(&cc.egui_ctx)` in `crates/pivot-app/src/app.rs`.
- [ ] Add icon helper module `crates/pivot-app/src/icons.rs` with typed helper functions:
  - `icon_btn(ui, icon_path, label, color) -> Response`
  - `icon_pill(ui, icon_path, text, bg, fg)`

---

### Phase 3: Studio Chrome & Pipeline Stage Stepper (`crates/pivot-app/src/chrome.rs`)
- [ ] **Top Studio Header**:
  - Replace the plain caption bar with the Deriva header:
    - Left: Brand mark `[P] PIVOT STUDIO v0.2.2`.
    - Presets & Quick Actions: `Presets ˅`, `Deriva Dark ˅` theme selector dropdown.
    - Center-Right Controls:
      - Engine indicator badge: `SQLite Pushdown` or `In-Memory Table`.
      - Active sheet count badge: `3 Sheets`.
      - High-visibility `▶ RECOMPUTE` button (emerald green with glow).
      - Health Status pill: `● 0 Errors` (green) or `● 2 Issues` (amber/red).
    - Window caption buttons: Minimize, Maximize, Close.
- [ ] **Horizontal Pipeline Stage Stepper (Sub-header)**:
  - Add a ribbon below the top bar:
    - `1. Data Sources` (icon: `database.svg`)
    - `2. Relations & Model` (icon: `git-merge.svg`)
    - `3. Pivot Layout` (icon: `table.svg`)
    - `4. Table Styling` (icon: `palette.svg`)
    - `5. Export & Output` (icon: `download.svg`)
  - Clicking any stage smoothly updates the main panel without clearing active worksheet state.

---

### Phase 4: Left Activity Bar (Icon Menu) & Collapsible Explorer
- [ ] **Left Activity Bar (44px Icon Strip)**:
  - Move the primary application menu from the top text bar to the **vertical left icon strip**:
    - `📁 Files`: File menu actions (New, Open Workspace, Import CSV/XLSX/DBF, Save).
    - `🔗 Model`: Data source schemas, relations, and column selection.
    - `📊 Layout`: Toggles the left Pivot Layout & Field Wells panel.
    - `🎨 Themes`: Table palettes, borders, and footer bands.
    - `⇡ Export`: Multi-worksheet XLSX, PDF, and HTML export dialog.
    - `⚙ Settings`: App brightness (Deriva Dark), output language, and preferences.
- [ ] Active state indicator: Cyan accent vertical bar on active icon.

---

### Phase 5: Left Docked Inspector & K-Color Field Badges (`dock.rs`)
- [ ] **Move Pivot Layout Panel from Right to Left (`dock_left`)**:
  - Relocate the Field Wells & Pivot Layout controls from the right sidebar to a 280px-320px resizable left panel adjacent to the Activity Bar.
  - Header with `:::` drag handle and stage badge (`Stage PIVOT`).
  - Two-column grid matching Deriva's *Register Assignments* and *Live Ranges*:
    - **Left Well: Active Field Assignments (K-Colors)**:
      - Rows: `[R0] branch_name` (assigned blue register-style badge)
      - Columns: `[C0] fiscal_year`
      - Values: `[V0] SUM(revenue)`
    - **Right Well: Cardinality & Summary Stats**:
      - Cardinality span: `4,210 unique row groups`
      - Null count: `0 nulls (100% matched)`
      - Memory footprint: `1.8 MB`
  - Subtotals and options collapsible accordion below.

---

### Phase 6: Smart Diagnostics & Actionable Tips System (`dock.rs`)
- [ ] Replace flat text string errors with **Deriva Diagnostic Cards** in the center-bottom split drawer:
  - Category tag pill: `RELATION`, `SCHEMA`, `STALE_REF`, or `DATA_TYPE`.
  - Coordinate badge: e.g. `Col 3 (kegiatan_id)` or `Sheet 02`.
  - Main issue description: Clean, high-legibility font.
  - **Actionable `💡 Tip` Box**:
    - Embedded midnight-blue card with lightbulb icon.
    - Contextual remediation advice (e.g. *"Trim whitespace from lookup column"* or *"Remove unused field from Row grouping"*).
  - Quick action buttons: `Copy Details` and `Dismiss`.
- [ ] Filter header tabs:
  - `All (N)` | `Errors (N)` | `Warnings (N)` | Search filter input.

---

### Phase 7: Status Bar Telemetry
- [ ] Update `chrome::status_bar`:
  - Left: Active Cell coordinate `[Row 12, Col 3]`.
  - Center: Status message and async progress bar / cancel button.
  - Right: Technical telemetry:
    - Row count: `48,210 rows`
    - Compute latency: `14ms`
    - Engine backend: `SQLite / Memory`
    - System health pill: `● Ready`

---

## 5. Quality Assurance & Verification Criteria

1. **Compilation & Test Pass**:
   - `cargo test --workspace` passes 100% with zero regressions.
   - `cargo check -p pivot-app` completes cleanly with zero warnings.
2. **Visual Fidelity Verification**:
   - UI matches `Screenshot 2026-11-10 115940.png` color schemes, panel borders, and badge styling.
   - SVGs render sharply with exact tinting across 100%, 125%, 150%, and 200% OS display scaling.
3. **Data Pipeline Parity**:
   - Lookup relations continue to never multiply base rows.
   - XLSX, PDF, and HTML export formats remain byte-for-byte consistent with current specifications.
4. **Theme Roundtrip**:
   - Switching between `Deriva Dark`, `Medium Dark`, `Light`, and `High Contrast` updates all panels, SVGs, and text inks immediately without app restart.

---

## 6. Execution Approvals & Next Steps

When approved to execute:
1. Create `crates/pivot-app/assets/icons/` and download the 14 approved SVGs.
2. Update `crates/pivot-app/Cargo.toml` with `egui_extras`.
3. Implement `chrome.rs` header and stage ribbon.
4. Implement `dock.rs` diagnostic cards and contextual inspector.

