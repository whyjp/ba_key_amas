//! One JSON line per input call, so what the add-on did to the desktop can be read afterwards.
//! Positions, the window under the point and the result only: never typed content or images.
use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

const ROTATE_AT: u64 = 1 << 20;

/// Where audit lines go. Tests supply a collector; production writes a file.
pub trait AuditSink: Send + Sync {
    fn record(&self, line: &Value);
}

/// Appends to a file; past ~1 MB the file becomes `<name>.1` (replacing an older one) and a new one starts.
pub struct FileSink {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileSink {
    pub fn new(path: PathBuf) -> Self {
        Self { path, lock: Mutex::new(()) }
    }

    /// `%APPDATA%\baocr\input.log`, or None when logging is switched off (`BAOCR_INPUT_LOG=0`) or there is no such folder.
    pub fn default_sink() -> Option<FileSink> {
        if std::env::var("BAOCR_INPUT_LOG").is_ok_and(|v| v == "0") {
            return None;
        }
        let dir = std::env::var_os("APPDATA").map(PathBuf::from)?.join("baocr");
        Some(FileSink::new(dir.join("input.log")))
    }

    fn write(&self, line: &str) -> std::io::Result<()> {
        let _g = self.lock.lock().unwrap();
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if std::fs::metadata(&self.path).is_ok_and(|m| m.len() > ROTATE_AT) {
            let mut old = self.path.clone().into_os_string();
            old.push(".1");
            let old = PathBuf::from(old);
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&self.path, &old)?;
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(f, "{line}")
    }
}

impl AuditSink for FileSink {
    /// A log that cannot be written does not stop the scan the player asked for: the failure is shown once.
    fn record(&self, line: &Value) {
        if let Err(e) = self.write(&line.to_string()) {
            static SAID: std::sync::Once = std::sync::Once::new();
            SAID.call_once(|| eprintln!("input log {}: {e}", self.path.display()));
        }
    }
}

/// `2026-11-23T12:34:56.789Z` for a moment (UTC).
pub fn iso_utc(t: SystemTime) -> String {
    let ms = t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    let (secs, milli) = (ms.div_euclid(1000), ms.rem_euclid(1000));
    let (days, day_secs) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil-from-days (proleptic Gregorian)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{milli:03}Z", day_secs / 3600, day_secs % 3600 / 60, day_secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn times_are_written_as_utc_dates() {
        assert_eq!(iso_utc(SystemTime::UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
        // 2026-11-23 12:34:56.789 UTC
        assert_eq!(iso_utc(SystemTime::UNIX_EPOCH + Duration::from_millis(1_795_437_296_789)), "2026-11-23T12:34:56.789Z");
        assert_eq!(iso_utc(SystemTime::UNIX_EPOCH + Duration::from_secs(951_782_400)), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn the_file_rotates_once_it_is_over_a_megabyte_and_keeps_one_old_file() {
        let dir = std::env::temp_dir().join(format!("baocr-audit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sink = FileSink::new(dir.join("input.log"));
        let line = serde_json::json!({"pad": "x".repeat(1000)});
        for _ in 0..1100 {
            sink.record(&line);
        }
        assert!(dir.join("input.log.1").exists(), "rotated");
        let (now, old) = (std::fs::metadata(dir.join("input.log")).unwrap().len(), std::fs::metadata(dir.join("input.log.1")).unwrap().len());
        assert!(now < ROTATE_AT && old > ROTATE_AT, "{now} {old}");
        for _ in 0..1100 {
            sink.record(&line);
        }
        assert!(!dir.join("input.log.2").exists(), "only one old file is kept");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
