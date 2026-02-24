use crate::error::Result;
use crate::napm::Napm;
use crate::util::require_root;

pub fn run(napm: &mut Napm, no_file_cache: bool) -> Result<()> {
    require_root()?;

    napm.update(".db")?;

    if !no_file_cache {
        napm.update(".files")?;
        napm.update_cache()?;
    }

    Ok(())
}
