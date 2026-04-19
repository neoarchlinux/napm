use crate::error::Result;
use crate::napm::Napm;
use crate::util::require_root;

pub fn run(napm: &mut Napm, files: bool) -> Result<()> {
    require_root()?;

    if files {
        napm.update(".files")?;
        napm.update_cache()?;
    } else {
        napm.update(".db")?;
    }

    Ok(())
}
