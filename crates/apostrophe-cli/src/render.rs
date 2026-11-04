//! Markdown dibujado para la terminal (`elision view`), al estilo de glow: titulos, enfasis,
//! listas, citas, codigo y tablas, con el texto ajustado al ancho.
//!
//! Con `color` se usan secuencias ANSI de 16 colores, las que entiende cualquier terminal
//! moderna (Windows Terminal, la consola de Windows 10+ con VT activado, VS Code). Sin
//! color sale el mismo diseño en texto plano: es lo que recibe quien redirige la salida a un
//! archivo o a otro programa.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use unicode_width::UnicodeWidthStr;

/// Ancho cuando no hay terminal de la que leerlo (salida redirigida).
pub const DEFAULT_WIDTH: usize = 80;
/// Por debajo de esto el ajuste deja de tener sentido: se dibuja a este ancho y la terminal
/// corta lo que sobre.
const MIN_WIDTH: usize = 20;
/// Lo minimo a lo que se angosta una columna de tabla para que la tabla entre.
const MIN_COLUMN: usize = 6;

/// Estilo de un tramo de texto.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: bool,
    dim: bool,
    heading: bool,
}

impl Style {
    /// Secuencia SGR que activa este estilo (vacia si es el normal).
    fn sgr(self) -> String {
        let mut codes: Vec<&str> = Vec::new();
        if self.bold || self.heading {
            codes.push("1");
        }
        if self.dim {
            codes.push("2");
        }
        if self.italic {
            codes.push("3");
        }
        if self.link {
            codes.push("4");
        }
        if self.strike {
            codes.push("9");
        }
        // Un solo color por tramo, en orden de importancia.
        if self.heading {
            codes.push("35"); // magenta
        } else if self.code {
            codes.push("33"); // amarillo
        } else if self.link {
            codes.push("34"); // azul
        }
        if codes.is_empty() {
            String::new()
        } else {
            format!("\x1b[{}m", codes.join(";"))
        }
    }
}

#[derive(Debug, Clone)]
struct Span {
    text: String,
    style: Style,
}

/// Prefijo de cada linea de un bloque: el de la primera y el de las que siguen (en una
/// lista, `• ` y luego dos espacios).
#[derive(Debug, Clone, Default)]
struct Prefix {
    first: Vec<Span>,
    rest: Vec<Span>,
}

impl Prefix {
    fn width(spans: &[Span]) -> usize {
        spans.iter().map(|s| s.text.width()).sum()
    }
}

/// Una lista abierta: el numero del proximo elemento si es ordenada.
struct List {
    next: Option<u64>,
}

struct Table {
    alignments: Vec<Alignment>,
    rows: Vec<Vec<Vec<Span>>>,
    header_rows: usize,
    cell: Vec<Span>,
    row: Vec<Vec<Span>>,
    in_head: bool,
}

struct Renderer {
    color: bool,
    width: usize,
    out: Vec<String>,
    /// Texto del bloque en curso, aun sin ajustar.
    inline: Vec<Span>,
    style: Style,
    /// Prefijos de los bloques abiertos (citas y elementos de lista), de afuera hacia adentro.
    containers: Vec<Prefix>,
    lists: Vec<List>,
    /// Marcador pendiente del elemento de lista: va solo en su primera linea.
    pending_marker: Option<String>,
    link_urls: Vec<String>,
    code_block: Option<String>,
    table: Option<Table>,
    /// Hay que dejar una linea en blanco antes del proximo bloque.
    gap: bool,
}

/// Dibuja `markdown` para la terminal. `width` es el ancho total disponible.
pub fn render(markdown: &str, width: usize, color: bool) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH;
    let mut r = Renderer {
        color,
        width: width.max(MIN_WIDTH),
        out: Vec::new(),
        inline: Vec::new(),
        style: Style::default(),
        containers: Vec::new(),
        lists: Vec::new(),
        pending_marker: None,
        link_urls: Vec::new(),
        code_block: None,
        table: None,
        gap: false,
    };
    for event in Parser::new_ext(markdown, options) {
        r.event(event);
    }
    r.flush();
    let mut text = r.out.join("\n");
    text.push('\n');
    text
}

