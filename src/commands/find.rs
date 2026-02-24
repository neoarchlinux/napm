use crate::ansi::*;
use crate::error::{Error, Result};
use crate::napm::Napm;

pub fn run(napm: &mut Napm, path: String, exact: bool) -> Result<()> {
    let results = napm.find(path, exact)?;

    if results.is_empty() {
        return Err(Error::NoResults);
    }

    for (pkg, path) in results {
        println!(
            "{}: {ANSI_BLUE}{}{ANSI_RESET}",
            pkg.formatted_name(false),
            path
        );
    }

    Ok(())
}
