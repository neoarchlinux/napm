use clap::{Parser, Subcommand};

pub mod ansi;
pub mod error;
pub mod log;
pub mod napm;
pub mod pkg;
pub mod util;

pub mod commands {
    pub mod files;
    pub mod find;
    pub mod info;
    pub mod install;
    pub mod list;
    pub mod remove;
    pub mod search;
    pub mod update;
    pub mod upgrade;
}

use error::{Error, Result};
use napm::Napm;

#[derive(Parser)]
#[command(name = "napm")]
#[command(about = "napm - NeoArch Package Manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "List the files of a package")]
    Files {
        package: String,

        #[arg(long, short, default_value_t = false, help = "Show directories too")]
        dirs: bool,
    },

    #[command(about = "Find packages that contain a specific file")]
    Find {
        path: String,

        #[arg(
            long,
            default_value_t = false,
            help = "Only match exact paths (e.g. /bin/sudo)"
        )]
        exact: bool,
    },

    #[command(about = "Show package information")]
    Info { package: String },

    #[command(about = "Install packages")]
    Install { packages: Vec<String> },

    #[command(about = "List installed packages")]
    List,

    #[command(about = "Remove a package")]
    Remove {
        packages: Vec<String>,

        #[arg(
            long,
            default_value_t = false,
            help = "Do not remove dependencies (not recommended)"
        )]
        no_deep: bool,
    },

    #[command(about = "Search for a package by name or description")]
    Search {
        search_terms: Vec<String>,

        #[arg(long, short)]
        num_results: Option<u32>,
    },

    #[command(about = "Update the package metadata, NOTE: this is not a system upgrade !!!")]
    Update {
        #[arg(
            long,
            default_value_t = false,
            help = "Do not update the file cache (just the package database)"
        )]
        no_file_cache: bool,
    },

    #[command(about = "Upgrade all packages on the system")]
    Upgrade,
}

#[derive(Subcommand)]
enum CacheSubcommand {
    Update,
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let mut napm = Napm::new()?;

    match cli.command {
        Commands::Update { no_file_cache } => commands::update::run(&mut napm, no_file_cache),
        Commands::Files { package, dirs } => commands::files::run(&mut napm, &package, dirs),
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
        Commands::Find { path, exact } => commands::find::run(&mut napm, path, exact),
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
            search_terms,
            num_results,
        } => commands::search::run(&napm, search_terms, num_results),
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
    }
}
