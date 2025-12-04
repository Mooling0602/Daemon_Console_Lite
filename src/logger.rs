//! Logging utilities with colored terminal output.
//!
//! This module provides a logging system with different severity levels
//! (Info, Warn, Error, Debug) and automatic timestamp formatting.

use chrono::Local;
use crossterm::style::{self, Color, ResetColor, SetForegroundColor};

/// Log level enumeration for categorizing log messages.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
    Critical,
}

/// Formats a log message with timestamp, level indicator, and color coding.
///
/// # Arguments
///
/// * `level` - Severity level of the log message
/// * `message` - Content of the log message
/// * `module_name` - Optional module name prefix
///
/// # Returns
///
/// Formatted string with ANSI color codes for terminal display
///
/// # Examples
///
/// ```
/// use daemon_console_lite::logger::{log_message, LogLevel};
///
/// let msg = log_message(LogLevel::Info, "Application started", Some("main"));
/// println!("{}", msg);
/// ```
pub fn log_message(level: LogLevel, message: &str, module_name: Option<&str>) -> String {
    let now = Local::now();
    let timestamp = now.format("%H:%M:%S").to_string();

    let (level_str, color) = match level {
        LogLevel::Info => ("INFO", Color::Green),
        LogLevel::Warn => ("WARN", Color::Yellow),
        LogLevel::Error => ("ERROR", Color::Red),
        LogLevel::Debug => ("DEBUG", Color::DarkGrey),
        LogLevel::Critical => ("CRITICAL", Color::AnsiValue(5)),
    };

    let module_prefix = module_name.map_or_else(String::new, |name| format!("{}/", name));

    match level {
        LogLevel::Info | LogLevel::Warn | LogLevel::Error | LogLevel::Critical => {
            format!(
                "[{}] {}[{}{}{}{}{}]{} {}{}",
                timestamp,
                style::Attribute::Bold,
                module_prefix,
                SetForegroundColor(color),
                level_str,
                ResetColor,
                style::Attribute::Bold,
                ResetColor,
                message,
                ResetColor
            )
        }
        LogLevel::Debug => {
            format!(
                "{}{}[{}] [{}{}] {}{}{}",
                SetForegroundColor(color),
                style::Attribute::Italic,
                timestamp,
                module_prefix,
                level_str,
                style::Attribute::Italic,
                message,
                ResetColor,
            )
        }
    }
}

/// Format multi-line messages with log-levels.
///
/// > Middleware method for macros like `get_info!`.
pub fn format_multiline_message(
    level: LogLevel,
    message: &str,
    module_name: Option<&str>,
) -> String {
    if !message.contains('\n') {
        return log_message(level, message, module_name);
    }

    message
        .lines()
        .map(|line| log_message(level, line, module_name))
        .collect::<Vec<String>>()
        .join("\n")
}

/// Macro for creating info-level log messages.
///
/// # Examples
///
/// ```
/// use daemon_console_lite::get_info;
///
/// let msg = get_info!("Server started");
/// let msg_with_module = get_info!("Database connected", "db");
/// ```
#[macro_export]
macro_rules! get_info {
    ($message:expr) => {
        $crate::logger::format_multiline_message($crate::logger::LogLevel::Info, $message, None)
    };
    ($message:expr, $module_name:expr) => {
        $crate::logger::format_multiline_message(
            $crate::logger::LogLevel::Info,
            $message,
            Some($module_name),
        )
    };
}

/// Macro for creating warning-level log messages.
///
/// # Examples
///
/// ```
/// use daemon_console_lite::get_warn;
///
/// let msg = get_warn!("Memory usage high");
/// let msg_with_module = get_warn!("Connection timeout", "network");
/// ```
#[macro_export]
macro_rules! get_warn {
    ($message:expr) => {
        $crate::logger::format_multiline_message($crate::logger::LogLevel::Warn, $message, None)
    };
    ($message:expr, $module_name:expr) => {
        $crate::logger::format_multiline_message(
            $crate::logger::LogLevel::Warn,
            $message,
            Some($module_name),
        )
    };
}

/// Macro for creating error-level log messages.
///
/// # Examples
///
/// ```
/// use daemon_console_lite::get_error;
///
/// let msg = get_error!("Failed to connect");
/// let msg_with_module = get_error!("Authentication failed", "auth");
/// ```
#[macro_export]
macro_rules! get_error {
    ($message:expr) => {
        $crate::logger::format_multiline_message($crate::logger::LogLevel::Error, $message, None)
    };
    ($message:expr, $module_name:expr) => {
        $crate::logger::format_multiline_message(
            $crate::logger::LogLevel::Error,
            $message,
            Some($module_name),
        )
    };
}

/// Macro for creating debug-level log messages.
///
/// # Examples
///
/// ```
/// use daemon_console_lite::get_debug;
///
/// let msg = get_debug!("Variable value: 42");
/// let msg_with_module = get_debug!("Request received", "http");
/// ```
#[macro_export]
macro_rules! get_debug {
    ($message:expr) => {
        $crate::logger::format_multiline_message($crate::logger::LogLevel::Debug, $message, None)
    };
    ($message:expr, $module_name:expr) => {
        $crate::logger::format_multiline_message(
            $crate::logger::LogLevel::Debug,
            $message,
            Some($module_name),
        )
    };
}

/// Macro for creating critical-level log messages.
///
/// # Examples
///
/// ```
/// use daemon_console_lite::get_critical;
///
/// let msg = get_critical!("Critical error");
/// let msg_with_module = get_critical!("Critical error", "database");
/// ```
#[macro_export]
macro_rules! get_critical {
    ($message:expr) => {
        $crate::logger::format_multiline_message($crate::logger::LogLevel::Critical, $message, None)
    };
    ($message:expr, $module_name:expr) => {
        $crate::logger::format_multiline_message(
            $crate::logger::LogLevel::Critical,
            $message,
            Some($module_name),
        )
    };
}
