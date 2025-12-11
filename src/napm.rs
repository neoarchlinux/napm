use alpm::{
    Alpm, AnyDownloadEvent, DownloadEvent, DownloadEventCompleted, DownloadEventProgress,
    DownloadResult, Error as AlpmErr, Package, Progress, SigLevel, TransFlag, Usage,
};
use anyhow::{Context, Result, anyhow};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use tar::Archive;
use zstd::stream::read::Decoder;

use crate::ansi::*;

static PROGRESS_BAR_STYLE: OnceLock<ProgressStyle> = OnceLock::new();
static PROGRESS_BAR_STYLE_FAILED: OnceLock<ProgressStyle> = OnceLock::new();

fn progress_bar_style(failed: bool) -> &'static ProgressStyle {
    let progress_chars = "=> ";

    if failed {
        PROGRESS_BAR_STYLE_FAILED.get_or_init(|| {
            ProgressStyle::with_template("[{elapsed:>3}] [{bar:40.red/blue}] [FAILED] {msg}")
                .unwrap()
                .progress_chars(progress_chars)
        })
    } else {
        PROGRESS_BAR_STYLE.get_or_init(|| {
            ProgressStyle::with_template("[{elapsed:>3}] [{bar:40.cyan/blue}] {percent:>3}% {msg}")
                .unwrap()
                .progress_chars(progress_chars)
        })
    }
}

#[derive(Debug, Clone)]
pub struct Pkg {
    pub name: String,
    pub version: String,
    pub db_name: String,
    pub desc: String,
}

impl Pkg {
    fn into_package_ref(self, handle: &Alpm) -> Result<&Package> {
        let expect_msg = format!(
            "[{ANSI_RED}FATAL{ANSI_RESET}]: package '{}' not found in '{}'",
            self.name, self.db_name
        );

        handle
            .syncdbs()
            .iter()
            .find(|db| *db.name() == self.db_name)
            .expect(&expect_msg)
            .pkg(self.name)
            .map_err(|_| anyhow!(expect_msg))
    }

    pub fn formatted_name(&self) -> String {
        format!(
            "{ANSI_BLUE}{}{ANSI_RESET}/{ANSI_CYAN}{}{ANSI_RESET}",
            self.db_name, self.name,
        )
    }
}

impl From<&Package> for Pkg {
    fn from(package: &Package) -> Self {
        Self {
            name: package.name().to_string(),
            version: package.version().to_string(),
            db_name: package
                .db()
                .map(|db| db.name())
                .unwrap_or("local")
                .to_string(),
            desc: package.desc().unwrap_or("").to_string(),
        }
    }
}

pub struct Napm {
    handle: Option<Alpm>,
}

impl Napm {
    pub fn new(root: &str) -> Result<Self> {
        let dbpath = format!("{root}/var/lib/pacman"); // TODO: maybe change "pacman" to "napm"

        let mut handle = Alpm::new(root, &dbpath) //
            .map_err(|e| anyhow!("failed to initialize alpm: {e}"))?;

        // TODO: get from config
        let dbs = [
            (
                &[
                    "https://artix.sakamoto.pl/$repo/os/$arch",
                    "https://mirrors.dotsrc.org/artix-linux/repos/$repo/os/$arch",
                ][..],
                &["system", "world", "galaxy"][..],
            ),
            (
                &[
                    "https://arch.sakamoto.pl/$repo/os/$arch",
                    "https://mirror.pkgbuild.com/$repo/os/$arch",
                ][..],
                &["core", "extra", "multilib"][..],
            ),
            // (
            //     &["http://localhost:8080/$repo/os/$arch"][..],
            //     &["matrix"][..],
            // ),
        ];

        for (url_fmts, names) in &dbs {
            for &name in names.iter() {
                let db = handle.register_syncdb_mut(
                    name,
                    SigLevel::USE_DEFAULT | SigLevel::DATABASE_OPTIONAL,
                )?;

                for url_fmt in *url_fmts {
                    let url = url_fmt.replace("$repo", name).replace("$arch", "x86_64");
                    db.add_server(url)?;
                }

                db.set_usage(Usage::all())?;
            }
        }

        handle.add_cachedir(format!("{root}/var/cache/pacman/pkg").as_str())?;

        handle.set_check_space(true);
        handle.set_parallel_downloads(5);

        let download_progress = Arc::new(Mutex::new((MultiProgress::new(), HashMap::new())));
        handle.set_dl_cb(download_progress, download_callback);

        let other_progress = Arc::new(Mutex::new((MultiProgress::new(), HashMap::new())));
        handle.set_progress_cb(other_progress, progress_callback);

        Ok(Self {
            handle: Some(handle),
        })
    }

