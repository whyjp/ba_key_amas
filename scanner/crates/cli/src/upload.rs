//! `--upload <server>`: sends the last student scan and progress scan to the ba-key-AMAS web server.
//!
//! The account is the hash of the member code the player typed (`account_key`). It is asked for once, and only the
//! hash is kept, in `upload.json` beside the other add-on files. The server recognises the account by it, so every
//! later upload updates the same entry. The nickname is only a display name. The hash travels in the request body,
//! so only https (or a loopback address for local testing) is accepted.
use crate::account_key;
use anyhow::{anyhow, bail, Context, Result};
use hmac_sha256::{Hash, HMAC};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The upload contract this add-on speaks. The server accepts a range of contracts (see web/src/version.ts);
/// raise this when the uploaded data changes shape and the server has learned the new one.
/// 2: accounts are identified by `account_id`; contract 1 sent a nickname and a key.
pub const UPLOAD_PROTOCOL: u32 = 2;

pub struct Request {
    pub server: String,
    pub nickname: Option<String>,
    /// `--public`: show this account on the leaderboard. Without it the server keeps what it has (new accounts are private).
    pub public: bool,
    pub students_dir: PathBuf,
    pub progress_dir: PathBuf,
    pub store: PathBuf,
}

/// Where an account id can come from without asking: scripts and tests.
#[derive(Default)]
pub struct Sources {
    /// `BAOCR_ACCOUNT_ID_FOR_TESTING`: an account id as it is (64 hex digits), for local tests.
    pub testing_id: Option<String>,
    /// `BAOCR_MEMBER_CODE`: the member code, for scripted use. Never a command-line argument (shell history).
    pub member_code: Option<String>,
}

impl Sources {
    pub fn from_env() -> Sources {
        Sources { testing_id: std::env::var("BAOCR_ACCOUNT_ID_FOR_TESTING").ok(), member_code: std::env::var("BAOCR_MEMBER_CODE").ok() }
    }
}

/// The saved account id, if `upload.json` has one. Anything else in the file (the keys of the older format) is ignored.
pub fn saved_account_id(path: &Path) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    v["account_id"].as_str().filter(|s| account_key::is_account_id(s)).map(str::to_string)
}

fn save_account_id(path: &Path, id: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&json!({ "account_id": id }))?).with_context(|| format!("writing {}", path.display()))
}

/// `--forget-account`: the next upload asks for a member code again.
pub fn forget_account(path: &Path) -> Result<String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok("저장된 계정 값을 지웠습니다. 다음 업로드에서 회원코드를 다시 입력합니다".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("저장된 계정 값이 없습니다".into()),
        Err(e) => Err(e).with_context(|| format!("removing {}", path.display())),
    }
}

const ATTEMPTS: usize = 3;

/// The account id to upload under: the testing id, the scripted code, the saved id, or the player's typed code
/// (twice, hidden). Only a typed code is saved, and only as its hash.
pub fn resolve_account_id(store: &Path, src: &Sources, ask: &mut dyn FnMut(&str) -> Result<String>) -> Result<String> {
    if let Some(id) = &src.testing_id {
        let id = id.trim().to_ascii_lowercase();
        if !account_key::is_account_id(&id) {
            bail!("BAOCR_ACCOUNT_ID_FOR_TESTING 은 64자리 16진수여야 합니다");
        }
        return Ok(id);
    }
    if let Some(raw) = &src.member_code {
        return Ok(account_key::account_id(&account_key::parse_member_code(raw).context("BAOCR_MEMBER_CODE")?));
    }
    if let Some(id) = saved_account_id(store) {
        return Ok(id);
    }
    println!("{}", account_key::GUIDE);
    for attempt in 1..=ATTEMPTS {
        let first = ask("회원코드: ")?;
        let second = ask("한 번 더: ")?;
        match account_key::confirmed(&first, &second) {
            Ok(code) => {
                let id = account_key::account_id(&code);
                save_account_id(store, &id)?;
                return Ok(id);
            }
            Err(e) if attempt < ATTEMPTS => println!("{e:#}. 다시 입력해 주세요"),
            Err(e) => return Err(e),
        }
    }
    unreachable!("the last attempt returns")
}

/// The official server: what `--upload` and `--manage` use when no address is given.
pub const DEFAULT_SERVER: &str = "https://key-amas.cxx.co.kr";

