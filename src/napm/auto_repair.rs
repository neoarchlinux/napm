use alpm::{
    CommitData, TransFlag, Error as AlpmErr, PrepareData,
};

use crate::napm::*;
use crate::{log_info, log_fatal};
use crate::util::require_root;

macro_rules! log_repair {
    ($($arg:tt)*) => {{
        use crate::ansi::*;
        eprintln!("[{ANSI_CYAN}AUTO REPAIR{ANSI_RESET}] {}", format!($($arg)*));
    }};
}

impl Napm {
    fn on_alpm_error(&mut self, error: AlpmErr, data: NapmErrorData) -> Result<()> {
        macro_rules! failed {
            ($e:ident) => {{
                log_fatal!("{}", Error::$e);
                Err(Error::$e)
            }};
        }

        use AlpmErr as E;
        match error {
            E::Ok => failed!(NoAutoRepairError),
            E::Memory => failed!(Memory),
            E::System => failed!(System),
            E::BadPerms => failed!(BadPerms),
            E::NotAFile | E::NotADir => failed!(UnexpectedType),
            E::WrongArgs => failed!(WrongArgs),
            E::DiskSpace => failed!(DiskSpace),
            E::HandleNull | E::HandleNotNull => failed!(Handle),
            E::HandleLock => {
                log_repair!(
                    "Handle lock detected. Attempting safe removal."
                );

                let failed_result = Err(Error::DbUnlock);
                let current_pid = std::process::id();

                let output_napm = std::process::Command::new("pgrep")
                    .arg("-a")
                    .arg("napm")
                    .output();

                match output_napm {
                    Ok(o) if !o.stdout.is_empty() => {
                        let output = String::from_utf8_lossy(&o.stdout);

                        let lines = output
                            .lines()
                            .filter(|line| {
                                if let Some(pid_str) = line.split_whitespace().next() {
                                    match pid_str.parse::<u32>() {
                                        Ok(pid) => pid != current_pid,
                                        Err(_) => true,
                                    }
                                } else {
                                    true
                                }
                            })
                            .collect::<Vec<_>>();

                        if !lines.is_empty() {
                            log_fatal!("Running napm processes (except {}):\n{}", current_pid, lines.join("\n"));
                            return failed_result;
                        } else {
                            log_repair!(" - No active napm processes detected.");
                        }
                    }
                    _ => log_repair!(" - No active napm processes detected."),
                }

                let output_pacman = std::process::Command::new("pgrep")
                    .arg("-a")
                    .arg("pacman")
                    .output();

                match output_pacman {
                    Ok(o) if !o.stdout.is_empty() => {
                        log_fatal!(
                            "Running pacman processes:\n{}",
                            String::from_utf8_lossy(&o.stdout)
                        );
                        return failed_result;
                    }
                    _ => log_repair!(" - No active pacman processes detected."),
                }

                let lock_path = self.h().lockfile();
                if std::path::Path::new(&lock_path).exists() {
                    log_repair!("Removing lock file at {lock_path}");
                    let _ = std::fs::remove_file(lock_path);
                }

                Ok(())
            }
            E::DbOpen
            | E::DbCreate
            | E::DbNull
            | E::DbNotNull
            | E::DbNotFound
            | E::DbInvalid
            | E::DbInvalidSig
            | E::DbVersion
            | E::DbWrite
            | E::DbRemove => {
                todo!("handling of {error:?} aka '{error}'");
            }
            E::ServerBadUrl | E::ServerNone => {
                // Repository/server issue - check URL, network connectivity
                todo!("handling of {error:?} aka '{error}'");
            }
            E::TransNotPrepared => Err(Error::NothingToDo),
            E::TransNotNull | E::TransNull => {
                todo!("handling of {error:?} aka '{error}'");
            }
            E::TransDupTarget
            | E::TransDupFileName
            | E::TransNotInitialized
            | E::TransAbort
            | E::TransType
            | E::TransNotLocked
            | E::TransHookFailed => {
                todo!("handling of {error:?} aka '{error}'");
            }
            E::PkgNotFound | E::PkgIgnored => {
                // Package not found - show error
                todo!("handling of {error:?} aka '{error}'");
            }
            E::PkgInvalid => {
                // Clear cache
                // Resync databases
                // Retry operation
                todo!("handling of {error:?} aka '{error}'");
            }
            E::PkgInvalidChecksum | E::PkgInvalidSig | E::PkgMissingSig => {
                // Refresh keyring
                // Resync databases
                todo!("handling of {error:?} aka '{error}'");
            }
            E::PkgOpen => {
                // Package file could not be opened - check permissions
                todo!("handling of {error:?} aka '{error}'");
            }
            E::PkgCantRemove => {
                // Package cannot be removed - maybe running process holds files
                todo!("handling of {error:?} aka '{error}'");
            }
            E::PkgInvalidName | E::PkgInvalidArch => {
                // Invalid package metadata - abort operation
                todo!("handling of {error:?} aka '{error}'");
            }
            E::SigMissing | E::SigInvalid => {
                // Refresh keyring
                // Resync databases
                todo!("handling of {error:?} aka '{error}'");
            }
            E::UnsatisfiedDeps => {
                if let NapmErrorData::UnsatisfiedDeps(missing) = &data {
                    for dep in missing {
                        if let Some(causing_pkg) = &dep.causing_pkg {
                            log_fatal!(
                                "Package {ANSI_YELLOW}{}{ANSI_RESET} requires {ANSI_YELLOW}{}{ANSI_RESET} to be installed",
                                dep.target,
                                causing_pkg
                            );
                        } else {
                            log_fatal!("Dependency {ANSI_YELLOW}{}{ANSI_RESET} missing", dep.target);
                        }
                    }
                }

                Err(Error::UnsatisfiedDeps)
            }
            E::ConflictingDeps => {
                if let NapmErrorData::ConflictingDeps(conflicts) = &data {
                    for c in conflicts {
                        log_fatal!(
                            "Conflicting packages: {ANSI_YELLOW}{}{ANSI_RESET} and {ANSI_YELLOW}{}{ANSI_RESET}",
                            c.pkg1.formatted_name(),
                            c.pkg2.formatted_name()
                        );
                    }
                }

                Err(Error::ConflictingDeps)
            }
            E::FileConflicts => {
                if let NapmErrorData::FileConflict(conflicts) = &data {
                    for c in conflicts {
                        log_fatal!("File conflict between {} and {}", c.pkg1.name, c.pkg2.name);
                        // TODO: Attempt to auto-remove conflicting files
                    }
                }

                Err(Error::FileConflicts)
            }
            E::RetrievePrepare | E::Retrieve => {
                // Downloading/preparing package failed - retry
                todo!("handling of {error:?} aka '{error}'");
            }
            E::InvalidRegex => {
                // Invalid regex in package/db query - abort
                todo!("handling of {error:?} aka '{error}'");
            }
            E::Libarchive | E::Libcurl | E::ExternalDownload | E::Gpgme => {
                // External library failure - check system libraries
                todo!("handling of {error:?} aka '{error}'");
            }
            E::MissingCapabilitySignatures => {
                // Some required signatures are missing
                todo!("handling of {error:?} aka '{error}'");
            }
        }
    }

