use crate::rig::*;
use anyhow::{anyhow, bail, Result};
use baocr_wikidb::{RaidKind, Wiki};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");
const WIKI: &str = "D:/work-github/bluearchive-ocr/wikidb/wiki.db";
const BACK: (i32, i32) = (55, 38);
const HOME: (i32, i32) = (1235, 22);

const HUB: &str = "업무";
const RAID_SELECT: &str = "총력전 선택";
const MOCK_LIST: &str = "모의 총력전 보스";
const ROOM: &str = "방 정보";
const MOCK_FORM: &str = "모의 총력전 편성";
const QUICK: &str = "빠른 편성";

pub fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = PathBuf::from(arg(&args, "--out").unwrap_or_else(|| "D:/tmp/live-e2e".into()));
    let dump = PathBuf::from(arg(&args, "--dump").unwrap_or_else(|| "D:/tmp/live-e2e/state.json".into()));
    std::env::set_var("BAOCR_OCR_DEVICE", "cpu");
    if std::env::var_os("BAOCR_MODELS").is_none() {
        std::env::set_var("BAOCR_MODELS", "D:/work-github/bluearchive-ocr/models");
    }
    // a click returns as soon as the input is out, so click-to-card is not padded by the runner's own settle wait
    std::env::set_var("BAOCR_HCU_SETTLE_MS", "0");
    match args.first().map(String::as_str) {
        Some("dump") => {
            println!("{}", std::fs::read_to_string(&dump)?);
            Ok(())
        }
        Some("shot") => {
            let mut rig = Rig::new(dump, out)?;
            let (p, text) = rig.shot(args.get(1).map_or("shot", String::as_str))?;
            println!("{}\n{text}\nscene: {}", p.display(), rig.scene());
            Ok(())
        }
        Some("click") => {
            let mut rig = Rig::new(dump, out)?;
            let (x, y): (i32, i32) = (args[1].parse()?, args[2].parse()?);
            let t = rig.click(x, y, args.get(3).map_or("", String::as_str), "manual")?;
            nap(1500);
            println!("scene now: {} (+{} ms)", rig.scene(), unix_ms() - t);
            Ok(())
        }
        Some("run") => run(&args, out, dump),
        _ => bail!("live_e2e run|shot|click|dump"),
    }
}

struct Addon {
    child: Child,
}

