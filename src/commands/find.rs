use crate::error::{Error, Result};
use crate::ansi::*;
use crate::napm::Napm;

pub fn run(napm: &mut Napm, file: String, exact: bool, fetch: bool) -> Result<()> {
    let results = napm.find(file, exact, fetch)?;
    
    if results.is_empty() {
        return Err(Error::NoResults);
    }

    for (pkg, path) in results {
        println!(
            "{ANSI_CYAN}{}{ANSI_WHITE}/{ANSI_MAGENTA}{}{ANSI_WHITE}: {ANSI_BLUE}{}{ANSI_RESET}",
            pkg.db_name, pkg.name, path
        );
    }

    Ok(())
}
