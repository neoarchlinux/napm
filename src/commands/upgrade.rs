use crate::error::Result;
use crate::napm::Napm;
use crate::util::require_root;

pub fn run(napm: &mut Napm) -> Result<()> {
    require_root()?;

    napm.upgrade()
}
