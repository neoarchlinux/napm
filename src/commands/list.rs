use anyhow::Result;

use crate::napm::Napm;

pub fn run(napm: &Napm) -> Result<()> {
    for pkg in napm.list() {
        println!("{} {}", pkg.name, pkg.version);
    }

    Ok(())
}
