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
//!     app.run("Welcome!", "Goodbye!").await?;
//!     Ok(())
//! }
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod logger;
pub mod tab;
pub mod utils;

use crossterm::{
    cursor,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, poll,
    },
    execute,
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};
use std::io::{Stdout, Write, stdout};
use std::time::Instant;
use unicode_width::UnicodeWidthChar;

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
    pub stdout_handle: Stdout,
    pub command_history: Vec<String>,
    pub current_input: String,
    pub history_index: Option<usize>,
    pub last_ctrl_c: Option<Instant>,
    pub cursor_position: usize,
    pub should_exit: bool,
    last_key_event: Option<KeyEvent>,
    tab_tree: Option<TabTree>,
    current_completions: Vec<CompletionCandidate>,
    hints_rendered: bool,
    selected_completion_index: usize,
}

impl Default for TerminalApp {
    fn default() -> Self {
        Self::new()
    }
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
    pub fn new() -> Self {
        Self {
            stdout_handle: stdout(),
            command_history: Vec::new(),
            current_input: String::new(),
            history_index: None,
            last_ctrl_c: None,
            cursor_position: 0,
            should_exit: false,
            last_key_event: None,
            tab_tree: None,
            current_completions: Vec::new(),
            hints_rendered: false,
            selected_completion_index: 0,
        }
    }

    /// Enables tab completion and initializes the completion tree.
    pub fn enable_tab_completion(&mut self) {
        self.tab_tree = Some(TabTree::new());
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
            tree.register_completions(context, completions);
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
            tree.register_completions_with_desc(context, items);
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

        Ok(())
    }

