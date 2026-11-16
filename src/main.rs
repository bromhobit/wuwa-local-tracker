#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use regex::{bytes::Regex as BytesRegex, Regex};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::Manager;
use url::{form_urlencoded, Url};
use walkdir::{DirEntry, WalkDir};

const GACHA_URL_PATTERN: &str =
    r#"https?://[^\s"'\]\)]+/aki/gacha/index\.html#/record\?[^\s"'\]\),]+"#;

const POOLS: [i64; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 12, 13];

const DIRECT_LOG_SUBPATHS: [&str; 2] = [
    "Wuthering Waves/Wuthering Waves Game/Client/Saved/Logs",
    "Wuthering Waves/Client/Saved/Logs",
];

const WINE_PREFIX_GAME_SUBPATHS: [&str; 8] = [
    "Wuthering Waves/Wuthering Waves Game/Client/Saved/Logs",
    "Wuthering Waves/Client/Saved/Logs",
    "Wuthering Waves Game/Client/Saved/Logs",
    "Program Files/Wuthering Waves/Wuthering Waves Game/Client/Saved/Logs",
    "Program Files/Wuthering Waves/Client/Saved/Logs",
    "Program Files/Epic Games/WutheringWavesj3oFh/Wuthering Waves Game/Client/Saved/Logs",
    "Program Files/Epic Games/WutheringWavesj3oFh/Client/Saved/Logs",
    "Games/Wuthering Waves/Wuthering Waves Game/Client/Saved/Logs",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Pull {
    player: String,
    pool: i64,
    time: String,
    seq: i64,
    name: String,
    quality: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryPayload<'a> {
    player_id: &'a str,
    record_id: &'a str,
    server_id: &'a str,
    card_pool_id: &'a str,
    card_pool_type: i64,
    language_code: &'a str,
}

fn db_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Не удалось открыть папку данных: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("Не удалось создать папку данных: {e}"))?;
    Ok(dir.join("history.sqlite3"))
}

fn open_db(app: &tauri::AppHandle) -> Result<Connection, String> {
    let path = db_path(app)?;
    let conn = Connection::open(path).map_err(|e| format!("Ошибка базы данных: {e}"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS pulls (
            player TEXT NOT NULL,
            pool INTEGER NOT NULL,
            time TEXT NOT NULL,
            seq INTEGER NOT NULL,
            name TEXT NOT NULL,
            quality INTEGER NOT NULL,
            PRIMARY KEY(player,pool,time,seq)
        );",
    )
    .map_err(|e| format!("Ошибка схемы базы данных: {e}"))?;
    Ok(conn)
}

fn insert_pulls(conn: &mut Connection, pulls: &[Pull]) -> Result<usize, String> {
    let tx = conn
        .transaction()
        .map_err(|e| format!("Ошибка транзакции: {e}"))?;
    let mut inserted = 0usize;
    {
        let mut stmt = tx
            .prepare("INSERT OR IGNORE INTO pulls VALUES (?1,?2,?3,?4,?5,?6)")
            .map_err(|e| format!("Ошибка подготовки SQL: {e}"))?;
        for p in pulls {
            let changed = stmt
                .execute(params![p.player, p.pool, p.time, p.seq, p.name, p.quality])
                .map_err(|e| format!("Ошибка записи в базу: {e}"))?;
            inserted += changed;
        }
    }
    tx.commit().map_err(|e| format!("Ошибка сохранения: {e}"))?;
    Ok(inserted)
}

#[tauri::command]
fn history(app: tauri::AppHandle) -> Result<Vec<Pull>, String> {
    let conn = open_db(&app)?;
    let mut stmt = conn
        .prepare("SELECT player,pool,time,seq,name,quality FROM pulls ORDER BY player,pool,time,seq")
        .map_err(|e| format!("Ошибка запроса: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Pull {
                player: row.get(0)?,
                pool: row.get(1)?,
                time: row.get(2)?,
                seq: row.get(3)?,
                name: row.get(4)?,
                quality: row.get(5)?,
            })
        })
        .map_err(|e| format!("Ошибка чтения истории: {e}"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("Ошибка строки истории: {e}"))?);
    }
    Ok(out)
}

