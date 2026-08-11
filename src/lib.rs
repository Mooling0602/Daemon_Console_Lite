//! # daemon_console_lite
//!
//! A lightweight and flexible console for daemon applications providing a terminal interface
//! with history navigation and colored logging.
//!
//! # Examples
//!
//! A simple way to create a `TerminalApp` instance.
//!
//! ```rust
//! use daemon_console_lite::TerminalApp;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut app = TerminalApp::new();
//!     Ok(())
//! }
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod logger;
pub mod tab;
pub mod utils;

use crossterm::{
    cursor::{self, RestorePosition, SavePosition},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, poll,
    },
    execute,
    style::{Color, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size},
};
use std::io::{Stdout, Write, stdout};
use std::time::Instant;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::logger::LogLevel;
use crate::tab::{CompletionCandidate, TabTree};

/// Main terminal application structure managing state and input/output.
///
/// `TerminalApp` provides a complete terminal interface with:
/// - Command history navigation
/// - Cursor management
/// - Colored logging support
/// - Non-blocking input handling
/// - Tab completion support
pub struct TerminalApp {
    /// Handle to stdout for terminal operations
    pub stdout_handle: Stdout,
    /// Command history for up/down navigation
    pub command_history: Vec<String>,
    /// Current input buffer
    pub current_input: String,
    /// Index in command history (None = not browsing history)
    pub history_index: Option<usize>,
    /// Timestamp of last Ctrl+C press for double-tap detection
    pub last_ctrl_c: Option<Instant>,
    /// A flag used to exit the application.
    pub should_exit: bool,
    /// Application name, could be set to any valid text your like.
    pub app_name: String,
    /// Whether raw mode is enabled
    pub raw_mode_enabled: bool,

    /// Maximum number of tab completion nodes allowed
    tab_completion_limit: usize,
    /// Current cursor position in the input line
    cursor_position: usize,
    /// Temporary storage for current input when browsing history
    pending_input: Option<String>,
    /// Cursor position for pending input
    pending_cursor_position: usize,
    /// Whether completions are currently hidden
    completions_hidden: bool,
    /// Whether focus is currently on completions (true) or text input (false)
    focus_on_completions: bool,
    last_key_event: Option<KeyEvent>,
    tab_tree: Option<TabTree>,
    current_completions: Vec<CompletionCandidate>,
    hints_rendered: bool,
    selected_completion_index: usize,
    warned_no_tab_tree: bool,
}

impl Default for TerminalApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to calculate width of a slice of chars
fn width_of_chars(chars: &[char]) -> usize {
    chars.iter().map(|c| c.width().unwrap_or(0)).sum()
}

impl TerminalApp {
    /// Removes a character at a specific index in a string.
    fn remove_char_at(&mut self, index: usize) {
        let mut chars: Vec<char> = self.current_input.chars().collect();
        if index < chars.len() {
            chars.remove(index);
            self.current_input = chars.into_iter().collect();
        }
    }

    /// Creates a new terminal application instance with default settings.
    ///
    /// Some attributes are allowed to be modified later, like `app_name`.
    pub fn new() -> Self {
        Self {
            stdout_handle: stdout(),
            command_history: Vec::new(),
            current_input: String::new(),
            history_index: None,
            last_ctrl_c: None,
            should_exit: false,
            app_name: String::from("Daemon Console"),
            raw_mode_enabled: false,

            tab_completion_limit: 10000,
            cursor_position: 0,
            pending_input: None,
            pending_cursor_position: 0,
            completions_hidden: false,
            focus_on_completions: false,
            last_key_event: None,
            tab_tree: None,
            current_completions: Vec::new(),
            hints_rendered: false,
            selected_completion_index: 0,
            warned_no_tab_tree: false,
        }
    }

    /// Enables tab completion and initializes the completion tree.
    pub fn enable_tab_completion(&mut self) {
        if self.tab_tree.is_none() {
            self.tab_tree = Some(TabTree::new());
        } else {
            self.logger(LogLevel::Warn, "Tab completion is already enabled.", None);
        }
    }

    /// Checks if tab completion is currently enabled.
    pub fn is_tab_completion_enabled(&self) -> bool {
        self.tab_tree.is_some()
    }

    /// Registers completions for a given context.
    ///
    /// # Arguments
    ///
    /// * `context` - The input prefix that triggers these completions (empty string for root)
    /// * `completions` - List of completion texts
    ///
    /// # Examples
    ///
    /// ```
    /// use daemon_console_lite::TerminalApp;
    ///
    /// let mut app = TerminalApp::new();
    /// app.enable_tab_completion();
    /// app.register_tab_completions("!config", &["start", "stop", "restart"]);
    /// ```
    pub fn register_tab_completions(&mut self, context: &str, completions: &[&str]) {
        if let Some(tree) = &mut self.tab_tree {
            // Check if adding these completions would exceed the limit
            let current_count = tree.count_total_items();
            if current_count + completions.len() > self.tab_completion_limit {
                self.logger(
                    LogLevel::Warn,
                    &format!(
                        "Cannot register {} completions: would exceed limit of {}. Current count: {}",
                        completions.len(),
                        self.tab_completion_limit,
                        current_count
                    ),
                    None,
                );
                return;
            }
            tree.register_completions(context, completions);
        } else if !self.warned_no_tab_tree {
            self.logger(
                LogLevel::Warn,
                "Tab completion is not enabled. Call enable_tab_completion() first.",
                None,
            );
            self.warned_no_tab_tree = true;
        }
    }