    pub fn update(&mut self) -> Result<bool> {
        log_info!("Updating {} databases", match self.h().dbext() {
            ".db" => "package",
            ".files" => "file",
            other => panic!("dbext = {other}")
        });

        require_root()?;

        match self.h_mut().syncdbs_mut().update(true) {
            Err(e) => {
                self.on_alpm_error(e, NapmErrorData::Empty)?;
                self.h_mut().syncdbs_mut().update(true).map_err(|_| Error::Update)
            }
            Ok(b) => Ok(b),
        }
    }

    pub fn trans_init(&mut self, flags: TransFlag) -> Result<()> {
        let (error, data) = {
            match self.h_mut().trans_init(flags) {
                Ok(()) => return Ok(()),
                Err(e) => (e, NapmErrorData::Empty),
            }
        };

        self.on_alpm_error(error, data)?;
        self.h_mut().trans_init(flags).map_err(|_| Error::TransInit)
    }

    pub fn trans_prepare(&mut self) -> Result<()> {
        let (error, data) = {
            match self.h_mut().trans_prepare() {
                Ok(()) => return Ok(()),
                Err(e) => {
                    (e.error(), match e.data() {
                        Some(PrepareData::PkgInvalidArch(list)) => {
                            NapmErrorData::PkgInvalidArch(list.iter().map(Pkg::from).collect())
                        }
                        Some(PrepareData::UnsatisfiedDeps(list)) => {
                            NapmErrorData::UnsatisfiedDeps(
                                list.iter()
                                    .map(|d| NapmDepMissing {
                                        target: d.target().to_string(),
                                        causing_pkg: d.causing_pkg().map(String::from),
                                    })
                                    .collect(),
                            )
                        }
                        Some(PrepareData::ConflictingDeps(list)) => {
                            NapmErrorData::ConflictingDeps(
                                list.iter()
                                    .map(|c| NapmConflict {
                                        pkg1: Pkg::from(c.package1()),
                                        pkg2: Pkg::from(c.package2()),
                                    })
                                    .collect(),
                            )
                        }
                        None => NapmErrorData::Empty,
                    })
                }
            }
        };

        self.on_alpm_error(error, data)?;
        self.h_mut().trans_prepare().map_err(|_| Error::TransPrepare)
    }

    pub fn trans_commit(&mut self) -> Result<()> {
        let (error, data) = {
            match self.h_mut().trans_commit() {
                Ok(()) => return Ok(()),
                Err(e) => {
                    (e.error(), match e.data() {
                        Some(CommitData::FileConflict(_)) => NapmErrorData::FileConflict(
                            // list
                            //     .iter()
                            //     .map(|c| NapmConflict {
                            //         pkg1: Pkg::from(c.package1()),
                            //         pkg2: Pkg::from(c.package2()),
                            //     })
                            //     .collect()
                            // alpm does not work (segfaults here), doing it from scratch
                            vec![],
                        ),
                        Some(CommitData::PkgInvalid(list)) => {
                            NapmErrorData::PkgInvalid(list.iter().map(String::from).collect())
                        }
                        Option::None => NapmErrorData::Empty,
                    })
                }
            }
        };

        self.on_alpm_error(error, data)?;
        self.h_mut().trans_commit().map_err(|_| Error::TransCommit)
    }
}