#[tauri::command]
fn import_backup(app: tauri::AppHandle, data: String) -> Result<usize, String> {
    let pulls: Vec<Pull> =
        serde_json::from_str(&data).map_err(|_| "Некорректная запись в файле".to_string())?;
    for p in &pulls {
        if p.player.trim().is_empty()
            || p.time.trim().is_empty()
            || p.name.trim().is_empty()
            || p.pool <= 0
            || p.seq < 0
            || !(1..=5).contains(&p.quality)
        {
            return Err("Некорректная запись в файле".to_string());
        }
    }
    let mut conn = open_db(&app)?;
    insert_pulls(&mut conn, &pulls)
}

#[tauri::command]
async fn sync(app: tauri::AppHandle, link: String) -> Result<usize, String> {
    let parsed = Url::parse(link.trim()).map_err(|_| "Некорректная ссылка".to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("Ожидается ссылка истории Kuro Games".to_string());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "Ожидается ссылка истории Kuro Games".to_string())?;
    let host_lower = host.to_ascii_lowercase();
    if !(host_lower == "aki-game.net"
        || host_lower == "aki-game.com"
        || host_lower.ends_with(".aki-game.net")
        || host_lower.ends_with(".aki-game.com"))
    {
        return Err("Ожидается ссылка истории Kuro Games".to_string());
    }

    if !parsed.path().ends_with("/aki/gacha/index.html") {
        return Err("Некорректный путь истории".to_string());
    }

    let fragment = parsed
        .fragment()
        .ok_or_else(|| "Некорректный путь истории".to_string())?;
    let (frag_path, query) = fragment
        .split_once('?')
        .ok_or_else(|| "В ссылке нет параметров".to_string())?;
    if frag_path != "/record" && !frag_path.ends_with("/record") {
        return Err("Некорректный путь истории".to_string());
    }
    if query.is_empty() {
        return Err("В ссылке нет параметров".to_string());
    }

    let params_map: HashMap<String, String> = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();

    let required = |key: &str| -> Result<String, String> {
        params_map
            .get(key)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("В ссылке отсутствует {key}"))
    };

    let player_id = required("player_id")?;
    let record_id = required("record_id")?;
    let svr_id = required("svr_id")?;
    let resources_id = required("resources_id")?;
    let lang = params_map
        .get("lang")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("en")
        .to_string();
    let api_url = match params_map.get("svr_area").map(String::as_str) {
        Some("cn") => "https://gmserver-api.aki-game2.com/gacha/record/query",
        _ => "https://gmserver-api.aki-game2.net/gacha/record/query",
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Mozilla/5.0")
        .build()
        .map_err(|_| "Ошибка соединения с игровым API".to_string())?;

    let mut all_pulls = Vec::new();
    for pool in POOLS {
        let payload = QueryPayload {
            player_id: &player_id,
            record_id: &record_id,
            server_id: &svr_id,
            card_pool_id: &resources_id,
            card_pool_type: pool,
            language_code: &lang,
        };

        let resp = client
            .post(api_url)
            .json(&payload)
            .send()
            .await
            .map_err(|_| "Ошибка соединения с игровым API".to_string())?;

        if !resp.status().is_success() {
            return Err("Ошибка HTTP игрового API".to_string());
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|_| "Не удалось прочитать ответ API".to_string())?;

        let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            let reason = body
                .get("message")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("Причина не указана.");
            return Err(format!(
                "API отклонил запрос для баннера {pool} (код {code}). {reason} Откройте историю круток в игре заново, дождитесь её загрузки и сразу нажмите «Найти в Steam», затем «Обновить». Если ошибка повторится, проверьте, открывается ли история внутри игры."
            ));
        }

        let items = match body.get("data") {
            Some(serde_json::Value::Array(arr)) => arr.as_slice(),
            Some(serde_json::Value::Null) | None => &[],
            _ => return Err("Не удалось прочитать ответ API".to_string()),
        };

        let mut seq_by_time: HashMap<String, i64> = HashMap::new();
        for item in items {
            let time = item
                .get("time")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "В ответе нет даты".to_string())?
                .to_string();
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "В ответе нет названия".to_string())?
                .to_string();
            let quality = item
                .get("qualityLevel")
                .and_then(|v| {
                    v.as_i64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                })
                .ok_or_else(|| "В ответе нет редкости".to_string())?;

            let counter = seq_by_time.entry(time.clone()).or_insert(0);
            *counter += 1;

            all_pulls.push(Pull {
                player: player_id.clone(),
                pool,
                time,
                seq: *counter,
                name,
                quality,
            });
        }
    }

    let mut conn = open_db(&app)?;
    insert_pulls(&mut conn, &all_pulls)
}

