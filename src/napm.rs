use alpm::{
    Alpm, AnyDownloadEvent, AnyEvent, AnyQuestion, DownloadEvent, DownloadEventCompleted,
    DownloadEventProgress, DownloadResult, Usage,
};
use indicatif::{MultiProgress, ProgressBar};
use pacmanconf::Config;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::ansi::*;
use crate::error::{Error, Result};
use crate::pkg::Pkg;
use crate::util::{choose, confirm};
use crate::{log_error, log_info, log_warn};

pub mod actions;
pub mod auto_repair;
pub mod cache;
pub mod style;
pub mod util;

// NAPM ERROR DATA

// struct NapmDep {
//     // TODO
// }

struct NapmConflict {
    pkg1: Pkg,
    pkg2: Pkg,
    // TODO: reason: NapmDep,
}

struct NapmDepMissing {
    target: String,
    causing_pkg: Option<String>,
    // TODO: dep: NapmDep,
}

#[allow(dead_code)]
enum NapmErrorData {
    Empty,
    FileConflict(Vec<NapmConflict>),
    PkgInvalid(Vec<String>),
    PkgInvalidArch(Vec<Pkg>),
    UnsatisfiedDeps(Vec<NapmDepMissing>),
    ConflictingDeps(Vec<NapmConflict>),
}

pub struct Napm {
    config: Config,
    handle: Option<Alpm>,
}

impl Napm {
    pub fn new() -> Result<Self> {
        let mut me = Self {
            config: Config::new().map_err(|_| Error::ConfigParse)?,
            handle: None,
        };
        me.reset()?;
        Ok(me)
    }

    pub fn reset(&mut self) -> Result<()> {
        let cfg = Config::new().map_err(|_| Error::ConfigParse)?;

        if cfg.root_dir != "/" {
            unimplemented!("Non / root");
        }

        let mut handle = Alpm::new("/", &cfg.db_path)?;

        let arch = cfg.architecture.first().map(String::as_str).unwrap();

        for dir in &cfg.cache_dir {
            let path: Vec<u8> = if dir.starts_with('/') {
                dir.clone()
            } else {
                format!("/{}", &dir)
            }
            .into();

            handle.add_cachedir(path)?;
        }

        handle.set_check_space(cfg.check_space);

        if cfg.parallel_downloads > 0 {
            handle.set_parallel_downloads(cfg.parallel_downloads as u32);
        }

        let local_siglevel = Self::parse_siglevel(&cfg.local_file_sig_level)?;
        let remote_siglevel = Self::parse_siglevel(&cfg.remote_file_sig_level)?;

        handle.set_local_file_siglevel(local_siglevel)?;
        handle.set_remote_file_siglevel(remote_siglevel)?;

        for repo in &cfg.repos {
            let siglevel = if repo.sig_level.is_empty() {
                remote_siglevel
            } else {
                Self::parse_siglevel(&repo.sig_level)?
            };

            let name: Vec<u8> = repo.clone().name.into();
            let db = handle.register_syncdb_mut(name, siglevel)?;

            for server in &repo.servers {
                let url = server.replace("$repo", &repo.name).replace("$arch", arch);
                db.add_server(url)?;
            }

            db.set_usage(Usage::all())?; // TODO? take from config
        }

        handle.add_hookdir("/usr/share/libalpm/hooks")?;

        for hook_dir in &cfg.hook_dir {
            handle.add_hookdir(hook_dir.clone())?;
        }

        let gpg_dir: Vec<u8> = cfg.gpg_dir.clone().into();
        handle.set_gpgdir(gpg_dir)?;

        // callbacks

        let download_progress = Arc::new(Mutex::new((MultiProgress::new(), HashMap::new())));
        handle.set_dl_cb(download_progress, download_callback);

        handle.set_event_cb((), event_callback);

        // let other_progress = Arc::new(Mutex::new((MultiProgress::new(), HashMap::new())));
        // handle.set_progress_cb(other_progress, progress_callback);

        handle.set_question_cb((), question_callback);

        // TODO: handle.set_fetch_cb

        self.config = cfg;
        self.handle = Some(handle);

        Ok(())
    }
}

impl Drop for Napm {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.unlock();
            let _ = h.release();
        }
    }
}