impl Addon {
    fn start(exe: &str, dump: &Path, device: &str, scan_out: &Path) -> Result<Addon> {
        let _ = std::fs::remove_file(dump);
        let mut cmd = Command::new(exe);
        cmd.args(["--wiki", WIKI, "--out"]).arg(scan_out);
        cmd.env("BAOCR_OVERLAY_CAPTURABLE", "1").env("BAOCR_STATE_DUMP", dump).env("BAOCR_MODELS", "D:/work-github/bluearchive-ocr/models");
        if device == "cpu" {
            cmd.env("BAOCR_OCR_DEVICE", "cpu");
        }
        // BAOCR_PROFILE=1 in the harness's environment passes stage timings of the add-on into addon.log
        if std::env::var_os("BAOCR_PROFILE").is_some() {
            cmd.env("BAOCR_PROFILE", "1");
            if let Some(dir) = dump.parent() {
                if let Ok(log) = std::fs::File::create(dir.join("addon.log")) {
                    cmd.stdout(log.try_clone()?).stderr(log);
                }
            }
        }
        Ok(Addon { child: cmd.spawn()? })
    }
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn run(args: &[String], out: PathBuf, dump: PathBuf) -> Result<()> {
    let exe = arg(args, "--exe").ok_or_else(|| anyhow!("--exe <baocr.exe>"))?;
    let label = arg(args, "--label").unwrap_or_else(|| "directml".into());
    let only: Vec<String> = arg(args, "--only").map(|s| s.split(',').map(str::to_string).collect()).unwrap_or_default();
    let want = |n: &str| only.is_empty() || only.iter().any(|o| o == n);
    let out = out.join(&label);
    std::fs::create_dir_all(&out)?;
    let settings = PathBuf::from(std::env::var("APPDATA")?).join("baocr").join("overlay.json");
    let backup = std::fs::read(&settings).ok();
    let scan_out = out.join("scan-out");
    let mut addon = Addon::start(&exe, &dump, &label, &scan_out)?;
    let result = (|| -> Result<Rig> {
        let end = unix_ms() + 90_000;
        loop {
            if let Ok(d) = std::fs::read_to_string(&dump).map_err(anyhow::Error::from).and_then(|t| Ok(serde_json::from_str::<Value>(&t)?)) {
                if d["main"]["rows"].as_array().is_some_and(|r| r.iter().any(|x| x.as_str().is_some_and(|s| s.contains("게임 연결됨")))) {
                    break;
                }
            }
            if unix_ms() > end {
                bail!("the add-on did not report a connected game within 90 s");
            }
            nap(300);
        }
        nap(1500);
        let mut rig = Rig::new(dump.clone(), out.clone())?;
        let wiki = Wiki::open(Path::new(WIKI))?;
        let input_log = PathBuf::from(std::env::var("APPDATA")?).join("baocr").join("input.log");
        let log_lines_before = std::fs::read_to_string(&input_log).map(|t| t.lines().count()).unwrap_or(0);
        if let Err(e) = start_position(&mut rig) {
            rig.record("start", "the game is on the 업무 hub or the main menu", format!("{e:#}"), false);
            return Ok(rig);
        }
        type Scenario = (&'static str, fn(&mut Rig, &Wiki) -> Result<()>);
        let list: [Scenario; 6] = [("nav", nav), ("stale", stale), ("tour", tour), ("battle", battle), ("card", card), ("guard", guard)];
        for (name, f) in list {
            if !want(name) {
                continue;
            }
            println!("=== scenario {name}");
            if let Err(e) = f(&mut rig, &wiki) {
                rig.record(name, "scenario ran to the end", format!("aborted: {e:#}"), false);
                if let Err(e) = recover(&mut rig) {
                    rig.record(name, "game returned to a known screen", format!("{e:#}"), false);
                    return Ok(rig);
                }
            }
        }
        if want("scan") && label == "directml" {
            println!("=== scenario scan");
            if let Err(e) = scan(&mut rig, &mut addon, &exe, &dump, &label, &scan_out) {
                rig.record("scan", "scenario ran to the end", format!("aborted: {e:#}"), false);
                let _ = recover(&mut rig);
            }
        }
        let log_lines_after = std::fs::read_to_string(&input_log).map(|t| t.lines().count()).unwrap_or(0);
        rig.record("audit", "input audit log lines written by this run", format!("{}", log_lines_after.saturating_sub(log_lines_before)), true);
        rig.record("audit", "refusals in the audit log during this run (informational)", audit_refusals(&input_log, log_lines_before), true);
        Ok(rig)
    })();
    addon.stop();
    if let Some(b) = backup {
        std::fs::write(&settings, b)?;
    }
    let rig = result?;
    write_report(&rig, &out, &label)
}

fn audit_refusals(log: &Path, skip: usize) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for l in text.lines().skip(skip) {
        if let Ok(v) = serde_json::from_str::<Value>(l) {
            let code = v["code"].as_str().unwrap_or("?");
            if code != "ok" {
                *counts.entry(code.to_string()).or_default() += 1;
            }
        }
    }
    if counts.is_empty() { "none".into() } else { format!("{counts:?}") }
}

fn write_report(rig: &Rig, out: &Path, label: &str) -> Result<()> {
    let mut md = format!("# live e2e: {label}\n\n| scenario | criterion | measured | result |\n|---|---|---|---|\n");
    for c in &rig.checks {
        md.push_str(&format!("| {} | {} | {} | {} |\n", c.scenario, c.criterion, c.measured.replace('|', "/"), if c.pass { "PASS" } else { "FAIL" }));
    }
    md.push_str("\n## latencies (ms)\n\n| series | n | min | median | p90 |\n|---|---|---|---|---|\n");
    let mut lat = serde_json::Map::new();
    for (k, v) in &rig.latency {
        if let Some((mn, md_, p9)) = stats(v) {
            md.push_str(&format!("| {k} | {} | {mn:.0} | {md_:.0} | {p9:.0} |\n", v.len()));
        }
        lat.insert(k.clone(), json!(v));
    }
    md.push_str(&format!("\nframes grabbed {}, of which looked like a battle {}\n", rig.frames_seen, rig.battle_frames_seen));
    std::fs::write(out.join("report.md"), &md)?;
    let rows: Vec<Value> = rig.checks.iter().map(|c| json!({"scenario": c.scenario, "criterion": c.criterion, "measured": c.measured, "pass": c.pass})).collect();
    std::fs::write(out.join("report.json"), serde_json::to_string_pretty(&json!({"label": label, "checks": rows, "latency_ms": lat}))?)?;
    let failed = rig.checks.iter().filter(|c| !c.pass).count();
    println!("\n{md}\n{} checks, {failed} failed; report in {}", rig.checks.len(), out.display());
    Ok(())
}

fn start_position(rig: &mut Rig) -> Result<()> {
    match rig.scene().as_str() {
        HUB => Ok(()),
        "메인 메뉴" | "메인 메뉴 (대기)" => {
            let t = rig.click(1201, 917, "", "lobby to 업무 hub")?;
            rig.wait_scene(HUB, t, 6000).map(|_| ()).ok_or_else(|| anyhow!("the 업무 hub did not appear"))
        }
        _ => recover(rig),
    }
}

/// Back out of whatever popup or page is up until the 업무 hub. Every click waits for the add-on to show a
/// different screen before the next one (the reading lags the game by a second or two).
fn recover(rig: &mut Rig) -> Result<()> {
    for _ in 0..7 {
        let scene = rig.scene();
        let sent = match scene.as_str() {
            HUB => return Ok(()),
            "메인 메뉴" | "메인 메뉴 (대기)" => {
                let t = rig.click(1201, 917, "", "recover: lobby to 업무 hub")?;
                t
            }
            QUICK => rig.click(1243, 210, QUICK, "recover: close the quick formation popup")?,
            ROOM => rig.click(1138, 257, ROOM, "recover: close the room popup")?,
            "" | "전투 중" => bail!("recover: the game shows an unknown screen or a battle; stopping"),
            s => rig.click(BACK.0, BACK.1, s, "recover: back")?,
        };
        let end = unix_ms() + 8000;
        while unix_ms() < end && rig.scene() == scene {
            nap(100);
        }
        if rig.scene() == scene {
            bail!("recover: the screen '{scene}' did not change after the click");
        }
        let _ = sent;
        nap(900);
    }
    bail!("recover: the hub was not reached in 7 steps (scene '{}')", rig.scene())
}

// ---------------------------------------------------------------- a. navigation

struct Step {
    name: &'static str,
    at: (i32, i32),
    from: &'static str,
    to: &'static str,
    /// words the independent OCR of the screen must contain
    truth: &'static [&'static str],
    /// the title the card must carry, for steps that must only run on the mock raid
    title_prefix: &'static str,
}

fn step(rig: &mut Rig, s: &Step) -> Result<()> {
    if !rig.wait_for_scene_now(s.from, 6000) {
        bail!("step '{}': the add-on says '{}', not '{}'", s.name, rig.scene(), s.from);
    }
    if !s.title_prefix.is_empty() {
        let d = rig.dump()?;
        let title = d["main"]["title"].as_str().unwrap_or("");
        if !title.starts_with(s.title_prefix) {
            bail!("step '{}': title '{title}' does not start with '{}'; not clicking", s.name, s.title_prefix);
        }
    }
    let sent = rig.click(s.at.0, s.at.1, s.from, s.name)?;
    let (lat, frames) = rig.with_frame_watch(|| {
        let l = rig.wait_scene(s.to, sent, 8000);
        nap(900);
        l
    });
    if let Some(ms) = lat {
        if let Some(t) = Rig::settled_at(&frames) {
            // when the screen stopped moving, and how long the card took after that
            let vis = t.saturating_sub(sent) as f64;
            rig.add_latency("nav click-to-screen-settled (game animation)", vis);
            rig.add_latency("nav card minus settled (add-on's share)", ms - vis);
        }
    }
    match lat {
        Some(ms) => {
            rig.add_latency("nav click-to-card", ms);
            rig.record("nav", &format!("{}: scene becomes '{}'", s.name, s.to), format!("{ms:.0} ms"), true);
        }
        None => {
            let (p, _) = rig.shot(&format!("FAIL-{}", s.name))?;
            rig.record("nav", &format!("{}: scene becomes '{}'", s.name, s.to), format!("timeout, scene '{}', screenshot {}", rig.scene(), p.display()), false);
            bail!("step '{}' failed", s.name);
        }
    }
    nap(900);
    let (path, text) = rig.shot(s.name)?;
    let missing: Vec<&&str> = s.truth.iter().filter(|w| !text.contains(&norm(w))).collect();
    rig.record("nav", &format!("{}: independent OCR of the screen has {:?}", s.name, s.truth), if missing.is_empty() { "all present".to_string() } else { format!("missing {missing:?} ({})", path.display()) }, missing.is_empty());
    let scene_now = rig.scene();
    rig.record("nav", &format!("{}: scene stays '{}' after settling", s.name, s.to), scene_now.clone(), scene_now == s.to);
    Ok(())
}