impl Renderer {
    fn event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if let Some(code) = self.code_block.as_mut() {
                    code.push_str(&text);
                } else {
                    self.push(&text, self.style);
                }
            }
            Event::Code(text) | Event::InlineMath(text) => {
                let style = Style {
                    code: true,
                    ..self.style
                };
                self.push(&text, style);
            }
            Event::DisplayMath(text) => {
                self.flush();
                self.code_lines(&text);
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                let style = Style {
                    dim: true,
                    ..self.style
                };
                self.push(html.trim_end_matches('\n'), style);
            }
            Event::SoftBreak => self.push(" ", self.style),
            Event::HardBreak => self.push("\n", self.style),
            Event::Rule => {
                self.flush();
                self.block_gap();
                let avail = self.width.saturating_sub(self.prefix_width());
                let rule = Span {
                    text: "─".repeat(avail),
                    style: Style {
                        dim: true,
                        ..Style::default()
                    },
                };
                let line = self.prefixed(true, vec![rule]);
                self.out.push(line);
                self.gap = true;
            }
            Event::TaskListMarker(done) => {
                self.push(if done { "[x] " } else { "[ ] " }, self.style);
            }
            Event::FootnoteReference(label) => {
                let style = Style {
                    dim: true,
                    ..self.style
                };
                self.push(&format!("[^{label}]"), style);
            }
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                self.flush();
                self.block_gap();
            }
            Tag::Heading { level, .. } => {
                self.flush();
                self.block_gap();
                let marks = "#".repeat(heading_number(level));
                let style = Style {
                    dim: true,
                    ..Style::default()
                };
                self.push(&format!("{marks} "), style);
                self.style.heading = true;
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.block_gap();
                let bar = Span {
                    text: "│ ".into(),
                    style: Style {
                        dim: true,
                        ..Style::default()
                    },
                };
                self.containers.push(Prefix {
                    first: vec![bar.clone()],
                    rest: vec![bar],
                });
                self.style.italic = true;
            }
            Tag::CodeBlock(kind) => {
                self.flush();
                self.block_gap();
                if let CodeBlockKind::Fenced(lang) = kind {
                    if !lang.is_empty() {
                        let label = Span {
                            text: format!("  {lang}"),
                            style: Style {
                                dim: true,
                                ..Style::default()
                            },
                        };
                        let line = self.prefixed(true, vec![label]);
                        self.out.push(line);
                    }
                }
                self.code_block = Some(String::new());
            }
            Tag::List(start) => {
                self.flush();
                // Una lista dentro de un elemento va pegada a su texto.
                if self.lists.is_empty() {
                    self.block_gap();
                }
                self.lists.push(List { next: start });
            }
            Tag::Item => {
                self.flush();
                let marker = match self.lists.last_mut().and_then(|l| l.next.as_mut()) {
                    Some(n) => {
                        let m = format!("{n}. ");
                        *n += 1;
                        m
                    }
                    None => "• ".to_string(),
                };
                let indent = " ".repeat(marker.width());
                self.containers.push(Prefix {
                    first: vec![Span {
                        text: marker.clone(),
                        style: Style::default(),
                    }],
                    rest: vec![Span {
                        text: indent,
                        style: Style::default(),
                    }],
                });
                self.pending_marker = Some(marker);
                self.gap = false;
            }
            Tag::FootnoteDefinition(label) => {
                self.flush();
                self.block_gap();
                let style = Style {
                    dim: true,
                    ..Style::default()
                };
                self.push(&format!("[^{label}]: "), style);
            }
            Tag::Table(alignments) => {
                self.flush();
                self.block_gap();
                self.table = Some(Table {
                    alignments,
                    rows: Vec::new(),
                    header_rows: 0,
                    cell: Vec::new(),
                    row: Vec::new(),
                    in_head: false,
                });
            }
            Tag::TableHead => {
                if let Some(t) = self.table.as_mut() {
                    t.in_head = true;
                }
                self.style.bold = true;
            }
            Tag::TableRow | Tag::TableCell => {}
            Tag::Emphasis => self.style.italic = true,
            Tag::Strong => self.style.bold = true,
            Tag::Strikethrough => self.style.strike = true,
            Tag::Link { dest_url, .. } => {
                self.style.link = true;
                self.link_urls.push(dest_url.to_string());
            }
            Tag::Image { dest_url, .. } => {
                let style = Style {
                    dim: true,
                    ..self.style
                };
                self.push("[image: ", style);
                self.link_urls.push(dest_url.to_string());
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush();
                self.gap = true;
            }
            TagEnd::Heading(_) => {
                self.flush();
                self.style.heading = false;
                self.gap = true;
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.containers.pop();
                self.style.italic = false;
                self.gap = true;
            }
            TagEnd::CodeBlock => {
                let code = self.code_block.take().unwrap_or_default();
                self.code_lines(&code);
                self.gap = true;
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
                self.gap = true;
            }
            TagEnd::Item => {
                self.flush();
                self.containers.pop();
                self.pending_marker = None;
                // Dentro de una lista los elementos van seguidos.
                self.gap = false;
            }
            TagEnd::FootnoteDefinition => {
                self.flush();
                self.gap = true;
            }
            TagEnd::TableCell => {
                if let Some(t) = self.table.as_mut() {
                    t.cell = std::mem::take(&mut self.inline);
                    t.row.push(std::mem::take(&mut t.cell));
                }
            }
            TagEnd::TableHead => {
                if let Some(t) = self.table.as_mut() {
                    t.rows.push(std::mem::take(&mut t.row));
                    t.header_rows = t.rows.len();
                    t.in_head = false;
                }
                self.style.bold = false;
            }
            TagEnd::TableRow => {
                if let Some(t) = self.table.as_mut() {
                    t.rows.push(std::mem::take(&mut t.row));
                }
            }
            TagEnd::Table => {
                if let Some(t) = self.table.take() {
                    self.table_lines(&t);
                }
                self.gap = true;
            }
            TagEnd::Emphasis => self.style.italic = false,
            TagEnd::Strong => self.style.bold = false,
            TagEnd::Strikethrough => self.style.strike = false,
            TagEnd::Link => {
                self.style.link = false;
                let url = self.link_urls.pop().unwrap_or_default();
                let shown: String = self.inline_tail_text();
                // Un autolink (`<https://…>`) ya muestra la direccion: no se repite.
                if !url.is_empty() && shown != url && !url.starts_with('#') {
                    let style = Style {
                        dim: true,
                        ..Style::default()
                    };
                    self.push(&format!(" ({url})"), style);
                }
            }
            TagEnd::Image => {
                let url = self.link_urls.pop().unwrap_or_default();
                let style = Style {
                    dim: true,
                    ..self.style
                };
                self.push(&format!("] ({url})"), style);
            }
            _ => {}
        }
    }

    /// Texto del ultimo enlace, para no repetir la direccion cuando ya es el texto.
    fn inline_tail_text(&self) -> String {
        self.inline
            .iter()
            .rev()
            .take_while(|s| s.style.link)
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    fn push(&mut self, text: &str, style: Style) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.inline.last_mut() {
            if last.style == style {
                last.text.push_str(text);
                return;
            }
        }
        self.inline.push(Span {
            text: text.to_string(),
            style,
        });
    }

    fn block_gap(&mut self) {
        if self.gap && !self.out.is_empty() {
            let line = self.prefixed(false, Vec::new());
            self.out.push(line.trim_end().to_string());
        }
        self.gap = false;
    }

    fn prefix_width(&self) -> usize {
        self.containers
            .iter()
            .map(|p| Prefix::width(&p.first).max(Prefix::width(&p.rest)))
            .sum()
    }

    /// Una linea con los prefijos de los bloques abiertos. `first` decide si el elemento de
    /// lista mas interno lleva su marcador (solo la primera linea de su texto).
    fn prefixed(&mut self, first: bool, content: Vec<Span>) -> String {
        let mut spans: Vec<Span> = Vec::new();
        let marker_pending = first && self.pending_marker.is_some();
        let last = self.containers.len().saturating_sub(1);
        for (i, prefix) in self.containers.iter().enumerate() {
            let use_first = i == last && marker_pending;
            spans.extend(if use_first {
                prefix.first.clone()
            } else {
                prefix.rest.clone()
            });
        }
        if marker_pending {
            self.pending_marker = None;
        }
        spans.extend(content);
        self.paint(&spans)
    }

    fn paint(&self, spans: &[Span]) -> String {
        let mut line = String::new();
        for span in spans {
            let sgr = if self.color {
                span.style.sgr()
            } else {
                String::new()
            };
            if sgr.is_empty() {
                line.push_str(&span.text);
            } else {
                line.push_str(&sgr);
                line.push_str(&span.text);
                line.push_str("\x1b[0m");
            }
        }
        line
    }

    /// Ajusta el texto acumulado al ancho y lo emite con los prefijos.
    fn flush(&mut self) {
        if self.table.is_some() {
            return; // las celdas se juntan en `TableCell`
        }
        let spans = std::mem::take(&mut self.inline);
        if spans.iter().all(|s| s.text.trim().is_empty()) {
            return;
        }
        let avail = self.width.saturating_sub(self.prefix_width()).max(10);
        for (i, line) in wrap(&spans, avail).into_iter().enumerate() {
            let painted = self.prefixed(i == 0, line);
            self.out.push(painted.trim_end().to_string());
        }
    }

    /// Codigo: sin ajustar (cortarlo cambiaria su sentido), con sangria y en otro color.
    fn code_lines(&mut self, code: &str) {
        let style = Style {
            code: true,
            ..Style::default()
        };
        for raw in code.trim_end_matches('\n').split('\n') {
            let span = Span {
                text: format!("  {}", raw.replace('\t', "    ")),
                style,
            };
            let line = self.prefixed(true, vec![span]);
            self.out.push(line.trim_end().to_string());
        }
    }

    fn table_lines(&mut self, table: &Table) {
        let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return;
        }
        let text_of = |cell: &[Span]| -> usize { cell.iter().map(|s| s.text.width()).sum() };
        let mut widths = vec![0usize; columns];
        for row in &table.rows {
            for (c, cell) in row.iter().enumerate() {
                widths[c] = widths[c].max(text_of(cell));
            }
        }
        // Si no entra, se angosta la columna mas ancha hasta que entre (o hasta el minimo) y
        // el texto de sus celdas se ajusta en varias lineas.
        let avail = self
            .width
            .saturating_sub(self.prefix_width())
            .saturating_sub(3 * (columns - 1));
        while widths.iter().sum::<usize>() > avail {
            let (widest, &w) = widths
                .iter()
                .enumerate()
                .max_by_key(|(_, w)| **w)
                .expect("hay columnas");
            if w <= MIN_COLUMN {
                break;
            }
            widths[widest] = w - 1;
        }
        let border = Style {
            dim: true,
            ..Style::default()
        };
        let sep = |s: &str| Span {
            text: s.to_string(),
            style: border,
        };
        for (r, row) in table.rows.iter().enumerate() {
            let cells: Vec<Vec<Vec<Span>>> = widths
                .iter()
                .enumerate()
                .map(|(c, w)| wrap(row.get(c).map(Vec::as_slice).unwrap_or(&[]), *w))
                .collect();
            let height = cells.iter().map(Vec::len).max().unwrap_or(1);
            for l in 0..height {
                let mut spans = Vec::new();
                for (c, width) in widths.iter().enumerate() {
                    if c > 0 {
                        spans.push(sep(" │ "));
                    }
                    let line = cells[c].get(l).cloned().unwrap_or_default();
                    let pad = width.saturating_sub(text_of(&line));
                    let (left, right) = match table.alignments.get(c) {
                        Some(Alignment::Right) => (pad, 0),
                        Some(Alignment::Center) => (pad / 2, pad - pad / 2),
                        _ => (0, pad),
                    };
                    spans.push(Span {
                        text: " ".repeat(left),
                        style: Style::default(),
                    });
                    spans.extend(line);
                    spans.push(Span {
                        text: " ".repeat(right),
                        style: Style::default(),
                    });
                }
                let line = self.prefixed(true, spans);
                self.out.push(line.trim_end().to_string());
            }
            if r + 1 == table.header_rows {
                let rule: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
                let line = self.prefixed(true, vec![sep(&rule.join("─┼─"))]);
                self.out.push(line);
            }
        }
    }
}

