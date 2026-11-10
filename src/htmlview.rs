//! A small HTML reader for the response preview.
//!
//! It is not a browser: it turns the page into headings, paragraphs, lists, quotes, code
//! blocks, simple tables and links, and skips scripts, styles and the `<head>`. For the
//! real page there is "Open in browser".

/// A piece of text with the style it was written in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub underline: bool,
    pub strike: bool,
    pub mark: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(u8, Vec<Run>),
    Paragraph(Vec<Run>),
    Item {
        depth: usize,
        marker: String,
        runs: Vec<Run>,
    },
    Quote(Vec<Run>),
    Pre(String),
    Rule,
    Row(Vec<Vec<Run>>),
}

/// Pages longer than this are cut (a preview does not need megabytes of text).
const INPUT_LIMIT: usize = 1_500_000;
/// Most blocks kept.
pub const BLOCK_LIMIT: usize = 3000;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Paragraph,
    Heading(u8),
    Item,
}

struct Builder {
    blocks: Vec<Block>,
    runs: Vec<Run>,
    kind: Kind,

    bold: u32,
    italic: u32,
    code: u32,
    underline: u32,
    strike: u32,
    mark: u32,
    links: Vec<Option<String>>,

    /// `(ordered, next number)` for every list we are inside.
    lists: Vec<(bool, u32)>,
    item_marker: String,
    item_depth: usize,
    quote_depth: u32,

    pre: Option<String>,
    skip: Vec<String>,

    row: Option<Vec<Vec<Run>>>,
    in_cell: bool,
}

