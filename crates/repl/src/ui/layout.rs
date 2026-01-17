//! Layout Manager
//!
//! Manages the split-pane layout for the TUI REPL.
//! Provides horizontal split between REPL (left) and IR (right) panes.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Layout configuration
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Percentage of width for the REPL pane (0-100)
    pub repl_width_percent: u16,
    /// Minimum width for each pane
    pub min_pane_width: u16,
    /// Height reserved for status bar
    pub status_bar_height: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            repl_width_percent: 50,
            min_pane_width: 20,
            status_bar_height: 1,
        }
    }
}

impl LayoutConfig {
    /// Create a new layout config
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the REPL width percentage
    #[allow(dead_code)]
    pub fn repl_width(mut self, percent: u16) -> Self {
        self.repl_width_percent = percent.clamp(10, 90);
        self
    }

    /// Set the status bar height (for future use)
    #[allow(dead_code)]
    pub fn status_bar(mut self, height: u16) -> Self {
        self.status_bar_height = height;
        self
    }
}

/// The computed layout areas
#[derive(Debug, Clone, Copy)]
pub struct ComputedLayout {
    /// Area for the REPL pane
    pub repl: Rect,
    /// Area for the IR pane
    pub ir: Rect,
    /// Area for the status bar
    pub status: Rect,
}

impl ComputedLayout {
    /// Compute the layout for a given terminal area
    pub fn compute(area: Rect, config: &LayoutConfig, show_ir: bool) -> Self {
        // First split: main content vs status bar
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(config.status_bar_height),
            ])
            .split(area);

        let main_area = vertical_chunks[0];
        let status_area = vertical_chunks[1];

        // If IR pane is disabled, give all space to REPL
        if !show_ir {
            return Self {
                repl: main_area,
                ir: Rect::default(),
                status: status_area,
            };
        }

        // Check if we have enough width for split view
        let total_width = main_area.width;
        let min_split_width = config.min_pane_width * 2;

        let (repl_area, ir_area) = if total_width >= min_split_width {
            // Horizontal split for REPL and IR panes
            let horizontal_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(config.repl_width_percent),
                    Constraint::Percentage(100 - config.repl_width_percent),
                ])
                .split(main_area);

            (horizontal_chunks[0], horizontal_chunks[1])
        } else {
            // Too narrow - show only REPL pane
            (main_area, Rect::default())
        };

        Self {
            repl: repl_area,
            ir: ir_area,
            status: status_area,
        }
    }

    /// Check if IR pane is visible
    pub fn ir_visible(&self) -> bool {
        self.ir.width > 0 && self.ir.height > 0
    }
}

/// Status bar content
#[derive(Debug, Clone, Default)]
pub struct StatusContent {
    /// Current filename (or temp indicator)
    pub filename: String,
    /// Current mode (Normal/Insert for Vi mode)
    pub mode: String,
    /// Current IR view name
    pub ir_view: String,
    /// Any additional status message
    pub message: Option<String>,
}

impl StatusContent {
    /// Create a new status content
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the filename
    pub fn filename(mut self, name: impl Into<String>) -> Self {
        self.filename = name.into();
        self
    }

    /// Set the mode
    pub fn mode(mut self, mode: impl Into<String>) -> Self {
        self.mode = mode.into();
        self
    }

    /// Set the IR view name
    pub fn ir_view(mut self, view: impl Into<String>) -> Self {
        self.ir_view = view.into();
        self
    }

    /// Set a status message (for future use)
    #[allow(dead_code)]
    pub fn message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }

    /// Format for display
    pub fn format(&self, width: u16) -> String {
        let left = format!(" {} ", self.filename);
        let middle = if let Some(msg) = &self.message {
            msg.clone()
        } else {
            String::new()
        };
        let right = format!(" {} | {} ", self.mode, self.ir_view);

        let padding_needed = (width as usize)
            .saturating_sub(left.len())
            .saturating_sub(middle.len())
            .saturating_sub(right.len());

        let left_pad = padding_needed / 2;
        let right_pad = padding_needed - left_pad;

        format!(
            "{}{}{}{}{}",
            left,
            " ".repeat(left_pad),
            middle,
            " ".repeat(right_pad),
            right
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_config_defaults() {
        let config = LayoutConfig::default();
        assert_eq!(config.repl_width_percent, 50);
        assert_eq!(config.min_pane_width, 20);
        assert_eq!(config.status_bar_height, 1);
    }

    #[test]
    fn test_layout_config_bounds() {
        let config = LayoutConfig::new().repl_width(5);
        assert_eq!(config.repl_width_percent, 10); // Clamped to min

        let config = LayoutConfig::new().repl_width(95);
        assert_eq!(config.repl_width_percent, 90); // Clamped to max
    }

    #[test]
    fn test_computed_layout() {
        let area = Rect::new(0, 0, 100, 30);
        let config = LayoutConfig::default();
        let layout = ComputedLayout::compute(area, &config, true);

        // Status bar at bottom
        assert_eq!(layout.status.height, 1);
        assert_eq!(layout.status.y, 29);

        // Main panes fill the rest
        assert!(layout.repl.width > 0);
        assert!(layout.ir.width > 0);
        assert!(layout.ir_visible());
    }

    #[test]
    fn test_narrow_layout() {
        let area = Rect::new(0, 0, 30, 30); // Too narrow for split
        let config = LayoutConfig::default();
        let layout = ComputedLayout::compute(area, &config, true);

        // IR pane should be hidden (too narrow)
        assert!(!layout.ir_visible());
        assert_eq!(layout.repl.width, 30);
    }

    #[test]
    fn test_ir_hidden_layout() {
        let area = Rect::new(0, 0, 100, 30);
        let config = LayoutConfig::default();
        let layout = ComputedLayout::compute(area, &config, false);

        // IR pane should be hidden (disabled)
        assert!(!layout.ir_visible());
        // REPL gets full width
        assert_eq!(layout.repl.width, 100);
    }

    #[test]
    fn test_status_content_format() {
        let status = StatusContent::new()
            .filename("test.seq")
            .mode("Normal")
            .ir_view("Stack Effects");

        let formatted = status.format(80);
        assert!(formatted.contains("test.seq"));
        assert!(formatted.contains("Normal"));
        assert!(formatted.contains("Stack Effects"));
    }
}
