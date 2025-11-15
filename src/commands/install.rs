use crate::napm::Napm;
use anyhow::{Result, anyhow};

pub fn run(napm: &mut Napm, pkg_names: &[&str], sync: bool) -> Result<()> {
    if sync {
        println!("Synchronizing databases");
        let _ = napm.sync(false)?;
    }

    let pkgs = {
        let pkgs_res = napm.pkgs(pkg_names);

        let invalid_errs = pkgs_res
            .iter()
            .filter_map(|pkg| pkg.as_ref().err())
            .collect::<Vec<_>>();

        if !invalid_errs.is_empty() {
            for invalid_err in invalid_errs {
                println!("{invalid_err}");
            }

            // TODO: ask to continue
        }

        let display_names: Vec<String> = pkgs_res
            .iter()
            .filter_map(|pkg| pkg.as_ref().ok())
            .map(|pkg| pkg.formatted_name())
            .collect();

        if display_names.is_empty() {
            return Err(anyhow!("No valid package to install"));
        }

        println!("Installing {}", display_names.join(" "));

        pkgs_res
            .into_iter()
            .filter_map(|pkg| pkg.ok())
            .collect::<Vec<_>>()
    };

    napm.install_pkgs(&pkgs)
}
