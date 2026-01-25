use std::env;
use std::path::Path;
use std::process::Command;
use std::os::unix::process::CommandExt;
use std::collections::HashMap;

use crate::ansi::*;
use crate::error::{Error, Result};
use crate::{log_info, log_warn, log_error};

pub fn confirm(prompt: &str, default_yes: bool) -> Result<bool> {
    use std::io::{self, Write};

    loop {
        eprint!("[{ANSI_BOLD}ACTION REQUIRED{ANSI_RESET}] {} [{}]: ", prompt, if default_yes { "Y/n" } else { "y/N" });
        io::stderr().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let lower = input.trim().to_string().to_lowercase();

        if lower.is_empty() {
            return Ok(default_yes);
        }

        if lower.starts_with("y") {
            return Ok(true);
        }

        if lower.starts_with("n") {
            return Ok(false);
        }
    }
}

pub fn choose(prompt: &str, options: &[String], default: i32) -> Result<i32> {
    use std::io::{self, Write};

    loop {
        log_warn!("{}", prompt);

        for (i, option) in options.iter().enumerate() {
            eprintln!(" - {ANSI_BOLD}{i}{ANSI_RESET}: {}", option);
        }

        eprint!("[{ANSI_BOLD}ACTION REQUIRED{ANSI_RESET}] Your choice (default = {}): ", default);
        io::stderr().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        input = input.trim().to_string();

        let choice: i32 = if input.is_empty() {
            default
        } else {
            match input.to_string().parse() {
                Ok(n) if n < options.len() as i32 => n,
                _ => {
                    log_error!("Invalid option '{input}', you must choose a number between 0 and {}", options.len() - 1);
                    continue;
                }
            }
        };

        return Ok(choice);
    }
}

pub const PE_TOOLS: &[&str] = &["sudo", "doas", "pkexec"];

fn detect_pe_program() -> Option<String> {
    for candidate in PE_TOOLS {
        if which(candidate) {
            return Some(candidate.to_string());
        }
    }
    None
}

fn which(cmd: &str) -> bool {
    if cmd.contains('/') {
        return Path::new(cmd).is_file();
    }

    if let Ok(path) = env::var("PATH") {
        for dir in path.split(':') {
            let full = Path::new(dir).join(cmd);
            if full.is_file() {
                return true;
            }
        }
    }

    false
}

pub fn require_root() -> Result<()> {
    if nix::unistd::Uid::effective().is_root() {
        return Ok(());
    }

    let pe_program = if let Some(program) = detect_pe_program() {
        program
    } else {
        return Err(Error::NoPETool);
    };

    let prompt = format!(
        "To run this command you must have root privileges, do you want to run it with {}?",
        pe_program
    );

    if !confirm(&prompt, true)? {
        return Err(Error::DeniedPE);
    }

    let env_vars = {
        let mut vars = HashMap::new();

        for k in ["RUST_BACKTRACE"] {
            if let Ok(v) = env::var(k) {
                vars.insert(k, v);
            }
        }

        vars
    };

    let exe = env::current_exe().expect("failed to get current executable");
    let args: Vec<String> = env::args().skip(1).collect();

    let mut cmd = Command::new(&pe_program);

    match pe_program.as_str() {
        "sudo" | "doas" | "pkexec" => {
            for (k, v) in env_vars.iter() {
                cmd.arg(format!("{k}={v}"));
            }

            cmd.arg(&exe);
        }
        _ => {
            unimplemented!("Unhandled privilege escalation program: {}", pe_program)
        }
    }

    cmd.args(&args);

    let safe_arg = |a: &str| if a.chars().all(|c| {
        "abcdefghijklmonpqrstuvwxyzABCDEFGHIJKLMONPQRSTUVWXYZ0123456789-_/."
    }.contains(c)) {
        a.to_string()
    } else {
        format!("\"{a}\"")
    };

    let args_display = args
        .iter()
        .map(|a| safe_arg(&a))
        .collect::<Vec<_>>()
        .join(" ");

    let exe = exe.into_os_string().into_string().unwrap();

    log_info!("$ {} {}{} {}",
        pe_program,
        if env_vars.is_empty() { "".to_string() } else { format!("{} ", env_vars.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" ")) },
        safe_arg(&exe),
        args_display
    );

    Err(cmd.exec().into())
}