/// Extract `"path"` values from a Steam `libraryfolders.vdf` file and map them to `steamapps` paths.
fn parse_libraryfolders_vdf(content: &str) -> Vec<PathBuf> {
    let re = Regex::new(r#"(?i)"path"\s+"([^"]+)""#).expect("valid VDF path regex");
    let mut libs = Vec::new();
    for cap in re.captures_iter(content) {
        let raw = cap[1].replace("\\\\", "\\");
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = PathBuf::from(trimmed);
        let is_steamapps = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("steamapps"));
        libs.push(if is_steamapps {
            path
        } else {
            path.join("steamapps")
        });
    }
    libs
}

fn push_unique_existing_dir(target: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, path: PathBuf) {
    if !path.is_dir() {
        return;
    }
    let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if seen.insert(key) {
        target.push(path);
    }
}

/// Collect candidate Steam installation roots across Linux, macOS, and Windows.
fn collect_steam_roots(home: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();

    if let Some(h) = home {
        for rel in [
            ".local/share/Steam",
            ".steam/steam",
            ".steam/root",
            ".steam/debian-installation",
            "steam",
            "Steam",
            ".var/app/com.valvesoftware.Steam/.local/share/Steam",
            ".var/app/com.valvesoftware.Steam/.steam/steam",
            ".var/app/com.valvesoftware.Steam/.steam/root",
            "snap/steam/common/.local/share/Steam",
            "snap/steam/common/.steam/steam",
            "Library/Application Support/Steam",
        ] {
            push_unique_existing_dir(&mut roots, &mut seen, h.join(rel));
        }
    }

    if let Some(xdg_data) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        push_unique_existing_dir(&mut roots, &mut seen, PathBuf::from(xdg_data).join("Steam"));
    }

    #[cfg(windows)]
    {
        for path in windows_registry_steam_paths() {
            push_unique_existing_dir(&mut roots, &mut seen, path);
        }
    }

    if cfg!(windows) {
        for var in ["PROGRAMFILES(X86)", "PROGRAMFILES", "LOCALAPPDATA"] {
            if let Some(val) = std::env::var_os(var).filter(|v| !v.is_empty()) {
                push_unique_existing_dir(&mut roots, &mut seen, PathBuf::from(val).join("Steam"));
            }
        }
        for drive in 'C'..='Z' {
            let root = PathBuf::from(format!("{drive}:\\"));
            if !root.exists() {
                continue;
            }
            for rel in [
                "Steam",
                "SteamLibrary",
                "Games\\Steam",
                "Games\\SteamLibrary",
                "Program Files (x86)\\Steam",
                "Program Files\\Steam",
            ] {
                push_unique_existing_dir(&mut roots, &mut seen, root.join(rel));
            }
        }
    }

    roots
}

/// Discover all `steamapps` directories from Steam roots and their `libraryfolders.vdf` files.
fn collect_steamapps_dirs(steam_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut steamapps = Vec::new();
    let mut seen = HashSet::new();

    for root in steam_roots {
        push_unique_existing_dir(&mut steamapps, &mut seen, root.join("steamapps"));
        for vdf_rel in ["steamapps/libraryfolders.vdf", "config/libraryfolders.vdf"] {
            let vdf_path = root.join(vdf_rel);
            if let Ok(bytes) = fs::read(&vdf_path) {
                let text = String::from_utf8_lossy(&bytes);
                for lib in parse_libraryfolders_vdf(&text) {
                    push_unique_existing_dir(&mut steamapps, &mut seen, lib);
                }
            }
        }
    }

    steamapps
}