    /// Registers completions with descriptions.
    ///
    /// # Arguments
    ///
    /// * `context` - The input prefix that triggers these completions
    /// * `items` - List of (text, description) tuples
    pub fn register_tab_completions_with_desc(&mut self, context: &str, items: &[(&str, &str)]) {
        if let Some(tree) = &mut self.tab_tree {
            // Check if adding these completions would exceed the limit
            let current_count = tree.count_total_items();
            if current_count + items.len() > self.tab_completion_limit {
                self.logger(
                    LogLevel::Warn,
                    &format!(
                        "Cannot register {} completions: would exceed limit of {}. Current count: {}",
                        items.len(),
                        self.tab_completion_limit,
                        current_count
                    ),
                    None,
                );
                return;
            }

            let duplicates = tree.register_completions_with_desc(context, items);
            // Warn about duplicate items
            for dup in duplicates {
                self.logger(
                    LogLevel::Warn,
                    &format!(
                        "Duplicate completion item '{}' ignored in context '{}'",
                        dup.text,
                        if context.is_empty() {
                            "<root>"
                        } else {
                            context
                        }
                    ),
                    None,
                );
            }
        } else if !self.warned_no_tab_tree {
            self.logger(
                LogLevel::Warn,
                "Tab completion is not enabled. Call enable_tab_completion() first.",
                None,
            );
            self.warned_no_tab_tree = true;
        }
    }

    /// Adds a single completion item to an existing context.
    ///
    /// # Arguments
    ///
    /// * `context` - The context to add to
    /// * `text` - Completion text
    /// * `description` - Optional description
    pub fn add_tab_completion(&mut self, context: &str, text: &str, description: Option<&str>) {
        if let Some(tree) = &mut self.tab_tree {
            // Check if adding this completion would exceed the limit
            let current_count = tree.count_total_items();
            if current_count + 1 > self.tab_completion_limit {
                self.logger(
                    LogLevel::Warn,
                    &format!(
                        "Cannot add completion: would exceed limit of {}. Current count: {}",
                        self.tab_completion_limit, current_count
                    ),
                    None,
                );
                return;
            }
            tree.add_completion(context, text, description);
        }
    }

    /// Initializes the terminal with raw mode and displays startup messages.
    ///
    /// # Arguments
    ///
    /// * `startup_message` - Message to display on startup
    ///
    /// # Errors
    ///
    /// Returns an error if terminal initialization fails.
    pub async fn init_terminal(
        &mut self,
        startup_message: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.setup_terminal()?;

        if !startup_message.is_empty() {
            self.print_startup_message(startup_message).await?;
        }

        enable_raw_mode()?;

        Ok(())
    }

    /// Sets up the terminal in raw mode and enables mouse capture
    ///
    /// Raw mode is disabled by default to allow text selection.
    /// Later, raw mode will be enabled in `init_terminal()`, make sure key events can be handled fine.
    /// Due to some unknown reason, in this case, text selection still works.
    /// If you want to disallow text selection, set `app.raw_mode_enabled` to `true`.
    fn setup_terminal(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.raw_mode_enabled {
            enable_raw_mode()?;
            execute!(&mut self.stdout_handle, EnableMouseCapture, cursor::Hide)?;
        } else {
            execute!(&mut self.stdout_handle, cursor::Hide)?;
        }

        self.stdout_handle.flush()?;
        Ok(())
    }

    /// Prints the startup message to the terminal
    async fn print_startup_message(
        &mut self,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        writeln!(self.stdout_handle, "{}", message)?;
        self.stdout_handle.flush()?;
        Ok(())
    }