impl Builder {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            runs: Vec::new(),
            kind: Kind::Paragraph,
            bold: 0,
            italic: 0,
            code: 0,
            underline: 0,
            strike: 0,
            mark: 0,
            links: Vec::new(),
            lists: Vec::new(),
            item_marker: String::new(),
            item_depth: 0,
            quote_depth: 0,
            pre: None,
            skip: Vec::new(),
            row: None,
            in_cell: false,
        }
    }

    fn style(&self) -> Run {
        Run {
            text: String::new(),
            bold: self.bold > 0 || matches!(self.kind, Kind::Heading(_)),
            italic: self.italic > 0,
            code: self.code > 0,
            underline: self.underline > 0 || self.links.last().is_some_and(Option::is_some),
            strike: self.strike > 0,
            mark: self.mark > 0,
            link: self.links.last().cloned().flatten(),
        }
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(pre) = &mut self.pre {
            pre.push_str(text);
            return;
        }

        // Runs of white space collapse into one space; none at the start of a block.
        let mut cleaned = String::with_capacity(text.len());
        let mut last_space = self
            .runs
            .last()
            .is_none_or(|r| r.text.ends_with(char::is_whitespace));
        for c in text.chars() {
            if c.is_whitespace() && c != '\u{a0}' {
                if !last_space {
                    cleaned.push(' ');
                }
                last_space = true;
            } else {
                cleaned.push(c);
                last_space = false;
            }
        }
        if cleaned.is_empty() {
            return;
        }

        let style = self.style();
        match self.runs.last_mut() {
            Some(last) if same_style(last, &style) => last.text.push_str(&cleaned),
            _ => self.runs.push(Run {
                text: cleaned,
                ..style
            }),
        }
    }

    fn push_break(&mut self) {
        if self.pre.is_some() {
            self.push_text("\n");
            return;
        }
        let style = self.style();
        self.runs.push(Run {
            text: "\n".into(),
            ..style
        });
    }

    /// Takes the pending runs (trimmed), or `None` when there is only blank text.
    fn take_runs(&mut self) -> Option<Vec<Run>> {
        let mut runs = std::mem::take(&mut self.runs);
        while runs
            .last()
            .is_some_and(|r| r.text.trim_end_matches([' ', '\n']).is_empty())
        {
            runs.pop();
        }
        if let Some(last) = runs.last_mut() {
            let trimmed = last.text.trim_end_matches([' ', '\n']).len();
            last.text.truncate(trimmed);
        }
        while runs
            .first()
            .is_some_and(|r| r.text.trim_start_matches([' ', '\n']).is_empty())
        {
            runs.remove(0);
        }
        if let Some(first) = runs.first_mut() {
            let trimmed = first.text.trim_start_matches([' ', '\n']).to_string();
            first.text = trimmed;
        }
        (!runs.is_empty()).then_some(runs)
    }

    fn push_block(&mut self, block: Block) {
        if self.blocks.len() < BLOCK_LIMIT {
            self.blocks.push(block);
        }
    }

    /// Ends the block being written.
    fn flush(&mut self) {
        if self.in_cell {
            return; // a table cell is closed by its own tag
        }
        let Some(runs) = self.take_runs() else {
            return;
        };
        let block = match self.kind {
            Kind::Heading(level) => Block::Heading(level, runs),
            Kind::Item => Block::Item {
                depth: self.item_depth,
                marker: std::mem::take(&mut self.item_marker),
                runs,
            },
            Kind::Paragraph if self.quote_depth > 0 => Block::Quote(runs),
            Kind::Paragraph => Block::Paragraph(runs),
        };
        self.push_block(block);
        if self.kind == Kind::Item {
            self.kind = Kind::Paragraph;
        }
    }

    fn open(&mut self, tag: &Tag) {
        let name = tag.name.as_str();

        if matches!(
            name,
            "head" | "script" | "style" | "title" | "noscript" | "template" | "svg" | "iframe"
        ) && !tag.self_closing
        {
            self.flush();
            self.skip.push(name.to_string());
            return;
        }
        if !self.skip.is_empty() {
            return;
        }

        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                self.kind = Kind::Heading(name.as_bytes()[1] - b'0');
            }
            "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "main"
            | "aside" | "form" | "figure" | "figcaption" | "address" | "details" | "summary"
            | "fieldset" | "dl" | "dt" | "dd" | "center" | "table" | "thead" | "tbody"
            | "tfoot" => {
                self.flush();
            }
            "ul" | "ol" => {
                self.flush();
                self.lists.push((name == "ol", 1));
            }
            "li" => {
                self.flush();
                self.kind = Kind::Item;
                self.item_depth = self.lists.len().saturating_sub(1);
                self.item_marker = match self.lists.last_mut() {
                    Some((true, n)) => {
                        let marker = format!("{n}.");
                        *n += 1;
                        marker
                    }
                    _ => "•".to_string(),
                };
            }
            "blockquote" => {
                self.flush();
                self.quote_depth += 1;
            }
            "pre" => {
                self.flush();
                self.pre = Some(String::new());
            }
            "hr" => {
                self.flush();
                self.push_block(Block::Rule);
            }
            "br" => self.push_break(),
            "tr" => {
                self.flush();
                self.row = Some(Vec::new());
            }
            "td" | "th" => {
                self.close_cell();
                if self.row.is_none() {
                    self.row = Some(Vec::new());
                }
                self.in_cell = true;
                if name == "th" {
                    self.bold += 1;
                }
            }
            "b" | "strong" => self.bold += 1,
            "i" | "em" | "cite" | "dfn" | "var" => self.italic += 1,
            "code" | "kbd" | "samp" | "tt" => self.code += 1,
            "u" | "ins" => self.underline += 1,
            "s" | "strike" | "del" => self.strike += 1,
            "mark" => self.mark += 1,
            "a" => self.links.push(tag.attr("href").map(str::to_string)),
            "img" => {
                let alt = tag.attr("alt").unwrap_or("").trim();
                let label = if alt.is_empty() {
                    "[image]".to_string()
                } else {
                    format!("[image: {alt}]")
                };
                let mut style = self.style();
                style.italic = true;
                style.text = label;
                self.runs.push(style);
            }
            "input" | "textarea" | "select" | "button" => {
                if name == "input" {
                    let shown = tag
                        .attr("value")
                        .or_else(|| tag.attr("placeholder"))
                        .unwrap_or("");
                    let kind = tag.attr("type").unwrap_or("text");
                    if kind != "hidden" {
                        let mut style = self.style();
                        style.code = true;
                        style.text = format!("[{}]", if shown.is_empty() { kind } else { shown });
                        self.runs.push(style);
                    }
                }
            }
            _ => {}
        }
    }

    fn close(&mut self, name: &str) {
        if let Some(top) = self.skip.last() {
            if top == name {
                self.skip.pop();
            }
            return;
        }

        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                self.kind = Kind::Paragraph;
            }
            "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "main"
            | "aside" | "form" | "figure" | "figcaption" | "address" | "details" | "summary"
            | "fieldset" | "dl" | "dt" | "dd" | "center" | "table" | "thead" | "tbody"
            | "tfoot" | "li" => self.flush(),
            "ul" | "ol" => {
                self.flush();
                self.lists.pop();
            }
            "blockquote" => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            "pre" => {
                if let Some(text) = self.pre.take() {
                    let text = text.trim_matches('\n').to_string();
                    if !text.trim().is_empty() {
                        self.push_block(Block::Pre(text));
                    }
                }
            }
            "tr" => {
                self.close_cell();
                if let Some(cells) = self.row.take() {
                    if cells.iter().any(|c| !c.is_empty()) {
                        self.push_block(Block::Row(cells));
                    }
                }
            }
            "td" | "th" => {
                self.close_cell();
                if name == "th" {
                    self.bold = self.bold.saturating_sub(1);
                }
            }
            "b" | "strong" => self.bold = self.bold.saturating_sub(1),
            "i" | "em" | "cite" | "dfn" | "var" => self.italic = self.italic.saturating_sub(1),
            "code" | "kbd" | "samp" | "tt" => self.code = self.code.saturating_sub(1),
            "u" | "ins" => self.underline = self.underline.saturating_sub(1),
            "s" | "strike" | "del" => self.strike = self.strike.saturating_sub(1),
            "mark" => self.mark = self.mark.saturating_sub(1),
            "a" => {
                self.links.pop();
            }
            _ => {}
        }
    }

    fn close_cell(&mut self) {
        if !self.in_cell {
            return;
        }
        self.in_cell = false;
        let runs = self.take_runs().unwrap_or_default();
        if let Some(row) = &mut self.row {
            row.push(runs);
        }
    }
}

