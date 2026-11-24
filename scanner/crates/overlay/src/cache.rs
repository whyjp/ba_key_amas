//! Change detection for the overlay thread, which wakes every few tens of milliseconds: what
//! has not changed is not drawn again. Both parts are plain data, no windows, so they are tested
//! on their own.

/// Remembers the last value it was shown and counts how many times it changed. The count is a
/// cheap token for "is this still what was drawn": comparing two numbers replaces comparing (or
/// formatting) the whole state.
#[derive(Debug, Clone)]
pub struct Tracker<T> {
    last: Option<T>,
    generation: u64,
}

impl<T: PartialEq + Clone> Tracker<T> {
    pub fn new() -> Self {
        Tracker { last: None, generation: 0 }
    }

    /// Take `now` in. Returns the generation, which moves on only if `now` differs from the last one.
    pub fn update(&mut self, now: &T) -> u64 {
        if self.last.as_ref() != Some(now) {
            self.last = Some(now.clone());
            self.generation += 1;
        }
        self.generation
    }

    pub fn current(&self) -> Option<&T> {
        self.last.as_ref()
    }
}

impl<T: PartialEq + Clone> Default for Tracker<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// The last rendered value and the key it was rendered for. Asking again with the same key
/// returns it without rendering.
#[derive(Debug)]
pub struct Memo<K, V> {
    slot: Option<(K, V)>,
    renders: u64,
}

impl<K: PartialEq, V> Memo<K, V> {
    pub fn new() -> Self {
        Memo { slot: None, renders: 0 }
    }

    /// The value for `key`, rendered by `render` only when `key` is not the one last asked for.
    /// The number returned counts renders so far: it differs from the previous call's exactly
    /// when the value is new, so it can key whatever was built from the value.
    pub fn get(&mut self, key: K, render: impl FnOnce() -> V) -> (&V, u64) {
        if self.slot.as_ref().map(|(k, _)| k) != Some(&key) {
            self.slot = Some((key, render()));
            self.renders += 1;
        }
        (&self.slot.as_ref().expect("filled above").1, self.renders)
    }

}

impl<K: PartialEq, V> Default for Memo<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generation_moves_only_when_the_value_does() {
        let mut t = Tracker::new();
        let first = t.update(&vec![1, 2]);
        assert_eq!(t.update(&vec![1, 2]), first);
        assert_eq!(t.update(&vec![1, 2]), first);
        let second = t.update(&vec![1, 3]);
        assert_ne!(second, first);
        assert_eq!(t.current(), Some(&vec![1, 3]));
        assert_eq!(t.update(&vec![1, 2]), second + 1, "going back is a change too");
    }

    #[test]
    fn a_value_is_rendered_once_per_key() {
        let mut m = Memo::new();
        let mut calls = 0;
        for key in [(1, 300), (1, 300), (1, 300), (2, 300), (2, 300), (2, 420), (1, 300)] {
            let (v, _) = m.get(key, || {
                calls += 1;
                key.0 * key.1
            });
            assert_eq!(*v, key.0 * key.1);
        }
        assert_eq!(calls, 4, "(1,300) (2,300) (2,420) (1,300)");
    }

    #[test]
    fn the_render_count_keys_what_is_built_from_the_value() {
        let mut m = Memo::new();
        let (_, a) = m.get("x", || 1);
        let (_, b) = m.get("x", || 1);
        let (_, c) = m.get("y", || 2);
        assert_eq!((a, b), (1, 1));
        assert_eq!(c, 2);
        let (_, d) = m.get("z", || 3);
        assert_eq!(d, 3);
    }
}