    fn h(&self) -> &Alpm {
        self.handle.as_ref().unwrap()
    }

    fn h_mut(&mut self) -> &mut Alpm {
        self.handle.as_mut().unwrap()
    }

    fn file_cache_dir(&self) -> PathBuf {
        Path::new(self.h().root()).join("var/cache/pacman/files")
    }

    pub fn sync(&mut self, force: bool) -> Result<bool> {
        let handle = self.h_mut();

        handle
            .syncdbs_mut()
            .update(force)
            .map_err(|e| anyhow!("sync failed: {e}"))
    }

    pub fn pkgs(&self, names: &[&str]) -> Vec<Result<Pkg>> {
        let mut result = Vec::new();

        for name in names {
            let mut found = None;

            for db in self.h().syncdbs() {
                match db.pkg(*name) {
                    Ok(pkg) => {
                        found = Some(Ok(Pkg::from(pkg)));
                        break;
                    }
                    Err(_) => continue,
                }
            }

            if let Some(pkg) = found {
                result.push(pkg);
            } else {
                result.push(Err(anyhow!("package '{name}' not found")));
            }
        }

        result
    }

    pub fn install_pkgs(&mut self, pkgs: &[Pkg]) -> Result<()> {
        let handle = self.h_mut();

        handle
            .trans_init(TransFlag::NONE)
            .map_err(|e| anyhow!("failed to initialize transaction: {e}"))?;

        for pkg in pkgs {
            let package = pkg.clone().into_package_ref(handle)?;

            handle
                .trans_add_pkg(package)
                .map_err(|e| anyhow!("failed to add package to transaction: {e}"))?;
        }

        handle
            .trans_prepare()
            .map_err(|e| anyhow!("failed to prepare transaction: {e}"))?;

        let commit_result = handle.trans_commit();

        match &commit_result {
            Ok(()) => {}
            Err(e) => match e.error() {
                AlpmErr::PkgInvalid => {
                    eprintln!(
                        "[{ANSI_MAGENTA}AUTO REPAIR{ANSI_RESET}] invalid package detected - running automatic repair"
                    );

                    for cachedir in handle.cachedirs().iter() {
                        eprintln!(
                            "[{ANSI_MAGENTA}AUTO REPAIR{ANSI_RESET}] removing broken cache entries from {cachedir}"
                        );

                        let mut removed = 0;

                        let cache_path = Path::new(cachedir);

                        if let Ok(entries) = fs::read_dir(cache_path) {
                            for entry in entries.flatten() {
                                let path = entry.path();

                                fs::remove_file(&path)?;
                                removed += 1;
                            }
                        }

                        eprintln!(
                            "[{ANSI_MAGENTA}AUTO REPAIR{ANSI_RESET}] removed {removed} cache entries from {cachedir}"
                        );
                    }

                    handle
                        .trans_release()
                        .map_err(|e| anyhow!("failed to release transaction: {e}"))?;

                    eprintln!(
                        "[{ANSI_MAGENTA}AUTO REPAIR{ANSI_RESET}] updating the package database"
                    );

                    handle.syncdbs_mut().update(true)?;

                    eprintln!("[{ANSI_MAGENTA}AUTO REPAIR{ANSI_RESET}] updated");

                    // TODO: key reinit

                    return self.install_pkgs(pkgs);
                }
                _ => {
                    eprintln!("[{ANSI_BLUE}TRACE{ANSI_RESET}] Install commit error: {e:?}");
                    commit_result.map_err(|e| anyhow!("failed to commit transaction: {e}"))?
                }
            },
        }

        Ok(())
    }

    pub fn update(&mut self) -> Option<Result<()>> {
        let h = self.h_mut();

        if let Err(e) = h.syncdbs_mut().update(true) {
            return Some(Err(anyhow!("failed to refresh dbs: {e}")));
        }

        if let Err(e) = h.trans_init(TransFlag::NONE) {
            return Some(Err(anyhow!("failed to initialize transaction: {e}")));
        }

        if let Err(e) = h.sync_sysupgrade(false) {
            return Some(Err(anyhow!("failed to upgrade: {e}")));
        }

        if let Err(e) = h.trans_prepare() {
            return Some(Err(anyhow!("failed to prepare transaction: {e}")));
        }

        match h.trans_commit() {
            Err(e) if e.to_string().contains("not prepared") => None,
            Err(e) => Some(Err(anyhow!("failed to commit transaction: {e}"))),
            _ => Some(Ok(())),
        }
    }