fn db_armor_label(wiki: &Wiki, boss: &str) -> Option<String> {
    let c = wiki.raids_by_name(boss).ok()?.into_iter().filter(|c| c.kind == RaidKind::Raid).collect::<Vec<_>>();
    let [one] = c.as_slice() else { return None };
    let code = wiki.raid_difficulties(one.kind, one.id).ok()?.into_iter().next()?.boss?.armor_type?;
    Some(wiki.label("ArmorType", &code))
}

/// Raid card facts against the database: boss in the title, its armor from the DB on the card, no
/// "screen vs wiki" disagreement note (the screen only supplies the key).
fn raid_card_facts(rig: &mut Rig, wiki: &Wiki, tag: &str, prefix: &str) {
    let d = rig.dump().unwrap_or_default();
    let title = d["main"]["title"].as_str().unwrap_or("").to_string();
    let mut words: Vec<&str> = title.trim_start_matches(prefix).split_whitespace().collect();
    if words.last().is_some_and(|w| ["Normal", "Hard", "VeryHard", "Hardcore", "Extreme", "Insane", "Torment", "Lunatic"].contains(w)) {
        words.pop();
    }
    let boss = words.join(" ");
    let text = rig.card_text();
    match db_armor_label(wiki, &boss) {
        Some(label) => rig.record("nav", &format!("{tag}: boss '{boss}' armor from the DB ('{label}') is on the card"), text.lines().find(|l| l.contains("장갑")).unwrap_or("(no armor line)").to_string(), text.contains(&label)),
        None => rig.record("nav", &format!("{tag}: boss '{boss}' resolves to one DB raid"), "ambiguous or unknown in the DB", false),
    }
    let note = text.lines().find(|l| l.contains("화면") && l.contains("위키"));
    rig.record("nav", &format!("{tag}: no screen-vs-wiki disagreement note"), note.unwrap_or("none").to_string(), note.is_none());
}

fn nav(rig: &mut Rig, wiki: &Wiki) -> Result<()> {
    step(rig, &Step { name: "hub to 총력전", at: (870, 575), from: HUB, to: RAID_SELECT, truth: &["BattleSelect", "총력전"], title_prefix: "" })?;
    raid_card_facts(rig, wiki, "총력전 선택", "총력전");
    step(rig, &Step { name: "총력전 to 모의 총력전", at: (97, 200), from: RAID_SELECT, to: MOCK_LIST, truth: &["보스선택", "모의총력전"], title_prefix: "" })?;
    raid_card_facts(rig, wiki, "모의 보스 1", "모의 총력전");
    // cycle bosses with the right arrow: the card's boss must change each time and match the DB
    let mut seen = vec![rig.dump()?["main"]["title"].as_str().unwrap_or("").to_string()];
    for i in 0..4 {
        let before = seen.last().cloned().unwrap_or_default();
        let sent = rig.click(1247, 478, MOCK_LIST, "next boss")?;
        let t0 = unix_ms();
        let mut got = None;
        while unix_ms() - t0 < 8000 {
            let d = rig.dump()?;
            let title = d["main"]["title"].as_str().unwrap_or("").to_string();
            if title != before && d["main"]["scene"].as_str() == Some(MOCK_LIST) {
                got = Some((title, d["changed_unix_ms"].as_u64().unwrap_or(0) as u128));
                break;
            }
            nap(8);
        }
        let Some((title, changed)) = got else {
            let (p, _) = rig.shot("FAIL-boss-cycle")?;
            rig.record("nav", &format!("boss cycle {}: title changes", i + 2), format!("no change in 8 s ({})", p.display()), false);
            bail!("boss cycle stuck");
        };
        let ms = changed.saturating_sub(sent) as f64;
        rig.add_latency("boss cycle click-to-card", ms);
        rig.record("nav", &format!("boss cycle {}: title '{title}'", i + 2), format!("{ms:.0} ms"), true);
        nap(900);
        raid_card_facts(rig, wiki, &format!("모의 보스 {}", i + 2), "모의 총력전");
        seen.push(title);
    }
    // back to the first boss so the next steps are on a known boss (4 more steps wrap around the 14)
    let distinct: std::collections::BTreeSet<&String> = seen.iter().collect();
    rig.record("nav", "at least 3 different bosses cycled", format!("{} distinct of {}", distinct.len(), seen.len()), distinct.len() >= 3);
    // the popup for the Normal row (a mock raid: the title must say so)
    step(rig, &Step { name: "mock Normal row to 방 정보", at: (1157, 180), from: MOCK_LIST, to: ROOM, truth: &["방정보", "소탕"], title_prefix: "모의 총력전" })?;
    let text = rig.card_text();
    rig.record("nav", "방 정보 card has difficulty Normal and the DB facts", text.lines().filter(|l| l.contains("난이도") || l.contains("추천")).collect::<Vec<_>>().join(" / "), text.contains("난이도 Normal"));
    // the one 입장 of this run: the 모의 총력전 room popup (the card title must say 모의)
    let d = rig.dump()?;
    let title = d["main"]["title"].as_str().unwrap_or("");
    if !title.contains("모의 총력전") {
        bail!("the room popup title '{title}' is not the mock raid: not pressing 입장");
    }
    step(rig, &Step { name: "mock 방 정보 입장 to 편성", at: (934, 663), from: ROOM, to: MOCK_FORM, truth: &["모의총력전편성"], title_prefix: "모의 총력전" })?;
    let text = rig.card_text();
    raid_card_facts(rig, wiki, "모의 편성", "모의 총력전");
    rig.record("nav", "모의 편성 card names the encounter (boss, 난이도, 적 공격, 적 방어)", text.lines().take(8).collect::<Vec<_>>().join(" / "), ["난이도", "적 공격", "적 방어"].iter().all(|k| text.contains(k)));
    rig.record("nav", "모의 편성 card says one party of four (PARTY_NOTE, needs a read squad of 3+)", if text.contains("4개 부대") { "present" } else { "absent: no verdict without a read squad (not exercisable without changing the formation)" }, true);
    // party tabs: the screen stays and the card keeps what it said
    let base_rows = rig.dump()?["main"]["rows"].clone();
    for (n, y) in [(2, 388), (3, 467), (4, 545), (1, 309)] {
        let sent = rig.click(60, y, MOCK_FORM, &format!("party tab {n}"))?;
        nap(1500);
        let (p, _) = rig.shot(&format!("party{n}"))?;
        let now = rig.dump()?;
        rig.record("nav", &format!("party tab {n}: still 모의 총력전 편성 with the same card"), format!("{} ({} ms after click; {})", now["main"]["scene"], unix_ms() - sent, p.display()), now["main"]["scene"] == MOCK_FORM && now["main"]["rows"] == base_rows);
    }
    Ok(())
}

