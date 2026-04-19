use std::path::Path;

use alpm::TransFlag;

use crate::util::run_upgrade;
use crate::{log_action_required, napm::*};
use crate::{log_fatal, log_info, log_warn};

impl Napm {
    pub fn install_pkgs(&mut self, pkgs: &[Pkg]) -> Result<()> {
        let mut to_install = pkgs.to_vec();

        match self.init_system() {
            Ok(init_system) => {
                let init_system_dependent_names = pkgs
                    .iter()
                    .map(|pkg| format!("{}-{}", &pkg.name, init_system.as_str()))
                    .collect::<Vec<_>>();

                let init_system_dependent = self
                    .pkgs(
                        init_system_dependent_names
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .as_slice(),
                    )
                    .into_iter()
                    .filter_map(|r| r.ok())
                    .collect::<Vec<_>>();

                if !init_system_dependent.is_empty() {
                    log_action_required!(
                        "There are some additional packages you may want to install"
                    );

                    for pkg in init_system_dependent {
                        if confirm(
                            &format!(
                                "Do you want to install {} - {ANSI_YELLOW}{}{ANSI_RESET}",
                                pkg.formatted_name(false),
                                pkg.desc
                            ),
                            true,
                        )? {
                            to_install.push(pkg);
                        }
                    }
                }
            }
            Err(Error::NoInitSystem) => {}
            Err(e) => return Err(e),
        }

        let result = self.install_pkgs_attempt(&to_install);

        if let Err(Error::UpgradeRequired) = &result {
            log_warn!("Stale database detected, update and upgrade required");

            let lock_path = self.h().lockfile();
            let _ = std::fs::remove_file(lock_path);

            let sync_path = Path::new(self.h().dbpath()).join("sync");
            run_upgrade(&sync_path)?;

            std::fs::File::create(std::path::Path::new(&lock_path))?;

            self.reset()?;

            return self.install_pkgs_attempt(pkgs);
        }

        result
    }

    fn install_pkgs_attempt(&mut self, pkgs: &[Pkg]) -> Result<()> {
        log_info!(
            "Installing {} with all {} dependencies",
            pkgs.iter()
                .map(|pkg| pkg.formatted_name(true))
                .collect::<Vec<_>>()
                .join(", "),
            if pkgs.len() == 1 { "its" } else { "their" }
        );

        {
            let handle = self.handle.take().unwrap();

            let conflicts = handle.check_conflicts(
                pkgs.iter()
                    .map(|pkg| pkg.clone().into_package_ref(&handle))
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .iter(),
            );

            self.handle = Some(handle);

            if !conflicts.is_empty() {
                log_fatal!("Conflicts occured");
                for c in conflicts {
                    log_fatal!(" - {c:?}");
                }
                return Err(Error::Conflicts);
            }
        }

        self.trans_init(TransFlag::NONE)?;

        {
            let handle = self.handle.take().unwrap();

            for pkg in pkgs {
                let package = pkg.clone().into_package_ref(&handle)?;
                handle
                    .trans_add_pkg(package)
                    .map_err(|_| Error::TransAddPkg)?;
            }

            self.handle = Some(handle);
        }

        self.trans_prepare()?;

        self.trans_commit()?;

        Ok(())
    }

    pub fn upgrade(&mut self) -> Result<()> {
        log_info!("Upgrading the system");

        // TODO: list upgradable packages and maybe ask for confimration

        self.trans_init(TransFlag::NONE)?;

        self.h_mut().sync_sysupgrade(false)?;

        self.trans_prepare()?;

        self.trans_commit()
    }

    pub fn remove_pkgs(&mut self, pkgs: &[Pkg], deep: bool) -> Result<()> {
        log_info!(
            "Removing {}{}",
            pkgs.iter()
                .map(|pkg| pkg.formatted_name(true))
                .collect::<Vec<_>>()
                .join(", "),
            if deep {
                format!(
                    " with all {} dependencies",
                    if pkgs.len() == 1 { "its" } else { "their" }
                )
            } else {
                "".to_string()
            }
        );

        self.trans_init(if deep {
            TransFlag::RECURSE | TransFlag::CASCADE | TransFlag::NO_SAVE
        } else {
            TransFlag::NONE
        })?;

        {
            let handle = self.handle.take().unwrap();

            for pkg in pkgs {
                let package = pkg.clone().into_package_ref(&handle)?;
                handle
                    .trans_remove_pkg(package)
                    .map_err(|_| Error::TransRemovePkg)?;
            }

            self.handle = Some(handle);
        }

        self.trans_prepare()?;

        self.trans_commit()?;

        Ok(())
    }

    pub fn find(&mut self, mut file: String, exact: bool) -> Result<Vec<(Pkg, String)>> {
        file = if file.starts_with("/") {
            file.to_owned()
        } else {
            format!("/{file}")
        };

        if exact {
            for part in ["bin", "lib", "lib64", "sbin"] {
                if file.starts_with(&format!("/{part}/")) {
                    file = format!("/usr{file}");
                    log_warn!("/{part} is a symlink, finding {file} instead");
                    break;
                }
            }
        }

        self.find_packages_by_file(&file, exact)
    }

    pub fn list(&self) -> Vec<Pkg> {
        self.h()
            .localdb()
            .pkgs()
            .into_iter()
            .map(Pkg::from)
            .collect()
    }
}
