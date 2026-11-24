//! The account id: what the add-on sends to the server instead of anything about the player.
//!
//! The player types the game's member code (게임 메뉴 모음 > 계정 > 고객센터 > 회원코드, 13 characters). It is
//! normalised, then hashed here on the PC: `account_id = hex(SHA-256("ba-key-amas/account/v1\0" + code))`.
//! The raw code is never stored, sent, logged or printed. Only the account id is kept (in `upload.json`) and sent.
//! The server hashes it once more under its own secret before storing it (web/README.md, "회원코드와 계정").
//!
//! Nothing was hashed under `account/v1` before this module existed, so the version string stays.
use anyhow::{anyhow, bail, Result};
use hmac_sha256::Hash;

pub const DOMAIN: &str = "ba-key-amas/account/v1\0";
pub const CODE_LEN: usize = 13;

pub const GUIDE: &str = "회원코드를 입력해 주세요.
  - 위치: 게임 메뉴 모음 > 계정 > 고객센터 > 회원코드 (영문 대문자와 숫자 13자, 소문자로 쳐도 같은 코드로 읽습니다. O는 0으로, I는 1로 읽고 L은 L 그대로입니다)
  - 같은 계정을 다음 업로드에서 알아보는 데만 쓰입니다.
  - 입력한 회원코드는 이 PC에서 변환(해시)된 값으로만 서버에 올라가며, 원본은 저장되거나 전송되지 않습니다. 서버는 그 값을 다시 비밀 값으로 변환해 저장합니다.
  - 화면에는 입력이 보이지 않습니다. 틀리게 입력하지 않도록 두 번 입력합니다.";

/// Trimmed and upper-cased first, then only `[A-Z0-9]` is kept and `O` is folded to `0` and `I` to `1`, so a typed
/// letter O for a zero does not split one account in two. The game's code is upper case only, so a lower case `l`
/// the player typed means the letter L and stays `L` (folding it to `1` would give a second account to the same
/// person). Only ASCII is upper-cased, so a look-alike such as the dotless `ı` is dropped, not read as `I`.
/// `None` when nothing is left.
pub fn normalise(raw: &str) -> Option<String> {
    let code: String = raw
        .trim()
        .to_ascii_uppercase()
        .chars()
        .filter_map(|c| match c {
            'O' => Some('0'),
            'I' => Some('1'),
            c if c.is_ascii_uppercase() || c.is_ascii_digit() => Some(c),
            _ => None,
        })
        .collect();
    (!code.is_empty()).then_some(code)
}

/// The normalised member code, or a Korean explanation of what is wrong. The message never repeats the input.
pub fn parse_member_code(raw: &str) -> Result<String> {
    let code = normalise(raw).ok_or_else(|| anyhow!("회원코드가 비어 있습니다"))?;
    let n = code.chars().count();
    if n != CODE_LEN {
        bail!("회원코드는 영문 대문자와 숫자 {CODE_LEN}자입니다. 입력한 값은 {n}자로 읽혔습니다 (공백과 기호는 무시합니다)");
    }
    Ok(code)
}

/// 64 lower case hex digits. `code` is normalised again, so any spelling of one code gives one id.
pub fn account_id(code: &str) -> String {
    let mut h = Hash::new();
    h.update(DOMAIN.as_bytes());
    h.update(normalise(code).unwrap_or_default().as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn is_account_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Two entries of the same code, as typed. Both must be valid and equal after normalising.
pub fn confirmed(first: &str, second: &str) -> Result<String> {
    let a = parse_member_code(first)?;
    let b = parse_member_code(second)?;
    if a != b {
        bail!("두 번 입력한 회원코드가 다릅니다");
    }
    Ok(a)
}

/// Reads one line from the console without echoing it.
pub fn read_hidden(prompt: &str) -> Result<String> {
    imp::read_hidden(prompt)
}

#[cfg(windows)]
mod imp {
    use anyhow::{bail, Context, Result};
    use std::io::{BufRead, BufReader, Write};
    use std::os::windows::io::AsRawHandle;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleMode(handle: isize, mode: *mut u32) -> i32;
        fn SetConsoleMode(handle: isize, mode: u32) -> i32;
    }
    const ENABLE_ECHO_INPUT: u32 = 0x0004;

    /// `CONIN$` is the console itself: the add-on has no standard input of its own (it is a windowless exe that attaches
    /// to the parent console), and a redirected or piped input is not a place to type a secret.
    pub fn read_hidden(prompt: &str) -> Result<String> {
        let conin = std::fs::OpenOptions::new().read(true).write(true).open("CONIN$").context("콘솔에서 실행해 주세요 (회원코드는 화면에 보이지 않게 입력합니다). 스크립트에서는 BAOCR_MEMBER_CODE 환경 변수를 쓰세요")?;
        let handle = conin.as_raw_handle() as isize;
        let mut mode = 0u32;
        if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
            bail!("콘솔에서 실행해 주세요 (회원코드는 화면에 보이지 않게 입력합니다)");
        }
        print!("{prompt}");
        std::io::stdout().flush().ok();
        unsafe { SetConsoleMode(handle, mode & !ENABLE_ECHO_INPUT) };
        let mut line = String::new();
        let read = BufReader::new(&conin).read_line(&mut line);
        unsafe { SetConsoleMode(handle, mode) };
        println!();
        read.context("입력을 읽지 못했습니다")?;
        Ok(line)
    }
}

