//! IR Pane Widget
//!
//! Displays IR information in different views:
//! - Stack Art: ASCII art stack effect diagrams
//! - Typed AST: Full AST with type annotations
//! - LLVM IR: Generated LLVM IR snippets

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

/// The different IR view modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IrViewMode {
    /// ASCII art stack effect diagrams
    #[default]
    StackArt,
    /// Full typed AST
    TypedAst,
    /// LLVM IR snippets
    LlvmIr,
}

impl IrViewMode {
    /// Get the next view mode (for cycling with arrow keys)
    pub fn next(self) -> Self {
        match self {
            Self::StackArt => Self::TypedAst,
            Self::TypedAst => Self::LlvmIr,
            Self::LlvmIr => Self::StackArt,
        }
    }

    /// Get the previous view mode
    pub fn prev(self) -> Self {
        match self {
            Self::StackArt => Self::LlvmIr,
            Self::TypedAst => Self::StackArt,
            Self::LlvmIr => Self::TypedAst,
        }
    }

    /// Get the display name for this mode
    pub fn name(&self) -> &'static str {
        match self {
            Self::StackArt => "Stack Effects",
            Self::TypedAst => "Typed AST",
            Self::LlvmIr => "LLVM IR",
        }
    }
}

/// Content to display in the IR pane
#[derive(Debug, Clone, Default)]
pub struct IrContent {
    /// Stack art lines (rendered ASCII art)
    pub stack_art: Vec<String>,
    /// Typed AST representation
    pub typed_ast: Vec<String>,
    /// LLVM IR snippet
    pub llvm_ir: Vec<String>,
    /// Any error messages
    pub errors: Vec<String>,
}

impl IrContent {
    /// Create empty IR content
    pub fn new() -> Self {
        Self::default()
    }

    /// Create content with an error message
    pub fn with_error(error: impl Into<String>) -> Self {
        Self {
            errors: vec![error.into()],
            ..Default::default()
        }
    }

    /// Get the content for the given view mode
    pub fn content_for(&self, mode: IrViewMode) -> &[String] {
        match mode {
            IrViewMode::StackArt => &self.stack_art,
            IrViewMode::TypedAst => &self.typed_ast,
            IrViewMode::LlvmIr => &self.llvm_ir,
        }
    }

    /// Check if there are errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// The IR pane widget
pub struct IrPane<'a> {
    /// Current view mode
    mode: IrViewMode,
    /// Content to display
    content: &'a IrContent,
    /// Scroll offset
    scroll: u16,
}

impl<'a> IrPane<'a> {
    /// Create a new IR pane
    pub fn new(content: &'a IrContent) -> Self {
        Self {
            mode: IrViewMode::default(),
            content,
            scroll: 0,
        }
    }

    /// Set the view mode
    pub fn mode(mut self, mode: IrViewMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set the scroll offset
    pub fn scroll(mut self, scroll: u16) -> Self {
        self.scroll = scroll;
        self
    }

    /// Apply syntax highlighting to content based on view mode
    fn style_content(&self, lines: &[String]) -> Vec<Line<'a>> {
        match self.mode {
            IrViewMode::StackArt => self.style_stack_art(lines),
            IrViewMode::TypedAst => self.style_ast(lines),
            IrViewMode::LlvmIr => self.style_llvm(lines),
        }
    }