fn same_style(a: &Run, b: &Run) -> bool {
    a.bold == b.bold
        && a.italic == b.italic
        && a.code == b.code
        && a.underline == b.underline
        && a.strike == b.strike
        && a.mark == b.mark
        && a.link == b.link
}

// ───────────────────────────── tokenizer ─────────────────────────────

struct Tag {
    name: String,
    closing: bool,
    self_closing: bool,
    attrs: Vec<(String, String)>,
}

impl Tag {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

const VOID: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Parses the text between `<` and `>`.
fn parse_tag(inner: &str) -> Option<Tag> {
    let mut chars = inner.trim().trim_end_matches('/').trim_end().chars().peekable();
    let closing = chars.peek() == Some(&'/');
    if closing {
        chars.next();
    }

    let mut name = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            break;
        }
        name.push(c.to_ascii_lowercase());
        chars.next();
    }
    if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }

    let mut attrs = Vec::new();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() {
                break;
            }
            key.push(c.to_ascii_lowercase());
            chars.next();
        }
        if key.is_empty() {
            break;
        }
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let mut value = String::new();
        if chars.peek() == Some(&'=') {
            chars.next();
            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
            match chars.peek().copied() {
                Some(q @ ('"' | '\'')) => {
                    chars.next();
                    for c in chars.by_ref() {
                        if c == q {
                            break;
                        }
                        value.push(c);
                    }
                }
                _ => {
                    while let Some(&c) = chars.peek() {
                        if c.is_whitespace() {
                            break;
                        }
                        value.push(c);
                        chars.next();
                    }
                }
            }
        }
        attrs.push((key, decode_entities(&value)));
    }

    let self_closing = VOID.contains(&name.as_str()) || inner.trim_end().ends_with('/');
    Some(Tag {
        name,
        closing,
        self_closing,
        attrs,
    })
}

/// Index of the `>` that ends a tag starting at `start` (quotes may hide a `>`).
fn tag_end(html: &str, start: usize) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in html[start..].char_indices() {
        match (quote, c) {
            (None, '"') | (None, '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(start + i),
            _ => {}
        }
    }
    None
}