    pub fn remove(&mut self, names: &[&str], deep: bool) -> Result<()> {
        let h = self.h_mut();

        h.trans_init(if deep {
            TransFlag::RECURSE | TransFlag::CASCADE | TransFlag::NO_SAVE
        } else {
            TransFlag::NONE
        })?;

        for n in names {
            let pkg = h.localdb().pkg(*n)?;
            h.trans_remove_pkg(pkg)?;
        }

        h.trans_prepare() //
            .map_err(|e| anyhow!("failed to prepare transaction: {e}"))?;

        h.trans_commit() //
            .map_err(|e| anyhow!("failed to commit transaction: {e}"))?;

        Ok(())
    }

    pub fn search(&self, needles: &[&str]) -> Result<Vec<Pkg>> {
        let mut out = Vec::new();

        for db in self.h().syncdbs() {
            out.extend(db.search(needles.iter())?);
        }

        Ok(out.into_iter().map(Pkg::from).collect())
    }

    pub fn unarchive_files_db(archive_path: &Path, extract_to: &Path) -> anyhow::Result<()> {
        let file = fs::File::open(archive_path)
            .with_context(|| format!("failed to open archive: {}", archive_path.display()))?;

        let decoder = Decoder::new(file).context("failed to create zstd decoder")?;

        let mut archive = Archive::new(decoder);

        if extract_to.exists() {
            fs::remove_dir_all(extract_to)
                .with_context(|| format!("failed to delete {}", extract_to.display()))?;
        }

        fs::create_dir_all(extract_to)?;

        for entry_result in archive.entries()? {
            let mut entry = entry_result?;

            let entry_path = match entry.path() {
                Ok(p) => p,
                Err(_) => continue,
            };

            if entry_path.as_os_str().is_empty() || entry_path == Path::new(".") {
                continue;
            }

            let full_path = extract_to.join(&entry_path);

            if entry.header().entry_type().is_dir() {
                fs::create_dir_all(&full_path)?;
                continue;
            }

            if entry.header().entry_type().is_file() {
                if let Some(parent) = full_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                let mut outfile = fs::File::create(&full_path)?;
                io::copy(&mut entry, &mut outfile)?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(mode) = entry.header().mode() {
                        fs::set_permissions(&full_path, fs::Permissions::from_mode(mode))?;
                    }
                }

                continue;
            }
        }

        Ok(())
    }

    pub fn query(&mut self, file: &str, mut fetch: bool) -> Result<Vec<(Pkg, String)>> {
        let cache_dir = self.file_cache_dir();

        if !cache_dir.exists() {
            println!("[{ANSI_BLUE}INFO{ANSI_RESET}] File listing not found, fetching");
            fetch = true;
        }

        if fetch {
            let h = self.h_mut();

            let db_path = Path::new(h.dbpath());
            let sync_dir = db_path.join("sync");

            if sync_dir.exists() {
                for entry in fs::read_dir(&sync_dir)? {
                    let entry = entry?;
                    let path = entry.path();

                    if let Some(filename) = path.file_name().and_then(|n| n.to_str())
                        && filename.ends_with(".files")
                    {
                        let db_name = filename.trim_end_matches(".files");
                        let db_cache_dir = cache_dir.join(db_name);

                        let should_update = if db_cache_dir.exists() {
                            let sync_mtime = fs::metadata(&path)?.modified()?;
                            let cache_mtime = fs::metadata(&db_cache_dir)?.modified()?;
                            sync_mtime > cache_mtime
                        } else {
                            true
                        };

                        if should_update {
                            fs::create_dir_all(&db_cache_dir)?;

                            Self::unarchive_files_db(&path, &db_cache_dir)
                                .map_err(|e| anyhow!("failed to unarchive {}: {}", filename, e))?;
                        }
                    }
                }
            }

            h.set_dbext(".files");
            h.syncdbs_mut()
                .update(false)
                .map_err(|e| anyhow!("failed to refresh dbs: {e}"))?;
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
                    let dir_name = pkg_entry.file_name().to_string_lossy().to_string();
                    let mut parts = dir_name.rsplitn(2, '-');
                    pkg_version = parts.next().unwrap_or("").to_string();
                    pkg_name = parts.next().unwrap_or(&dir_name).to_string();
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

                    if line.ends_with(&format!("/{file}")) {
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
                .map(|f| f.name().to_owned())
                .collect());
        }

        unimplemented!("non-local files");
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
                pb.set_style(progress_bar_style(false).clone());
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
                        pb.set_style(progress_bar_style(true).clone());
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
        pb.set_style(progress_bar_style(false).clone());
        pb.set_message(file.to_string());

        e.insert(pb);
    }

    let pb: &mut ProgressBar = bar_map.get_mut(file).unwrap();

    pb.set_length(percent as u64);
    pb.set_message(format!("{file} {:?} {}/{}", progress, current, how_many));
}
