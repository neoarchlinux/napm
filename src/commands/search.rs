use crate::ansi::*;
use crate::error::Result;
use crate::napm::Napm;

pub fn run(napm: &Napm, search_terms: Vec<String>, num_results: Option<u32>) -> Result<()> {
    let results = napm.search(search_terms)?;

    let results = if let Some(n) = num_results {
        results.iter().take(n as usize).collect::<Vec<_>>()
    } else {
        results.iter().collect::<Vec<_>>()
    };

    for (i, pkg) in results.iter().enumerate().rev() {
        println!(
            " {ANSI_RED}-{ANSI_RESET} {ANSI_YELLOW}[{ANSI_BOLD}{}{ANSI_RESET}{ANSI_YELLOW}]{ANSI_RESET} {} {}",
            i + 1,
            pkg.formatted_name(true),
            pkg.desc,
        );
    }

    Ok(())
}
