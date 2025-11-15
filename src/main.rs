use clap::{Parser, Subcommand};

pub mod ansi;
pub mod napm;

pub mod commands {
    pub mod files;
    pub mod info;
    pub mod install;
    pub mod list;
    pub mod query;
    pub mod remove;
    pub mod search;
    pub mod update;
}

use napm::Napm;

#[derive(Parser)]
#[command(name = "napm")]
#[command(about = "NeoArch Package Manager")]
struct Cli {
    #[arg(long, global = true)]
    root: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Files {
        package: String,
    },
    Info {
        package: String,
    },
    Install {
        packages: Vec<String>,
        #[arg(long, default_value_t = false)]
        no_sync: bool,
    },
    List,
    Query {
        file: String,
        #[arg(long, default_value_t = false)]
        fetch: bool,
    },
    Remove {
        packages: Vec<String>,
        #[arg(long, default_value_t = false)]
        no_deep: bool,
    },
    Search {
        package: String,
        #[arg(long, default_value_t = false)]
        no_sync: bool,
        #[arg(long, short)]
        num_results: Option<u32>,
    },
    Update,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut napm = Napm::new(&cli.root.unwrap_or("/".to_string()))?;

    match cli.command {
        Commands::Files { package } => commands::files::run(&napm, &package),
        Commands::Info { package } => commands::info::run(&napm, &package),
        Commands::Install { packages, no_sync } => commands::install::run(
            &mut napm,
            packages
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .as_slice(),
            !no_sync,
        ),
        Commands::List => commands::list::run(&napm),
        Commands::Query { file, fetch } => commands::query::run(&mut napm, &file, fetch),
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
            no_sync,
            num_results,
        } => commands::search::run(&mut napm, &package, !no_sync, num_results),
        Commands::Update => commands::update::run(&mut napm),
    }?;

    Ok(())
}
