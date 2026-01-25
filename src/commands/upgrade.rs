use crate::error::Result;
use crate::napm::Napm;

pub fn run(napm: &mut Napm) -> Result<()> {
    napm.upgrade()
}
