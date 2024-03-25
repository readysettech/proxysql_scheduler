use std::process;

use chrono::{DateTime, Local};

enum MessageType {
    Info,
    Warning,
    Error,
}
fn print_message_with_ts(message: &str, message_type: MessageType) {
    let datetime_now: DateTime<Local> = Local::now();
    let date_formatted = datetime_now.format("%Y-%m-%d %H:%M:%S");
    let pid = process::id();
    match message_type {
        MessageType::Info => println!("{} [INFO] Readyset[{}]: {}", date_formatted, pid, message),
        MessageType::Warning => println!(
            "{} [WARNING] Readyset[{}]: {}",
            date_formatted, pid, message
        ),
        MessageType::Error => {
            eprintln!("{} [ERROR] Readyset[{}]: {}", date_formatted, pid, message)
        }
    }
}

pub fn print_info(message: &str) {
    print_message_with_ts(message, MessageType::Info);
}

pub fn print_warning(message: &str) {
    print_message_with_ts(message, MessageType::Warning);
}

pub fn print_error(message: &str) {
    print_message_with_ts(message, MessageType::Error);
}