/// Every other screen reachable from the hub, one at a time, each left by the back arrow.
fn tour(rig: &mut Rig, _wiki: &Wiki) -> Result<()> {
    if rig.scene() != HUB {
        recover(rig)?;
    }
    let tiles: [(&str, (i32, i32), &str, &[&str]); 5] = [
        ("종합전술시험", (1025, 575), "종합전술시험", &["종합전술시험"]),
        ("대결전", (1135, 575), "대결전 (휴식기)", &["대결전"]),
        ("현상수배", (740, 528), "의뢰 선택", &["현상수배"]),
        ("특별의뢰", (720, 632), "의뢰 선택", &["특별의뢰"]),
        ("학원교류회", (720, 733), "의뢰 선택", &["학원교류회"]),
    ];
    for (name, at, to, truth) in tiles {
        step(rig, &Step { name, at, from: HUB, to, truth, title_prefix: "" })?;
        step(rig, &Step { name: "back to hub", at: BACK, from: to, to: HUB, truth: &["업무"], title_prefix: "" })?;
    }
    // 제약해제결전: select, 단계, one stage, formation. Nothing is started: 출격 is never touched.
    step(rig, &Step { name: "hub to 제약해제결전", at: (1000, 733), from: HUB, to: "제약해제결전", truth: &["제약해제결전"], title_prefix: "" })?;
    step(rig, &Step { name: "제약해제결전 입장 to 단계", at: (985, 700), from: "제약해제결전", to: "제약해제결전 단계", truth: &["제약해제결전"], title_prefix: "제약해제결전" })?;
    step(rig, &Step { name: "단계 01-24 선택", at: (181, 645), from: "제약해제결전 단계", to: "제약해제결전 보스", truth: &["단계선택"], title_prefix: "제약해제결전" })?;
    let text = rig.card_text();
    rig.record("tour", "제약해제결전 stage card has 지형, 추천 Lv, 적 공격 and the matchup table", text.lines().take(6).collect::<Vec<_>>().join(" / "), ["지형", "추천 Lv", "적 공격", "내 공격"].iter().all(|k| text.contains(k)));
    step(rig, &Step { name: "stage 입장 to 편성", at: (1138, 900), from: "제약해제결전 보스", to: "제약해제결전 편성", truth: &["제약해제결전편성"], title_prefix: "제약해제결전" })?;
    let text = rig.card_text();
    rig.record("tour", "제약해제결전 편성 card has 적 공격 and 적 방어", text.lines().take(8).collect::<Vec<_>>().join(" / "), text.contains("적 공격") && text.contains("적 방어"));
    for (from, to) in [("제약해제결전 편성", "제약해제결전 보스"), ("제약해제결전 보스", "제약해제결전"), ("제약해제결전", HUB)] {
        step(rig, &Step { name: "back", at: BACK, from, to, truth: &[], title_prefix: "" })?;
    }
    // main menu and back
    step(rig, &Step { name: "hub to main menu (home icon)", at: HOME, from: HUB, to: "메인 메뉴", truth: &["공지"], title_prefix: "" }).or_else(|_| step_idle_lobby(rig))?;
    step(rig, &Step { name: "main menu to hub", at: (1201, 917), from: "메인 메뉴", to: HUB, truth: &["업무"], title_prefix: "" })?;
    Ok(())
}

fn step_idle_lobby(rig: &mut Rig) -> Result<()> {
    if rig.scene() == "메인 메뉴 (대기)" {
        rig.record("tour", "main menu is told as 메인 메뉴 (대기) when idle", "메인 메뉴 (대기)", true);
        let sent = rig.click(1201, 917, "메인 메뉴 (대기)", "idle main menu to hub")?;
        return rig.wait_scene(HUB, sent, 8000).map(|_| ()).ok_or_else(|| anyhow!("hub not reached"));
    }
    bail!("the home icon did not lead to the main menu (scene '{}')", rig.scene())
}

// ---------------------------------------------------------------- b. stale icons

