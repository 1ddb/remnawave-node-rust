pub const DEFAULT_WIDTH: usize = 62;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Center,
    Left,
}

pub struct BoxOptions {
    pub align: Align,
    pub width: usize,
}

impl Default for BoxOptions {
    fn default() -> Self {
        Self { align: Align::Center, width: DEFAULT_WIDTH }
    }
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for raw_line in text.split('\n') {
        let mut current = String::new();
        for word in raw_line.split(' ') {
            if !current.is_empty() && current.len() + 1 + word.len() > width {
                lines.push(current.clone());
                current.clear();
            }
            if current.is_empty() {
                current.push_str(word);
            } else {
                current.push(' ');
                current.push_str(word);
            }
            while current.len() > width {
                lines.push(current.chars().take(width).collect());
                current = current.chars().skip(width).collect();
            }
        }
        lines.push(current);
    }
    lines
}

fn pad(text: &str, width: usize) -> String {
    let mut out = text.to_string();
    let len = out.chars().count();
    if len < width {
        out.push_str(&" ".repeat(width - len));
    }
    out
}

fn center(text: &str, width: usize) -> String {
    let len = text.chars().count();
    let left = width.saturating_sub(len) / 2;
    pad(&format!("{}{}", " ".repeat(left), text), width)
}

pub fn render_box(title: &str, sections: &[String], options: BoxOptions) -> String {
    let width = options.width;
    let inner = width.saturating_sub(4);
    let horizontal = "─".repeat(width.saturating_sub(2));
    let mut out = Vec::new();

    out.push(format!("┌{horizontal}┐"));
    out.push(format!("│ {} │", center(title, inner)));

    for section in sections {
        out.push(format!("├{horizontal}┤"));
        for text in wrap(section, inner) {
            let rendered = match options.align {
                Align::Center => center(&text, inner),
                Align::Left => pad(&text, inner),
            };
            out.push(format!("│ {rendered} │"));
        }
    }

    out.push(format!("└{horizontal}┘"));
    out.join("\n")
}
