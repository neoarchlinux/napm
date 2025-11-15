use anyhow::Result;

use crate::napm::Napm;

pub fn run(napm: &mut Napm) -> Result<()> {
    if let Some(r) = napm.update() {
        r
    } else {
        println!("nothing to do");

        Ok(())
    }
}
