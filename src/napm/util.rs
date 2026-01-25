use alpm::{
    Alpm, SigLevel,
};
use flate2::read::GzDecoder;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tar::Archive;

use crate::napm::*;
use crate::log_info;
use crate::util::require_root;

impl Napm {
    pub fn h(&self) -> &Alpm {
        self.handle.as_ref().unwrap()
    }

    pub fn h_mut(&mut self) -> &mut Alpm {
        self.handle.as_mut().unwrap()
    }

    pub fn file_cache_dir(&self) -> PathBuf {
        Path::new(self.h().root()).join("var/cache/pacman/files")
    }

    pub fn local_pkg(&self, name: &str) -> Result<Pkg> {
        match self.h().localdb().pkg(name) {
            Ok(pkg) => Ok(Pkg::from(pkg)),
            Err(_) => Err(Error::PackageNotInLocalDb(name.to_string())),
        }
    }

    pub fn local_pkgs(&self, names: &[&str]) -> Vec<Result<Pkg>> {
        names
            .iter()
            .map(|name| self.local_pkg(name))
            .collect()
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
        names
            .iter()
            .map(|name| self.pkg(name))
            .collect()
    }

    pub fn update_file_listing_cache(&mut self) -> Result<()> {
        require_root()?;

        self.h_mut().set_dbext(".files");

        self.update()?;

        let cache_dir = self.file_cache_dir();
        
        let handle = self.h_mut();

        let db_path = Path::new(handle.dbpath());
        let sync_dir = db_path.join("sync");

        if sync_dir.exists() {
            for entry in fs::read_dir(&sync_dir).map_err(Error::InternalIO)? {
                let entry = entry.map_err(Error::InternalIO)?;
                let path = entry.path();

                if let Some(filename) = path.file_name().and_then(|n| n.to_str())
                    && filename.ends_with(".files")
                {
                    let db_name = filename.trim_end_matches(".files");
                    let db_cache_dir = cache_dir.join(db_name);

                    let should_update = if db_cache_dir.exists() {
                        let sync_mtime = fs::metadata(&path).map_err(Error::InternalIO)?.modified().map_err(Error::InternalIO)?;
                        let cache_mtime = fs::metadata(&db_cache_dir).map_err(Error::InternalIO)?.modified().map_err(Error::InternalIO)?;
                        sync_mtime > cache_mtime
                    } else {
                        true
                    };

                    if should_update {
                        log_info!("Updating file data for {db_name}");
                        
                        fs::remove_dir_all(&db_cache_dir).map_err(Error::InternalIO)?;
                        fs::create_dir_all(&db_cache_dir).map_err(Error::InternalIO)?;

                        Self::unarchive_files_db(&path, &db_cache_dir)
                            .map_err(|_| Error::ExtractArchive)?;
                    } else {
                        log_info!("File data for {db_name} up to date");
                    }
                }
            }
        }

        Ok(())
    }
    
    pub fn unarchive_files_db(archive_path: &Path, extract_to: &Path) -> Result<()> {
        if extract_to.exists() {
            fs::remove_dir_all(extract_to)?;
        }
        fs::create_dir_all(extract_to)?;

        let file = fs::File::open(archive_path).map_err(|_| Error::OpenArchive)?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            if path.as_os_str().is_empty() || path == Path::new(".") {
                continue;
            }
            entry.unpack_in(extract_to)?;
        }

        Ok(())
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
