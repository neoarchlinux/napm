use crate::error::Result;
use crate::napm::Napm;

pub fn run(napm: &Napm, pkg: &str) -> Result<()> {
    let p = napm.info(pkg)?;

    println!("Name          : {}", p.name);
    println!("Version       : {}", p.version);
    println!("Description   : {}", p.desc);

    Ok(())
}