    /// Processes a single terminal event and returns whether the app should quit.
    ///
    /// # Arguments
    ///
    /// * `event` - Terminal event to process
    ///
    /// # Returns
    ///
    /// `Ok(true)` if the application should exit, `Ok(false)` otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if event processing fails.
    pub async fn process_event(
        &mut self,
        event: Event,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let mut should_quit = false;

        if let Event::Key(key_event) = &event {
            if key_event.kind == KeyEventKind::Release {
                return Ok(should_quit);
            }

            if let Some(last_event) = &self.last_key_event
                && last_event.code == key_event.code
                && last_event.modifiers == key_event.modifiers
                && last_event.kind == key_event.kind
            {
                let is_control_key = match key_event.code {
                    KeyCode::Char('c') if key_event.modifiers == KeyModifiers::CONTROL => true,
                    KeyCode::Char('d') if key_event.modifiers == KeyModifiers::CONTROL => true,
                    _ => false,
                };

                if !is_control_key {
                    return Ok(should_quit);
                }
            }

            match key_event.code {
                KeyCode::Char('c') if key_event.modifiers == KeyModifiers::CONTROL => {
                    self.last_key_event = Some(*key_event);
                }
                KeyCode::Char('d') if key_event.modifiers == KeyModifiers::CONTROL => {
                    self.last_key_event = Some(*key_event);
                }
                _ => {}
            }
        }

        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event
        {
            match code {
                KeyCode::Char('d') if modifiers == KeyModifiers::CONTROL => {
                    should_quit = self.handle_ctrl_d().await?;
                }
                KeyCode::Char('c') if modifiers == KeyModifiers::CONTROL => {
                    let (quit, message) = self.handle_ctrl_c().await?;
                    should_quit = quit;
                    self.print_log_entry(&message);
                }
                KeyCode::Esc => {
                    self.completions_hidden = true;
                    self.focus_on_completions = false;
                    self.render_input_line()?;
                }
                KeyCode::Up => {
                    if self.focus_on_completions {
                        // Candidate area -> Command prompt
                        self.focus_on_completions = false;
                        self.render_input_line()?;
                    } else {
                        // Command prompt history navigation
                        self.handle_up_key();
                        self.render_input_line()?;
                    }
                }
                KeyCode::Down => {
                    if self.focus_on_completions {
                        // Already in candidates, maybe wrap or stay? User said "Left/Right move highlight".
                        // "Up moves back". Down isn't specified for candidate nav, but usually does nothing or wraps?
                        // Let's keep it doing nothing or maybe looping?
                        // User request: "Candidate area can only press Up key to switch back to command prompt"
                        // So Down in candidate area does nothing.
                    } else {
                        // In command prompt
                        // "From command prompt press key found no new history command, then switch to candidate area"
                        // This corresponds to when history_index is None (editing current line) or at the very end.
                        if self.history_index.is_none() {
                            if !self.current_completions.is_empty() && !self.completions_hidden {
                                self.focus_on_completions = true;
                                self.render_input_line()?;
                            }
                        } else {
                            // Try to move down in history
                            self.handle_down_key();
                            // If after moving down we are now at the "current" input (None),
                            // we don't automatically jump to completions yet, user needs to press Down again.
                            // Logic matching: "press key found no new history command" -> which implies the press *failed* to find new history.
                            // My handle_down_key moves to None if at last history item.
                            // So if we *were* at None, we go to completions.
                            self.render_input_line()?;
                        }
                    }
                }
                KeyCode::Left => {
                    if self.focus_on_completions
                        && !self.completions_hidden
                        && !self.current_completions.is_empty()
                    {
                        if self.selected_completion_index == 0 {
                            self.selected_completion_index = self.current_completions.len() - 1;
                        } else {
                            self.selected_completion_index -= 1;
                        }
                        self.render_input_line()?;
                    } else {
                        // Ensure focus is reset if we fell through (e.g. was focused but now hidden)
                        self.focus_on_completions = false;
                        if self.cursor_position > 0 {
                            self.cursor_position -= 1;
                            self.render_input_line()?;
                        }
                    }
                }
                KeyCode::Right => {
                    if self.focus_on_completions
                        && !self.completions_hidden
                        && !self.current_completions.is_empty()
                    {
                        if self.selected_completion_index == self.current_completions.len() - 1 {
                            self.selected_completion_index = 0;
                        } else {
                            self.selected_completion_index += 1;
                        }
                        self.render_input_line()?;
                    } else {
                        // Ensure focus is reset if we fell through
                        self.focus_on_completions = false;

                        // In command prompt
                        if self.cursor_position < self.current_input.chars().count() {
                            // Normal move right
                            self.cursor_position += 1;
                            self.render_input_line()?;
                        } else {
                            // At end of input -> "Jump to below to move candidate area highlight"
                            if !self.current_completions.is_empty() && !self.completions_hidden {
                                self.focus_on_completions = true;
                                self.render_input_line()?;
                            }
                        }
                    }
                }
                KeyCode::Tab => {
                    // "Tab candidates rendered -> Tab completes. Not rendered -> Tab toggles"
                    if !self.completions_hidden && !self.current_completions.is_empty() {
                        self.handle_tab_key();
                    } else {
                        self.completions_hidden = !self.completions_hidden;
                    }
                    self.render_input_line()?;
                }
                KeyCode::Enter => {
                    let (should_exit, _) = self.handle_enter_key("> ").await?;
                    if should_exit {
                        return Ok(true);
                    }
                }
                KeyCode::Char(c) => {
                    self.handle_char_input(c);
                    self.update_completions();
                    self.render_input_line()?;
                }
                KeyCode::Backspace => {
                    if self.cursor_position > 0 {
                        self.remove_char_at(self.cursor_position - 1);
                        self.cursor_position -= 1;
                        self.update_completions();
                        self.render_input_line()?;
                    }
                }
                _ => {}
            }
        } else if let Event::Resize(_, _) = event {
            // When terminal is resized, we should hide completions to prevent visual artifacts
            // and require user to request them again or type to see them.
            self.completions_hidden = true;
            self.focus_on_completions = false;
            self.render_input_line()?;
        }
        Ok(should_quit)
    }

    /// Shuts down the terminal and displays exit messages.
    ///
    /// # Arguments
    ///
    /// * `exit_message` - Message to display on exit
    ///
    /// # Errors
    ///
    /// Returns an error if terminal shutdown fails.
    pub async fn shutdown_terminal(
        &mut self,
        exit_message: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.raw_mode_enabled {
            disable_raw_mode()?;
            execute!(self.stdout_handle, DisableMouseCapture, cursor::Show)?;
        } else {
            // If raw mode wasn't enabled, we only need to show the cursor
            execute!(self.stdout_handle, cursor::Show)?;
        }
        writeln!(self.stdout_handle, "{}", exit_message)?;
        let _ = execute!(self.stdout_handle, cursor::MoveToColumn(0));
        Ok(())
    }