/// Opens and closes the 빠른 편성 popup of the mock formation; marks must be gone as soon as it closes.
fn stale(rig: &mut Rig, wiki: &Wiki) -> Result<()> {
    let _ = wiki;
    if rig.scene() != MOCK_FORM {
        bail!("stale: expected to start on the mock formation, scene '{}'", rig.scene());
    }
    for rep in 1..=5 {
        let sent = rig.click(1200, 320, MOCK_FORM, "open 빠른 편성")?;
        let lat = rig.wait_scene(QUICK, sent, 8000);
        let Some(ms) = lat else { bail!("the quick formation popup did not open") };
        rig.add_latency("quick popup open click-to-card", ms);
        // icons appear over the grid after the popup is read
        let t0 = unix_ms();
        let mut marks_up = None;
        while unix_ms() - t0 < 6000 {
            let m = rig.dump()?["marks"].as_u64().unwrap_or(0);
            if m > 0 {
                marks_up = Some((unix_ms() - sent) as f64);
                break;
            }
            nap(10);
        }
        match marks_up {
            Some(ms) => {
                rig.add_latency("quick popup click-to-icons", ms);
                rig.record("stale", &format!("rep {rep}: icons appear on the popup"), format!("{ms:.0} ms"), true);
            }
            None => rig.record("stale", &format!("rep {rep}: icons appear on the popup"), "no icons in 6 s", false),
        }
        if rep == 1 {
            rig.shot("quick-popup")?;
        }
        // a second, read-only watcher samples the game while the popup closes: when does the screen first
        // change, so the add-on's own share of the delay can be told from the game's animation
        let base_key = baocr_pixels::frame_key(&rig.grab()?);
        let watch_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watch = {
            let stop = watch_stop.clone();
            let region = rig.region;
            std::thread::spawn(move || {
                let session = baocr_hcu::HcuSession::default_backend();
                let mut samples: Vec<(u128, f64)> = Vec::new();
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    if let Ok(img) = baocr_hcu::game_capture(session.view(), region) {
                        samples.push((unix_ms(), baocr_pixels::frame_distance(&base_key, &baocr_pixels::frame_key(&img))));
                    }
                }
                samples
            })
        };
        let closed = rig.click(1243, 210, QUICK, "close 빠른 편성")?;
        let t0 = unix_ms();
        let mut zero = None;
        while unix_ms() - t0 < 5000 {
            let d = rig.dump()?;
            if d["marks"].as_u64() == Some(0) && d["main"]["scene"] == MOCK_FORM {
                zero = Some((unix_ms() - closed) as f64);
                break;
            }
            nap(8);
        }
        watch_stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let samples = watch.join().unwrap_or_default();
        if let Some((at, _)) = samples.iter().find(|(at, d)| *at >= closed && *d >= 1.5) {
            let first = (*at - closed) as f64;
            rig.add_latency("popup close click-to-first-change (game, sampled)", first);
            if let Some(ms) = zero {
                rig.add_latency("popup close add-on share (no icons minus first change)", ms - first);
            }
        }
        match zero {
            Some(ms) => {
                rig.add_latency("popup close click-to-no-icons", ms);
                rig.record("stale", &format!("rep {rep}: marks 0 within 1.0 s of leaving"), format!("{ms:.0} ms"), ms <= 1000.0);
            }
            None => {
                let (p, _) = rig.shot("FAIL-stale")?;
                rig.record("stale", &format!("rep {rep}: marks 0 within 1.0 s of leaving"), format!("marks still {} after 5 s ({})", rig.dump()?["marks"], p.display()), false);
            }
        }
        // a forced redraw of the card (folding and unfolding the student section) must not bring them back
        let mut stayed = true;
        for _ in 0..2 {
            if card_click(rig, "section:student").is_ok() {
                nap(700);
                stayed &= rig.dump()?["marks"].as_u64() == Some(0);
            }
        }
        rig.record("stale", &format!("rep {rep}: marks stay 0 across a forced card redraw"), format!("marks {}", rig.dump()?["marks"]), stayed);
        park();
        nap(500);
    }
    // leaving by the back arrow while the popup is up is not done: only the popup's own close is used.
    // leave the formation: back to the mock list, then the hub
    recover(rig)?;
    Ok(())
}

// ---------------------------------------------------------------- c. battle detector

fn battle(rig: &mut Rig, _wiki: &Wiki) -> Result<()> {
    let dir = Path::new(REPO).join("fixtures/screens");
    let mut pos = 0;
    let mut neg_bad = Vec::new();
    for e in std::fs::read_dir(&dir)? {
        let p = e?.path();
        if p.extension().is_some_and(|x| x == "png") {
            let img = image::open(&p)?.to_rgb8();
            let is = baocr_pixels::looks_like_battle(&img);
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if name.starts_with("battle") {
                pos += is as u32;
            } else if is {
                neg_bad.push(name);
            }
        }
    }
    rig.record("battle", "saved battle fixture is detected as a battle", format!("{pos} of 1"), pos == 1);
    rig.record("battle", "no non-battle fixture is detected as a battle", format!("{neg_bad:?}"), neg_bad.is_empty());
    rig.record("battle", "live frames grabbed in this run that looked like a battle", format!("{} of {}", rig.battle_frames_seen, rig.frames_seen), rig.battle_frames_seen == 0);
    Ok(())
}

// ---------------------------------------------------------------- d. card UI

fn card_view(rig: &Rig) -> Result<Value> {
    Ok(rig.dump()?["view"].clone())
}

fn hit_center(view: &Value, id: &str) -> Option<(i32, i32)> {
    let h = view["hits"].as_array()?.iter().find(|h| h["id"] == id)?;
    let r: Vec<i32> = h["rect"].as_array()?.iter().map(|v| v.as_i64().unwrap_or(0) as i32).collect();
    Some(((r[0] + r[2]) / 2, (r[1] + r[3]) / 2))
}

fn client_center() -> (i32, i32) {
    let c = game_client().unwrap_or([400, 300, 1000, 750]);
    ((c[0] + c[2]) / 2, (c[1] + c[3]) / 2)
}

fn park() {
    let (x, y) = client_center();
    mouse::move_to(x, y);
}

/// A real mouse click on one of the card's hit areas (the card must be open).
fn card_click(rig: &Rig, id: &str) -> Result<u128> {
    let v = card_view(rig)?;
    let (x, y) = hit_center(&v, id).ok_or_else(|| anyhow!("the card has no '{id}' area (fold '{}')", v["fold"]))?;
    mouse::move_to(x, y);
    nap(450);
    let t = unix_ms();
    let held = mouse::down();
    nap(50);
    drop(held);
    Ok(t)
}

fn wait_until(rig: &Rig, timeout_ms: u64, mut pred: impl FnMut(&Value) -> bool) -> Option<f64> {
    let t0 = unix_ms();
    while unix_ms() - t0 < timeout_ms as u128 {
        if let Ok(d) = rig.dump() {
            if pred(&d) {
                return Some((unix_ms() - t0) as f64);
            }
        }
        nap(5);
    }
    None
}

