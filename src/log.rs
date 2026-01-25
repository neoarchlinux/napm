#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!("[{ANSI_BLUE}DEBUG{ANSI_RESET}] {}", format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!("[{ANSI_GREEN}INFO{ANSI_RESET}] {}", format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!("[{ANSI_YELLOW}WARN{ANSI_RESET}] {}", format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!("[{ANSI_RED}ERROR{ANSI_RESET}] {}", format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! log_fatal {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!("[{ANSI_RED}{ANSI_BOLD}FATAL ERROR{ANSI_RESET}] {}", format!($($arg)*));
    }};
}