fn heading_number(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Reparte los tramos en lineas de a lo sumo `width` columnas, cortando en los espacios.
/// Una palabra mas larga que el ancho queda sola en su linea (la terminal la corta). Los
/// saltos duros (`\n`) fuerzan una linea nueva.
fn wrap(spans: &[Span], width: usize) -> Vec<Vec<Span>> {
    let mut lines: Vec<Vec<Span>> = vec![Vec::new()];
    let mut used = 0usize;
    // Espacio pendiente: se escribe solo si despues viene una palabra en la misma linea.
    let mut pending_space: Option<Style> = None;

    for span in spans {
        for (i, segment) in span.text.split('\n').enumerate() {
            if i > 0 {
                lines.push(Vec::new());
                used = 0;
                pending_space = None;
            }
            for (j, word) in segment.split(' ').enumerate() {
                if j > 0 {
                    pending_space = Some(span.style);
                }
                if word.is_empty() {
                    continue;
                }
                let w = word.width();
                let space = usize::from(pending_space.is_some() && used > 0);
                if used > 0 && used + space + w > width {
                    lines.push(Vec::new());
                    used = 0;
                } else if let (Some(style), true) = (pending_space, used > 0) {
                    push_span(lines.last_mut().unwrap(), " ", style);
                    used += 1;
                }
                pending_space = None;
                push_span(lines.last_mut().unwrap(), word, span.style);
                used += w;
            }
        }
    }
    lines
}

fn push_span(line: &mut Vec<Span>, text: &str, style: Style) {
    if let Some(last) = line.last_mut() {
        if last.style == style {
            last.text.push_str(text);
            return;
        }
    }
    line.push(Span {
        text: text.to_string(),
        style,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(md: &str) -> String {
        render(md, 40, false)
    }

    #[test]
    fn titulos_parrafos_y_espacios_entre_bloques() {
        let out = plain("# Hola\n\nPrimer parrafo.\n\n## Dos\n\nOtro.");
        assert_eq!(out, "# Hola\n\nPrimer parrafo.\n\n## Dos\n\nOtro.\n");
    }

    #[test]
    fn el_texto_se_ajusta_al_ancho() {
        let out = render(
            "uno dos tres cuatro cinco seis siete ocho nueve diez",
            MIN_WIDTH,
            false,
        );
        for line in out.lines() {
            assert!(line.width() <= MIN_WIDTH, "{line:?}");
        }
        assert_eq!(out.split_whitespace().count(), 10, "no se pierden palabras");
    }

    #[test]
    fn listas_con_vinetas_numeros_y_sangria_al_ajustar() {
        let out = render(
            "- uno\n- dos con un texto bastante largo\n\n1. a\n2. b",
            MIN_WIDTH,
            false,
        );
        assert_eq!(
            out,
            "• uno\n• dos con un texto\n  bastante largo\n\n1. a\n2. b\n"
        );
    }

    #[test]
    fn listas_anidadas() {
        let out = plain("- a\n  - b\n- c");
        assert_eq!(out, "• a\n  • b\n• c\n");
    }

    #[test]
    fn citas_y_codigo() {
        let out = plain("> citado\n\n```rust\nfn main() {}\n```");
        assert_eq!(out, "│ citado\n\n  rust\n  fn main() {}\n");
    }

    #[test]
    fn enlaces_muestran_la_direccion_salvo_si_ya_es_el_texto() {
        assert_eq!(plain("[sitio](https://x.y)"), "sitio (https://x.y)\n");
        assert_eq!(plain("<https://x.y>"), "https://x.y\n");
    }

    #[test]
    fn tablas_alineadas_con_bordes() {
        let out = plain("| a | bb |\n|---|---:|\n| ccc | d |");
        assert_eq!(out, "a   │ bb\n────┼───\nccc │  d\n");
    }

    #[test]
    fn una_tabla_que_no_entra_ajusta_sus_celdas() {
        let out = render(
            "| corto | una celda con bastante texto |\n|---|---|\n| a | b |",
            24,
            false,
        );
        for line in out.lines() {
            assert!(line.width() <= 24, "{line:?}");
        }
        assert_eq!(
            out,
            "corto │ una celda con\n      │ bastante texto\n──────┼─────────────────\na     │ b\n"
        );
    }

    #[test]
    fn tareas_y_regla() {
        let out = plain("- [x] hecho\n- [ ] falta\n\n---");
        assert!(out.starts_with("• [x] hecho\n• [ ] falta\n\n"));
        assert!(out.trim_end().ends_with(&"─".repeat(40)));
    }

    #[test]
    fn con_color_usa_ansi_y_sin_color_no() {
        let colored = render("# T\n\n**negrita** y `codigo`", 40, true);
        assert!(colored.contains("\x1b[1;35mT\x1b[0m"));
        assert!(colored.contains("\x1b[1mnegrita\x1b[0m"));
        assert!(colored.contains("\x1b[33mcodigo\x1b[0m"));
        assert!(!plain("# T\n\n**negrita**").contains('\x1b'));
    }

    #[test]
    fn los_acentos_cuentan_como_un_ancho() {
        let out = render("áéíóú áéíóú áéíóú áéíóú", MIN_WIDTH, false);
        assert_eq!(out, "áéíóú áéíóú áéíóú\náéíóú\n");
    }
}