    /// Sets up the terminal in raw mode and enables mouse capture
    fn setup_terminal(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        execute!(&mut self.stdout_handle, EnableMouseCapture, cursor::Hide)?;
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
                KeyCode::Up => {
                    self.handle_up_key();
                    self.render_input_line()?;
                }
                KeyCode::Down => {
                    self.handle_down_key();
                    self.render_input_line()?;
                }
                KeyCode::Left => {
                    if modifiers == KeyModifiers::ALT {
                        if !self.current_completions.is_empty()
                            && self.selected_completion_index > 0
                        {
                            self.selected_completion_index -= 1;
                            self.render_input_line()?;
                        }
                    } else if self.cursor_position > 0 {
                        self.cursor_position -= 1;
                        self.render_input_line()?;
                    }
                }
                KeyCode::Right => {
                    if modifiers == KeyModifiers::ALT {
                        if !self.current_completions.is_empty()
                            && self.selected_completion_index < self.current_completions.len() - 1
                        {
                            self.selected_completion_index += 1;
                            self.render_input_line()?;
                        }
                    } else if self.cursor_position < self.current_input.chars().count() {
                        self.cursor_position += 1;
                        self.render_input_line()?;
                    }
                }
                KeyCode::Tab => {
                    self.handle_tab_key();
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
        disable_raw_mode()?;
        execute!(self.stdout_handle, DisableMouseCapture, cursor::Show)?;
        writeln!(self.stdout_handle, "{}", exit_message)?;
        self.stdout_handle.flush()?;
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

        while let Some(input) = self.read_input().await? {
            self.info(&format!("You entered: {}", input));
        }

        self.shutdown_terminal(exit_message).await?;
        Ok(())
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
        let _ = writeln!(self.stdout_handle, "{}", log_line);
        let _ = self.stdout_handle.flush();
        let _ = execute!(self.stdout_handle, cursor::MoveToColumn(0));
        let _ = self.render_input_line_no_clear();
    }

    /// Calculates the visual cursor position accounting for Unicode character widths.
    ///
    /// Returns the column position where the cursor should be displayed,
    /// including the 2-character prompt ("> ").
    fn calculate_visual_cursor_pos(&self) -> usize {
        2 + self
            .current_input
            .chars()
            .take(self.cursor_position)
            .map(|c| c.width().unwrap_or(0))
            .sum::<usize>()
    }

    /// Renders prompt, input text, and completion hints.
    ///
    /// This is the core rendering logic shared by both `render_input_line()`
    /// and `render_input_line_no_clear()`.
    fn render_input_content(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        execute!(
            self.stdout_handle,
            crossterm::style::Print("> "),
            crossterm::style::Print(&self.current_input)
        )?;

        if !self.current_completions.is_empty() {
            self.render_completion_hints()?;
        }

        let visual_cursor_pos = self.calculate_visual_cursor_pos();
        execute!(
            self.stdout_handle,
            cursor::MoveToColumn(visual_cursor_pos as u16),
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
    /// Creates a new line for hints using a newline character, then uses
    /// `SavePosition`/`RestorePosition` to render hints without permanently
    /// affecting the cursor position. Sets `hints_rendered` to true.
    ///
    /// Displays up to 5 completion candidates with smooth scrolling. The selected
    /// candidate is always visible and highlighted in cyan, others in dark gray.
    fn render_completion_hints(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        use crossterm::cursor::{RestorePosition, SavePosition};
        use crossterm::style::{Color, ResetColor, SetForegroundColor};

        let total_count = self.current_completions.len();
        let max_display = 5;

        // Calculate the display window to ensure the selected item is visible
        let (start_idx, end_idx) = if total_count <= max_display {
            (0, total_count)
        } else {
            // Center the selected item in the window when possible
            let half_window = max_display / 2;
            let start = if self.selected_completion_index <= half_window {
                0
            } else if self.selected_completion_index >= total_count - half_window {
                total_count - max_display
            } else {
                self.selected_completion_index - half_window
            };
            (start, start + max_display)
        };

        execute!(
            self.stdout_handle,
            SavePosition,
            crossterm::style::Print("\n"),
            cursor::MoveToColumn(0)
        )?;

        for (idx, candidate) in self
            .current_completions
            .iter()
            .enumerate()
            .skip(start_idx)
            .take(end_idx - start_idx)
        {
            if idx > start_idx {
                execute!(self.stdout_handle, crossterm::style::Print("  "))?;
            }

            let is_selected = idx == self.selected_completion_index;
            let color = if is_selected {
                Color::Cyan
            } else {
                Color::DarkGrey
            };

            execute!(self.stdout_handle, SetForegroundColor(color))?;

            let mut item_text = String::from("[");
            item_text.push_str(&candidate.completion);
            if let Some(desc) = &candidate.description {
                item_text.push_str(": ");
                item_text.push_str(desc);
            }
            item_text.push(']');

            execute!(self.stdout_handle, crossterm::style::Print(&item_text))?;
        }

        // Show indicator for hidden items
        if start_idx > 0 || end_idx < total_count {
            let hidden_left = start_idx;
            let hidden_right = total_count - end_idx;
            if hidden_left > 0 && hidden_right > 0 {
                execute!(
                    self.stdout_handle,
                    crossterm::style::Print(&format!("  (+{}/+{})", hidden_left, hidden_right))
                )?;
            } else if hidden_left > 0 {
                execute!(
                    self.stdout_handle,
                    crossterm::style::Print(&format!("  (+{}←)", hidden_left))
                )?;
            } else {
                execute!(
                    self.stdout_handle,
                    crossterm::style::Print(&format!("  (→+{})", hidden_right))
                )?;
            }
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
        if !self.current_input.is_empty() {
            self.current_input.clear();
            self.cursor_position = 0;
            self.current_completions.clear();
            self.last_ctrl_c = Some(Instant::now());
            return Ok((
                false,
                get_info!(
                    "Input cleared. Press Ctrl+C again to exit.",
                    "Daemon Console"
                ),
            ));
        }
        if let Some(last_time) = self.last_ctrl_c
            && last_time.elapsed().as_secs() < 5
        {
            return Ok((
                true,
                get_warn!("Exiting application. Goodbye!", "Daemon Console"),
            ));
        }
        self.last_ctrl_c = Some(Instant::now());
        Ok((
            false,
            get_info!("Press Ctrl+C again to exit.", "Daemon Console"),
        ))
    }

    /// Handles up the arrow key press for command history navigation.
    fn handle_up_key(&mut self) {
        if self.command_history.is_empty() {
            return;
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
                self.current_input.clear();
                self.cursor_position = 0;
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
    /// If a completion is selected (via Alt+Left/Right), uses that completion.
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
