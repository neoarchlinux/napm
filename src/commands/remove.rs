use anyhow::Result;

use crate::napm::Napm;

pub fn run(napm: &mut Napm, pkgs: &[&str], deep: bool) -> Result<()> {
    napm.remove(pkgs, deep)
}