    /// Waits for and returns the next user input event.
    ///
    /// This method processes terminal events in a non-blocking manner and returns
    /// when the user presses Enter with non-empty input or when a quit signal is received.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(String))` - User entered a non-empty string
    /// - `Ok(None)` - User should exit (Ctrl+C, Ctrl+D, or should_exit flag set)
    ///
    /// # Errors
    ///
    /// Returns an error if terminal event processing fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use daemon_console_lite::TerminalApp;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut app = TerminalApp::new();
    ///     app.init_terminal("Welcome!").await?;
    ///
    ///     while let Some(input) = app.read_input().await? {
    ///         app.info(&format!("You entered: {}", input));
    ///     }
    ///
    ///     app.shutdown_terminal("Goodbye!").await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn read_input(&mut self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {
                    if poll(std::time::Duration::from_millis(0))?
                        && let Ok(event) = event::read() {
                            if let Event::Key(KeyEvent { code: KeyCode::Enter, .. }) = event {
                                let (should_exit, input) = self.handle_enter_key("> ").await?;
                                if should_exit {
                                    return Ok(None);
                                }
                                if let Some(user_input) = input {
                                    return Ok(Some(user_input));
                                }
                            } else if self.process_event(event).await? {
                                return Ok(None);
                            }
                        }
                }
            }

            if self.should_exit {
                return Ok(None);
            }
        }
    }

    /// Simple convenience method that runs a basic input loop.
    ///
    /// For more control, use `init_terminal()`, `read_input()`, and `shutdown_terminal()` separately.
    ///
    /// # Arguments
    ///
    /// * `startup_message` - Optional message to display on startup
    /// * `exit_message` - Optional message to display on exit
    ///
    /// # Errors
    ///
    /// Returns an error if terminal initialization or event handling fails.
    pub async fn run(
        &mut self,
        startup_message: &str,
        exit_message: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.init_terminal(startup_message).await?;

        let loop_result: Result<(), Box<dyn std::error::Error>> = async {
            while let Some(input) = self.read_input().await? {
                self.info(&format!("You entered: {}", input));
            }
            Ok(())
        }
        .await;

        let cleanup_result: Result<(), Box<dyn std::error::Error>> = (|| {
            if self.raw_mode_enabled {
                disable_raw_mode()?;
                execute!(self.stdout_handle, DisableMouseCapture, cursor::Show)?;
            } else {
                execute!(self.stdout_handle, cursor::Show)?;
            }
            Ok(())
        })();

        if !exit_message.is_empty() {
            writeln!(self.stdout_handle, "{}", exit_message)?;
        }

        loop_result.and(cleanup_result)
    }

    /// Clears the current input line and completion hints if rendered.
    ///
    /// If `hints_rendered` is true, this clears both the input line and the line below it
    /// containing completion hints. Otherwise, only the current line is cleared.
    pub fn clear_input_line(&mut self) {
        if self.hints_rendered {
            let _ = execute!(
                self.stdout_handle,
                cursor::MoveToColumn(0),
                Clear(ClearType::CurrentLine),
                cursor::MoveDown(1),
                Clear(ClearType::CurrentLine),
                cursor::MoveUp(1),
                cursor::MoveToColumn(0)
            );
            self.hints_rendered = false;
        } else {
            let _ = execute!(
                self.stdout_handle,
                cursor::MoveToColumn(0),
                Clear(ClearType::CurrentLine)
            );
        }
    }

    /// Prints a log entry while preserving the input line.
    ///
    /// Clears the input line, outputs the log message, then re-renders the input line
    /// on a new line without clearing first.
    pub fn print_log_entry(&mut self, log_line: &str) {
        self.clear_input_line();
        if log_line.contains('\n') {
            for line in log_line.lines() {
                let _ = writeln!(self.stdout_handle, "{}", line);
                let _ = execute!(self.stdout_handle, cursor::MoveToColumn(0));
            }
        } else {
            let _ = writeln!(self.stdout_handle, "{}", log_line);
        }

        // let _ = self.stdout_handle.flush();
        let _ = execute!(self.stdout_handle, cursor::MoveToColumn(0));
        let _ = self.render_input_line_no_clear();
    }

    /// Truncates a string to the specified maximum length, adding "..." if truncated.
    ///
    /// # Arguments
    ///
    /// * `text` - The text to truncate
    /// * `max_length` - Maximum length including the "..." suffix
    ///
    /// # Returns
    ///
    /// Truncated string with "..." if it exceeds max_length, otherwise the original string
    fn truncate_text(&self, text: &str, max_length: usize) -> String {
        if text.chars().count() <= max_length {
            return text.to_string();
        }

        if max_length <= 3 {
            return "...".to_string();
        }

        let truncated_chars: Vec<char> = text.chars().take(max_length - 3).collect();
        format!("{}...", truncated_chars.iter().collect::<String>())
    }

    /// Renders prompt, input text, and completion hints.
    ///
    /// This is the core rendering logic shared by both `render_input_line()`
    /// and `render_input_line_no_clear()`.
    /// Renders prompt, input text, and completion hints.
    ///
    /// This is the core rendering logic shared by both `render_input_line()`
    /// and `render_input_line_no_clear()`.
    ///
    /// Handles text truncation if the input line exceeds the terminal width,
    /// adding "..." at the start or end to keep the cursor visible.
    fn render_input_content(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let (term_cols, _) = size()?;
        let term_width = term_cols as usize;
        let prompt_width = 2; // "> "
        let available_width = term_width.saturating_sub(prompt_width).saturating_sub(1); // -1 safety

        let input_chars: Vec<char> = self.current_input.chars().collect();
        let input_len = input_chars.len();

        // Calculate total display width of the full input
        let total_input_width = self.current_input.width();

        let (display_text, visual_idx_start) = if total_input_width <= available_width {
            (self.current_input.clone(), 0)
        } else {
            // Need truncation.
            // We want to keep cursor visible.
            // Window strategy:
            // available_width is the constraint (in columns).
            // ellipsis takes 3 columns.

            // Convert cursor index (char index) to visual offset? No, simpler to operate on char indices first?
            // Widths vary. We must iterate.

            let cursor_char_idx = self.cursor_position;

            // Determining the window [start_char_idx, end_char_idx]
            // We blindly try to center the cursor or just keep it in view?
            // User: "If too long, hide front..., move back hide back...".
            // This suggests: if cursor is at end, show tail. If cursor is at start, show head.

            // Let's find a range of characters [start, end] such that:
            // 1. start <= cursor <= end
            // 2. width(chars[start..end]) + markers <= available_width
            // 3. Maximizes visible content (greedy).

            // Simplistic approach:
            // If cursor is near end (right side), anchor right.
            // If cursor is near start (left side), anchor left.

            // Let's walk and measure.

            // Determine required left ellipsis
            // If we are showing from the very first char, no left ellipsis.
            // If we skip chars, we need left ellipsis "..." (width 3).

            // Determine required right ellipsis
            // If we show until the last char, no right ellipsis.
            // Otherwise, right ellipsis "...".

            // This is actually a bit circular (need range to know ellipsis, need ellipsis to know range).
            // Heuristic:
            // 1. Try showing from char 0. If cursor fits and end fits -> done (already covered by total check).
            // 2. If cursor is comfortably inside the first N chars that fit -> show head, add tail "..."
            // 3. If cursor is deep?

            // Robust sliding window:
            // Center the window around cursor?
            // Target width = available_width

            // Step 1: scan widths of all chars
            let char_widths: Vec<usize> =
                input_chars.iter().map(|c| c.width().unwrap_or(0)).collect();

            // Find visible range [idx_l, idx_r) (char indices)
            let mut idx_l;
            let mut idx_r;

            // Check if head fits (cursor must be visible)
            // Try [0, idx_r] such that width fits.
            // If cursor <= idx_r, we can show head.
            // Check width of [0 .. something] + 3 ("...")

            let mut current_width = 0;
            let mut limit = 0; // how many chars fit from start
            for (i, w) in char_widths.iter().enumerate() {
                if current_width + w + 3 > available_width {
                    break;
                }
                current_width += w;
                limit = i + 1;
            }

            if cursor_char_idx <= limit && limit < input_len {
                // Cursor is in the "Head" part, and we can't show everything.
                // Show: chars[0..limit] + "..."
                idx_l = 0;
                idx_r = limit;
                // Optimization: maybe we can squeeze one more char if "..." fits exactly?
                // My loop broke early. current_width + 3 <= available.
            } else {
                // Cursor is not in the simple head view.
                // Try "Tail" view?
                // Calculate width backwards from end.
                // needed chars + 3 ("...") <= available

                let mut tail_width = 0;
                let mut start_from = input_len;
                for (i, w) in char_widths.iter().enumerate().rev() {
                    if tail_width + w + 3 > available_width {
                        break;
                    }
                    tail_width += w;
                    start_from = i;
                }

                if cursor_char_idx >= start_from {
                    // Cursor is in the tail view.
                    // Show "..." + chars[start_from..input_len]
                    idx_l = start_from;
                    idx_r = input_len; // all the way to end
                // Note: logic implies we hide start.
                } else {
                    // Cursor is in the middle. We need "..." + content + "..."
                    // Width available for content = available_width - 6.
                    // Center around cursor.

                    // Width of chars up to cursor
                    // let _width_before = char_widths[0..cursor_char_idx].iter().sum::<usize>();

                    // We need to pick idx_l and idx_r such that idx_l < cursor < idx_r
                    // and char_widths[idx_l..idx_r].sum() <= available_width - 6

                    // Let's expand outwards from cursor
                    let content_budget = available_width.saturating_sub(6);
                    idx_l = cursor_char_idx;
                    idx_r = cursor_char_idx; // idx_r is exclusive? Let's say idx_r exclusive.
                    // Initially include the char at cursor (if any, cursor can be at len)
                    // If cursor at len, it's effectively tail view, captured above?
                    // If cursor == input_len, `start_from` loop ensures it's caught (since start_from <= input_len).

                    if cursor_char_idx == input_len {
                        // Should have been tail view
                        // But for safety:
                        idx_l = input_len.saturating_sub(1);
                        idx_r = input_len;
                    }

                    let mut used = 0;
                    // Expand
                    loop {
                        let mut expanded = false;
                        // Try left
                        if idx_l > 0 && used + char_widths[idx_l - 1] <= content_budget {
                            idx_l -= 1;
                            used += char_widths[idx_l];
                            expanded = true;
                        }
                        // Try right
                        if idx_r < input_len && used + char_widths[idx_r] <= content_budget {
                            used += char_widths[idx_r]; // idx_r is newly added char index
                            idx_r += 1;
                            expanded = true;
                        }
                        if !expanded {
                            break;
                        }
                    }
                }
            }

            // Construct string
            let sub: String = input_chars[idx_l..idx_r].iter().collect();
            let mut out = String::new();

            let has_left_ellipsis = idx_l > 0;
            let has_right_ellipsis = idx_r < input_len;

            if has_left_ellipsis {
                out.push_str("...");
            }
            out.push_str(&sub);
            if has_right_ellipsis {
                out.push_str("...");
            }

            (
                out,
                if has_left_ellipsis { 3 } else { 0 }
                    + width_of_chars(&input_chars[idx_l..cursor_char_idx]),
            )
        };

        execute!(
            self.stdout_handle,
            crossterm::style::Print("> "),
            crossterm::style::Print(&display_text)
        )?;

        if !self.current_completions.is_empty() {
            self.render_completion_hints()?;
        }

        // Calculate visual cursor position
        // If we truncated, we calculated offset above.
        // If no truncation, we calculate normally.
        let visual_cursor_col = if total_input_width <= available_width {
            // Full normal calculation
            // Re-calculate to be safe or reuse logic?
            // self.calculate_visual_cursor_pos() helper uses full string, correct.
            2 + self
                .current_input
                .chars()
                .take(self.cursor_position)
                .map(|c| c.width().unwrap_or(0))
                .sum::<usize>()
        } else {
            // Truncated version
            // prompt (2) + relative position calculated in if-block
            2 + visual_idx_start
        };

        execute!(
            self.stdout_handle,
            cursor::MoveToColumn(visual_cursor_col as u16),
            cursor::Show
        )?;
        self.stdout_handle.flush()?;
        Ok(())
    }

    /// Renders the input line with prompt, text, and completion hints.
    ///
    /// Clears the current line first, then displays the prompt and input text.
    /// If completions are available, renders hints below the input line.
    /// Finally, positions the cursor at `cursor_position`.
    fn render_input_line(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            execute!(self.stdout_handle, cursor::Hide)?;
            self.clear_input_line();
            self.render_input_content()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = execute!(self.stdout_handle, cursor::Show);
        }
        result
    }

    /// Renders the input line without clearing first.
    ///
    /// Used after log output where the cursor is already on a new line.
    /// Ensures the cursor starts at column 0, then renders prompt, input text,
    /// and completion hints if available.
    fn render_input_line_no_clear(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            execute!(self.stdout_handle, cursor::Hide)?;
            execute!(self.stdout_handle, cursor::MoveToColumn(0))?;
            self.render_input_content()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = execute!(self.stdout_handle, cursor::Show);
        }
        result
    }

    /// Renders completion hints below the input line.
    ///
    /// This method dynamically calculates which completion candidates fits within the current
    /// terminal width, ensuring the selected candidate is always visible.
    fn render_completion_hints(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Don't render if completions are hidden
        if self.completions_hidden {
            self.hints_rendered = false;
            return Ok(());
        }

        // Get current terminal size
        let (term_cols, _) = size()?;
        // Use a safety buffer of 1 char to absolutely prevent wrapping
        let term_width = (term_cols as usize).saturating_sub(1);

        let total_count = self.current_completions.len();
        if total_count == 0 {
            self.hints_rendered = false;
            return Ok(());
        }

        // Helper closure to build the display string for a candidate
        let get_display_text = |app: &TerminalApp, idx: usize, max_len: usize| -> String {
            let candidate = &app.current_completions[idx];
            let mut item_text = String::from("[");

            if candidate.completion.is_empty() && candidate.description.is_some() {
                item_text.clear();
                item_text.push('<');
                item_text.push_str(candidate.description.as_ref().unwrap_or(&String::from("")));
                item_text.push('>');
            } else if app.current_completions.len() == 1 {
                // For single item, we might be lenient or strict. Let's respect max_len to avoid overflow.
                let truncated_completion = app.truncate_text(&candidate.completion, max_len);
                item_text.push_str(&truncated_completion);
                if let Some(desc) = &candidate.description {
                    item_text.push_str(": ");
                    item_text.push_str(desc);
                }
                item_text.push(']');
            } else {
                let truncated_completion = app.truncate_text(&candidate.completion, max_len);
                item_text.push_str(&truncated_completion);
                if let Some(desc) = &candidate.description {
                    item_text.push_str(": ");
                    let truncated_desc = app.truncate_text(desc, max_len);
                    item_text.push_str(&truncated_desc);
                }
                item_text.push(']');
            }
            item_text
        };

        // Calculate a dynamic max length per item.
        // A minimal usable length is e.g. 10. Max is term_width - overhead.
        let dynamic_max_len = term_width.saturating_sub(6).max(10);

        // Start with the selected item
        let mut start_idx = self.selected_completion_index;
        let mut end_idx = self.selected_completion_index + 1; // Exclusive

        let selected_text = get_display_text(self, self.selected_completion_index, dynamic_max_len);
        let mut current_width = selected_text.width();

        // Expand outwards
        loop {
            let hidden_left = start_idx;
            let hidden_right = total_count - end_idx;

            // Calculate overhead for hidden markers
            let left_marker_width = if hidden_left > 0 {
                format!(" (+{})", hidden_left).width()
            } else {
                0
            };
            let right_marker_width = if hidden_right > 0 {
                format!(" (+{})", hidden_right).width()
            } else {
                0
            };

            let extra_left_space = if hidden_left > 0 { 1 } else { 0 };

            // Check if we fit
            let total_needed =
                left_marker_width + extra_left_space + current_width + right_marker_width;

            if total_needed > term_width {
                break;
            }

            // Try to expand
            let can_go_left = start_idx > 0;
            let can_go_right = end_idx < total_count;

            if !can_go_left && !can_go_right {
                break;
            }

            // Strategy: Balance expansion around selection
            let left_count = self.selected_completion_index - start_idx;
            let right_count = end_idx - 1 - self.selected_completion_index;

            let mut added = false;

            if can_go_left && (left_count <= right_count || !can_go_right) {
                let prev_idx = start_idx - 1;
                let text = get_display_text(self, prev_idx, dynamic_max_len);
                let added_width = 1 + text.width();

                let new_hidden_left = prev_idx;
                let new_left_marker = if new_hidden_left > 0 {
                    format!(" (+{})", new_hidden_left).width()
                } else {
                    0
                };
                let new_extra_space = if new_hidden_left > 0 { 1 } else { 0 };

                let new_content_width = current_width + added_width;
                if new_left_marker + new_extra_space + new_content_width + right_marker_width
                    <= term_width
                {
                    start_idx = prev_idx;
                    current_width += added_width;
                    added = true;
                }
            }

            if !added && can_go_right {
                let next_idx = end_idx;
                let text = get_display_text(self, next_idx, dynamic_max_len);
                let added_width = 1 + text.width();

                let new_hidden_right = total_count - (next_idx + 1);
                let new_right_marker = if new_hidden_right > 0 {
                    format!(" (+{})", new_hidden_right).width()
                } else {
                    0
                };

                let current_total_left = left_marker_width + extra_left_space;

                let new_content_width = current_width + added_width;
                if current_total_left + new_content_width + new_right_marker <= term_width {
                    end_idx += 1;
                    current_width += added_width;
                    added = true;
                }
            }

            if !added {
                break;
            }
        }

        execute!(
            self.stdout_handle,
            SavePosition,
            crossterm::style::Print("\n"),
            cursor::MoveToColumn(0)
        )?;

        let hidden_left = start_idx;
        let hidden_right = total_count - end_idx;

        if hidden_left > 0 {
            execute!(
                self.stdout_handle,
                SetForegroundColor(Color::DarkGrey),
                crossterm::style::Print(&format!(" (+{})", hidden_left))
            )?;
        }

        for idx in start_idx..end_idx {
            if idx > start_idx || hidden_left > 0 {
                execute!(self.stdout_handle, crossterm::style::Print(" "))?;
            }

            let is_selected = idx == self.selected_completion_index;
            let color = if is_selected && self.focus_on_completions {
                Color::Cyan
            } else {
                Color::DarkGrey
            };

            execute!(self.stdout_handle, SetForegroundColor(color))?;

            // Re-generate text (a bit redundant but safe)
            let item_text = get_display_text(self, idx, dynamic_max_len);
            execute!(self.stdout_handle, crossterm::style::Print(&item_text))?;
        }

        if hidden_right > 0 {
            execute!(
                self.stdout_handle,
                SetForegroundColor(Color::DarkGrey),
                crossterm::style::Print(&format!(" (+{})", hidden_right))
            )?;
        }

        execute!(
            self.stdout_handle,
            ResetColor,
            Clear(ClearType::UntilNewLine),
            RestorePosition
        )?;

        self.hints_rendered = true;
        Ok(())
    }

    /// Handles Ctrl+D key press to exit the application.
    ///
    /// Clears the input line and completions before returning true to signal exit.
    pub async fn handle_ctrl_d(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        self.current_input.clear();
        self.cursor_position = 0;
        self.current_completions.clear();
        self.clear_input_line();
        Ok(true)
    }

    /// Handles Ctrl+C key press with double-press confirmation.
    ///
    /// - First press (with input): clears input and completions
    /// - First press (no input): prompts for confirmation
    /// - Second press within 5 seconds: exits application
    ///
    /// Returns (should_quit, message_to_display).
    pub async fn handle_ctrl_c(&mut self) -> Result<(bool, String), Box<dyn std::error::Error>> {
        self.current_completions.clear();
        if !self.current_input.is_empty() {
            self.current_input.clear();
            self.cursor_position = 0;
            self.last_ctrl_c = Some(Instant::now());
            return Ok((
                false,
                get_info!("Input cleared. Press Ctrl+C again to exit.", &self.app_name),
            ));
        }
        if let Some(last_time) = self.last_ctrl_c
            && last_time.elapsed().as_secs() < 5
        {
            return Ok((
                true,
                get_warn!("Exiting application. Goodbye!", &self.app_name),
            ));
        }
        self.last_ctrl_c = Some(Instant::now());
        Ok((
            false,
            get_info!("Press Ctrl+C again to exit.", &self.app_name),
        ))
    }

    /// Handles up the arrow key press for command history navigation.
    fn handle_up_key(&mut self) {
        if self.command_history.is_empty() {
            return;
        }

        // Save current input if we're at the end (no history selected yet)
        if self.history_index.is_none() {
            self.pending_input = Some(self.current_input.clone());
            self.pending_cursor_position = self.cursor_position;
        }

        let new_index = match self.history_index {
            Some(idx) if idx > 0 => idx - 1,
            Some(_) => return,
            None => self.command_history.len() - 1,
        };
        self.history_index = Some(new_index);
        self.current_input = self.command_history[new_index].clone();
        self.cursor_position = self.current_input.chars().count();
        self.update_completions();
    }

    /// Handles down the arrow key press for command history navigation.
    fn handle_down_key(&mut self) {
        let new_index = match self.history_index {
            Some(idx) if idx < self.command_history.len() - 1 => idx + 1,
            Some(_) => {
                self.history_index = None;
                // Restore pending input if available, otherwise clear
                if let Some(pending) = self.pending_input.take() {
                    self.current_input = pending;
                    self.cursor_position = self.pending_cursor_position;
                } else {
                    self.current_input.clear();
                    self.cursor_position = 0;
                }
                self.update_completions();
                return;
            }
            None => return,
        };
        self.history_index = Some(new_index);
        self.current_input = self.command_history[new_index].clone();
        self.cursor_position = self.current_input.chars().count();
        self.update_completions();
    }

    /// Handles Enter key press to submit input.
    ///
    /// If input is non-empty, adds it to history, echoes it with the prefix,
    /// clears the input state, and returns the input string. If empty, just
    /// clears and re-renders the input line.
    ///
    /// Returns (should_exit, optional_input_string).
    pub async fn handle_enter_key(
        &mut self,
        input_prefix: &str,
    ) -> Result<(bool, Option<String>), Box<dyn std::error::Error>> {
        if !self.current_input.trim().is_empty() {
            self.command_history.push(self.current_input.clone());
            self.current_completions.clear();
            self.clear_input_line();
            writeln!(self.stdout_handle, "{}{}", input_prefix, self.current_input)?;

            let input_copy = self.current_input.clone();
            self.current_input.clear();
            self.cursor_position = 0;
            self.history_index = None;
            self.render_input_line()?;

            Ok((self.should_exit, Some(input_copy)))
        } else {
            self.current_completions.clear();
            self.clear_input_line();
            self.render_input_line()?;
            Ok((self.should_exit, None))
        }
    }

    /// Handles Tab key press to apply the selected completion.
    ///
    /// If a completion is selected (via Left/Right arrows), uses that completion.
    /// Otherwise, uses the best match from the completion tree.
    fn handle_tab_key(&mut self) {
        if !self.current_completions.is_empty()
            && self.selected_completion_index < self.current_completions.len()
        {
            self.current_input = self.current_completions[self.selected_completion_index]
                .full_text
                .clone();
            self.cursor_position = self.current_input.chars().count();
            self.update_completions();
        } else if let Some(tree) = &mut self.tab_tree
            && let Some(completion) = tree.get_best_match(&self.current_input)
        {
            self.current_input = completion;
            self.cursor_position = self.current_input.chars().count();
            self.update_completions();
        }
    }

    /// Updates completion candidates based on current input.
    ///
    /// Resets the selected completion index to 0 when candidates change.
    fn update_completions(&mut self) {
        if let Some(tree) = &mut self.tab_tree {
            self.current_completions = tree.get_candidates(&self.current_input);
            self.selected_completion_index = 0;
        }
    }

    /// Handles character input by inserting at the cursor position.
    fn handle_char_input(&mut self, c: char) {
        let char_count = self.current_input.chars().count();

        if self.cursor_position > char_count {
            self.cursor_position = char_count;
        }

        let mut chars: Vec<char> = self.current_input.chars().collect();
        chars.insert(self.cursor_position, c);
        self.current_input = chars.into_iter().collect();
        self.cursor_position += 1;
    }

    /// Log info-level messages.
    ///
    /// This method ensures proper terminal line management by clearing the current
    /// input line, printing the log message, and then re-rendering the input line.
    ///
    /// # Arguments
    ///
    /// * `message` - The message content to be logged.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use daemon_console_lite::TerminalApp;
    ///
    /// fn needless_main() {
    ///     let mut app = TerminalApp::new();
    ///     app.info("Application started successfully!");
    ///     app.info("Running tasks...");
    /// }
    /// ```
    pub fn info(&mut self, message: &str) {
        self.logger(LogLevel::Info, message, Some("Stream"));
    }

    /// Log debug-level messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use daemon_console_lite::TerminalApp;
    ///
    /// fn needless_main() {
    ///     let mut app = TerminalApp::new();
    ///     app.debug("Debugging information...");
    ///     app.debug("Debugging more...");
    /// }
    /// ```
    pub fn debug(&mut self, message: &str) {
        self.logger(LogLevel::Debug, message, Some("Stream"));
    }

    /// Log warn-level messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use daemon_console_lite::TerminalApp;
    ///
    /// fn needless_main() {
    ///     let mut app = TerminalApp::new();
    ///     app.warn("You get a warning!");
    ///     app.warn("Continue running...");
    /// }
    /// ```
    pub fn warn(&mut self, message: &str) {
        self.logger(LogLevel::Warn, message, Some("Stream"));
    }

    /// Log error-level messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use daemon_console_lite::TerminalApp;
    ///
    /// fn needless_main() {
    ///     let mut app = TerminalApp::new();
    ///     app.error("An error occurred!");
    ///     app.error("Failed to run tasks.");
    /// }
    /// ```
    pub fn error(&mut self, message: &str) {
        self.logger(LogLevel::Error, message, Some("Stream"));
    }

    /// Log critical-level messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use daemon_console_lite::TerminalApp;
    ///
    /// fn needless_main() {
    ///     let mut app = TerminalApp::new();
    ///     app.critical("Application crashed!");
    ///     app.critical("Exception: unknown.");
    /// }
    /// ```
    pub fn critical(&mut self, message: &str) {
        self.logger(LogLevel::Critical, message, Some("Stream"));
    }

    /// Unified logger method that allows specifying a custom module name for the log message.
    ///
    /// # Arguments
    ///
    /// * `level` - The log level (Info, Warn, Error, Debug, Critical)
    /// * `message` - The message content to be logged
    /// * `module_name` - The name of the module to associate with the log message (optional)
    ///
    /// # Examples
    ///
    /// ```
    /// use daemon_console_lite::{TerminalApp, logger::LogLevel};
    ///
    /// fn example() {
    ///     let mut app = TerminalApp::new();
    ///     app.logger(LogLevel::Info, "Application started", Some("Main"));
    ///     app.logger(LogLevel::Error, "Database connection failed", None);
    /// }
    /// ```
    pub fn logger(&mut self, level: LogLevel, message: &str, module_name: Option<&str>) {
        let module_name = if module_name.is_none() {
            Some(self.app_name.as_str())
        } else {
            module_name
        };
        let formatted_message = match level {
            LogLevel::Info => {
                if let Some(module) = module_name {
                    get_info!(message, module)
                } else {
                    get_info!(message)
                }
            }
            LogLevel::Warn => {
                if let Some(module) = module_name {
                    get_warn!(message, module)
                } else {
                    get_warn!(message)
                }
            }
            LogLevel::Error => {
                if let Some(module) = module_name {
                    get_error!(message, module)
                } else {
                    get_error!(message)
                }
            }
            LogLevel::Debug => {
                if let Some(module) = module_name {
                    get_debug!(message, module)
                } else {
                    get_debug!(message)
                }
            }
            LogLevel::Critical => {
                if let Some(module) = module_name {
                    get_critical!(message, module)
                } else {
                    get_critical!(message)
                }
            }
        };
        self.print_log_entry(&formatted_message);
    }
}