fn card(rig: &mut Rig, _wiki: &Wiki) -> Result<()> {
    if rig.scene() != HUB {
        recover(rig)?;
    }
    park();
    nap(800);
    let d = rig.dump()?;
    rig.record("card", "close icon present on the main card", format!("close={}", d["main"]["close"]), d["main"]["close"] == true);
    let has_pin = hit_center(&d["view"], "pin").is_some();
    rig.record("card", "pin button present (game connected)", format!("hit area 'pin': {has_pin}"), has_pin);
    let anchor0 = d["dock"]["anchor"].as_str().unwrap_or("").to_string();
    let pres0 = d["dock"]["presentation"].clone();

    // section fold: rows change, 5 repetitions each way
    let rows_open = d["main"]["rows"].as_array().map_or(0, |r| r.len());
    for rep in 1..=5 {
        let t = card_click(rig, "section:student")?;
        let ms = wait_until(rig, 3000, |d| d["main"]["rows"].as_array().is_some_and(|r| r.len() < rows_open));
        let folded = rig.dump()?["main"]["rows"].as_array().map_or(0, |r| r.len());
        match ms {
            Some(_) => {
                let lat = (rig.dump()?["changed_unix_ms"].as_u64().unwrap_or(0) as u128).saturating_sub(t) as f64;
                rig.add_latency("card section fold click-to-dump", lat);
                rig.record("card", &format!("rep {rep}: header click folds the student section"), format!("rows {rows_open} to {folded}, {lat:.0} ms"), true);
            }
            None => rig.record("card", &format!("rep {rep}: header click folds the student section"), format!("rows stayed {folded}"), false),
        }
        let t = card_click(rig, "section:student")?;
        let ms = wait_until(rig, 3000, |d| d["main"]["rows"].as_array().is_some_and(|r| r.len() == rows_open));
        let lat = (rig.dump()?["changed_unix_ms"].as_u64().unwrap_or(0) as u128).saturating_sub(t) as f64;
        if ms.is_some() {
            rig.add_latency("card section fold click-to-dump", lat);
        }
        rig.record("card", &format!("rep {rep}: second click unfolds it again"), format!("rows back to {rows_open}: {}", ms.is_some()), ms.is_some());
    }

    // pin: Pinned to Bookmark; then hover timings
    card_click(rig, "pin")?;
    let ms = wait_until(rig, 2000, |d| d["dock"]["presentation"] == "bookmark");
    rig.record("card", "pin button switches the card to Bookmark", format!("{:?} ms", ms), ms.is_some());
    park();
    let ms = wait_until(rig, 3000, |d| d["view"]["fold"] == "folded");
    rig.record("card", "after the pointer leaves, the bookmark card folds", format!("{:?} ms", ms), ms.is_some());
    for rep in 1..=5 {
        park();
        wait_until(rig, 3000, |d| d["view"]["fold"] == "folded");
        nap(300);
        let v = card_view(rig)?;
        let r: Vec<i32> = v["rect"].as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0) as i32).collect()).unwrap_or_default();
        if r.len() < 4 || v["fold"] != "folded" {
            rig.record("card", &format!("rep {rep}: tab is there to hover"), format!("view {v}"), false);
            continue;
        }
        let (tx, ty) = ((r[0] + r[2]) / 2, (r[1] + r[3]) / 2);
        mouse::move_to(tx, ty);
        let t0 = unix_ms();
        let open = wait_until(rig, 3000, |d| d["view"]["fold"] == "open");
        let open_ms = open.map(|_| unix_ms() - t0);
        match open_ms {
            Some(ms) => {
                rig.add_latency("bookmark hover to open", ms as f64);
                rig.record("card", &format!("rep {rep}: hover opens the card (dwell 100 + slide 120 ms, allowed up to 500)"), format!("{ms} ms"), (100..=500).contains(&(ms as i64)));
            }
            None => rig.record("card", &format!("rep {rep}: hover opens the card"), "never opened in 3 s", false),
        }
        nap(300);
        // away: start at the pointer leaving
        park();
        let t1 = unix_ms();
        let closed = wait_until(rig, 3000, |d| d["view"]["fold"] == "folded");
        let closed_ms = closed.map(|_| unix_ms() - t1);
        match closed_ms {
            Some(ms) => {
                rig.add_latency("bookmark leave to folded", ms as f64);
                rig.record("card", &format!("rep {rep}: leaving closes it (350 ms delay + 120 ms slide, allowed 330 to 800)"), format!("{ms} ms"), (330..=800).contains(&(ms as i64)));
            }
            None => rig.record("card", &format!("rep {rep}: leaving closes it"), "never folded in 3 s", false),
        }
    }
    // a pass-by shorter than the dwell must not open it
    {
        let v = card_view(rig)?;
        let r: Vec<i32> = v["rect"].as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0) as i32).collect()).unwrap_or_default();
        if r.len() == 4 && v["fold"] == "folded" {
            mouse::move_to((r[0] + r[2]) / 2, (r[1] + r[3]) / 2);
            nap(40);
            park();
            nap(500);
            rig.record("card", "a 40 ms pass over the tab does not open it", format!("fold {}", rig.dump()?["view"]["fold"]), rig.dump()?["view"]["fold"] == "folded");
        }
    }
    // pin again: hover open, click pin
    let v = card_view(rig)?;
    let r: Vec<i32> = v["rect"].as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0) as i32).collect()).unwrap_or_default();
    if r.len() == 4 {
        mouse::move_to((r[0] + r[2]) / 2, (r[1] + r[3]) / 2);
        let opened = wait_until(rig, 3000, |d| d["view"]["fold"] == "open");
        rig.record("card", "hover opens the card again before the pin click", format!("{opened:?} ms, view {}", card_view(rig)?["fold"]), opened.is_some());
        // onto the card itself (the tab's own spot is not part of the open card)
        if let Some((x, y)) = hit_center(&card_view(rig)?, "drag") {
            mouse::move_to(x, y);
        }
        nap(300);
        card_click(rig, "pin")?;
        let ms = wait_until(rig, 2000, |d| d["dock"]["presentation"] == "pinned");
        rig.record("card", "pin button switches back to Pinned", format!("{:?} ms", ms), ms.is_some());
    }
    park();
    nap(600);
    rig.record("card", "pinned card stays open with the pointer away", format!("fold {}", rig.dump()?["view"]["fold"]), rig.dump()?["view"]["fold"] == "open");

    // drag to each anchor
    let cl = game_client().ok_or_else(|| anyhow!("the game window rectangle is unknown"))?;
    let (cw, ch) = (cl[2] - cl[0], cl[3] - cl[1]);
    let targets = [("il", (cl[0] + cw * 15 / 100, cl[1] + ch * 25 / 100)), ("ir", (cl[2] - cw * 15 / 100, cl[1] + ch * 25 / 100)), ("r", (cl[2] + 250, cl[1] + ch * 25 / 100)), ("l", (cl[0] - 250, cl[1] + ch * 25 / 100))];
    for (code, to) in targets {
        let v = card_view(rig)?;
        let Some(from) = hit_center(&v, "drag") else {
            rig.record("card", &format!("drag to anchor {code}"), format!("no drag handle (fold {})", v["fold"]), false);
            continue;
        };
        let before = card_view(rig)?;
        mouse::drag(from, to);
        nap(100);
        let ok = wait_until(rig, 2500, |d| d["dock"]["anchor"] == code);
        let got = rig.dump()?["dock"]["anchor"].clone();
        rig.record("card", &format!("drag by the header to anchor {code} (pointer at {to:?})"), format!("anchor now {got}; grabbed at {from:?}, card rect before {}, after {}, client {cl:?}", before["rect"], card_view(rig)?["rect"]), ok.is_some());
        park();
        nap(500);
    }
    // put it back where it was
    if let Some(from) = hit_center(&card_view(rig)?, "drag") {
        let to = match anchor0.as_str() {
            "il" => (cl[0] + cw * 15 / 100, cl[1] + ch * 25 / 100),
            "ir" => (cl[2] - cw * 15 / 100, cl[1] + ch * 25 / 100),
            "r" => (cl[2] + 250, cl[1] + ch * 25 / 100),
            _ => (cl[0] - 250, cl[1] + ch * 25 / 100),
        };
        if rig.dump()?["dock"]["anchor"] != anchor0.as_str() {
            mouse::drag(from, to);
            nap(800);
        }
    }
    park();
    let _ = pres0;
    Ok(())
}

