use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::error::{Error, Result};
use crate::napm::Napm;
use crate::log_warn;
use crate::util::confirm;

pub fn run(napm: &mut Napm, pkg_name: &str) -> Result<()> {
    let files = if napm.local_pkg(pkg_name).is_ok() {
        napm.files(pkg_name)?
    } else if let Ok(pkg) = napm.pkg(pkg_name) {
        let file_path = format!(
            "{}/{}/{}-{}/files",
            napm.file_cache_dir().display(),
            pkg.db_name,
            pkg.name,
            pkg.version
        );

        let file = match File::open(&file_path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log_warn!("File {file_path} not found");
                if !confirm("File listing cache is probably stale, do you want to update?", true)? {
                    return Err(Error::Stopped);
                }

                napm.update_file_listing_cache()?;

                match File::open(&file_path) {
                    Ok(file) => file,
                    error => return error.map(|_| ()).map_err(Error::InternalIO),
                }
            }
            other_error => return other_error.map(|_| ()).map_err(Error::InternalIO),
        };
        let reader = BufReader::new(file);

        reader
            .lines()
            .skip(1)
            .map(|l| l.map_err(Error::InternalIO))
            .collect::<std::result::Result<_, _>>()?
    } else {
        return Err(Error::PackageNotFound(pkg_name.to_string()));
    };

    for f in files {
        println!("{}", f);
    }

    Ok(())
}
