use alpm::{Alpm, Package};

use crate::error::{Error, Result};
use crate::ansi::*;

#[derive(Debug, Clone)]
pub struct Pkg {
    pub name: String,
    pub version: String,
    pub db_name: String,
    pub desc: String,
}

impl Pkg {
    pub fn into_package_ref(self, handle: &Alpm) -> Result<&Package> {
        let expect_msg = format!(
            "package '{}' not found in '{}'",
            self.name, self.db_name
        );

        if self.db_name == "local" {
            handle
                .localdb()
                .pkg(self.name)

        } else {
            handle
                .syncdbs()
                .iter()
                .find(|db| *db.name() == self.db_name)
                .expect(&expect_msg)
                .pkg(self.name)
        }.map_err(|_| Error::FindPkg)
    }

    pub fn format_name(name: &str) -> String {
        format!(
            "{ANSI_CYAN}{}{ANSI_RESET}",
            name,
        )
    }

    pub fn formatted_name(&self) -> String {
        Self::format_name(&self.name)
    }
}

impl From<&Package> for Pkg {
    fn from(package: &Package) -> Self {
        Self {
            name: package.name().to_string(),
            version: package.version().to_string(),
            db_name: package
                .db()
                .map(|db| db.name())
                .unwrap_or("local")
                .to_string(),
            desc: package.desc().unwrap_or("").to_string(),
        }
    }
}