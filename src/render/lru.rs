//! Byte-bounded least-recently-used cache used by the in-memory render tiers.

use std::{
  borrow::Borrow,
  collections::{BTreeMap, HashMap},
  hash::Hash,
};

pub(super) struct LruCache<K, V> {
  /// Size limit in bytes; `0` disables the limit.
  max_bytes: u64,
  bytes: u64,
  tick: u64,
  entries: HashMap<K, Entry<V>>,
  /// Recency order: the smallest tick is the least recently used entry.
  order: BTreeMap<u64, K>,
}

struct Entry<V> {
  value: V,
  size: u64,
  tick: u64,
}

impl<K: Clone + Eq + Hash, V> LruCache<K, V> {
  pub(super) fn new(max_bytes: u64) -> Self {
    Self {
      max_bytes,
      bytes: 0,
      tick: 0,
      entries: HashMap::new(),
      order: BTreeMap::new(),
    }
  }

  /// Look up `key` and mark it as most recently used.
  pub(super) fn get<Q>(&mut self, key: &Q) -> Option<&V>
  where
    K: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
  {
    let tick = self.next_tick();
    let entry = self.entries.get_mut(key)?;
    let old_tick = std::mem::replace(&mut entry.tick, tick);
    if let Some(key) = self.order.remove(&old_tick) {
      self.order.insert(tick, key);
    }
    Some(&entry.value)
  }

  /// Mark `key` as most recently used; false when it is not cached.
  pub(super) fn touch<Q>(&mut self, key: &Q) -> bool
  where
    K: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
  {
    self.get(key).is_some()
  }

  /// Look up `key` without changing its recency.
  pub(super) fn peek<Q>(&self, key: &Q) -> Option<&V>
  where
    K: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
  {
    self.entries.get(key).map(|entry| &entry.value)
  }

  pub(super) fn insert(&mut self, key: K, value: V, size: u64) {
    self.remove(&key);
    let tick = self.next_tick();
    self.bytes = self.bytes.saturating_add(size);
    self.order.insert(tick, key.clone());
    self.entries.insert(key, Entry { value, size, tick });
    self.enforce_limit();
  }

  pub(super) fn remove<Q>(&mut self, key: &Q)
  where
    K: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
  {
    if let Some(old) = self.entries.remove(key) {
      self.bytes = self.bytes.saturating_sub(old.size);
      self.order.remove(&old.tick);
    }
  }

  pub(super) fn clear(&mut self) {
    self.bytes = 0;
    self.entries.clear();
    self.order.clear();
  }

  fn next_tick(&mut self) -> u64 {
    self.tick += 1;
    self.tick
  }

  /// Evict least recently used entries until the cache fits, always keeping
  /// the newest entry even when it alone exceeds the limit.
  fn enforce_limit(&mut self) {
    if self.max_bytes == 0 {
      return;
    }
    while self.bytes > self.max_bytes && self.entries.len() > 1 {
      let Some((_, key)) = self.order.pop_first() else {
        break;
      };
      if let Some(old) = self.entries.remove(&key) {
        self.bytes = self.bytes.saturating_sub(old.size);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn evicts_least_recently_used_first() {
    let mut cache = LruCache::new(3);
    cache.insert("a", 1, 1);
    cache.insert("b", 2, 1);
    cache.insert("c", 3, 1);
    assert_eq!(cache.get("a"), Some(&1));
    cache.insert("d", 4, 1);
    assert_eq!(cache.get("b"), None);
    assert_eq!(cache.get("a"), Some(&1));
    assert_eq!(cache.get("c"), Some(&3));
    assert_eq!(cache.get("d"), Some(&4));
  }

  #[test]
  fn keeps_oversized_newest_entry() {
    let mut cache = LruCache::new(2);
    cache.insert("a", 1, 1);
    cache.insert("big", 2, 10);
    assert_eq!(cache.get("a"), None);
    assert_eq!(cache.get("big"), Some(&2));
    cache.remove("big");
    assert_eq!(cache.bytes, 0);
  }
}
