use crate::error::{Error, Result};
use crate::{log_info, log_error};
use crate::util::confirm;
use crate::napm::Napm;

pub fn run(napm: &mut Napm, pkg_names: &[&str], deep: bool) -> Result<()> {
    let pkgs = {
        let pkgs_res = napm.local_pkgs(pkg_names);

        let display_names: Vec<String> = pkgs_res
            .iter()
            .filter_map(|pkg| pkg.as_ref().ok())
            .map(|pkg| pkg.formatted_name())
            .collect();

        let invalid_errs = pkgs_res
            .iter()
            .filter_map(|pkg| pkg.as_ref().err())
            .collect::<Vec<_>>();

        if !invalid_errs.is_empty() {
            for invalid_err in invalid_errs {
                log_error!("{invalid_err}");
            }

            let confirm_message = format!("Some packages were invalid, do you still want to remove the rest ({})?", display_names.join(", "));

            if !display_names.is_empty() && !confirm(&confirm_message, true)? {
                return Err(Error::Stopped);
            }
        }

        if display_names.is_empty() {
            return Err(Error::NoValidPackage);
        }

        log_info!("Removing {}", display_names.join(", "));

        pkgs_res
            .into_iter()
            .filter_map(|pkg| pkg.ok())
            .collect::<Vec<_>>()
    };
    
    napm.remove_pkgs(&pkgs, deep)
}