    /// Style stack art content
    fn style_stack_art(&self, lines: &[String]) -> Vec<Line<'a>> {
        lines
            .iter()
            .map(|line| {
                let mut spans = Vec::new();
                let chars: Vec<char> = line.chars().collect();
                let mut i = 0;

                while i < chars.len() {
                    let ch = chars[i];
                    // Box drawing characters in cyan
                    if "┌┐└┘├┤─│".contains(ch) {
                        spans.push(Span::styled(
                            ch.to_string(),
                            Style::default().fg(Color::Cyan),
                        ));
                        i += 1;
                    }
                    // Arrow in yellow
                    else if ch == '→' {
                        spans.push(Span::styled(
                            "→",
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ));
                        i += 1;
                    }
                    // Type names (capitalized words) in green
                    else if ch.is_uppercase() {
                        let start = i;
                        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                            i += 1;
                        }
                        let word: String = chars[start..i].iter().collect();
                        spans.push(Span::styled(word, Style::default().fg(Color::Green)));
                    }
                    // Rest variables (..a) in magenta
                    else if ch == '.' && i + 1 < chars.len() && chars[i + 1] == '.' {
                        let start = i;
                        i += 2; // Skip ..
                        while i < chars.len() && chars[i].is_alphanumeric() {
                            i += 1;
                        }
                        let word: String = chars[start..i].iter().collect();
                        spans.push(Span::styled(word, Style::default().fg(Color::Magenta)));
                    }
                    // Default
                    else {
                        spans.push(Span::raw(ch.to_string()));
                        i += 1;
                    }
                }

                Line::from(spans)
            })
            .collect()
    }

    /// Style AST content
    fn style_ast(&self, lines: &[String]) -> Vec<Line<'a>> {
        lines
            .iter()
            .map(|line| {
                // Simple styling: keywords in blue, types in green
                Line::from(Span::styled(
                    line.clone(),
                    Style::default().fg(Color::White),
                ))
            })
            .collect()
    }

    /// Style LLVM IR content with syntax highlighting
    fn style_llvm(&self, lines: &[String]) -> Vec<Line<'a>> {
        lines
            .iter()
            .map(|line| {
                let trimmed = line.trim_start();

                // Comments in dark gray (whole line)
                if trimmed.starts_with(';') {
                    return Line::from(Span::styled(
                        line.clone(),
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                // Labels in cyan (whole line)
                if trimmed.ends_with(':') && !trimmed.contains(' ') {
                    return Line::from(Span::styled(
                        line.clone(),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                // For other lines, do token-based highlighting
                Line::from(self.tokenize_llvm_line(line))
            })
            .collect()
    }

    /// Tokenize and style a single LLVM IR line
    fn tokenize_llvm_line(&self, line: &str) -> Vec<Span<'a>> {
        let mut spans = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let ch = chars[i];

            // Preserve leading whitespace
            if ch.is_whitespace() {
                let start = i;
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
                spans.push(Span::raw(chars[start..i].iter().collect::<String>()));
                continue;
            }

            // % registers/variables in magenta
            if ch == '%' {
                let start = i;
                i += 1;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    i += 1;
                }
                spans.push(Span::styled(
                    chars[start..i].iter().collect::<String>(),
                    Style::default().fg(Color::Magenta),
                ));
                continue;
            }

            // @ function names in yellow
            if ch == '@' {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                spans.push(Span::styled(
                    chars[start..i].iter().collect::<String>(),
                    Style::default().fg(Color::Yellow),
                ));
                continue;
            }

            // Numbers in blue
            if ch.is_ascii_digit()
                || (ch == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
            {
                let start = i;
                if ch == '-' {
                    i += 1;
                }
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                spans.push(Span::styled(
                    chars[start..i].iter().collect::<String>(),
                    Style::default().fg(Color::Blue),
                ));
                continue;
            }

            // Identifiers (keywords, types, instructions)
            if ch.is_alphabetic() || ch == '_' {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let style = self.llvm_word_style(&word);
                spans.push(Span::styled(word, style));
                continue;
            }

            // Operators and punctuation in default color
            spans.push(Span::raw(ch.to_string()));
            i += 1;
        }

        spans
    }

    /// Get style for an LLVM IR word
    fn llvm_word_style(&self, word: &str) -> Style {
        // Keywords (calling conventions, linkage, etc.)
        const KEYWORDS: &[&str] = &[
            "define", "declare", "tailcc", "fastcc", "ccc", "private", "internal", "external",
            "global", "constant", "align", "to", "null", "true", "false", "undef", "nuw", "nsw",
            "exact", "inbounds",
        ];

        // Instructions
        const INSTRUCTIONS: &[&str] = &[
            "ret",
            "br",
            "switch",
            "invoke",
            "resume",
            "unreachable",
            "add",
            "sub",
            "mul",
            "udiv",
            "sdiv",
            "urem",
            "srem",
            "and",
            "or",
            "xor",
            "shl",
            "lshr",
            "ashr",
            "fadd",
            "fsub",
            "fmul",
            "fdiv",
            "frem",
            "alloca",
            "load",
            "store",
            "getelementptr",
            "fence",
            "cmpxchg",
            "atomicrmw",
            "trunc",
            "zext",
            "sext",
            "fptrunc",
            "fpext",
            "fptoui",
            "fptosi",
            "uitofp",
            "sitofp",
            "ptrtoint",
            "inttoptr",
            "bitcast",
            "addrspacecast",
            "icmp",
            "fcmp",
            "phi",
            "select",
            "call",
            "va_arg",
            "extractelement",
            "insertelement",
            "shufflevector",
            "extractvalue",
            "insertvalue",
        ];

        // Type names
        const TYPES: &[&str] = &[
            "void", "i1", "i8", "i16", "i32", "i64", "i128", "half", "float", "double", "fp128",
            "ptr", "label", "metadata", "type",
        ];

        if KEYWORDS.contains(&word) {
            Style::default().fg(Color::Yellow)
        } else if INSTRUCTIONS.contains(&word) {
            Style::default().fg(Color::Green)
        } else if TYPES.contains(&word)
            || word.starts_with('i') && word[1..].chars().all(|c| c.is_ascii_digit())
        {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        }
    }
}

impl Widget for &IrPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Create the border with title showing current mode
        let title = format!(" {} ", self.mode.name());

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        block.render(area, buf);

        // Get content and adapt to available width
        let available_width = inner.width as usize;
        let lines = self.width_adapted_lines(available_width);

        let paragraph = Paragraph::new(lines)
            .scroll((self.scroll, 0))
            .wrap(Wrap { trim: false });

        paragraph.render(inner, buf);
    }
}

