use crate::ExportStyle;
use pivot_core::{Cell, CellRole, FooterBandKind, ThemedTableModel};
use pivot_theme::table::{RoleStyle, TableStyle};

pub fn render(model: &ThemedTableModel, style: ExportStyle) -> String {
    render_sheets(&[(&model.title, model)], style)
}

pub fn render_sheets(sheets: &[(&str, &ThemedTableModel)], style: ExportStyle) -> String {
    let ts = style.resolve();
    let mut out = String::with_capacity(24 * 1024 * sheets.len().max(1));
    let title = sheets.first().map(|(n, _)| *n).unwrap_or("Pivot Export");
    out.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    out.push_str("<title>");
    out.push_str(&escape(title));
    out.push_str("</title><style>");
    out.push_str(&css_multi(&ts));
    out.push_str("</style></head><body>");
    if sheets.len() > 1 {
        out.push_str("<div class=\"sheet-tabs\" role=\"tablist\">");
        for (i, (name, _)) in sheets.iter().enumerate() {
            out.push_str("<button class=\"tab");
            if i == 0 { out.push_str(" active"); }
            out.push_str("\" role=\"tab\" data-sheet=\"");
            out.push_str(&i.to_string());
            out.push_str("\" onclick=\"showSheet(");
            out.push_str(&i.to_string());
            out.push_str(")\">");
            out.push_str(&escape(name));
            out.push_str("</button>");
        }
        out.push_str("</div>");
    }
    for (i, (name, model)) in sheets.iter().enumerate() {
        out.push_str("<div class=\"sheet-panel");
        if i == 0 { out.push_str(" active"); }
        out.push_str("\" data-sheet-panel=\"");
        out.push_str(&i.to_string());
        out.push_str("\"><h2 class=\"sheet-title\">");
        out.push_str(&escape(name));
        out.push_str("</h2><div class=\"pivot-wrap\">");
        render_table(&mut out, model, &ts);
        out.push_str("</div></div>");
    }
    if sheets.len() > 1 {
        out.push_str("<script>function showSheet(i){document.querySelectorAll('.tab').forEach((b,idx)=>b.classList.toggle('active',idx===i));document.querySelectorAll('.sheet-panel').forEach((p,idx)=>p.classList.toggle('active',idx===i));}</script>");
    }
    out.push_str("</body></html>");
    out
}

fn render_table(out: &mut String, model: &ThemedTableModel, ts: &TableStyle) {
    out.push_str("<table class=\"pivot\" cellspacing=\"0\" cellpadding=\"0\">");
    out.push_str("<caption>");
    out.push_str(&escape(&model.title));
    out.push_str("</caption>");

    // Header
    out.push_str("<thead>");
    for row in &model.header_rows {
        out.push_str("<tr>");
        for cell in row {
            render_th(out, cell, ts);
        }
        out.push_str("</tr>");
    }
    out.push_str("</thead>");

    // Body
    out.push_str("<tbody>");
    for (ri, row) in model.body_rows.iter().enumerate() {
        let zebra = ri % 2 == 1;
        out.push_str(if zebra { "<tr class=\"zebra\">" } else { "<tr>" });
        for cell in row {
            render_td(out, cell, ts, zebra);
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody>");

    // Footer bands (AC3.2 / AC6.1)
    if !model.footer_bands.is_empty() {
        out.push_str("<tfoot>");
        for band in &model.footer_bands {
            out.push_str("<tr data-band=\"");
            out.push_str(band_kind(band.kind));
            out.push_str("\">");
            for cell in &band.cells {
                render_td(out, cell, ts, false);
            }
            out.push_str("</tr>");
        }
        out.push_str("</tfoot>");
    }

    out.push_str("</table>");
}

fn render_th(out: &mut String, cell: &Cell, ts: &TableStyle) {
    let role = role_name(cell.role);
    let style = inline_cell(cell, cell_role_style(cell.role, ts, false));
    out.push_str("<th");
    attrs(out, cell, role);
    out.push_str(" style=\"");
    out.push_str(&style);
    out.push_str("\">");
    out.push_str(&escape(&cell.text));
    out.push_str("</th>");
}

fn render_td(out: &mut String, cell: &Cell, ts: &TableStyle, zebra: bool) {
    let role = role_name(cell.role);
    let style = inline_cell(cell, cell_role_style(cell.role, ts, zebra));
    out.push_str("<td");
    attrs(out, cell, role);
    out.push_str(" style=\"");
    out.push_str(&style);
    out.push_str("\">");
    out.push_str(&escape(&cell.text));
    out.push_str("</td>");
}

fn attrs(out: &mut String, cell: &Cell, role: &str) {
    out.push_str(" data-role=\"");
    out.push_str(role);
    out.push('"');
    if cell.colspan > 1 {
        out.push_str(" colspan=\"");
        out.push_str(&cell.colspan.to_string());
        out.push('"');
    }
    if cell.rowspan > 1 {
        out.push_str(" rowspan=\"");
        out.push_str(&cell.rowspan.to_string());
        out.push('"');
    }
}

fn inline_cell(cell: &Cell, rs: RoleStyle) -> String {
    let mut s = format!(
        "background:{};color:{};border:1px solid {};padding:6px 8px;text-align:{};font-weight:{};white-space:nowrap;",
        rgb(rs.fill), rgb(rs.text), rgb(rs.grid_line),
        if matches!(cell.role, CellRole::Body | CellRole::Subtotal | CellRole::GrandTotal | CellRole::Footer) && looks_numeric(&cell.text) { "right" } else { "left" },
        if rs.strong { "600" } else { "400" },
    );
    if cell.indent > 0 {
        use std::fmt::Write as _;
        let _ = write!(s, "padding-left:{}px;", 8 + cell.indent * 14);
    }
    s
}

fn looks_numeric(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | '-' | '+' | '%' | ' ' | '(' | ')'))
}

