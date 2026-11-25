//! A pause that ends early when something has changed, so the overlay thread draws a new card
//! at once instead of at its next poll (a poll every 90 ms adds 45 ms on average).
use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[derive(Default)]
pub(crate) struct Wake {
    changed: Mutex<bool>,
    signal: Condvar,
}

impl Wake {
    /// Something to draw: the waiting thread (now or next) returns at once.
    pub fn notify(&self) {
        if let Ok(mut c) = self.changed.lock() {
            *c = true;
            self.signal.notify_one();
        }
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    /// Pause for at most `max`, less when `notify` is called. A call made before the wait counts.
    pub fn wait(&self, max: Duration) {
        let Ok(guard) = self.changed.lock() else {
            std::thread::sleep(max);
            return;
        };
        if let Ok((mut c, _)) = self.signal.wait_timeout_while(guard, max, |changed| !*changed) {
            *c = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn without_a_notify_the_whole_pause_is_slept() {
        let w = Wake::default();
        let t = Instant::now();
        w.wait(Duration::from_millis(60));
        assert!(t.elapsed() >= Duration::from_millis(55));
    }

    #[test]
    fn a_notify_ends_the_pause_early_and_one_made_before_it_is_not_lost() {
        let w = Arc::new(Wake::default());
        let other = w.clone();
        let t = Instant::now();
        let h = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            other.notify();
        });
        w.wait(Duration::from_secs(5));
        assert!(t.elapsed() < Duration::from_secs(2), "woken by the notify, not by the timeout");
        h.join().unwrap();
        w.notify();
        let t = Instant::now();
        w.wait(Duration::from_secs(5));
        assert!(t.elapsed() < Duration::from_secs(2));
        let t = Instant::now();
        w.wait(Duration::from_millis(40));
        assert!(t.elapsed() >= Duration::from_millis(35), "a notify wakes one wait only");
    }
}
