use crate::{
    error::{Error, Result},
    napm::Napm,
};

use strum::{EnumIter, IntoEnumIterator};

#[derive(Debug, EnumIter)]
pub enum InitSystem {
    Openrc,
    Systemd,
    Runit,
    S6,
    Dinit,
}

impl InitSystem {
    pub fn as_str(&self) -> &'static str {
        match self {
            InitSystem::Openrc => "openrc",
            InitSystem::Systemd => "systemd",
            InitSystem::Runit => "runit",
            InitSystem::S6 => "s6",
            InitSystem::Dinit => "dinit",
        }
    }
}

impl Napm {
    pub fn init_system(&self) -> Result<InitSystem> {
        for init_system in InitSystem::iter() {
            match self.local_pkg(init_system.as_str()) {
                Ok(_) => return Ok(init_system),
                Err(Error::PackageNotInLocalDb(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(Error::NoInitSystem)
    }
}
