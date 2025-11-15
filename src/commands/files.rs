use anyhow::Result;

use crate::napm::Napm;

pub fn run(napm: &Napm, pkg: &str) -> Result<()> {
    for f in napm.files(pkg)? {
        println!("{}", f);
    }

    Ok(())
}