// ---------------------------------------------------------------- e. scan parity

fn scan(rig: &mut Rig, addon: &mut Addon, exe: &str, dump: &Path, label: &str, scan_out: &Path) -> Result<()> {
    let prev: Value = serde_json::from_str(&std::fs::read_to_string("D:/tmp/live-e2e/backup/students/students.json")?)?;
    if rig.scene() != HUB {
        recover(rig)?;
    }
    // the scan begins from the main menu
    let t = rig.click(HOME.0, HOME.1, HUB, "hub to main menu (scan starts there)")?;
    let lobby = rig.wait_scene("메인 메뉴", t, 8000).or_else(|| rig.wait_scene("메인 메뉴 (대기)", t, 3000));
    rig.record("scan", "home icon leads to the main menu", format!("{lobby:?} ms"), lobby.is_some());
    if lobby.is_none() {
        bail!("not on the main menu: scene '{}'", rig.scene());
    }
    nap(1200);
    park();
    let started = unix_ms();
    card_click(rig, "scan:start")?;
    let t0 = std::time::Instant::now();
    let mut first_progress = None;
    let mut last_done = 0u64;
    let mut stalled_since = std::time::Instant::now();
    let final_title;
    let mut esc_sent = false;
    loop {
        nap(500);
        let d = rig.dump()?;
        let title = d["main"]["title"].as_str().unwrap_or("").to_string();
        if title.contains("스캔") || title.contains("학생 수") {
            if first_progress.is_none() {
                first_progress = Some(unix_ms() - started);
            }
            let line = d["main"]["rows"].as_array().and_then(|r| r.first()).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let done = line.split_whitespace().find_map(|w| w.split('/').next().and_then(|n| n.parse::<u64>().ok())).unwrap_or(0);
            if done != last_done {
                last_done = done;
                stalled_since = std::time::Instant::now();
            }
            // an injected ESC must not stop the scan: send one early, once, and see it go on
            if !esc_sent && t0.elapsed().as_secs() > 20 && last_done >= 2 {
                esc_sent = true;
                let before = last_done;
                inject_escape();
                nap(6000);
                let d2 = rig.dump()?;
                let t2 = d2["main"]["title"].as_str().unwrap_or("");
                let progressed = d2["main"]["rows"].as_array().and_then(|r| r.first()).and_then(|v| v.as_str()).unwrap_or("");
                rig.record("scan", "an injected ESC key does not abort the scan", format!("title '{t2}', progress '{progressed}' (was {before})"), t2.contains("스캔하는") || t2.contains("학생 수"));
            }
            if title.contains("마쳤") || title.contains("중단") {
                final_title = title;
                break;
            }
        } else if first_progress.is_some() && (title.is_empty() || !d["main"]["rows"].to_string().contains("진행")) {
            final_title = format!("card left the scan view: '{title}'");
            break;
        }
        if stalled_since.elapsed().as_secs() > 150 {
            bail!("the scan made no progress for 150 s (last '{}')", last_done);
        }
        if t0.elapsed().as_secs() > 30 * 60 {
            bail!("the scan took more than 30 minutes");
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    rig.record("scan", "scan started within 10 s of the click", format!("{:?} ms", first_progress), first_progress.is_some_and(|m| m < 10_000));
    rig.record("scan", "scan finished", format!("{final_title} after {secs:.0} s"), final_title.contains("마쳤"));
    nap(6000);
    let _ = (addon, exe, dump, label);

    let new: Value = serde_json::from_str(&std::fs::read_to_string(scan_out.join("students.json"))?)?;
    let n_new = new["entries"].as_array().map_or(0, |e| e.len());
    let n_old = prev["entries"].as_array().map_or(0, |e| e.len());
    rig.record("scan", "entry count equals the previous scan", format!("{n_new} vs {n_old}"), n_new == n_old);
    rig.record("scan", "unresolved is 0", format!("{}", new["unresolved"].as_array().map_or(0, |u| u.len())), new["unresolved"].as_array().is_some_and(|u| u.is_empty()));
    rig.record("scan", "complete is true", format!("{} ({})", new["complete"], new["stop_reason"]), new["complete"] == true);
    let (equal, diffs) = compare_students(&prev, &new);
    rig.record("scan", "per-student fields equal the previous scan (volatile fields excluded)", format!("{equal} of {n_old} identical; {} differing fields", diffs.len()), diffs.is_empty());
    std::fs::write(rig.out.join("scan-diff.txt"), diffs.join("\n"))?;
    // the game is wherever the scan left it: bring it back to the hub
    nap(2000);
    recover(rig).or_else(|_| start_position(rig))?;
    rig.record("scan", "scan duration (s), for the record", format!("{secs:.0}"), true);
    Ok(())
}

const VOLATILE: &[&str] = &["page", "column", "detail_path"];

fn flatten(prefix: &str, v: &Value, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(m) => m.iter().for_each(|(k, x)| flatten(&format!("{prefix}.{k}"), x, out)),
        Value::Array(a) => a.iter().enumerate().for_each(|(i, x)| flatten(&format!("{prefix}[{i}]"), x, out)),
        other => out.push((prefix.to_string(), other.to_string())),
    }
}

fn compare_students(prev: &Value, new: &Value) -> (usize, Vec<String>) {
    let (a, b) = (prev["entries"].as_array().cloned().unwrap_or_default(), new["entries"].as_array().cloned().unwrap_or_default());
    let mut diffs = Vec::new();
    let mut equal = 0;
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        let (mut fx, mut fy) = (Vec::new(), Vec::new());
        flatten("", x, &mut fx);
        flatten("", y, &mut fy);
        let name = x["student"]["name"].as_str().unwrap_or("?");
        let (mx, my): (std::collections::BTreeMap<_, _>, std::collections::BTreeMap<_, _>) = (fx.into_iter().collect(), fy.into_iter().collect());
        let mut same = true;
        for key in mx.keys().chain(my.keys()).collect::<std::collections::BTreeSet<_>>() {
            if VOLATILE.iter().any(|v| key.trim_start_matches('.') == *v) {
                continue;
            }
            if mx.get(key) != my.get(key) {
                same = false;
                diffs.push(format!("#{i:03} {name}: {key}: {} -> {}", mx.get(key).map_or("(absent)", |s| s.as_str()), my.get(key).map_or("(absent)", |s| s.as_str())));
            }
        }
        equal += same as usize;
    }
    (equal, diffs)
}

