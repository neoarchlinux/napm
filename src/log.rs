#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!("{ANSI_BLUE}{ANSI_BOLD}D{ANSI_RESET}: {}", format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!("{ANSI_GREEN}{ANSI_BOLD}I{ANSI_RESET}: {}", format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!("{ANSI_YELLOW}{ANSI_BOLD}W{ANSI_RESET}: {}", format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!("{ANSI_RED}{ANSI_BOLD}E{ANSI_RESET}: {}", format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! log_fatal {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!("{ANSI_MAGENTA}{ANSI_BOLD}F{ANSI_RESET}: {}", format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! format_action_required {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        format!("{ANSI_BOLD}ACT{ANSI_RESET}: {}", format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! log_action_required {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!(format_action_required!($($arg)*));
    }};
}

#[macro_export]
macro_rules! log_repair {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!("[{ANSI_CYAN}AUTO REPAIR{ANSI_RESET}] {}", format!($($arg)*));
    }};
}
