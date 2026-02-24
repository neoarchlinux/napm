use crate::ansi::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No error, just nothing is to be done")]
    NothingToDo,

    #[error("Failed to parse the config")]
    ConfigParse,

    #[error("Internal IO error: {0}")]
    InternalIO(std::io::Error),

    #[error("Internal ALPM error: {0}")]
    InternalALPM(alpm::Error),

    #[error("Automatic repair called despite no apparent error")]
    NoAutoRepairError,

    #[error("Out of memory")]
    Memory,

    #[error("System error encountered")]
    System,

    #[error("Permission error")]
    BadPerms,

    #[error("No supported privilege escalation tool found ({}).", crate::util::PE_TOOLS.join(", "))]
    NoPETool,

    #[error("No supported shell found ({}).", crate::util::SHELLS.join(", "))]
    NoShell,

    #[error("User denied required privilege escalation, please run {ANSI_YELLOW}{0}{ANSI_RESET}")]
    DeniedPE(String),

    #[error("Stopped by the user")]
    Stopped,

    #[error("No results")]
    NoResults,

    #[error("Package {ANSI_YELLOW}{0}{ANSI_RESET} is already installed")]
    PackageAlreadyInstalled(String),

    #[error("Unexpected file or directory type")]
    UnexpectedType,

    #[error("Invalid arguments passed to ALPM")]
    WrongArgs,

    #[error("Disk full")]
    DiskSpace,

    #[error("Unexpected handle")]
    Handle,

    #[error("Cannot unlock database")]
    DbUnlock,

    #[error("Could not release transaction")]
    TransRelease,

    #[error("Could not initialize transaction")]
    TransInit,

    #[error("Could not prepare transaction, even after automatic repair")]
    TransPrepare,

    #[error("Could not commit transaction, even after automatic repair")]
    TransCommit,

    #[error("Dependency missing")]
    UnsatisfiedDeps,

    #[error("Dependency conflicts")]
    ConflictingDeps,

    #[error("File conflicts")]
    FileConflicts,

    #[error("Conflicts")]
    Conflicts,

    #[error("Failed to refresh databases")]
    DbRefresh,

    #[error("Failed to upgrade")]
    Upgrade,

    #[error("Failed to open archive")]
    OpenArchive,

    #[error("Failed to extract archive")]
    ExtractArchive,

    #[error("Failed to find package")]
    FindPkg,

    #[error("No valid package to install")]
    NoValidPackage,

    #[error("Package {ANSI_YELLOW}{0}{ANSI_RESET} not found")]
    PackageNotFound(String),

    #[error("Package {ANSI_YELLOW}{0}{ANSI_RESET} is not installed or does not exist")]
    PackageNotInLocalDb(String),

    #[error("Failed to parse `SigLevel = {0}` in the config")]
    SigLevelParse(String),

    #[error("Failed to update")]
    Update,

    #[error("Failed to add a package")]
    TransAddPkg,

    #[error("Failed to remove a package")]
    TransRemovePkg,

    #[error("Cache database error: {0}")]
    CacheDatabaseError(rusqlite::Error),

    #[error("System upgrade reqiuired")]
    UpgradeRequired,
}

impl Error {
    pub fn die(&self) {
        crate::log_fatal!("{}", self);
        std::process::exit(1);
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::InternalIO(err)
    }
}

impl From<alpm::Error> for Error {
    fn from(err: alpm::Error) -> Error {
        Error::InternalALPM(err)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Error {
        Error::CacheDatabaseError(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