#[cfg(windows)]
fn windows_registry_steam_paths() -> Vec<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut out = Vec::new();
    for (key, value) in [
        (r"HKCU\Software\Valve\Steam", "SteamPath"),
        (r"HKLM\SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
        (r"HKLM\SOFTWARE\Valve\Steam", "InstallPath"),
    ] {
        let Ok(res) = std::process::Command::new("reg")
            .args(["query", key, "/v", value])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        else {
            continue;
        };
        if !res.status.success() {
            continue;
        }
        let stdout = String::from_utf8_lossy(&res.stdout);
        for line in stdout.lines() {
            if let Some((_, rest)) = line.split_once("REG_SZ") {
                let candidate = rest.trim().replace('/', "\\");
                if !candidate.is_empty() {
                    out.push(PathBuf::from(candidate));
                }
            }
        }
    }
    out
}

/// Extract `InstallLocation` paths from Epic Games `.item` manifest JSON contents.
fn parse_epic_manifest_install_location(content: &str) -> Option<PathBuf> {
    let val: serde_json::Value = serde_json::from_str(content).ok()?;
    val.get("InstallLocation")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Extract game directory/executable/prefix paths from a Lutris YAML config file.
fn parse_lutris_yaml_paths(content: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        for key in ["directory:", "prefix:", "exe:"] {
            if let Some(rest) = trimmed.strip_prefix(key) {
                let raw = rest.trim().trim_matches(|c| c == '"' || c == '\'');
                if raw.is_empty() {
                    continue;
                }
                let p = PathBuf::from(raw);
                if key == "exe:" {
                    if let Some(parent) = p.parent() {
                        paths.push(parent.to_path_buf());
                    }
                } else {
                    paths.push(p);
                }
            }
        }
    }
    paths
}

fn push_wine_prefix_log_dirs(
    prefix_root: &Path,
    direct_log_dirs: &mut Vec<PathBuf>,
    seen_logs: &mut HashSet<PathBuf>,
) {
    for drive_c in [prefix_root.join("drive_c"), prefix_root.join("pfx/drive_c")] {
        if drive_c.is_dir() {
            for sub in WINE_PREFIX_GAME_SUBPATHS {
                push_unique_existing_dir(direct_log_dirs, seen_logs, drive_c.join(sub));
            }
        }
    }
}

