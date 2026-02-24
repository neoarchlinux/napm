use flate2::read::GzDecoder;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rusqlite::Connection;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::Path,
};
use tar::Archive;

use crate::error::{Error, Result};
use crate::log_warn;
use crate::napm::*;
use crate::util::require_cache;

pub const NAPM_CACHE_FILE: &str = "/var/cache/napm.sqlite";

impl Napm {
    fn init_cache_schema(conn: &Connection) -> Result<()> {
        conn.execute(
            "
            CREATE TABLE package_desc (
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                desc TEXT,
                repo TEXT NOT NULL,
                files_done BOOL NOT NULL,
                CONSTRAINT package_desc_repo_name_unique UNIQUE (repo, name)
            );
            ",
            (),
        )?;

        conn.execute(
            "
            CREATE TABLE package_files (
                repo TEXT NOT NULL,
                name TEXT NOT NULL,
                path TEXT NOT NULL,
                CONSTRAINT package_files_unique UNIQUE (repo, name, path)
            );
            ",
            (),
        )?;

        conn.execute(
            "
            CREATE INDEX idx_package_desc_repo_name ON package_desc(repo, name);
            ",
            (),
        )?;

        conn.execute(
            "
            CREATE INDEX idx_package_files_name ON package_files(name);
            ",
            (),
        )?;

        conn.execute(
            "
            CREATE INDEX idx_package_files_path ON package_files(path);
            ",
            (),
        )?;

        Ok(())
    }

    fn repo_priority(&self) -> String {
        self.repo_priority_with_column_name("repo")
    }

    fn repo_priority_with_column_name(&self, col_name: &str) -> String {
        format!(
            "CASE {col_name} {} ELSE 1000 END",
            self.config
                .repos
                .iter()
                .enumerate()
                .map(|(i, r)| format!("WHEN '{}' THEN {}", r.name, i))
                .collect::<Vec<_>>()
                .join(" ")
        )
    }

    fn pkg_exists(conn: &Connection, pkg_name: &str) -> Result<bool> {
        Ok(conn
            .prepare("SELECT 1 FROM package_desc WHERE name = ?1")?
            .exists([pkg_name])?)
    }

