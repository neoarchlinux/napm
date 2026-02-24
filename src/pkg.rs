use alpm::{Alpm, Package};

use crate::ansi::*;
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct Pkg {
    pub name: String,
    pub version: String,
    pub repo: String,
    pub desc: String,
}

impl Pkg {
    pub fn into_package_ref(self, handle: &Alpm) -> Result<&Package> {
        let expect_msg = format!("package '{}' not found in '{}'", self.name, self.repo);

        if self.repo == "local" {
            handle.localdb().pkg(self.name)
        } else {
            handle
                .syncdbs()
                .iter()
                .find(|db| *db.name() == self.repo)
                .expect(&expect_msg)
                .pkg(self.name)
        }
        .map_err(|_| Error::FindPkg)
    }

    pub fn format_name(name: &str, version: Option<&str>) -> String {
        if let Some(v) = version {
            format!(
                "{ANSI_CYAN}{}{ANSI_RESET}-{ANSI_MAGENTA}{}{ANSI_RESET}",
                name, v
            )
        } else {
            format!("{ANSI_CYAN}{}{ANSI_RESET}", name,)
        }
    }

    pub fn formatted_name(&self, with_version: bool) -> String {
        Self::format_name(
            &self.name,
            if with_version {
                Some(&self.version)
            } else {
                None
            },
        )
    }
}

impl From<&Package> for Pkg {
    fn from(package: &Package) -> Self {
        Self {
            name: package.name().to_string(),
            version: package.version().to_string(),
            repo: package
                .db()
                .map(|db| db.name())
                .unwrap_or("local")
                .to_string(),
            desc: package.desc().unwrap_or("").to_string(),
        }
    }
}