/// Collect Heroic, Lutris, and Bottles game/prefix directories on Linux/macOS.
fn collect_launcher_dirs(home: Option<&Path>) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut install_dirs = Vec::new();
    let mut seen_installs = HashSet::new();
    let mut wine_prefixes = Vec::new();
    let mut seen_prefixes = HashSet::new();

    let Some(h) = home else {
        return (install_dirs, wine_prefixes);
    };

    let heroic_bases = [
        h.join(".config/heroic"),
        h.join(".var/app/com.heroicgameslauncher.hgl/config/heroic"),
        h.join("Library/Application Support/heroic"),
    ];

    for base in heroic_bases {
        let legendary_installed = base.join("legendaryConfig/legendary/installed.json");
        if let Ok(text) = fs::read_to_string(&legendary_installed) {
            if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&text) {
                for entry in map.values() {
                    if let Some(p) = entry.get("install_path").and_then(|v| v.as_str()) {
                        push_unique_existing_dir(
                            &mut install_dirs,
                            &mut seen_installs,
                            PathBuf::from(p),
                        );
                    }
                }
            }
        }

        let sideload_lib = base.join("sideload_apps/library.json");
        if let Ok(text) = fs::read_to_string(&sideload_lib) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(arr) = val.get("apps").and_then(|v| v.as_array()) {
                    for app in arr {
                        if let Some(exe) = app
                            .get("install")
                            .and_then(|i| i.get("executable"))
                            .and_then(|v| v.as_str())
                        {
                            if let Some(parent) = Path::new(exe).parent() {
                                push_unique_existing_dir(
                                    &mut install_dirs,
                                    &mut seen_installs,
                                    parent.to_path_buf(),
                                );
                            }
                        }
                    }
                }
            }
        }

        let games_config = base.join("GamesConfig");
        if let Ok(entries) = fs::read_dir(&games_config) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(text) = fs::read_to_string(entry.path()) {
                    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&text) {
                        for val in map.values() {
                            if let Some(pfx) = val.get("winePrefix").and_then(|v| v.as_str()) {
                                push_unique_existing_dir(
                                    &mut wine_prefixes,
                                    &mut seen_prefixes,
                                    PathBuf::from(pfx),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    for lutris_dir in [
        h.join(".local/share/lutris/games"),
        h.join(".config/lutris/games"),
    ] {
        if let Ok(entries) = fs::read_dir(&lutris_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str());
                if !matches!(ext, Some("yml") | Some("yaml")) {
                    continue;
                }
                if let Ok(text) = fs::read_to_string(&path) {
                    for candidate in parse_lutris_yaml_paths(&text) {
                        push_unique_existing_dir(
                            &mut install_dirs,
                            &mut seen_installs,
                            candidate.clone(),
                        );
                        push_unique_existing_dir(&mut wine_prefixes, &mut seen_prefixes, candidate);
                    }
                }
            }
        }
    }

    for bottles_root in [
        h.join(".local/share/bottles/bottles"),
        h.join(".var/app/com.usebottles.bottles/data/bottles/bottles"),
    ] {
        if let Ok(entries) = fs::read_dir(&bottles_root) {
            for entry in entries.flatten() {
                push_unique_existing_dir(&mut wine_prefixes, &mut seen_prefixes, entry.path());
            }
        }
    }

    (install_dirs, wine_prefixes)
}

/// Collect direct `Saved/Logs` directories and bounded fallback walk roots across all platforms.
fn collect_search_targets(
    home: Option<&Path>,
    steam_roots: &[PathBuf],
    steamapps_dirs: &[PathBuf],
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut direct_log_dirs = Vec::new();
    let mut seen_logs = HashSet::new();
    let mut walk_roots = Vec::new();
    let mut seen_walks = HashSet::new();

    for steamapps in steamapps_dirs {
        for sub in DIRECT_LOG_SUBPATHS {
            push_unique_existing_dir(
                &mut direct_log_dirs,
                &mut seen_logs,
                steamapps.join("common").join(sub),
            );
        }

        // Check Non-Steam Proton prefixes in compatdata/<id>/pfx/drive_c without walking all Wine system files.
        let compatdata = steamapps.join("compatdata");
        if let Ok(entries) = fs::read_dir(&compatdata) {
            for entry in entries.flatten() {
                push_wine_prefix_log_dirs(&entry.path(), &mut direct_log_dirs, &mut seen_logs);
            }
        }

        push_unique_existing_dir(&mut walk_roots, &mut seen_walks, steamapps.join("common"));
        push_unique_existing_dir(&mut walk_roots, &mut seen_walks, steamapps.clone());
    }

    for root in steam_roots {
        push_unique_existing_dir(&mut walk_roots, &mut seen_walks, root.clone());
    }

    let (launcher_dirs, wine_prefixes) = collect_launcher_dirs(home);
    for dir in launcher_dirs {
        push_unique_existing_dir(
            &mut direct_log_dirs,
            &mut seen_logs,
            dir.join("Client/Saved/Logs"),
        );
        push_unique_existing_dir(
            &mut direct_log_dirs,
            &mut seen_logs,
            dir.join("Wuthering Waves Game/Client/Saved/Logs"),
        );
        push_unique_existing_dir(&mut walk_roots, &mut seen_walks, dir);
    }
    for prefix in wine_prefixes {
        push_wine_prefix_log_dirs(&prefix, &mut direct_log_dirs, &mut seen_logs);
    }

    if let Some(h) = home {
        let games = h.join("Games");
        if games.is_dir() {
            for sub in DIRECT_LOG_SUBPATHS {
                push_unique_existing_dir(&mut direct_log_dirs, &mut seen_logs, games.join(sub));
            }
            push_unique_existing_dir(&mut walk_roots, &mut seen_walks, games);
        }
    }

    if cfg!(windows) {
        if let Some(prog_data) = std::env::var_os("PROGRAMDATA").filter(|v| !v.is_empty()) {
            let manifests = PathBuf::from(prog_data).join("Epic\\EpicGamesLauncher\\Data\\Manifests");
            if let Ok(entries) = fs::read_dir(&manifests) {
                for entry in entries.flatten() {
                    if entry.path().extension().and_then(|e| e.to_str()) != Some("item") {
                        continue;
                    }
                    if let Ok(text) = fs::read_to_string(entry.path()) {
                        if let Some(loc) = parse_epic_manifest_install_location(&text) {
                            push_unique_existing_dir(
                                &mut direct_log_dirs,
                                &mut seen_logs,
                                loc.join("Client\\Saved\\Logs"),
                            );
                            push_unique_existing_dir(
                                &mut direct_log_dirs,
                                &mut seen_logs,
                                loc.join("Wuthering Waves Game\\Client\\Saved\\Logs"),
                            );
                            push_unique_existing_dir(&mut walk_roots, &mut seen_walks, loc);
                        }
                    }
                }
            }
        }

        let mut bases = Vec::new();
        for var in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
            if let Some(val) = std::env::var_os(var).filter(|v| !v.is_empty()) {
                bases.push(PathBuf::from(val));
            }
        }
        for drive in 'C'..='Z' {
            let root = PathBuf::from(format!("{drive}:\\"));
            if root.exists() {
                bases.push(root.clone());
                bases.push(root.join("Games"));
                bases.push(root.join("Program Files"));
                bases.push(root.join("Program Files (x86)"));
            }
        }
        for base in bases {
            for rel in [
                "Wuthering Waves\\Wuthering Waves Game\\Client\\Saved\\Logs",
                "Wuthering Waves\\Client\\Saved\\Logs",
                "Wuthering Waves Game\\Client\\Saved\\Logs",
                "Epic Games\\WutheringWavesj3oFh\\Wuthering Waves Game\\Client\\Saved\\Logs",
                "Epic Games\\WutheringWavesj3oFh\\Client\\Saved\\Logs",
            ] {
                push_unique_existing_dir(&mut direct_log_dirs, &mut seen_logs, base.join(rel));
            }
        }
    }

    (direct_log_dirs, walk_roots)
}