fn event_callback(ev: AnyEvent, _: &mut ()) {
    use alpm::{HookWhen, PackageOperation};

    use alpm::Event as E;
    match ev.event() {
        E::CheckDepsStart => log_info!("Checking dependencies"),
        E::CheckDepsDone => (),
        E::FileConflictsStart => log_info!("Checking for file conflicts"),
        E::FileConflictsDone => (),
        E::ResolveDepsStart => log_info!("Resolving dependencies"),
        E::ResolveDepsDone => (),
        E::InterConflictsStart => log_info!("Checking for conflicts"),
        E::InterConflictsDone => (),
        E::TransactionStart => log_info!("Starting transaction"), // TODO: command specific message
        E::TransactionDone => (),
        E::PackageOperationStart(pkg_op_ev) => match pkg_op_ev.operation() {
            PackageOperation::Install(p) => log_info!("Installing {}-{}", p.name(), p.version()),
            PackageOperation::Upgrade(p1, p2) => log_info!(
                "Upgrading {} from {} to {}",
                p1.name(),
                p1.version(),
                p2.version()
            ),
            PackageOperation::Reinstall(p1, _p2) => {
                log_info!("Reinstalling {}-{}", p1.name(), p1.version())
            }
            PackageOperation::Downgrade(p1, p2) => log_info!(
                "Downgrading {} ({} => {})",
                p1.name(),
                p1.version(),
                p2.version()
            ),
            PackageOperation::Remove(p) => log_info!("Removing {}-{}", p.name(), p.version()),
        },
        E::PackageOperationDone(_) => (),
        E::IntegrityStart => log_info!("Checking for file integrity"),
        E::IntegrityDone => (),
        E::LoadStart => (),
        E::LoadDone => (),
        E::ScriptletInfo(scriptlet_info) => log_info!("  {}", scriptlet_info.line().trim()),
        E::RetrieveStart => log_info!("Retrieving files"),
        E::RetrieveDone => (),
        E::RetrieveFailed => log_info!("Failed to retrieve some"),
        E::PkgRetrieveStart(retrieve_ev) => log_info!(
            "Retrieving {} packages, total size {}",
            retrieve_ev.num(),
            retrieve_ev.total_size()
        ),
        E::PkgRetrieveDone(_retrieve_ev) => (),
        E::PkgRetrieveFailed(_retrieve_ev) => log_error!("Package retireve failed"),
        E::DiskSpaceStart => log_info!("Checking availible disk space"),
        E::DiskSpaceDone => (),
        E::OptDepRemoval(opt_dep_rm_ev) => {
            if let Some(desc) = opt_dep_rm_ev.optdep().desc() {
                log_info!(
                    "Package {} optionally requires {}: {}",
                    opt_dep_rm_ev.pkg().name(),
                    opt_dep_rm_ev.optdep().name(),
                    desc
                )
            } else {
                log_info!(
                    "Package {} optionally requires {}",
                    opt_dep_rm_ev.pkg().name(),
                    opt_dep_rm_ev.optdep().name()
                )
            }
        }
        E::DatabaseMissing(dm_missing_ev) => {
            log_error!("Database {} missing", dm_missing_ev.dbname())
        }
        E::KeyringStart => log_info!("Checking keys in keyring"),
        E::KeyringDone => (),
        E::KeyDownloadStart => log_info!("Downloading keys"),
        E::KeyDownloadDone => (),
        E::PacnewCreated(pacnew_ev) => log_warn!(
            "File {} installed as {}.pacnew",
            pacnew_ev.file(),
            pacnew_ev.file()
        ),
        E::PacsaveCreated(pacsave_ev) => log_warn!(
            "File {} saved as {}.pacsave",
            pacsave_ev.file(),
            pacsave_ev.file()
        ),
        E::HookStart(hook_ev) => log_info!(
            "Running {} hooks",
            match hook_ev.when() {
                HookWhen::PreTransaction => "pre transaction",
                HookWhen::PostTransaction => "post transaction",
            }
        ),
        E::HookDone(_hook_ev) => (),
        E::HookRunStart(hook_run_ev) => log_info!(
            "Running hook {}/{}: {}",
            hook_run_ev.position(),
            hook_run_ev.total(),
            hook_run_ev
                .desc()
                .unwrap_or(hook_run_ev.name())
                .trim_end_matches("...")
        ),
        E::HookRunDone(_hook_run_ev) => (),
    };
}