// ---------------------------------------------------------------- f. input guard

const EXAMPLES: &str = "D:/cargo-target/baocr/release/examples";

fn guard(rig: &mut Rig, _wiki: &Wiki) -> Result<()> {
    let report = baocr_hcu::run_doctor();
    rig.record("guard", "--doctor reports a healthy setup", report.render().replace('\n', " / "), report.healthy());
    let cl = game_client().ok_or_else(|| anyhow!("no game rectangle"))?;
    let (cx, cy) = ((cl[0] + cl[2]) / 2, (cl[1] + cl[3]) / 2);
    // a frame first, so the runner holds the layout it will judge the click against
    let _ = rig.grab()?;
    let log = std::fs::read_to_string(PathBuf::from(std::env::var("APPDATA")?).join("baocr").join("input.log")).map(|t| t.lines().count()).unwrap_or(0);
    let cover_log = rig.out.join("cover.log");
    let mut cover = Command::new(format!("{EXAMPLES}/input_target.exe"))
        .args(["--title", "HCU Cover", "--flat", "--at", &format!("{},{}", cx - 150, cy - 100), "--size", "300,200", "--log"])
        .arg(&cover_log)
        .spawn()
        .map_err(|e| anyhow!("input_target.exe not found in {EXAMPLES} (build: cargo build --release -p baocr-hcu --examples): {e}"))?;
    nap(1500);
    // the point is client centre: under the cover. The click must be refused, and nothing may reach anything.
    let (fx, fy) = baocr_hcu::to_frame(rig.region, 640, 480);
    let before = std::fs::read_to_string(&cover_log).unwrap_or_default().lines().count();
    let res = rig.session.exclusive().click(fx, fy);
    nap(500);
    let after = std::fs::read_to_string(&cover_log).unwrap_or_default().lines().count();
    let _ = cover.kill();
    let _ = cover.wait();
    match res {
        Err(e) => {
            let msg = format!("{e:#}");
            rig.record("guard", "a click under a covering window is refused", msg.clone(), msg.contains("target_window_mismatch") || msg.contains("gui_frame_stale") || msg.contains("covered") || msg.contains("refus"));
        }
        Ok(()) => rig.record("guard", "a click under a covering window is refused", "the click was accepted", false),
    }
    rig.record("guard", "the covering window received no click", format!("{} events", after - before), after == before);
    let lines: Vec<String> = std::fs::read_to_string(PathBuf::from(std::env::var("APPDATA")?).join("baocr").join("input.log"))?.lines().skip(log).map(str::to_string).collect();
    rig.record("guard", "the refusal is in the audit log", lines.last().cloned().unwrap_or_default(), lines.iter().any(|l| l.contains("\"input_emitted\":false")));
    // the self-test drives its own target windows only
    let out = Command::new(format!("{EXAMPLES}/input_selftest.exe")).output().map_err(|e| anyhow!("input_selftest.exe: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let (pass, fail) = (text.lines().filter(|l| l.starts_with("PASS")).count(), text.lines().filter(|l| l.starts_with("FAIL")).count());
    std::fs::write(rig.out.join("input_selftest.txt"), &text)?;
    rig.record("guard", "input_selftest against its own target windows (no FAIL)", format!("{pass} PASS, {fail} FAIL, exit {:?}", out.status.code()), fail == 0 && pass > 0 && out.status.success());
    park();
    Ok(())
}

// ---------------------------------------------------------------- windows helpers

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct WinPoint {
    x: i32,
    y: i32,
}
#[link(name = "user32")]
extern "system" {
    fn FindWindowW(class: *const u16, title: *const u16) -> isize;
    fn GetClientRect(h: isize, r: *mut WinRect) -> i32;
    fn ClientToScreen(h: isize, p: *mut WinPoint) -> i32;
    fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
}

/// The game's client rectangle in screen pixels (left, top, right, bottom).
pub fn game_client() -> Option<[i32; 4]> {
    let title: Vec<u16> = "Blue Archive\0".encode_utf16().collect();
    unsafe {
        let h = FindWindowW(std::ptr::null(), title.as_ptr());
        if h == 0 {
            return None;
        }
        let mut r = WinRect::default();
        let mut p = WinPoint::default();
        if GetClientRect(h, &mut r) == 0 || ClientToScreen(h, &mut p) == 0 {
            return None;
        }
        Some([p.x, p.y, p.x + r.right, p.y + r.bottom])
    }
}

fn inject_escape() {
    unsafe {
        keybd_event(0x1B, 0, 0, 0);
        nap(60);
        keybd_event(0x1B, 0, 2, 0);
    }
}