fn cell_role_style(role: CellRole, ts: &TableStyle, zebra: bool) -> RoleStyle {
    match role {
        CellRole::Title => RoleStyle { fill: ts.body.fill, text: ts.title, strong: true, grid_line: ts.border, border_mode: pivot_theme::table::CellBorderMode::None },
        CellRole::StubHead => ts.stub_head,
        CellRole::ColHeader => ts.col_header,
        CellRole::RowHeader => ts.row_header,
        CellRole::Body => if ts.zebra && zebra { ts.body_zebra } else { ts.body },
        CellRole::Subtotal => ts.subtotal,
        CellRole::GrandTotal => ts.grand_total,
        CellRole::Footer => ts.footer,
    }
}

fn role_name(role: CellRole) -> &'static str {
    match role {
        CellRole::Title => "title",
        CellRole::StubHead => "stub_head",
        CellRole::ColHeader => "col_header",
        CellRole::RowHeader => "row_header",
        CellRole::Body => "body",
        CellRole::Subtotal => "subtotal",
        CellRole::GrandTotal => "grand_total",
        CellRole::Footer => "footer",
    }
}

fn band_kind(kind: FooterBandKind) -> &'static str {
    match kind {
        FooterBandKind::Subtotal => "subtotal",
        FooterBandKind::GrandTotal => "grand_total",
        FooterBandKind::Average => "average",
        FooterBandKind::Count => "count",
        FooterBandKind::Notes => "notes",
        FooterBandKind::Custom => "custom",
    }
}

fn css_multi(ts: &TableStyle) -> String {
    format!(
        "body{{margin:16px;background:{};color:{};font:13px Segoe UI,system-ui,sans-serif;}}\
         .pivot-wrap{{overflow:auto;}}\
         table.pivot{{border-collapse:collapse;border:1px solid {};min-width:320px;}}\
         caption{{caption-side:top;text-align:left;font-size:18px;font-weight:700;color:{};padding:8px 2px 10px;}}\
         tfoot td{{border-top:1px solid {};}}\
         .sheet-tabs{{display:flex;gap:6px;margin:0 0 12px;flex-wrap:wrap;}}\
         .tab{{padding:6px 12px;border:1px solid {};background:{};color:{};cursor:pointer;border-radius:4px;font:inherit;}}\
         .tab.active{{background:{};color:{};font-weight:600;}}\
         .sheet-panel{{display:none;}}.sheet-panel.active{{display:block;}}\
         .sheet-title{{margin:0 0 8px;font-size:14px;color:{};}}",
        rgb(ts.body.fill), rgb(ts.body.text), rgb(ts.border), rgb(ts.title), rgb(ts.border),
        rgb(ts.border), rgb(ts.stub_head.fill), rgb(ts.body.text),
        rgb(ts.col_header.fill), rgb(ts.col_header.text),
        rgb(ts.body.text)
    )
}

fn rgb(c: egui::Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(ch),
        }
    }
    o
}
