use clap::{Parser, Subcommand};

pub mod ansi;
pub mod log;
pub mod error;
pub mod pkg;
pub mod util;
pub mod napm;

pub mod commands {
    pub mod files;
    pub mod info;
    pub mod install;
    pub mod list;
    pub mod find;
    pub mod remove;
    pub mod search;
    pub mod upgrade;
}

use error::{Error, Result};
use napm::{ConfigOverride, Napm};

#[derive(Parser)]
#[command(name = "napm")]
#[command(about = "napm - NeoArch Package Manager")]
struct Cli {
    #[arg(long, help = "Set an alternate system root")]
    root: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Display the current config")]
    Config,

    #[command(about = "List the files of a package")]
    Files {
        package: String,
    },

    #[command(about = "Show package information")]
    Info {
        package: String,
    },

    #[command(about = "Install packages")]
    Install {
        packages: Vec<String>,
    },

    #[command(about = "List installed packages")]
    List,

    #[command(about = "Find packages that contain a specific file")]
    Find {
        file: String,

        #[arg(long, default_value_t = false, help = "Only match exact paths, e.g. /usr/bin/sudo")]
        exact: bool,

        #[arg(long, default_value_t = false, help = "Force fetch the files databases")]
        fetch: bool,
    },

    #[command(about = "Remove a package")]
    Remove {
        packages: Vec<String>,

        #[arg(long, default_value_t = false, help = "Do not remove dependencies (not recommended)")]
        no_deep: bool,
    },

    #[command(about = "Search for a package by name or description")]
    Search {
        package: String,

        #[arg(long, short)]
        num_results: Option<u32>,
    },

    #[command(about = "Upgrade all packages on the system")]
    Upgrade,
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let cfg_override = ConfigOverride { root: cli.root };

    if let Commands::Config = cli.command {
        println!("{:#?}", Napm::cfg(cfg_override)?);
        return Ok(());
    }

    let mut napm = Napm::new(cfg_override)?;

    match cli.command {
        Commands::Config => panic!(),
        Commands::Files { package } => commands::files::run(&mut napm, &package),
        Commands::Info { package } => commands::info::run(&napm, &package),
        Commands::Install { packages } => commands::install::run(
            &mut napm,
            packages
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .as_slice(),
        ),
        Commands::List => commands::list::run(&napm),
        Commands::Find { file, exact, fetch } => commands::find::run(&mut napm, file, exact, fetch),
        Commands::Remove { packages, no_deep } => commands::remove::run(
            &mut napm,
            packages
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .as_slice(),
            !no_deep,
        ),
        Commands::Search {
            package,
            num_results,
        } => commands::search::run(&mut napm, &package, num_results),
        Commands::Upgrade => commands::upgrade::run(&mut napm),
    }?;

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        if let Error::NothingToDo = err {
            log_info!("Nothing to do");
        } else {
            log_fatal!("{}", err);
            std::process::exit(1)
        }
    } else {
        log_info!("Done");
    }
}