fn should_descend(entry: &DirEntry) -> bool {
    if entry.depth() == 0 || !entry.file_type().is_dir() {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    let lower = name.to_ascii_lowercase();
    !matches!(
        lower.as_str(),
        "dosdevices"
            | "compatdata"
            | "shadercache"
            | "depotcache"
            | "downloading"
            | "temp"
            | "workshop"
            | "crashes"
            | "aftermathshaderdebuginfo_backup"
            | "ngx"
            | "windows"
            | "system32"
            | "syswow64"
            | "cache"
            | "gpucache"
            | "code cache"
            | "ebwebview"
            | "krwebviewuserdata"
    )
}

fn is_saved_logs_file(path: &Path) -> bool {
    let ext_ok = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("log"));
    if !ext_ok {
        return false;
    }

    let parts: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str().map(|t| t.to_ascii_lowercase()),
            _ => None,
        })
        .collect();

    if parts.iter().any(|p| p == "crashes") {
        return false;
    }

    parts.windows(2).any(|w| w[0] == "saved" && w[1] == "logs")
}

fn collect_logs_from_dirs(dirs: &[PathBuf], max_depth: usize) -> Vec<(SystemTime, bool, PathBuf)> {
    let mut seen_files = HashSet::new();
    let mut results = Vec::new();

    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        let walker = WalkDir::new(dir)
            .follow_links(true)
            .max_depth(max_depth)
            .into_iter()
            .filter_entry(should_descend);

        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.into_path();
            if !is_saved_logs_file(&path) {
                continue;
            }
            let canon = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if !seen_files.insert(canon.clone()) {
                continue;
            }
            let mtime = fs::metadata(&canon)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let is_primary_client_log = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case("Client.log"));
            results.push((mtime, is_primary_client_log, path));
        }
    }

    // Newest first, prioritizing primary Client.log on identical timestamps.
    results.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    results
}

fn extract_url_from_bytes(raw: &[u8], re: &BytesRegex) -> Option<String> {
    let raw_match = re.find_iter(raw).last().map(|m| (m.end(), m.as_bytes().to_vec()));

    let decoded: Vec<u8> = raw
        .iter()
        .map(|&b| b ^ if (b & 1) != 0 { 0xa5 } else { 0xef })
        .collect();
    let dec_match = re
        .find_iter(&decoded)
        .last()
        .map(|m| (m.end(), m.as_bytes().to_vec()));

    let best = match (raw_match, dec_match) {
        (Some(a), Some(b)) => {
            if b.0 >= a.0 {
                Some(b.1)
            } else {
                Some(a.1)
            }
        }
        (Some(a), None) => Some(a.1),
        (None, Some(b)) => Some(b.1),
        (None, None) => None,
    }?;

    String::from_utf8(best).ok()
}

