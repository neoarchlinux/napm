use alpm::{
    Alpm, AnyEvent, AnyQuestion, AnyDownloadEvent, DownloadEvent, DownloadEventCompleted,
    DownloadEventProgress, DownloadResult, Progress, Usage,
};
use indicatif::{MultiProgress, ProgressBar};
use pacmanconf::Config;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    path::PathBuf,
};

use crate::ansi::*;
use crate::error::{Error, Result};
use crate::pkg::Pkg;
use crate::log_fatal;
use crate::util::{confirm, choose};

pub mod actions;
pub mod auto_repair;
pub mod util;
pub mod style;

// CFG

pub struct ConfigOverride {
    pub root: Option<String>,
}

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
    handle: Option<Alpm>,
}

impl Napm {
    pub fn cfg(cfg_override: ConfigOverride) -> Result<Config> {
        let cfg = if let Some(root) = cfg_override.root {
            use cini::Ini;

            let mut config = Config::default();

            let conf = { // because pacmanconf::Config does not use --sysroot option, but --root, I have to parse pacman-conf output by myself
                let mut cmd = std::process::Command::new("pacman-conf");
                
                cmd.arg("--sysroot").arg(root);

                let output = cmd.output()?;

                if !output.status.success() {
                    log_fatal!("Your config is incorrect");
                    for line in String::from_utf8(output.stderr).map_err(|_| Error::ConfigParse)?.lines() {
                        log_fatal!("    {line}");
                    }
                    return Err(Error::ConfigParse);
                }

                let mut conf = String::from_utf8(output.stdout).map_err(|_| Error::ConfigParse)?;
                if conf.ends_with('\n') {
                    conf.pop().unwrap();
                }

                conf
            };

            config.parse_str(&conf).map_err(|_| Error::ConfigParse)?;

            Ok(config)
        } else {
            Config::new()
        }
        .map_err(|_| Error::ConfigParse)?;

        Ok(cfg)
    }

    pub fn new(cfg_override: ConfigOverride) -> Result<Self> {
        let cfg = Self::cfg(cfg_override)?;

        let root_dir: Vec<u8> = cfg.root_dir.clone().into();
        let db_path: Vec<u8> = cfg.db_path.into();

        let mut handle = Alpm::new(root_dir, db_path)?;

        let arch = cfg.architecture.first().map(String::as_str).unwrap();

        for dir in &cfg.cache_dir {
            let path: Vec<u8> = if dir.starts_with('/') {
                format!("{}{}", &cfg.root_dir, &dir)
            } else {
                format!("{}/{}", &cfg.root_dir, &dir)
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

            db.set_usage(Usage::all())?; // TODO: from config?
        }

        {
            let mut path = PathBuf::new();
            path.push(cfg.root_dir);
            path.push("/usr/share/libalpm/hooks");
            handle.add_hookdir(path.display().to_string())?;
        }
        
        for hook_dir in cfg.hook_dir {
            handle.add_hookdir(hook_dir)?;
        }

        let gpg_dir: Vec<u8> = cfg.gpg_dir.into();
        handle.set_gpgdir(gpg_dir)?;

        // callbacks

        let download_progress = Arc::new(Mutex::new((MultiProgress::new(), HashMap::new())));
        handle.set_dl_cb(download_progress, download_callback);

        handle.set_event_cb((), event_callback);

        let other_progress = Arc::new(Mutex::new((MultiProgress::new(), HashMap::new())));
        handle.set_progress_cb(other_progress, progress_callback);

        handle.set_question_cb((), question_callback);

        // TODO: handle.set_fetch_cb

        Ok(Self {
            handle: Some(handle),
        })
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

fn event_callback(
    ev: AnyEvent,
    _: &mut (),
) {
    crate::log_debug!("{:?}", ev.event()); // TODO: handle accordingly
}

fn question_callback(
    q: AnyQuestion,
    _: &mut (),
) {
    use alpm::Question as Q;
    use std::path::Path;

    match q.question() {
        Q::Conflict(mut x) => {
            let pkg_a = x.conflict().package1().name();
            let pkg_b = x.conflict().package2().name();
            let prompt = format!("Conflict between {ANSI_CYAN}{pkg_a}{ANSI_RESET} and {ANSI_CYAN}{pkg_b}{ANSI_RESET}; Remove {ANSI_RED}{pkg_b}{ANSI_RESET}?");

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
            let prompt = format!("File {ANSI_MAGENTA}{filename}{ANSI_RESET} is corrupted: {reason}. Remove package?");

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
            let providers = x.providers()
                .into_iter()
                .map(Pkg::from)
                .map(|pkg| pkg.name)
                .collect::<Vec<_>>();

            let prompt = format!("There are several providers for {ANSI_MAGENTA}{name}{ANSI_RESET} and you must choose one");

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

fn progress_callback(
    progress: Progress,
    file: &str,
    percent: i32,
    how_many: usize,
    current: usize,
    bars: &mut Arc<Mutex<(MultiProgress, HashMap<String, ProgressBar>)>>,
) {
    let mut bars_guard = bars.lock().unwrap();
    let (mp, bar_map) = &mut *bars_guard;

    if let std::collections::hash_map::Entry::Vacant(e) = bar_map.entry(file.to_string()) {
        let pb = mp.add(ProgressBar::new(100));
        pb.set_style(Napm::progress_bar_style(false).clone());
        pb.set_message(file.to_string());

        e.insert(pb);
    }

    let pb: &mut ProgressBar = bar_map.get_mut(file).unwrap();

    pb.set_length(percent as u64);
    pb.set_message(format!("{file} {:?} {}/{}", progress, current, how_many));
}