pub fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        let end = after
            .char_indices()
            .take(12)
            .find(|(_, c)| *c == ';')
            .map(|(i, _)| i);
        let decoded = end.and_then(|end| {
            let entity = &after[..end];
            let value = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                "copy" => Some('©'),
                "reg" => Some('®'),
                "trade" => Some('™'),
                "hellip" => Some('…'),
                "mdash" => Some('—'),
                "ndash" => Some('–'),
                "laquo" => Some('«'),
                "raquo" => Some('»'),
                "bull" => Some('•'),
                "middot" => Some('·'),
                "euro" => Some('€'),
                "lsquo" => Some('‘'),
                "rsquo" => Some('’'),
                "ldquo" => Some('“'),
                "rdquo" => Some('”'),
                _ => {
                    let number = entity.strip_prefix('#')?;
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => number.parse().ok()?,
                    };
                    char::from_u32(code)
                }
            }?;
            Some((value, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Turns an HTML page into blocks that can be drawn.
pub fn parse(html: &str) -> Vec<Block> {
    let html = if html.len() > INPUT_LIMIT {
        let mut cut = INPUT_LIMIT;
        while !html.is_char_boundary(cut) {
            cut -= 1;
        }
        &html[..cut]
    } else {
        html
    };

    let mut builder = Builder::new();
    let mut rest = 0;

    while rest < html.len() && builder.blocks.len() < BLOCK_LIMIT {
        let Some(offset) = html[rest..].find('<') else {
            let text = decode_entities(&html[rest..]);
            if builder.skip.is_empty() {
                builder.push_text(&text);
            }
            break;
        };
        let lt = rest + offset;
        if lt > rest && builder.skip.is_empty() {
            builder.push_text(&decode_entities(&html[rest..lt]));
        }

        let tail = &html[lt..];
        if tail.starts_with("<!--") {
            rest = tail.find("-->").map_or(html.len(), |i| lt + i + 3);
            continue;
        }
        let Some(end) = tag_end(html, lt + 1) else {
            break;
        };
        let inner = &html[lt + 1..end];
        rest = end + 1;

        if inner.starts_with('!') || inner.starts_with('?') {
            continue;
        }
        let Some(tag) = parse_tag(inner) else {
            // A stray "<": keep it as text.
            if builder.skip.is_empty() {
                builder.push_text("<");
                rest = lt + 1;
            }
            continue;
        };

        // Raw-text elements hold no tags: jump to their end.
        if !tag.closing
            && !tag.self_closing
            && matches!(tag.name.as_str(), "script" | "style")
        {
            let close = format!("</{}", tag.name);
            let lower = html[rest..].to_ascii_lowercase();
            rest = lower.find(&close).map_or(html.len(), |i| rest + i);
            continue;
        }

        if tag.closing {
            builder.close(&tag.name);
        } else {
            builder.open(&tag);
            if tag.self_closing && !VOID.contains(&tag.name.as_str()) {
                builder.close(&tag.name);
            }
        }
    }

    builder.close_cell();
    builder.flush();
    builder.blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(runs: &[Run]) -> String {
        runs.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn headings_paragraphs_and_inline_styles() {
        let blocks = parse(
            "<html><head><title>T</title><style>p{}</style></head><body>\
             <h1>Hello &amp; welcome</h1>\
             <p>Some <b>bold</b> and <a href=\"/x?a=1&amp;b=2\">a link</a>.</p></body></html>",
        );
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            Block::Heading(1, runs) => {
                assert_eq!(plain(runs), "Hello & welcome");
                assert!(runs[0].bold);
            }
            other => panic!("{other:?}"),
        }
        match &blocks[1] {
            Block::Paragraph(runs) => {
                assert_eq!(plain(runs), "Some bold and a link.");
                assert!(runs.iter().any(|r| r.bold && r.text == "bold"));
                let link = runs.iter().find(|r| r.link.is_some()).unwrap();
                assert_eq!(link.link.as_deref(), Some("/x?a=1&b=2"));
                assert!(link.underline);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn lists_are_numbered_and_nested() {
        let blocks = parse("<ul><li>one</li><li>two<ol><li>x</li><li>y</li></ol></li></ul>");
        let items: Vec<(usize, String, String)> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Item {
                    depth,
                    marker,
                    runs,
                } => Some((*depth, marker.clone(), plain(runs))),
                _ => None,
            })
            .collect();
        assert_eq!(
            items,
            vec![
                (0, "•".into(), "one".into()),
                (0, "•".into(), "two".into()),
                (1, "1.".into(), "x".into()),
                (1, "2.".into(), "y".into()),
            ]
        );
    }

    #[test]
    fn scripts_comments_and_pre() {
        let blocks = parse(
            "<!-- hidden --><script>if (a < b) { x(); }</script><p>shown</p>\
             <pre>line 1\n  line 2</pre><hr>",
        );
        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[0], Block::Paragraph(r) if plain(r) == "shown"));
        assert_eq!(blocks[1], Block::Pre("line 1\n  line 2".into()));
        assert_eq!(blocks[2], Block::Rule);
    }

    #[test]
    fn tables_become_rows() {
        let blocks = parse("<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>");
        assert_eq!(blocks.len(), 2);
        match &blocks[1] {
            Block::Row(cells) => {
                assert_eq!(cells.len(), 2);
                assert_eq!(plain(&cells[1]), "2");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn entities_and_broken_markup() {
        assert_eq!(decode_entities("a &lt; b &#65; &#x42; &unknown; &"), "a < b A B &unknown; &");
        let blocks = parse("<p>1 < 2 and <b>open");
        assert!(matches!(&blocks[0], Block::Paragraph(r) if plain(r).starts_with("1 < 2 and")));
        assert!(parse("").is_empty());
    }
}
