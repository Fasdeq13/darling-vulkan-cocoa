#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

pub fn log(level: LogLevel, message: &str) {
    match level {
        LogLevel::Info => println!("[INFO] {message}"),
        LogLevel::Warning => eprintln!("[WARNING] {message}"),
        LogLevel::Error => eprintln!("[ERROR] {message}"),
    }
}
