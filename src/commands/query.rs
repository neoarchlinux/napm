use anyhow::Result;

use crate::ansi::*;
use crate::napm::Napm;

pub fn run(napm: &mut Napm, file: &str, fetch: bool) -> Result<()> {
    for (pkg, path) in napm.query(file, fetch)? {
        println!(
            "{ANSI_CYAN}{}{ANSI_WHITE}/{ANSI_MAGENTA}{}{ANSI_WHITE}: {ANSI_BLUE}{}{ANSI_RESET}",
            pkg.db_name, pkg.name, path
        );
    }

    Ok(())
}