    fn count_archive_files(path: &Path) -> Result<usize> {
        let file = fs::File::open(path).map_err(|_| Error::OpenArchive)?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);
        Ok(archive
            .entries()
            .map_err(|_| Error::ExtractArchive)?
            .count())
    }

    fn process_archive<F>(
        mp: &MultiProgress,
        total_pb: &ProgressBar,
        path: &Path,
        repo: &str,
        action: &str,
        mut f: F,
    ) -> Result<()>
    where
        F: FnMut(&mut tar::Entry<GzDecoder<fs::File>>) -> Result<()>,
    {
        let file = fs::File::open(path).map_err(|_| Error::OpenArchive)?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);

        let len = Self::count_archive_files(path)?;

        let pb = mp.insert_before(total_pb, ProgressBar::new(len as u64));
        pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed:>3}] [{bar:40.cyan/blue}] {percent:>3}% {msg} {pos}/{len}",
            )
            .unwrap()
            .progress_chars("=> "),
        );
        pb.set_message(format!("caching {repo}: {action}..."));
        pb.set_length(Self::count_archive_files(path)? as u64);

        for entry in archive.entries().map_err(|_| Error::ExtractArchive)? {
            pb.inc(1);
            total_pb.inc(1);

            let mut entry = entry.map_err(|_| Error::ExtractArchive)?;
            if !entry.header().entry_type().is_file() {
                continue;
            }

            f(&mut entry)?;
        }

        pb.set_style(
            ProgressStyle::with_template("[{elapsed:>3}] [{bar:40.cyan/blue}] {percent:>3}% {msg}")
                .unwrap()
                .progress_chars("=> "),
        );
        pb.finish_with_message(format!("caching {repo}: {action} done"));

        Ok(())
    }

    fn parse_entry_path(entry: &tar::Entry<GzDecoder<fs::File>>) -> Result<(String, String)> {
        let path = entry.path().map_err(|_| Error::ExtractArchive)?;
        let parts: Vec<_> = path.iter().map(|os| os.to_string_lossy()).collect();
        if parts.len() < 2 {
            return Err(Error::ExtractArchive);
        }
        Ok((parts[0].to_string(), parts[1].to_string()))
    }

    pub fn update_cache(&self) -> Result<()> {
        log_info!("Updating cache");

        let cache_path = Path::new(NAPM_CACHE_FILE);
        let needs_init = !cache_path.exists();
        let mut conn = Connection::open(cache_path)?;

        if needs_init {
            log_warn!("Creating the cache from scratch, this will take some time...");
            Self::init_cache_schema(&conn)?;
        }

        let handle = self.h();
        let sync_dir = Path::new(handle.dbpath()).join("sync");

        let mut total_work = 0usize;

        for entry in fs::read_dir(&sync_dir)? {
            let entry = entry?;
            let path = entry.path();

            let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !fname.ends_with(".files") {
                continue;
            }

            total_work += 2 * Self::count_archive_files(&path)?;
        }

        let mp = MultiProgress::new();
        let total_pb = mp.add(ProgressBar::new(total_work as u64));

        total_pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed:>3}] [{bar:40.cyan/blue}] {percent:>3}% caching total {pos}/{len} ETA {eta}"
            )
            .unwrap()
            .progress_chars("=> "),
        );

        for entry in fs::read_dir(&sync_dir)? {
            let entry = entry?;
            let path = entry.path();

            let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !fname.ends_with(".files") {
                continue;
            }

            let repo = fname.trim_end_matches(".files");

            let already_cached: HashSet<String> = {
                let mut stmt = conn.prepare("SELECT name || '-' || version FROM package_desc WHERE repo = ?1 AND files_done")?;

                stmt.query_map([&repo], |row| row.get(0))?
                    .filter_map(|r| r.ok())
                    .collect()
            };

            let mut id_to_pkg: HashMap<String, String> = HashMap::new();

            Self::process_archive(&mp, &total_pb, &path, repo, "descriptions", |entry| {
                let (identifier, file_name) = Self::parse_entry_path(entry)?;
                if file_name != "desc" || already_cached.contains(&identifier) {
                    return Ok(());
                }

                let mut contents = Vec::new();
                entry.read_to_end(&mut contents)?;
                let contents = String::from_utf8(contents).map_err(|_| Error::ExtractArchive)?;

                let mut name = None;
                let mut version = None;
                let mut desc = None;

                let mut lines = contents.lines();
                while let Some(tag) = lines.next() {
                    match tag {
                        "%NAME%" => name = lines.next().map(str::to_string),
                        "%VERSION%" => version = lines.next().map(str::to_string),
                        "%DESC%" => desc = lines.next().map(str::to_string),
                        _ => {}
                    }
                }

                let pkg_name = name.clone().unwrap();
                id_to_pkg.insert(identifier.clone(), pkg_name.clone());

                let pkg = Pkg {
                    repo: repo.to_string(),
                    name: pkg_name,
                    version: version.unwrap(),
                    desc: desc.unwrap_or_default(),
                };

                conn.execute(
                    "INSERT OR REPLACE INTO package_desc (name, version, desc, repo, files_done) VALUES (?1, ?2, ?3, ?4, false)",
                    (&pkg.name, &pkg.version, &pkg.desc, &pkg.repo),
                )?;

                Ok(())
            })?;

            Self::process_archive(&mp, &total_pb, &path, repo, "files", |entry| {
                let (identifier, file_name) = Self::parse_entry_path(entry)?;
                if file_name != "files" || already_cached.contains(&identifier) {
                    return Ok(());
                }

                if let Some(package_name) = id_to_pkg.get(&identifier) {
                    let tx = conn.transaction()?;

                    tx.execute(
                        "DELETE FROM package_files WHERE repo = ?1 AND name = ?2",
                        (&repo, &package_name),
                    )?;

                    let mut contents = Vec::new();
                    entry.read_to_end(&mut contents)?;
                    let contents =
                        String::from_utf8(contents).map_err(|_| Error::ExtractArchive)?;

                    for line in contents.lines().skip(1) {
                        tx.execute(
                            "INSERT INTO package_files (repo, name, path) VALUES (?1, ?2, ?3)",
                            (&repo, &package_name, &line),
                        )?;
                    }

                    tx.execute(
                        "UPDATE package_desc SET files_done = true WHERE repo = ?1 AND name = ?2",
                        (&repo, &package_name),
                    )?;

                    tx.commit()?;
                } else {
                    log_warn!("Package {identifier} found in files, but not in desc");
                }

                Ok(())
            })?;
        }

        total_pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed:>3}] [{bar:40.cyan/blue}] {percent:>3}% caching done",
            )
            .unwrap()
            .progress_chars("=> "),
        );
        total_pb.finish();

        Ok(())
    }

    pub fn info(&self, pkg_name: &str) -> Result<Pkg> {
        require_cache()?;

        let cache_path = Path::new(NAPM_CACHE_FILE);

        let conn = Connection::open(cache_path)?;

        let mut stmt = conn.prepare(&format!(
            "
            SELECT name, version, repo, desc
            FROM package_desc
            WHERE name = ?1 AND repo = (
                SELECT repo
                FROM package_desc
                WHERE name = ?1
                ORDER BY {}
                LIMIT 1
            )
            ",
            self.repo_priority()
        ))?;

        use rusqlite::Error as E;
        match stmt.query_one([pkg_name], |row| {
            Ok(Pkg {
                name: row.get(0)?,
                version: row.get(1)?,
                desc: row.get(2)?,
                repo: row.get(3)?,
            })
        }) {
            Ok(pkg) => Ok(pkg),
            Err(E::QueryReturnedNoRows) => Err(Error::PackageNotFound(pkg_name.to_string())),
            Err(err) => Err(Error::CacheDatabaseError(err)),
        }
    }

    pub fn files(&self, pkg_name: &str, with_dirs: bool) -> Result<Vec<String>> {
        require_cache()?;

        let cache_path = Path::new(NAPM_CACHE_FILE);

        let conn = Connection::open(cache_path)?;

        if !Self::pkg_exists(&conn, pkg_name)? {
            return Err(Error::PackageNotFound(pkg_name.to_string()));
        }

        let mut stmt = conn.prepare(&format!(
            "
            SELECT '/' || path
            FROM package_files
            WHERE name = ?1 AND repo = (
                SELECT repo
                FROM package_desc
                WHERE name = ?1
                ORDER BY {}
                LIMIT 1
            ) {}
            ",
            self.repo_priority(),
            if with_dirs {
                ""
            } else {
                "AND path NOT LIKE '%/'"
            }
        ))?;

        Ok(stmt
            .query_map([pkg_name], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect())
    }

    pub fn find_packages_by_file(&self, path: &str, exact: bool) -> Result<Vec<(Pkg, String)>> {
        require_cache()?;

        let cache_path = Path::new(NAPM_CACHE_FILE);

        let conn = Connection::open(cache_path)?;

        let mut stmt = conn.prepare(&format!(
            "
            SELECT
                d.name,
                d.version,
                d.desc,
                d.repo,
                '/' || f.path
            FROM package_files AS f
            JOIN package_desc  AS d ON f.name = d.name AND f.repo = d.repo
            WHERE
                {}
            AND d.repo = (
                SELECT d2.repo
                FROM package_desc AS d2
                WHERE d2.name = d.name
                ORDER BY {}
                LIMIT 1
            )
            ORDER BY d.name, f.path;
            ",
            if exact {
                "'/' || f.path = ?1"
            } else {
                "'/' || f.path LIKE ?1"
            },
            self.repo_priority_with_column_name("d2.repo"),
        ))?;

        Ok(stmt
            .query_map(
                [&if exact {
                    path.to_string()
                } else {
                    format!("%{path}")
                }],
                |row| {
                    Ok((
                        Pkg {
                            name: row.get(0)?,
                            version: row.get(1)?,
                            desc: row.get(2)?,
                            repo: row.get(3)?,
                        },
                        row.get(4)?,
                    ))
                },
            )?
            .filter_map(|r| r.ok())
            .collect())
    }

    fn tokenize(s: &str) -> Vec<String> {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| w.to_lowercase())
            .collect()
    }

    fn select_candidates(&self, conn: &Connection, query_words: &[String]) -> Result<Vec<Pkg>> {
        let mut where_clauses = Vec::new();
        let mut params = Vec::new();

        for q in query_words {
            where_clauses.push("(LOWER(name) LIKE ? OR LOWER(desc) LIKE ?)");
            let like = format!("%{}%", q);
            params.push(like.clone());
            params.push(like);
        }

        let sql = format!(
            "
            WITH matched AS (
                SELECT *
                FROM package_desc
                WHERE {}
            )
            SELECT name, version, desc, repo
            FROM matched AS d
            WHERE repo = (
                SELECT repo
                FROM matched AS d2
                WHERE d2.name = d.name
                ORDER BY {}
                LIMIT 1
            )
            ",
            where_clauses.join(" OR "),
            self.repo_priority_with_column_name("d2.repo")
        );

        let mut stmt = conn.prepare(&sql)?;

        let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
            Ok(Pkg {
                name: row.get(0)?,
                version: row.get(1)?,
                desc: row.get(2)?,
                repo: row.get(3)?,
            })
        })?;

        Ok(rows.filter_map(rusqlite::Result::ok).collect())
    }

    fn levenshtein_cutoff(a: &str, b: &str, max_dist: usize) -> Option<usize> {
        let la = a.len();
        let lb = b.len();

        if la.abs_diff(lb) > max_dist {
            return None;
        }

        let mut prev: Vec<usize> = (0..=lb).collect();
        let mut curr = vec![0; lb + 1];

        for (i, ca) in a.chars().enumerate() {
            curr[0] = i + 1;
            let mut min_row = curr[0];

            for (j, cb) in b.chars().enumerate() {
                let cost = if ca == cb { 0 } else { 1 };
                curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);

                min_row = min_row.min(curr[j + 1]);
            }

            if min_row > max_dist {
                return None;
            }

            std::mem::swap(&mut prev, &mut curr);
        }

        let d = prev[lb];
        (d <= max_dist).then_some(d)
    }

    fn expand_query_words(conn: &Connection, query_words: &[String]) -> Result<Vec<String>> {
        let mut stmt = conn.prepare("SELECT DISTINCT LOWER(name) FROM package_desc")?;

        let dict: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(rusqlite::Result::ok)
            .collect();

        const MAX_DISTANCE: usize = 2;
        const MAX_LEN_DIFF: usize = 2;

        let mut expanded = std::collections::HashSet::new();

        for q in query_words {
            expanded.insert(q.clone());

            for w in &dict {
                if w.len().abs_diff(q.len()) > MAX_LEN_DIFF {
                    continue;
                }

                if Self::levenshtein_cutoff(w, q, MAX_DISTANCE).is_some() {
                    expanded.insert(w.clone());
                }
            }
        }

        Ok(expanded.into_iter().collect())
    }

    fn compute_df(candidates: &[Pkg], query_words: &[String]) -> HashMap<String, usize> {
        let mut df = HashMap::new();

        for pkg in candidates {
            let text = format!("{} {}", pkg.name.to_lowercase(), pkg.desc.to_lowercase());

            let tokens = Self::tokenize(&text);

            for q in query_words {
                if tokens.iter().any(|t| t == q) {
                    *df.entry(q.clone()).or_insert(0) += 1;
                }
            }
        }

        df
    }

    fn fuzzy_weight(d: usize) -> f64 {
        (3 - d) as f64
    }

    fn score_packages(
        candidates: Vec<Pkg>,
        query_words: &[String],
        df: &HashMap<String, usize>,
    ) -> Vec<(f64, Pkg)> {
        const MAX_DISTANCE: usize = 2;
        const MAX_LEN_DIFF: usize = 2;

        let total_docs = candidates.len().max(1) as f64;
        let mut scored = Vec::new();

        for pkg in candidates {
            let mut score = 0.0;

            let name_lc = pkg.name.to_lowercase();
            let desc_lc = pkg.desc.to_lowercase();
            let desc_tokens = Self::tokenize(&desc_lc);

            for q in query_words {
                let df_q = *df.get(q).unwrap_or(&1) as f64;
                let idf = (total_docs / df_q).ln();

                if name_lc.contains(q) {
                    score += 5.0 * idf;
                }

                if desc_tokens.contains(q) {
                    score += 1.5 * idf;
                }

                for token in
                    std::iter::once(name_lc.as_str()).chain(desc_tokens.iter().map(String::as_str))
                {
                    if token.len().abs_diff(q.len()) > MAX_LEN_DIFF {
                        continue;
                    }

                    if let Some(d) = Self::levenshtein_cutoff(token, q, MAX_DISTANCE) {
                        score += Self::fuzzy_weight(d) * idf;
                    }
                }
            }

            if score > 0.0 {
                scored.push((score, pkg));
            }
        }

        scored
    }

    pub fn search(&self, search_terms: Vec<String>) -> Result<Vec<Pkg>> {
        require_cache()?;

        let conn = Connection::open(NAPM_CACHE_FILE)?;

        let query = search_terms.join(" ");
        let query_words = Self::tokenize(&query);

        if query_words.is_empty() {
            return Ok(Vec::new());
        }

        let expanded = Self::expand_query_words(&conn, &query_words)?;
        let candidates = self.select_candidates(&conn, &expanded)?;

        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let df = Self::compute_df(&candidates, &query_words);
        let mut scored = Self::score_packages(candidates, &query_words, &df);

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

        Ok(scored.into_iter().map(|(_, pkg)| pkg).collect())
    }
}