fn question_callback(q: AnyQuestion, _: &mut ()) {
    use alpm::Question as Q;
    use std::path::Path;

    match q.question() {
        Q::Conflict(mut x) => {
            let pkg_a = x.conflict().package1().name();
            let pkg_b = x.conflict().package2().name();
            let prompt = format!(
                "Conflict between {ANSI_CYAN}{pkg_a}{ANSI_RESET} and {ANSI_CYAN}{pkg_b}{ANSI_RESET}; Remove {ANSI_RED}{pkg_b}{ANSI_RESET}?"
            );

            match confirm(&prompt, true) {
                Ok(ans) => x.set_remove(ans),
                Err(err) => err.die(),
            }
        }
        Q::Replace(x) => {
            let old = x.oldpkg().name();
            let new = x.newpkg().name();
            let prompt = format!("Replace package {ANSI_CYAN}{old} with {ANSI_CYAN}{new}?");

            match confirm(&prompt, true) {
                Ok(ans) => x.set_replace(ans),
                Err(err) => err.die(),
            }
        }
        Q::Corrupted(mut x) => {
            let filepath = x.filepath();
            let filename = Path::new(filepath).file_name().unwrap().to_str().unwrap();
            let reason = x.reason();
            let prompt = format!(
                "File {ANSI_MAGENTA}{filename}{ANSI_RESET} is corrupted: {reason}. Remove package?"
            );

            match confirm(&prompt, true) {
                Ok(ans) => x.set_remove(ans),
                Err(err) => err.die(),
            }
        }
        Q::ImportKey(mut x) => {
            let fingerprint = x.fingerprint();
            let name = x.uid();
            let prompt = format!("Import key {ANSI_BOLD}{fingerprint}{ANSI_RESET}, \"{name}\"?");

            match confirm(&prompt, true) {
                Ok(ans) => x.set_import(ans),
                Err(err) => err.die(),
            }
        }
        Q::SelectProvider(mut x) => {
            let dep = x.depend();
            let name = dep.name();
            let providers = x
                .providers()
                .into_iter()
                .map(Pkg::from)
                .map(|pkg| pkg.name)
                .collect::<Vec<_>>();

            let prompt = format!(
                "There are several providers for {ANSI_MAGENTA}{name}{ANSI_RESET} and you must choose one"
            );

            match choose(&prompt, providers.as_slice(), 0) {
                Ok(ans) => x.set_index(ans),
                Err(err) => err.die(),
            }
        }
        _ => (),
    }
}

fn download_callback(
    file: &str,
    ev: AnyDownloadEvent,
    bars: &mut Arc<Mutex<(MultiProgress, HashMap<String, ProgressBar>)>>,
) {
    match ev.event() {
        DownloadEvent::Init(_) => {
            let mut bars_guard = bars.lock().unwrap();
            let (mp, bar_map) = &mut *bars_guard;

            if let std::collections::hash_map::Entry::Vacant(e) = bar_map.entry(file.to_string()) {
                let pb = mp.add(ProgressBar::new(100));
                pb.set_style(Napm::progress_bar_style(false).clone());
                pb.set_message(file.to_string());
                e.insert(pb);
            }
        }

        DownloadEvent::Progress(DownloadEventProgress { downloaded, total }) => {
            let bars_guard = bars.lock().unwrap();
            let (_, bar_map) = &*bars_guard;

            if let Some(pb) = bar_map.get(file) {
                pb.set_length(total as u64);
                pb.set_position(downloaded as u64);
            }
        }

        DownloadEvent::Completed(DownloadEventCompleted { total, result }) => {
            let mut bars_guard = bars.lock().unwrap();
            let (_, bar_map) = &mut *bars_guard;

            if let Some(pb) = bar_map.remove(file) {
                pb.set_position(total as u64);
                match result {
                    DownloadResult::Success => pb.finish_with_message(format!("{file} done")),
                    DownloadResult::UpToDate => {
                        pb.finish_with_message(format!("{file} up to date"))
                    }
                    DownloadResult::Failed => {
                        pb.set_style(Napm::progress_bar_style(true).clone());
                        pb.finish_with_message(format!("{file} failed"));
                    }
                }
            }
        }

        DownloadEvent::Retry(_) => {}
    }
}

// fn progress_callback(
//     progress: Progress,
//     file: &str,
//     percent: i32,
//     how_many: usize,
//     current: usize,
//     bars: &mut Arc<Mutex<(MultiProgress, HashMap<String, ProgressBar>)>>,
// ) {
//     let mut bars_guard = bars.lock().unwrap();
//     let (mp, bar_map) = &mut *bars_guard;

//     if let std::collections::hash_map::Entry::Vacant(e) = bar_map.entry(file.to_string()) {
//         let pb = mp.add(ProgressBar::new(100));
//         pb.set_style(Napm::progress_bar_style(false).clone());
//         pb.set_message(file.to_string());

//         e.insert(pb);
//     }

//     let pb: &mut ProgressBar = bar_map.get_mut(file).unwrap();

//     pb.set_length(percent as u64);
//     pb.set_message(format!("{file} {:?} {}/{}", progress, current, how_many));
// }
