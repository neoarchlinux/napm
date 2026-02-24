use alpm::{Alpm, SigLevel};

use crate::napm::*;

impl Napm {
    pub fn h(&self) -> &Alpm {
        self.handle.as_ref().unwrap()
    }

    pub fn h_mut(&mut self) -> &mut Alpm {
        self.handle.as_mut().unwrap()
    }

    pub fn local_pkg(&self, name: &str) -> Result<Pkg> {
        match self.h().localdb().pkg(name) {
            Ok(pkg) => Ok(Pkg::from(pkg)),
            Err(_) => Err(Error::PackageNotInLocalDb(name.to_string())),
        }
    }

    pub fn local_pkgs(&self, names: &[&str]) -> Vec<Result<Pkg>> {
        names.iter().map(|name| self.local_pkg(name)).collect()
    }

    pub fn pkg(&self, name: &str) -> Result<Pkg> {
        for db in self.h().syncdbs() {
            match db.pkg(name) {
                Ok(pkg) => {
                    return Ok(Pkg::from(pkg));
                }
                Err(_) => continue,
            }
        }

        Err(Error::PackageNotFound(name.to_string()))
    }

    pub fn pkgs(&self, names: &[&str]) -> Vec<Result<Pkg>> {
        names.iter().map(|name| self.pkg(name)).collect()
    }

    pub fn parse_siglevel(values: &[String]) -> Result<SigLevel> {
        let mut level = SigLevel::empty();

        for v in values {
            match v.as_str() {
                "PackageNever" => {
                    level.remove(SigLevel::PACKAGE);
                    level.remove(SigLevel::PACKAGE_OPTIONAL);
                    level.remove(SigLevel::PACKAGE_MARGINAL_OK);
                    level.remove(SigLevel::PACKAGE_UNKNOWN_OK);
                }
                "PackageOptional" => {
                    level |= SigLevel::PACKAGE_OPTIONAL;
                }
                "PackageRequired" => {
                    level |= SigLevel::PACKAGE;
                }
                "PackageTrustedOnly" => {
                    level |= SigLevel::PACKAGE;
                    level.remove(SigLevel::PACKAGE_MARGINAL_OK);
                    level.remove(SigLevel::PACKAGE_UNKNOWN_OK);
                }

                "DatabaseNever" => {
                    level.remove(SigLevel::DATABASE);
                    level.remove(SigLevel::DATABASE_OPTIONAL);
                    level.remove(SigLevel::DATABASE_MARGINAL_OK);
                    level.remove(SigLevel::DATABASE_UNKNOWN_OK);
                }
                "DatabaseOptional" => {
                    level |= SigLevel::DATABASE_OPTIONAL;
                }
                "DatabaseRequired" => {
                    level |= SigLevel::DATABASE;
                }
                "DatabaseTrustedOnly" => {
                    level |= SigLevel::DATABASE;
                    level.remove(SigLevel::DATABASE_MARGINAL_OK);
                    level.remove(SigLevel::DATABASE_UNKNOWN_OK);
                }

                "UseDefault" => {
                    level |= SigLevel::USE_DEFAULT;
                }

                other => {
                    return Err(Error::SigLevelParse(other.to_string()));
                }
            }
        }

        Ok(level)
    }
}
