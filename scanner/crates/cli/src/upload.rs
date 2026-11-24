//! `--upload <server>`: sends the last student scan and progress scan to the ba-key-AMAS web server.
//!
//! The nickname is the account nickname the progress scan read. The server answers a new nickname with a key
//! (`nickname#hex6`); it is saved beside the other add-on files so the next upload updates the same entry.
//! The key travels in the request body, so only https (or a loopback address for local testing) is accepted.
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The upload contract this add-on speaks. The server accepts a range of contracts (see web/src/version.ts);
/// raise this when the uploaded data changes shape and the server has learned the new one.
pub const UPLOAD_PROTOCOL: u32 = 1;

pub struct Request {
    pub server: String,
    pub nickname: Option<String>,
    pub public: bool,
    pub students_dir: PathBuf,
    pub progress_dir: PathBuf,
    pub store: PathBuf,
}

/// The saved keys, by `server|nickname`.
#[derive(Default)]
pub struct KeyStore(BTreeMap<String, String>);

impl KeyStore {
    pub fn load(path: &Path) -> Self {
        let map = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        KeyStore(map)
    }

    pub fn get(&self, server: &str, nickname: &str) -> Option<&str> {
        self.0.get(&slot(server, nickname)).map(String::as_str)
    }

    pub fn set(&mut self, server: &str, nickname: &str, key: &str) {
        self.0.insert(slot(server, nickname), key.to_string());
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&self.0)?).with_context(|| format!("writing {}", path.display()))
    }
}

fn slot(server: &str, nickname: &str) -> String {
    format!("{}|{}", server.trim_end_matches('/'), nickname)
}

/// https always; plain http only for this machine, where a key cannot cross a network.
pub fn check_server(server: &str) -> Result<()> {
    let rest = server.strip_prefix("https://").map(|_| true).or_else(|| server.strip_prefix("http://").map(|_| false));
    let Some(secure) = rest else { bail!("server must start with https:// (or http://localhost for testing): {server}") };
    if secure {
        return Ok(());
    }
    let host = server.trim_start_matches("http://").split(['/', ':']).next().unwrap_or("");
    if matches!(host, "localhost" | "127.0.0.1" | "[::1]") {
        Ok(())
    } else {
        bail!("refusing to send a key over plain http to {host}; use https")
    }
}

pub fn nickname_of(progress: &Option<Value>) -> Option<String> {
    progress.as_ref()?.pointer("/account/nickname")?.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

pub fn build_body(nickname: &str, key: Option<&str>, public: bool, roster: Value, progress: Option<Value>) -> Value {
    let mut body = json!({ "nickname": nickname, "public": public, "roster": roster });
    if let Some(k) = key {
        body["key"] = json!(k);
    }
    if let Some(p) = progress {
        body["progress"] = p;
    }
    body
}

fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn run(req: &Request) -> Result<String> {
    check_server(&req.server)?;
    let roster = read_json(&req.students_dir.join("students.json")).context("run the student scan first")?;
    let progress = read_json(&req.progress_dir.join("latest.json")).ok();
    let nickname = req.nickname.clone().or_else(|| nickname_of(&progress)).ok_or_else(|| anyhow!("no account nickname: run the progress scan or pass --nickname"))?;
    let mut store = KeyStore::load(&req.store);
    let body = build_body(&nickname, store.get(&req.server, &nickname), req.public, roster, progress);
    let url = format!("{}/api/upload", req.server.trim_end_matches('/'));
    let res = match ureq::post(&url)
        .set("content-type", "application/json")
        .set("x-amas-protocol", &UPLOAD_PROTOCOL.to_string())
        .set("x-amas-addon", env!("CARGO_PKG_VERSION"))
        .send_json(body) {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let v = r.into_json::<Value>().unwrap_or(Value::Null);
            let msg = v["error"].as_str().unwrap_or_default();
            match v["download"].as_str() {
                Some(url) if code == 426 => bail!("server answered {code}: {msg}
update the add-on: {url}"),
                _ => bail!("server answered {code}: {msg}"),
            }
        }
        Err(e) => return Err(e.into()),
    };
    let out: Value = res.into_json()?;
    let key = out["key"].as_str().ok_or_else(|| anyhow!("the server sent no key"))?;
    store.set(&req.server, &nickname, key);
    store.save(&req.store)?;
    let new = out["created"].as_bool().unwrap_or(false);
    Ok(format!("{} {} ({} students, score {})", if new { "new key" } else { "updated" }, key, out["students"], out["score"]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_and_loopback_only() {
        assert!(check_server("https://ba.example.workers.dev").is_ok());
        assert!(check_server("http://127.0.0.1:8787").is_ok());
        assert!(check_server("http://localhost").is_ok());
        assert!(check_server("http://ba.example.com").is_err());
        assert!(check_server("ftp://x").is_err());
        assert!(check_server("http://localhost.evil.com").is_err());
    }

    #[test]
    fn the_nickname_comes_from_the_progress_scan() {
        let p = Some(json!({"account": {"nickname": " 소다팝 "}}));
        assert_eq!(nickname_of(&p).as_deref(), Some("소다팝"));
        assert_eq!(nickname_of(&Some(json!({"account": {"nickname": null}}))), None);
        assert_eq!(nickname_of(&None), None);
    }

    #[test]
    fn the_body_carries_the_key_only_when_there_is_one() {
        let a = build_body("a", None, false, json!({"entries": []}), None);
        assert!(a.get("key").is_none() && a.get("progress").is_none());
        assert_eq!(a["public"], json!(false));
        let b = build_body("a", Some("a#123456"), true, json!({}), Some(json!({"x": 1})));
        assert_eq!((b["key"].as_str(), b["public"].as_bool(), b["progress"]["x"].as_i64()), (Some("a#123456"), Some(true), Some(1)));
    }

    #[test]
    fn the_protocol_is_a_positive_number() {
        assert!(UPLOAD_PROTOCOL >= 1);
    }

    #[test]
    fn keys_are_kept_per_server_and_nickname() {
        let dir = std::env::temp_dir().join(format!("baocr-upload-{}", std::process::id()));
        let path = dir.join("upload.json");
        let mut s = KeyStore::default();
        s.set("https://a.example/", "소다팝", "소다팝#aaaaaa");
        s.save(&path).unwrap();
        let back = KeyStore::load(&path);
        assert_eq!(back.get("https://a.example", "소다팝"), Some("소다팝#aaaaaa"));
        assert_eq!(back.get("https://b.example", "소다팝"), None);
        std::fs::remove_dir_all(dir).ok();
    }
}
