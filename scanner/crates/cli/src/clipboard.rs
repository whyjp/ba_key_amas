//! The clipboard and the browser, for the add-on's card (the member code is pasted in, the page of the upload is
//! copied out or opened). Windows only; elsewhere nothing happens.

#[cfg(windows)]
mod sys {
    #[link(name = "user32")]
    extern "system" {
        pub fn OpenClipboard(owner: isize) -> i32;
        pub fn CloseClipboard() -> i32;
        pub fn EmptyClipboard() -> i32;
        pub fn GetClipboardData(format: u32) -> isize;
        pub fn SetClipboardData(format: u32, handle: isize) -> isize;
    }
    #[link(name = "kernel32")]
    extern "system" {
        pub fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
        pub fn GlobalLock(handle: isize) -> *mut u16;
        pub fn GlobalUnlock(handle: isize) -> i32;
    }
    #[link(name = "shell32")]
    extern "system" {
        pub fn ShellExecuteW(hwnd: isize, op: *const u16, file: *const u16, params: *const u16, dir: *const u16, show: i32) -> isize;
    }
    pub const CF_UNICODETEXT: u32 = 13;
    pub const GMEM_MOVEABLE: u32 = 2;
    pub const SW_SHOWNORMAL: i32 = 1;
}

/// The text on the clipboard, if there is any.
pub fn get_text() -> Option<String> {
    #[cfg(windows)]
    unsafe {
        if sys::OpenClipboard(0) == 0 {
            return None;
        }
        let handle = sys::GetClipboardData(sys::CF_UNICODETEXT);
        let text = if handle == 0 {
            None
        } else {
            let p = sys::GlobalLock(handle);
            if p.is_null() {
                None
            } else {
                let mut len = 0;
                while *p.add(len) != 0 {
                    len += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
                sys::GlobalUnlock(handle);
                Some(s)
            }
        };
        sys::CloseClipboard();
        text
    }
    #[cfg(not(windows))]
    None
}

/// Put `text` on the clipboard; whether it worked.
pub fn set_text(text: &str) -> bool {
    #[cfg(windows)]
    unsafe {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        if sys::OpenClipboard(0) == 0 {
            return false;
        }
        sys::EmptyClipboard();
        let handle = sys::GlobalAlloc(sys::GMEM_MOVEABLE, wide.len() * 2);
        let ok = if handle == 0 {
            false
        } else {
            let p = sys::GlobalLock(handle);
            if p.is_null() {
                false
            } else {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                sys::GlobalUnlock(handle);
                sys::SetClipboardData(sys::CF_UNICODETEXT, handle) != 0
            }
        };
        sys::CloseClipboard();
        ok
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        false
    }
}

/// Open an address in the default browser; whether it was handed over.
pub fn open_url(url: &str) -> bool {
    #[cfg(windows)]
    unsafe {
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        sys::ShellExecuteW(0, wide("open").as_ptr(), wide(url).as_ptr(), std::ptr::null(), std::ptr::null(), sys::SW_SHOWNORMAL) > 32
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        false
    }
}

/// Only the project's own pages are opened from the card.
pub fn is_openable(url: &str, official: &str) -> bool {
    url.starts_with(&format!("{}/", official.trim_end_matches('/')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pages_of_the_official_server_are_opened() {
        let official = "https://key-amas.cxx.co.kr";
        assert!(is_openable("https://key-amas.cxx.co.kr/#/u/abc", official));
        assert!(!is_openable("https://key-amas.cxx.co.kr.evil.test/#/u/abc", official));
        assert!(!is_openable("file:///C:/Windows/System32/cmd.exe", official));
    }
}