fn find_url_in_targets(direct_log_dirs: &[PathBuf], walk_roots: &[PathBuf]) -> Result<String, String> {
    let re = BytesRegex::new(GACHA_URL_PATTERN).expect("valid gacha URL regex");

    let direct_logs = collect_logs_from_dirs(direct_log_dirs, 2);
    for (_, _, path) in &direct_logs {
        if let Ok(bytes) = fs::read(path) {
            if let Some(url) = extract_url_from_bytes(&bytes, &re) {
                return Ok(url);
            }
        }
    }

    let direct_set: HashSet<PathBuf> = direct_logs
        .into_iter()
        .map(|(_, _, p)| fs::canonicalize(&p).unwrap_or(p))
        .collect();

    let fallback_logs = collect_logs_from_dirs(walk_roots, 10);
    for (_, _, path) in fallback_logs {
        let canon = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if direct_set.contains(&canon) {
            continue;
        }
        if let Ok(bytes) = fs::read(&path) {
            if let Some(url) = extract_url_from_bytes(&bytes, &re) {
                return Ok(url);
            }
        }
    }

    Err("Ссылка не найдена. Откройте историю круток в игре или вставьте ссылку вручную.".to_string())
}

fn find_url_impl() -> Result<String, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);

    if home.is_none() && !cfg!(windows) {
        return Err("Не удалось найти домашнюю папку".to_string());
    }

    let steam_roots = collect_steam_roots(home.as_deref());
    let steamapps_dirs = collect_steamapps_dirs(&steam_roots);
    let (direct_log_dirs, walk_roots) =
        collect_search_targets(home.as_deref(), &steam_roots, &steamapps_dirs);

    find_url_in_targets(&direct_log_dirs, &walk_roots)
}

