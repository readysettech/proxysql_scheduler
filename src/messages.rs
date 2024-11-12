use std::process;

use chrono::{DateTime, Local};
use once_cell::sync::Lazy;
use std::sync::Mutex;

#[derive(Clone, Copy, serde::Deserialize, Debug, Default, PartialEq, PartialOrd)]
pub enum MessageType {
    /// Information message, this will not result in any action
    Info,
    /// Note message, this will result in some action that changes state
    #[default]
    Note,
    /// Warning message
    Warning,
    /// Error message
    Error,
}

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MessageType::Info => write!(f, "Info"),
            MessageType::Note => write!(f, "Note"),
            MessageType::Warning => write!(f, "Warning"),
            MessageType::Error => write!(f, "Error"),
        }
    }
}

static LOG_VERBOSITY: Lazy<Mutex<MessageType>> = Lazy::new(|| Mutex::new(MessageType::default()));

pub fn set_log_verbosity(level: MessageType) {
    let mut verbosity = LOG_VERBOSITY.lock().unwrap();
    *verbosity = level;
}

pub fn get_log_verbosity() -> MessageType {
    let verbosity = LOG_VERBOSITY.lock().unwrap();
    *verbosity
}

fn print_message_with_ts(message: &str, message_type: MessageType) {
    let datetime_now: DateTime<Local> = Local::now();
    let date_formatted = datetime_now.format("%Y-%m-%d %H:%M:%S");
    let pid = process::id();
    match message_type {
        MessageType::Info => {
            if MessageType::Info >= get_log_verbosity() {
                println!("{} [INFO] Readyset[{}]: {}", date_formatted, pid, message);
            }
        }
        MessageType::Note => {
            if MessageType::Note >= get_log_verbosity() {
                println!("{} [NOTE] Readyset[{}]: {}", date_formatted, pid, message);
            }
        }
        MessageType::Warning => {
            if MessageType::Warning >= get_log_verbosity() {
                eprintln!(
                    "{} [WARNING] Readyset[{}]: {}",
                    date_formatted, pid, message
                );
            }
        }
        MessageType::Error => {
            if MessageType::Error >= get_log_verbosity() {
                eprintln!("{} [ERROR] Readyset[{}]: {}", date_formatted, pid, message);
            }
        }
    }
}

pub fn print_info(message: &str) {
    print_message_with_ts(message, MessageType::Info);
}

pub fn print_note(message: &str) {
    print_message_with_ts(message, MessageType::Note);
}

pub fn print_warning(message: &str) {
    print_message_with_ts(message, MessageType::Warning);
}

pub fn print_error(message: &str) {
    print_message_with_ts(message, MessageType::Error);
}
