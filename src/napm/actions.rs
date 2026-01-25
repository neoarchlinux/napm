use alpm::TransFlag;
use std::fs;

use crate::napm::*;
use crate::{log_info, log_warn, log_fatal};
use crate::util::require_root;

impl Napm {
    pub fn install_pkgs(&mut self, pkgs: &[Pkg]) -> Result<()> {
        require_root()?;

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
        require_root()?;

        self.update()?;

        self.trans_init(TransFlag::NONE)?;

        self.h_mut().sync_sysupgrade(false)?;

        self.trans_prepare()?;

        self.trans_commit()
    }

    pub fn remove_pkgs(&mut self, pkgs: &[Pkg], deep: bool) -> Result<()> {
        require_root()?;

        self.trans_init(if deep {
            TransFlag::RECURSE | /* TransFlag::CASCADE | */ TransFlag::NO_SAVE
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

    pub fn search(&self, needles: &[&str]) -> Result<Vec<Pkg>> {
        let mut out = Vec::new();

        for db in self.h().syncdbs() {
            out.extend(db.search(needles.iter())?);
        }

        Ok(out.into_iter().map(Pkg::from).collect())
    }

    pub fn find(&mut self, mut file: String, exact: bool, mut fetch: bool) -> Result<Vec<(Pkg, String)>> {
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
        
        let cache_dir = self.file_cache_dir();

        if !cache_dir.exists() {
            log_info!("File listing not found, fetching");
            fetch = true;
        }

        if fetch {
            self.update_file_listing_cache()?;
        }

        let mut out = Vec::new();

        for db_entry in fs::read_dir(&cache_dir)? {
            let db_entry = db_entry?;
            let db_cache_dir = db_entry.path();

            if !db_cache_dir.is_dir() {
                continue;
            }

            let db_name = db_entry.file_name().to_string_lossy().to_string();

            for pkg_entry in fs::read_dir(&db_cache_dir)? {
                let pkg_entry = pkg_entry?;
                let pkg_path = pkg_entry.path();

                if !pkg_path.is_dir() {
                    continue;
                }

                let desc_path = pkg_path.join("desc");

                let mut pkg_name = String::new();
                let mut pkg_version = String::new();
                let mut pkg_desc = String::new();

                if desc_path.exists() {
                    let content = fs::read_to_string(&desc_path)?;
                    let mut current_key: Option<&str> = None;

                    for line in content.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        if line.starts_with('%') && line.ends_with('%') {
                            current_key = Some(line.trim_matches('%'));
                            continue;
                        }

                        match current_key {
                            Some("NAME") => pkg_name = line.to_string(),
                            Some("VERSION") => pkg_version = line.to_string(),
                            Some("DESC") => {
                                if pkg_desc.is_empty() {
                                    pkg_desc = line.to_string();
                                } else {
                                    pkg_desc.push(' ');
                                    pkg_desc.push_str(line);
                                }
                            }
                            _ => {}
                        }
                    }
                } else {
                    log_warn!("File {} does not exist, skipping", desc_path.display());
                    continue;
                }

                let files_path = pkg_path.join("files");
                if !files_path.exists() {
                    continue;
                }

                let files_content = fs::read_to_string(&files_path)?;
                for line in files_content.lines() {
                    if line.starts_with('%') || line.trim().is_empty() {
                        continue;
                    }

                    let line = format!("/{line}");

                    if {
                        if exact {
                            line == file
                        } else {
                            line.ends_with(&file)
                        }
                    } {
                        out.push((
                            Pkg {
                                name: pkg_name.clone(),
                                version: pkg_version.clone(),
                                db_name: db_name.clone(),
                                desc: pkg_desc.clone(),
                            },
                            line.to_string(),
                        ));
                    }
                }
            }
        }

        Ok(out)
    }

    pub fn info(&self, name: &str) -> Result<Pkg> {
        let local_pkg = self.h().localdb().pkg(name);

        if let Ok(pkg) = local_pkg {
            return Ok(Pkg::from(pkg));
        }

        unimplemented!("non-local info");
    }

    pub fn list(&self) -> Vec<Pkg> {
        self.h()
            .localdb()
            .pkgs()
            .into_iter()
            .map(Pkg::from)
            .collect()
    }

    pub fn files(&self, name: &str) -> Result<Vec<String>> {
        let local_pkg = self.h().localdb().pkg(name);

        if let Ok(pkg) = local_pkg {
            return Ok(pkg
                .files()
                .files()
                .iter()
                .map(|f| String::from_utf8(f.name().into()).unwrap())
                .collect());
        }

        panic!("Napm.files called with non-local package");
    }
}