/// https always; plain http only for this machine, where the account id cannot cross a network.
pub fn check_server(server: &str) -> Result<()> {
    let rest = server.strip_prefix("https://").map(|_| true).or_else(|| server.strip_prefix("http://").map(|_| false));
    let Some(secure) = rest else { bail!("server must start with https:// (or http://localhost for testing): {server}") };
    if secure {
        return Ok(());
    }
    let authority = server.trim_start_matches("http://").split(['/', '?', '#']).next().unwrap_or("");
    let host = match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split_once(']').map_or("", |(h, _)| h),
        None => authority.split(':').next().unwrap_or(""),
    };
    if !authority.contains('@') && matches!(host, "localhost" | "127.0.0.1" | "::1") {
        Ok(())
    } else {
        bail!("refusing to send an account id over plain http to {host}; use https")
    }
}

pub fn nickname_of(progress: &Option<Value>) -> Option<String> {
    progress.as_ref()?.pointer("/account/nickname")?.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

pub fn build_body(account_id: &str, nickname: &str, public: bool, roster: Value, progress: Option<Value>) -> Value {
    let mut body = json!({ "account_id": account_id, "nickname": nickname, "roster": roster });
    if public {
        body["public"] = json!(true);
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

fn profile_url(server: &str, public_id: &str) -> String {
    format!("{}/#/u/{public_id}", server.trim_end_matches('/'))
}

pub fn run(req: &Request, src: &Sources, ask: &mut dyn FnMut(&str) -> Result<String>) -> Result<String> {
    check_server(&req.server)?;
    let roster = read_json(&req.students_dir.join("students.json")).context("run the student scan first")?;
    let progress = read_json(&req.progress_dir.join("latest.json")).ok();
    let nickname = req.nickname.clone().or_else(|| nickname_of(&progress)).ok_or_else(|| anyhow!("no account nickname: run the progress scan or pass --nickname"))?;
    let id = resolve_account_id(&req.store, src, ask)?;
    let out = post_json(&format!("{}/api/upload", req.server.trim_end_matches('/')), build_body(&id, &nickname, req.public, roster, progress))?;
    let public_id = out["public_id"].as_str().ok_or_else(|| anyhow!("the server sent no public_id"))?;
    Ok(format!(
        "{} ({} students, score {}, {})\nyour page: {}",
        if out["created"].as_bool().unwrap_or(false) { "new account" } else { "updated" },
        out["students"],
        out["score"],
        if out["public"].as_bool().unwrap_or(false) { "public" } else { "private" },
        profile_url(&req.server, public_id)
    ))
}

pub struct Manage {
    pub server: String,
    pub public: bool,
    pub store: PathBuf,
}

/// Public or private is changed here and nowhere on the website.
pub fn manage(req: &Manage, src: &Sources, ask: &mut dyn FnMut(&str) -> Result<String>) -> Result<String> {
    check_server(&req.server)?;
    let id = resolve_account_id(&req.store, src, ask)?;
    let out = post_json(&format!("{}/api/visibility", req.server.trim_end_matches('/')), json!({ "account_id": id, "public": req.public }))?;
    let link = out["public_id"].as_str().map(|p| format!("\nyour page: {}", profile_url(&req.server, p))).unwrap_or_default();
    Ok(format!("leaderboard: {}{link}", if out["public"].as_bool().unwrap_or(false) { "public" } else { "private" }))
}

/// The key the official release build signs its requests with. CI injects it at build time (the environment
/// variable `BAOCR_ADDON_SIGNING_KEY` while compiling), so it is not in the source: a build made from the source has
/// none and the server refuses its requests. The same variable read at run time lets a developer talk to a local
/// server that was started with a key of their own.
fn signing_key() -> Option<String> {
    std::env::var("BAOCR_ADDON_SIGNING_KEY")
        .ok()
        .or_else(|| option_env!("BAOCR_ADDON_SIGNING_KEY").map(str::to_string))
        .filter(|k| k.len() >= 16)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `HMAC-SHA256(key, "{ts}\n{path}\n{sha256(body)}")` in hex: the same value the server computes (web/src/security.ts).
pub fn signature(key: &str, ts: &str, path: &str, body: &[u8]) -> String {
    let digest = hex(&Hash::hash(body));
    hex(&HMAC::mac(format!("{ts}{n}{path}{n}{digest}", n = '\n').as_bytes(), key.as_bytes()))
}

/// `/api/upload` of `https://host:port/api/upload?x#y`.
fn path_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path = rest.find('/').map_or("/", |i| &rest[i..]);
    path.split(['?', '#']).next().unwrap_or("/").to_string()
}

fn post_json(url: &str, body: Value) -> Result<Value> {
    // the exact bytes that are signed are the bytes that are sent
    let bytes = serde_json::to_vec(&body)?;
    let mut request = ureq::post(url)
        .set("content-type", "application/json")
        .set("x-amas-protocol", &UPLOAD_PROTOCOL.to_string())
        .set("x-amas-addon", env!("CARGO_PKG_VERSION"));
    let signed = match signing_key() {
        Some(key) => {
            let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs().to_string();
            request = request.set("x-amas-sig", &signature(&key, &ts, &path_of(url), &bytes)).set("x-amas-ts", &ts);
            true
        }
        None => false,
    };
    let res = match request.send_bytes(&bytes) {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let v = r.into_json::<Value>().unwrap_or(Value::Null);
            let msg = v["error"].as_str().unwrap_or_default();
            match v["download"].as_str() {
                Some(url) if code == 426 => bail!("server answered {code}: {msg}\nupdate the add-on: {url}"),
                _ if code == 401 && !signed => bail!("server answered {code}: {msg}\nthis build has no signing key; only the official release of the add-on can upload"),
                _ => bail!("server answered {code}: {msg}"),
            }
        }
        Err(e) => return Err(e.into()),
    };
    Ok(res.into_json()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE: &str = "AB12CD34EF56G";
    const FAKE_ID: &str = "c3b695e22c163896b1d76d133b9cb0ddf56296451f1ae5ff6af982828e73a7d3";

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("baocr-upload-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("upload.json")
    }

    fn typing(lines: &[&str]) -> impl FnMut(&str) -> Result<String> {
        let mut it: std::vec::IntoIter<String> = lines.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter();
        move |_| it.next().ok_or_else(|| anyhow!("asked more than expected"))
    }

    #[test]
    fn the_default_server_is_an_accepted_https_address() {
        assert!(DEFAULT_SERVER.starts_with("https://"));
        assert!(check_server(DEFAULT_SERVER).is_ok());
    }

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
    fn ipv6_loopback_is_accepted_and_the_host_is_not_fooled() {
        assert!(check_server("http://[::1]:8787").is_ok());
        assert!(check_server("http://[::1]").is_ok());
        assert!(check_server("http://[::1]:8787/path").is_ok());
        assert!(check_server("http://evil.example").is_err());
        assert!(check_server("http://[2001:db8::1]:8787").is_err());
        assert!(check_server("http://localhost:80@evil.example/").is_err(), "user info does not make a loopback host");
        assert!(check_server("http://127.0.0.1.evil.example:8787").is_err());
        assert!(check_server("http://[::1").is_err());
    }

    #[test]
    fn the_nickname_comes_from_the_progress_scan() {
        let p = Some(json!({"account": {"nickname": " 소다팝 "}}));
        assert_eq!(nickname_of(&p).as_deref(), Some("소다팝"));
        assert_eq!(nickname_of(&Some(json!({"account": {"nickname": null}}))), None);
        assert_eq!(nickname_of(&None), None);
    }

    #[test]
    fn the_body_carries_the_account_id_and_public_only_when_asked() {
        let a = build_body(FAKE_ID, "a", false, json!({"entries": []}), None);
        assert_eq!(a["account_id"], json!(FAKE_ID));
        assert!(a.get("public").is_none() && a.get("progress").is_none() && a.get("key").is_none());
        let b = build_body(FAKE_ID, "a", true, json!({}), Some(json!({"x": 1})));
        assert_eq!((b["public"].as_bool(), b["progress"]["x"].as_i64()), (Some(true), Some(1)));
        assert!(!a.to_string().contains(FAKE) && !b.to_string().contains(FAKE), "the member code is not in the body");
    }

    #[test]
    fn the_protocol_moved_past_the_key_contract() {
        assert!(UPLOAD_PROTOCOL >= 2);
    }

    #[test]
    fn the_first_upload_asks_twice_and_saves_only_the_hash() {
        let store = temp("first");
        let id = resolve_account_id(&store, &Sources::default(), &mut typing(&[FAKE, " ab12-cd34ef56g "])).unwrap();
        assert_eq!(id, FAKE_ID);
        let file = std::fs::read_to_string(&store).unwrap();
        assert!(file.contains(FAKE_ID) && !file.to_uppercase().contains(FAKE), "only the hash is on disk");
        assert_eq!(resolve_account_id(&store, &Sources::default(), &mut typing(&[])).unwrap(), FAKE_ID, "the next time nothing is asked");
        std::fs::remove_dir_all(store.parent().unwrap()).ok();
    }

    #[test]
    fn a_typo_is_caught_by_the_second_entry_and_asked_again() {
        let store = temp("typo");
        let id = resolve_account_id(&store, &Sources::default(), &mut typing(&[FAKE, "AB12CD34EF56H", FAKE, FAKE])).unwrap();
        assert_eq!(id, FAKE_ID);
        let store2 = temp("typo3");
        let err = resolve_account_id(&store2, &Sources::default(), &mut typing(&["a", "a", "b", "b", "c", "c"])).unwrap_err();
        assert!(format!("{err:#}").contains("회원코드"));
        assert!(saved_account_id(&store2).is_none(), "nothing is saved from a bad entry");
        std::fs::remove_dir_all(store.parent().unwrap()).ok();
        std::fs::remove_dir_all(store2.parent().unwrap()).ok();
    }

    #[test]
    fn scripted_sources_win_and_are_not_saved() {
        let store = temp("env");
        let code = Sources { testing_id: None, member_code: Some("ab12cd34ef56g".into()) };
        assert_eq!(resolve_account_id(&store, &code, &mut typing(&[])).unwrap(), FAKE_ID);
        assert!(!store.exists(), "a scripted code is not saved");
        let bad = Sources { testing_id: None, member_code: Some("short".into()) };
        assert!(resolve_account_id(&store, &bad, &mut typing(&[])).is_err());
        let id = Sources { testing_id: Some(FAKE_ID.to_uppercase()), member_code: None };
        assert_eq!(resolve_account_id(&store, &id, &mut typing(&[])).unwrap(), FAKE_ID);
        let junk = Sources { testing_id: Some("nope".into()), member_code: None };
        assert!(resolve_account_id(&store, &junk, &mut typing(&[])).is_err());
        std::fs::remove_dir_all(store.parent().unwrap()).ok();
    }

    #[test]
    fn forgetting_the_account_asks_again_and_the_old_key_file_is_ignored() {
        let store = temp("forget");
        std::fs::write(&store, r#"{"http://x|소다팝": "소다팝#aaaaaa"}"#).unwrap();
        assert_eq!(saved_account_id(&store), None, "the older key format is not an account");
        save_account_id(&store, FAKE_ID).unwrap();
        assert!(!std::fs::read_to_string(&store).unwrap().contains("소다팝"), "saving drops the old keys");
        assert!(forget_account(&store).unwrap().contains("지웠습니다"));
        assert_eq!(saved_account_id(&store), None);
        assert!(forget_account(&store).unwrap().contains("없습니다"));
        std::fs::remove_dir_all(store.parent().unwrap()).ok();
    }

    #[test]
    fn the_page_link_uses_the_public_id() {
        assert_eq!(profile_url("http://127.0.0.1:8787/", "0123456789abcdef"), "http://127.0.0.1:8787/#/u/0123456789abcdef");
    }

    #[test]
    fn the_signature_matches_an_independent_hmac() {
        // node -e 'const c=require("crypto");const d=c.createHash("sha256").update("{\"a\":1}").digest("hex");console.log(c.createHmac("sha256","test-signing-key-0123456789abcdef").update("1700000000\n/api/upload\n"+d).digest("hex"))'
        let got = signature("test-signing-key-0123456789abcdef", "1700000000", "/api/upload", br#"{"a":1}"#);
        assert_eq!(got, "ef7245b4c0b83655a1f80281e721a9b347b49e29d9bce309b98ac16975d8f421");
        assert_ne!(got, signature("test-signing-key-0123456789abcdef", "1700000000", "/api/delete", br#"{"a":1}"#), "the path is part of it");
        assert_ne!(got, signature("test-signing-key-0123456789abcdef", "1700000001", "/api/upload", br#"{"a":1}"#), "so is the time");
        assert_ne!(got, signature("test-signing-key-0123456789abcdef", "1700000000", "/api/upload", br#"{"a":2}"#), "and the body");
    }

    #[test]
    fn the_signed_path_is_the_path_of_the_url() {
        assert_eq!(path_of("https://key-amas.cxx.co.kr/api/upload"), "/api/upload");
        assert_eq!(path_of("http://127.0.0.1:8787/api/visibility?x=1#y"), "/api/visibility");
        assert_eq!(path_of("https://host"), "/");
    }
}
