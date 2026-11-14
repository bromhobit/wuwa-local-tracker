use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};
use tauri::Manager;

#[derive(Serialize, Deserialize)]
struct Pull { player: String, pool: i64, time: String, seq: i64, name: String, quality: i64 }
fn db(app: &tauri::AppHandle) -> Result<Connection,String> {
    let dir = app.path().app_data_dir().map_err(|e|e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    let c = Connection::open(dir.join("history.sqlite3")).map_err(|e|e.to_string())?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS pulls (player TEXT NOT NULL, pool INTEGER NOT NULL, time TEXT NOT NULL, seq INTEGER NOT NULL, name TEXT NOT NULL, quality INTEGER NOT NULL, PRIMARY KEY(player,pool,time,seq));").map_err(|e|e.to_string())?; Ok(c)
}
#[tauri::command]
fn history(app: tauri::AppHandle) -> Result<Vec<Pull>,String> {
    let c=db(&app)?; let mut s=c.prepare("SELECT player,pool,time,seq,name,quality FROM pulls ORDER BY player,pool,time,seq").map_err(|e|e.to_string())?;
    let rows=s.query_map([],|r|Ok(Pull{player:r.get(0)?,pool:r.get(1)?,time:r.get(2)?,seq:r.get(3)?,name:r.get(4)?,quality:r.get(5)?})).map_err(|e|e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}
#[tauri::command]
fn import_backup(app: tauri::AppHandle, data: String) -> Result<usize,String> {
    let pulls: Vec<Pull> = serde_json::from_str(&data).map_err(|e|e.to_string())?; let mut c=db(&app)?; let tx=c.transaction().map_err(|e|e.to_string())?; let mut n=0;
    for p in pulls { if p.player.is_empty() || p.time.is_empty() || p.name.is_empty() || !(1..=8).contains(&p.pool) || p.seq < 1 || !(1..=5).contains(&p.quality) { return Err("Некорректная запись в файле".into()); } n+=tx.execute("INSERT OR IGNORE INTO pulls VALUES (?1,?2,?3,?4,?5,?6)",params![p.player,p.pool,p.time,p.seq,p.name,p.quality]).map_err(|e|e.to_string())?; }
    tx.commit().map_err(|e|e.to_string())?; Ok(n)
}
#[tauri::command]
fn find_url() -> Result<String,String> {
    let home=std::env::var("HOME").or_else(|_|std::env::var("USERPROFILE")).map_err(|_|"Не удалось найти домашнюю папку")?;
    let mut roots=vec![PathBuf::from(format!("{home}/.local/share/Steam/steamapps")),PathBuf::from(format!("{home}/.steam/steam/steamapps")),PathBuf::from(format!("{home}/Library/Application Support/Steam/steamapps"))];
    if cfg!(windows) { for drive in b'C'..=b'Z' { for path in ["Program Files (x86)/Steam/steamapps", "SteamLibrary/steamapps", "Program Files/Wuthering Waves", "Wuthering Waves"] { roots.push(PathBuf::from(format!("{}:/{}",drive as char,path))); } } }
    let re=regex::Regex::new(r#"https?://[^\s\"'\]\)]+/aki/gacha/index\.html#/record\?[^\s\"'\]\),]+"#).unwrap(); let mut logs: Vec<PathBuf>=Vec::new();
    for root in roots { for e in walkdir::WalkDir::new(root).max_depth(14).follow_links(false).into_iter().filter_map(Result::ok) { if e.file_type().is_file() && e.file_name().to_string_lossy().to_lowercase().ends_with(".log") && e.path().to_string_lossy().contains("Saved/Logs") { logs.push(e.path().to_owned()); } } }
    logs.sort_by_key(|p|std::cmp::Reverse(fs::metadata(p).and_then(|m|m.modified()).ok()));
    for p in logs { if let Ok(raw)=fs::read(p) { let decoded: Vec<u8>=raw.iter().map(|b|if b&1==1 {b^0xa5}else{b^0xef}).collect(); for bytes in [&raw,&decoded] { let text=String::from_utf8_lossy(bytes); if let Some(m)=re.find_iter(&text).last(){return Ok(m.as_str().to_owned())} } } }
    Err("Ссылка не найдена. Откройте историю круток в игре или вставьте ссылку вручную.".into())
}
#[tauri::command]
async fn sync(app: tauri::AppHandle, link: String) -> Result<usize,String> {
    tauri::async_runtime::spawn_blocking(move || sync_inner(app,link)).await.map_err(|e|e.to_string())?
}
fn sync_inner(app: tauri::AppHandle, link:String) -> Result<usize,String> {
    let u=url::Url::parse(&link).map_err(|_|"Некорректная ссылка")?;
    let host=u.host_str().unwrap_or(""); if u.scheme()!="https" || !(host.ends_with(".aki-game.net") || host.ends_with(".aki-game.com")) || u.path()!="/aki/gacha/index.html" { return Err("Ожидается ссылка истории Kuro Games".into()); }
    let frag=u.fragment().ok_or("В ссылке нет параметров")?; let query=frag.strip_prefix("/record?").ok_or("Некорректный путь истории")?; let p:HashMap<String,String>=url::form_urlencoded::parse(query.as_bytes()).into_owned().collect();
    let get=|k:&str|p.get(k).cloned().ok_or_else(||format!("В ссылке отсутствует {k}")); let player=get("player_id")?; let record=get("record_id")?; let server=get("svr_id")?; let resource=get("resources_id")?;
    let endpoint=if p.get("svr_area").map(String::as_str)==Some("cn") {"https://gmserver-api.aki-game2.com/gacha/record/query"} else {"https://gmserver-api.aki-game2.net/gacha/record/query"};
    let client=reqwest::blocking::Client::builder().user_agent("Mozilla/5.0").timeout(std::time::Duration::from_secs(30)).redirect(reqwest::redirect::Policy::none()).build().map_err(|e|e.to_string())?; let mut all=Vec::new();
    for pool in 1..=8 { let body=serde_json::json!({"playerId":player,"recordId":record,"serverId":server,"cardPoolId":resource,"cardPoolType":pool,"languageCode":p.get("lang").map(String::as_str).unwrap_or("en")});
        let response:serde_json::Value=client.post(endpoint).json(&body).send().map_err(|_|"Ошибка соединения с игровым API")?.error_for_status().map_err(|_|"Ошибка HTTP игрового API")?.json().map_err(|_|"Не удалось прочитать ответ API")?;
        if response["code"].as_i64()!=Some(0) {return Err(format!("API отклонил запрос для баннера {pool} (код {}). {} Откройте историю круток в игре заново, дождитесь её загрузки и сразу нажмите «Найти в Steam», затем «Обновить». Если ошибка повторится, проверьте, открывается ли история внутри игры.", response["code"], response["message"].as_str().unwrap_or("Причина не указана.")));}
        let data=response["data"].as_array().cloned().unwrap_or_default(); let mut counts:HashMap<String,i64>=HashMap::new();
        for item in data.iter().rev() { let time=item["time"].as_str().ok_or("В ответе нет даты")?.to_owned(); let seq=counts.entry(time.clone()).or_default(); *seq+=1; let name=item["name"].as_str().ok_or("В ответе нет названия")?.to_owned(); let quality=item["qualityLevel"].as_i64().or_else(||item["qualityLevel"].as_str().and_then(|s|s.parse().ok())).ok_or("В ответе нет редкости")?; all.push(Pull{player:player.clone(),pool,time,seq:*seq,name,quality}); }
    }
    import_backup(app,serde_json::to_string(&all).map_err(|e|e.to_string())?)
}
fn main(){tauri::Builder::default().invoke_handler(tauri::generate_handler![history,find_url,sync,import_backup]).run(tauri::generate_context!()).expect("Ошибка запуска приложения");}