#[cfg(not(windows))]
mod imp {
    use anyhow::{bail, Result};

    pub fn read_hidden(_prompt: &str) -> Result<String> {
        bail!("회원코드의 숨김 입력은 Windows 콘솔에서만 됩니다. BAOCR_MEMBER_CODE 환경 변수를 쓰세요")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Made-up codes only. The expected ids come from an independent implementation:
    //   node -e 'console.log(require("crypto").createHash("sha256").update("ba-key-amas/account/v1\0"+"AB12CD34EF56G").digest("hex"))'
    const FAKE: &str = "AB12CD34EF56G";
    const FAKE_ID: &str = "c3b695e22c163896b1d76d133b9cb0ddf56296451f1ae5ff6af982828e73a7d3";
    const LOOKALIKE_ID: &str = "2656dad47d0a114a69400c8ba03bb3116342916128e5138e74bb02d502a423bf"; // "AB01L1CDEFG23"

    #[test]
    fn account_id_matches_an_independent_sha256() {
        assert_eq!(account_id(FAKE), FAKE_ID);
        assert_eq!(account_id(" ab12-cd34 ef56g "), FAKE_ID);
        assert!(is_account_id(&account_id("XYZ")));
        assert_eq!(account_id("XYZ"), "14defe57023ce0f389494e3dcead72971d451deeeb50e63e80d092c7111f6809");
    }

    #[test]
    fn normalise_trims_uppercases_and_keeps_letters_and_digits() {
        assert_eq!(normalise("  ab12-cd34 ef56g\r\n").as_deref(), Some(FAKE));
        assert_eq!(normalise("---  "), None);
        assert_eq!(normalise(""), None);
        assert_eq!(normalise("한글ab"), Some("AB".into()), "non-ASCII characters are dropped");
    }

    #[test]
    fn only_o_and_i_are_folded_and_a_typed_l_is_the_letter_l() {
        assert_eq!(normalise("AbOIl1CDEFG23").as_deref(), Some("AB01L1CDEFG23"));
        assert_eq!(account_id("AbOIl1CDEFG23"), LOOKALIKE_ID);
        assert_eq!(account_id("ab0il1cdefg23"), LOOKALIKE_ID, "o and i fold, l is L");
        assert_eq!(account_id("AB01L1CDEFG23"), account_id("ab01l1cdefg23"), "a lower case l is the same account as a capital L");
        assert_ne!(account_id("ABL"), account_id("AB1"), "L and 1 are different codes");
        assert_eq!(normalise("LL").as_deref(), Some("LL"));
        assert_eq!(normalise("ll").as_deref(), Some("LL"));
        assert_eq!(normalise("ıİ").as_deref(), None, "non-ASCII look-alikes of I are dropped, not folded");
    }

    #[test]
    fn a_member_code_is_13_characters() {
        assert_eq!(parse_member_code(" ab12 cd34 ef56 g ").unwrap(), FAKE);
        for bad in ["", "AB12", "AB12CD34EF56GH", "!!!"] {
            let msg = format!("{:#}", parse_member_code(bad).unwrap_err());
            assert!(msg.contains("회원코드"), "{msg}");
            assert!(bad.is_empty() || !msg.contains(bad), "the message does not repeat what was typed");
        }
        assert!(format!("{:#}", parse_member_code("AB12").unwrap_err()).contains("4자"));
    }

    #[test]
    fn two_entries_must_agree_after_normalising() {
        assert_eq!(confirmed("AB12CD34EF56G", " ab12cd34ef56g ").unwrap(), FAKE);
        assert!(format!("{:#}", confirmed("AB12CD34EF56G", "AB12CD34EF56H").unwrap_err()).contains("다릅니다"));
        assert!(confirmed("AB12CD34EF56G", "short").is_err());
    }

    #[test]
    fn the_guide_carries_the_disclosure() {
        assert!(GUIDE.contains("원본은 저장되거나 전송되지 않습니다"));
        assert!(GUIDE.contains("메뉴 모음 > 계정 > 고객센터 > 회원코드"));
    }
}
