//! Numbered handles for objects that Rust owns and Kotlin refers to by a `Long`.
//!
//! Kotlin used to hold a raw pointer, so a call after `free` read freed memory, and only the order
//! of calls in `TunnelService` prevented that; no test could catch a mistake there. With a table, a
//! freed or unknown handle finds nothing, and a call already holding the object keeps it alive.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

pub struct Handles<T> {
    inner: Mutex<Table<T>>,
}

struct Table<T> {
    /// The last id handed out. Ids start at 1, because Kotlin treats 0 as "no session".
    last: i64,
    live: HashMap<i64, Arc<T>>,
}

impl<T> Handles<T> {
    pub fn new() -> Self {
        Self { inner: Mutex::new(Table { last: 0, live: HashMap::new() }) }
    }

    pub fn insert(&self, value: T) -> i64 {
        let mut table = self.lock();
        table.last += 1;
        let id = table.last;
        table.live.insert(id, Arc::new(value));
        id
    }

    pub fn get(&self, id: i64) -> Option<Arc<T>> {
        self.lock().live.get(&id).cloned()
    }

    /// Returns false when the handle was already freed or never existed.
    pub fn remove(&self, id: i64) -> bool {
        self.lock().live.remove(&id).is_some()
    }

    fn lock(&self) -> MutexGuard<'_, Table<T>> {
        // A panic elsewhere while holding the lock leaves the map itself intact.
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_freed_handle_finds_nothing() {
        let table = Handles::new();
        let id = table.insert("session");
        assert_eq!(table.get(id).as_deref(), Some(&"session"));
        assert!(table.remove(id));
        assert!(table.get(id).is_none());
        assert!(!table.remove(id), "freeing twice is harmless");
    }

    #[test]
    fn ids_are_never_zero_and_never_reused() {
        let table = Handles::new();
        let a = table.insert(1);
        table.remove(a);
        let b = table.insert(2);
        assert!(a != 0 && b != 0 && a != b);
        assert!(table.get(0).is_none());
    }

    #[test]
    fn a_call_in_progress_keeps_the_object_alive() {
        let table = Handles::new();
        let id = table.insert(String::from("client"));
        let in_use = table.get(id).unwrap();
        table.remove(id);
        assert_eq!(*in_use, "client");
    }
}
