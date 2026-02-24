use crate::error::{Error, Result};
use crate::log_error;
use crate::napm::Napm;
use crate::util::{confirm, require_root};

pub fn run(napm: &mut Napm, pkg_names: &[&str]) -> Result<()> {
    require_root()?;

    let pkgs = {
        let pkgs_res = napm
            .pkgs(pkg_names)
            .into_iter()
            .map(|pkg| {
                if let Ok(ref p) = pkg
                    && let Ok(_) = napm.local_pkg(&p.name)
                {
                    Err(Error::PackageAlreadyInstalled(p.name.clone()))
                } else {
                    pkg
                }
            })
            .collect::<Vec<_>>();

        let display_names: Vec<String> = pkgs_res
            .iter()
            .filter_map(|pkg| pkg.as_ref().ok())
            .map(|pkg| pkg.formatted_name(false))
            .collect();

        let invalid_errs = pkgs_res
            .iter()
            .filter_map(|pkg| pkg.as_ref().err())
            .collect::<Vec<_>>();

        if !invalid_errs.is_empty() {
            for invalid_err in invalid_errs {
                log_error!("{invalid_err}");
            }

            let confirm_message = format!(
                "Some packages were invalid, do you still want to install the rest ({})?",
                display_names.join(", ")
            );

            if !display_names.is_empty() && !confirm(&confirm_message, true)? {
                return Err(Error::Stopped);
            }
        }

        if display_names.is_empty() {
            return Err(Error::NoValidPackage);
        }

        pkgs_res
            .into_iter()
            .filter_map(|pkg| pkg.ok())
            .collect::<Vec<_>>()
    };

    napm.install_pkgs(&pkgs)
}