#[tauri::command]
fn find_url() -> Result<String, String> {
    find_url_impl()
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--find-url") {
        match find_url_impl() {
            Ok(url) => {
                println!("{url}");
                return;
            }
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![history, find_url, sync, import_backup])
        .run(tauri::generate_context!())
        .expect("Ошибка запуска приложения");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("wuwa-test-{name}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn xor_encode(bytes: &[u8]) -> Vec<u8> {
        // Because 0xa5 and 0xef are both odd, encoding flips parity relative to decoding.
        bytes
            .iter()
            .map(|&b| b ^ if (b & 1) == 0 { 0xa5 } else { 0xef })
            .collect()
    }

    #[test]
    fn parses_libraryfolders_vdf_linux_and_windows() {
        let vdf = r#"
"libraryfolders"
{
    "0"
    {
        "path"      "/home/user/.local/share/Steam"
    }
    "1"
    {
        "path"      "/mnt/nvme/steam"
    }
    "2"
    {
        "path"      "D:\\SteamLibrary"
    }
}
"#;
        let libs = parse_libraryfolders_vdf(vdf);
        assert_eq!(
            libs,
            vec![
                PathBuf::from("/home/user/.local/share/Steam/steamapps"),
                PathBuf::from("/mnt/nvme/steam/steamapps"),
                PathBuf::from(r"D:\SteamLibrary").join("steamapps"),
            ]
        );
    }

    #[test]
    fn extracts_raw_and_xor_encoded_urls_latest_first() {
        let re = BytesRegex::new(GACHA_URL_PATTERN).unwrap();
        let url1 = "https://aki-gm-resources-oversea.aki-game.net/aki/gacha/index.html#/record?svr_id=1&player_id=111&record_id=abc&resources_id=res";
        let url2 = "https://aki-gm-resources-oversea.aki-game.net/aki/gacha/index.html#/record?svr_id=1&player_id=222&record_id=def&resources_id=res";

        let raw = format!("prefix \"{url1}\"\nmore \"{url2}\" suffix").into_bytes();
        assert_eq!(extract_url_from_bytes(&raw, &re).as_deref(), Some(url2));

        let xored = xor_encode(&raw);
        assert_eq!(extract_url_from_bytes(&xored, &re).as_deref(), Some(url2));
    }

    #[test]
    fn finds_url_from_secondary_vdf_library_and_symlinks_newest_first() {
        let root = unique_temp_dir("discovery");
        let steam_root = root.join(".local/share/Steam");
        let secondary_lib = root.join("custom_steam");
        let real_game_dir = root.join("external_ssd/Wuthering Waves");
        let logs_dir = real_game_dir.join("Wuthering Waves Game/Client/Saved/Logs");
        let crashes_dir = real_game_dir.join("Client/Saved/Crashes/UE4CC_0000");

        fs::create_dir_all(steam_root.join("steamapps")).unwrap();
        fs::create_dir_all(secondary_lib.join("steamapps/common")).unwrap();
        fs::create_dir_all(&logs_dir).unwrap();
        fs::create_dir_all(&crashes_dir).unwrap();

        // Simulate Wine dosdevices/z: -> root symlink inside compatdata/prefix to ensure no loop/escape.
        let dosdevices = secondary_lib.join("steamapps/compatdata/3513350/pfx/dosdevices");
        fs::create_dir_all(&dosdevices).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&root, dosdevices.join("z:")).unwrap();

        // Symlinked game directory inside secondary library's steamapps/common.
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            &real_game_dir,
            secondary_lib.join("steamapps/common/Wuthering Waves"),
        )
        .unwrap();
        #[cfg(not(unix))]
        fs::create_dir_all(
            secondary_lib.join("steamapps/common/Wuthering Waves/Wuthering Waves Game/Client/Saved/Logs"),
        )
        .unwrap();

        let vdf = format!(
            "\"libraryfolders\"\n{{\n  \"0\" {{ \"path\" \"{}\" }}\n  \"1\" {{ \"path\" \"{}\" }}\n}}\n",
            steam_root.display(),
            secondary_lib.display()
        );
        fs::write(steam_root.join("steamapps/libraryfolders.vdf"), vdf).unwrap();

        let crash_url = "https://aki-gm-resources-oversea.aki-game.net/aki/gacha/index.html#/record?svr_id=1&player_id=crash&record_id=c&resources_id=r";
        let old_url = "https://aki-gm-resources-oversea.aki-game.net/aki/gacha/index.html#/record?svr_id=1&player_id=old&record_id=o&resources_id=r";
        let new_url = "https://aki-gm-resources-oversea.aki-game.net/aki/gacha/index.html#/record?svr_id=1&player_id=new&record_id=n&resources_id=r";

        fs::write(crashes_dir.join("Client.log"), crash_url).unwrap();
        let backup_log = logs_dir.join("Client-backup-2026.11.15.log");
        fs::write(&backup_log, xor_encode(old_url.as_bytes())).unwrap();

        // Ensure Client.log is strictly newer than the backup log.
        std::thread::sleep(Duration::from_millis(20));
        let active_log = logs_dir.join("Client.log");
        fs::write(&active_log, xor_encode(new_url.as_bytes())).unwrap();

        let steam_roots = vec![steam_root];
        let steamapps_dirs = collect_steamapps_dirs(&steam_roots);
        let (direct_logs, walk_roots) =
            collect_search_targets(Some(&root), &steam_roots, &steamapps_dirs);
        let found = find_url_in_targets(&direct_logs, &walk_roots).unwrap();
        assert_eq!(found, new_url);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parses_epic_manifest_and_lutris_yaml() {
        let manifest = r#"{
            "DisplayName": "Wuthering Waves",
            "InstallLocation": "D:\\Games\\WutheringWavesj3oFh"
        }"#;
        assert_eq!(
            parse_epic_manifest_install_location(manifest),
            Some(PathBuf::from(r"D:\Games\WutheringWavesj3oFh"))
        );

        let yaml = "\ngame:\n  exe: /mnt/games/WuWa/Wuthering Waves Game/Wuthering Waves.exe\n  prefix: /mnt/games/prefixes/wuwa\n";
        assert_eq!(
            parse_lutris_yaml_paths(yaml),
            vec![
                PathBuf::from("/mnt/games/WuWa/Wuthering Waves Game"),
                PathBuf::from("/mnt/games/prefixes/wuwa"),
            ]
        );
    }
}
