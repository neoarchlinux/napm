use std::collections::HashMap;
use std::env;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::ansi::*;
use crate::error::{Error, Result};
use crate::napm::cache::NAPM_CACHE_FILE;
use crate::{format_action_required, log_error, log_info, log_warn};

pub fn confirm(prompt: &str, default_yes: bool) -> Result<bool> {
    use std::io::{self, Write};

    loop {
        eprint!(
            "{}",
            format_action_required!("{} [{}]: ", prompt, if default_yes { "Y/n" } else { "y/N" })
        );
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

        eprint!(
            "{}",
            format_action_required!("Your choice (default = {}): ", default)
        );
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
                    log_error!(
                        "Invalid option '{input}', you must choose a number between 0 and {}",
                        options.len() - 1
                    );
                    continue;
                }
            }
        };

        return Ok(choice);
    }
}

pub const PE_TOOLS: &[&str] = &["sudo", "doas", "pkexec"];

fn detect_pe_program() -> Result<String> {
    for candidate in PE_TOOLS {
        if which(candidate) {
            return Ok(candidate.to_string());
        }
    }

    Err(Error::NoPETool)
}

pub const SHELLS: &[&str] = &["bash"]; // TODO: zsh, fish, etc.

fn detect_shell() -> Result<String> {
    for candidate in SHELLS {
        if which(candidate) {
            return Ok(candidate.to_string());
        }
    }

    Err(Error::NoShell)
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

pub fn is_root() -> bool {
    nix::unistd::Uid::effective().is_root()
}

pub fn current_exe() -> String {
    env::args().next().unwrap_or("napm".to_string())
}

pub fn current_args() -> Vec<String> {
    env::args().skip(1).collect()
}

fn napm_as_root_cmd(args: Vec<String>) -> Result<(Command, String)> {
    let cmd: &str = &current_exe();

    let mut command = if is_root() {
        Command::new(cmd)
    } else {
        Command::new(detect_pe_program()?)
    };

    let envs = {
        let mut vars = HashMap::new();

        for k in ["RUST_BACKTRACE"] {
            if let Ok(v) = env::var(k) {
                vars.insert(k, v);
            }
        }

        vars
    };

    let safe_arg = |a: &str| {
        if a.chars().all(|c| {
            { "abcdefghijklmonpqrstuvwxyzABCDEFGHIJKLMONPQRSTUVWXYZ0123456789-_/." }.contains(c)
        }) {
            a.to_string()
        } else {
            format!("\"{a}\"")
        }
    };

    let envs_str = if envs.is_empty() {
        "".to_string()
    } else {
        format!(
            "{} ",
            envs.iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };

    let args_str = args
        .iter()
        .map(|a| safe_arg(a))
        .collect::<Vec<_>>()
        .join(" ");

    if is_root() {
        command.envs(envs);
        command.args(args);
    } else {
        if !envs.is_empty() {
            match detect_pe_program()?.as_str() {
                "sudo" => {
                    for (k, v) in envs.iter() {
                        command.arg(format!("{k}={v}"));
                    }

                    command.arg(cmd);

                    command.args(args);
                }
                "doas" | "pkexec" => {
                    let shell = detect_shell()?;

                    if shell == "bash" {
                        // TODO: match when more shells
                        command.arg(shell);
                        command.arg("-c");
                        command.arg(&format!("{envs_str}{args_str}"));
                    } else {
                        unimplemented!("Unhandled shell: {shell}");
                    }
                }
                other_pe_program => unimplemented!("Unhandled PE program: {other_pe_program}"),
            }
        } else {
            command.arg(cmd);
            command.args(args);
        }
    }

    let cmd_display = if is_root() {
        format!("{}{} {}", envs_str, safe_arg(cmd), args_str)
    } else {
        format!(
            "{} {}{} {}",
            detect_pe_program()?,
            envs_str,
            safe_arg(cmd),
            args_str
        )
    };

    Ok((command, cmd_display))
}

pub fn require_root() -> Result<()> {
    if is_root() {
        return Ok(());
    }

    let (mut cmd, cmd_display) = napm_as_root_cmd(current_args())?;

    if is_root() {
        log_info!("# {}", cmd_display);
    } else {
        log_warn!(
            "You cannot perform this command without {ANSI_YELLOW}root priviledges{ANSI_RESET}"
        );

        let prompt = format!(
            "Do you want to run {ANSI_YELLOW}{}{ANSI_RESET} automatically?",
            cmd_display
        );

        if !confirm(&prompt, true)? {
            return Err(Error::DeniedPE(cmd_display));
        }

        log_info!("$ {}", cmd_display);
    }

    Err(cmd.exec().into())
}

pub fn run_cache_update() -> Result<()> {
    let (mut cmd, cmd_display) = napm_as_root_cmd(vec!["update".to_string()])?;

    if is_root() {
        log_warn!("System needs to be updated");

        let prompt = format!(
            "Do you want to run {ANSI_YELLOW}{}{ANSI_RESET} automatically?",
            cmd_display
        );

        if !confirm(&prompt, true)? {
            return Err(Error::DeniedPE(cmd_display));
        }

        log_info!("# {}", cmd_display);
    } else {
        log_warn!(
            "System needs to be updated and you need {ANSI_YELLOW}root priviledges{ANSI_RESET} for that"
        );

        let prompt = format!(
            "Do you want to run {ANSI_YELLOW}{}{ANSI_RESET} automatically?",
            cmd_display
        );

        if !confirm(&prompt, true)? {
            return Err(Error::DeniedPE(cmd_display));
        }

        log_info!("$ {}", cmd_display);
    }

    match cmd.spawn()?.wait() {
        Ok(status) => {
            if status.success() {
                Ok(())
            } else {
                Err(Error::System)
            }
        }
        Err(err) => Err(Error::InternalIO(err)),
    }
}

pub fn require_cache() -> Result<()> {
    let cache_path = Path::new(NAPM_CACHE_FILE);

    if cache_path.exists() {
        return Ok(());
    }

    run_cache_update()
}

pub fn run_upgrade() -> Result<()> {
    let (mut cmd_ud, cmd_ud_display) =
        napm_as_root_cmd(vec!["update".to_string(), "--no-file-cache".to_string()])?;
    let (mut cmd_ug, cmd_ug_display) = napm_as_root_cmd(vec!["upgrade".to_string()])?;

    if is_root() {
        log_warn!("System needs to be updated and upgraded");

        let prompt = format!(
            "Do you want to run {ANSI_YELLOW}{}{ANSI_RESET} and {ANSI_YELLOW}{}{ANSI_RESET} automatically?",
            cmd_ud_display, cmd_ug_display
        );

        if !confirm(&prompt, true)? {
            return Err(Error::DeniedPE(format!(
                "{}{ANSI_RESET}, {ANSI_YELLOW}{}",
                cmd_ud_display, cmd_ud_display
            )));
        }
    } else {
        log_warn!(
            "System needs to be updated and upgraded and you need {ANSI_YELLOW}root priviledges{ANSI_RESET} for that"
        );

        let prompt = format!(
            "Do you want to run {ANSI_YELLOW}{}{ANSI_RESET} and {ANSI_YELLOW}{}{ANSI_RESET} automatically?",
            cmd_ud_display, cmd_ug_display
        );

        if !confirm(&prompt, true)? {
            return Err(Error::DeniedPE(format!(
                "{}{ANSI_RESET}, {ANSI_YELLOW}{}",
                cmd_ud_display, cmd_ud_display
            )));
        }
    }

    log_info!("{} {}", if is_root() { "#" } else { "$" }, cmd_ud_display);

    match cmd_ud.spawn()?.wait() {
        Ok(status) => {
            if status.success() {
                Ok(())
            } else {
                Err(Error::System)
            }
        }
        Err(err) => Err(Error::InternalIO(err)),
    }?;

    log_info!("{} {}", if is_root() { "#" } else { "$" }, cmd_ug_display);

    match cmd_ug.spawn()?.wait() {
        Ok(status) => {
            if status.success() {
                Ok(())
            } else {
                Err(Error::System)
            }
        }
        Err(err) => Err(Error::InternalIO(err)),
    }
}