impl<'a> IrPane<'a> {
    /// Get lines adapted to available width
    fn width_adapted_lines(&self, available_width: usize) -> Vec<Line<'a>> {
        if self.content.has_errors() {
            return self
                .content
                .errors
                .iter()
                .map(|e| Line::from(Span::styled(e.clone(), Style::default().fg(Color::Red))))
                .collect();
        }

        let lines = self.content.content_for(self.mode);
        if lines.is_empty() {
            return vec![Line::from(Span::styled(
                format!("No {} available", self.mode.name().to_lowercase()),
                Style::default().fg(Color::DarkGray),
            ))];
        }

        // Check if content is too wide
        let max_line_width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);

        if max_line_width <= available_width {
            // Content fits - use normal styled rendering
            self.style_content(lines)
        } else {
            // Content too wide - use compact version
            self.compact_content(lines, available_width)
        }
    }

    /// Generate compact content for narrow windows
    fn compact_content(&self, lines: &[String], available_width: usize) -> Vec<Line<'a>> {
        lines
            .iter()
            .filter_map(|line| {
                // Skip decorative box lines (help header box)
                if line.contains('╭')
                    || line.contains('╮')
                    || line.contains('╰')
                    || line.contains('╯')
                {
                    return None;
                }
                // Convert box header content to plain text
                if line.starts_with('│') && line.ends_with('│') {
                    let inner = line.trim_start_matches('│').trim_end_matches('│').trim();
                    if !inner.is_empty() {
                        return Some(Line::from(Span::styled(
                            inner.to_string(),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        )));
                    }
                    return None;
                }

                // Skip ASCII art stack boxes if too wide
                let has_box_chars = line.contains('┌')
                    || line.contains('┐')
                    || line.contains('└')
                    || line.contains('┘')
                    || line.contains('├')
                    || line.contains('┤');

                if has_box_chars && line.chars().count() > available_width {
                    // For stack art, extract just the effect signature
                    if line.contains('(') && line.contains(')') {
                        // This is likely a signature line like "swap ( ..a x y -- ..a y x )"
                        return Some(Line::from(Span::styled(
                            line.clone(),
                            Style::default().fg(Color::Yellow),
                        )));
                    }
                    return None;
                }

                // Truncate other long lines
                let display = if line.chars().count() > available_width {
                    let truncated: String = line
                        .chars()
                        .take(available_width.saturating_sub(1))
                        .collect();
                    format!("{}…", truncated)
                } else {
                    line.clone()
                };

                Some(Line::from(Span::styled(
                    display,
                    Style::default().fg(Color::White),
                )))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_mode_cycling() {
        let mode = IrViewMode::StackArt;
        assert_eq!(mode.next(), IrViewMode::TypedAst);
        assert_eq!(mode.next().next(), IrViewMode::LlvmIr);
        assert_eq!(mode.next().next().next(), IrViewMode::StackArt);

        assert_eq!(mode.prev(), IrViewMode::LlvmIr);
    }

    #[test]
    fn test_view_mode_names() {
        assert_eq!(IrViewMode::StackArt.name(), "Stack Effects");
        assert_eq!(IrViewMode::TypedAst.name(), "Typed AST");
        assert_eq!(IrViewMode::LlvmIr.name(), "LLVM IR");
    }

    #[test]
    fn test_ir_content_empty() {
        let content = IrContent::new();
        assert!(!content.has_errors());
        assert!(content.content_for(IrViewMode::StackArt).is_empty());
    }

    #[test]
    fn test_ir_content_with_error() {
        let content = IrContent::with_error("test error");
        assert!(content.has_errors());
        assert_eq!(content.errors[0], "test error");
    }

    #[test]
    fn test_ir_pane_creation() {
        let content = IrContent::new();
        let pane = IrPane::new(&content).mode(IrViewMode::LlvmIr);
        assert_eq!(pane.mode, IrViewMode::LlvmIr);
    }

    #[test]
    fn test_ir_pane_render() -> Result<(), String> {
        let content = IrContent {
            stack_art: vec![
                "┌───┐".to_string(),
                "│ 5 │".to_string(),
                "└───┘".to_string(),
            ],
            ..Default::default()
        };

        let pane = IrPane::new(&content);

        // Create a buffer and render
        let area = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(area);
        (&pane).render(area, &mut buf);

        // Verify the title is rendered
        let title_cell = buf.cell((1, 0)).ok_or("cell (1,0) should exist")?;
        assert!(title_cell.symbol().chars().next().is_some());
        Ok(())
    }
}
