use std::borrow::Borrow;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct LruCacheEntry<T> {
    item:      T,
    last_used: Instant,
}

pub struct LruCache<K, V> {
    cache:      Mutex<lru::LruCache<K, LruCacheEntry<V>>>,
    max_unused: Duration,
}

impl<K, V> LruCache<K, V>
where
    K: Hash + Eq,
    V: Clone,
{
    pub fn new(max_unused: Duration) -> LruCache<K, V> {
        LruCache {
            cache: Mutex::new(lru::LruCache::unbounded()),
            max_unused,
        }
    }

    pub fn put(&self, item_key: K, item_value: V)
    where
        K: Hash + Eq + Clone,
    {
        let mut cache = self.cache.lock().unwrap();
        cache.put(
            item_key,
            LruCacheEntry {
                item:      item_value,
                last_used: Instant::now(),
            },
        );
    }

    pub fn get<Q: ?Sized>(&self, item_key: &Q) -> Option<V>
    where
        lru::KeyRef<K>: Borrow<Q>,
        Q: Hash + Eq,
    {
        let mut cache = self.cache.lock().unwrap();
        cache.get_mut(item_key).map(|e| {
            let v = e.item.clone();
            e.last_used = Instant::now();
            v
        })
    }

    pub fn expire(&self) {
        let mut cache = self.cache.lock().unwrap();
        let now = Instant::now();
        while let Some((_, peek)) = cache.peek_lru() {
            if now.duration_since(peek.last_used) >= self.max_unused {
                cache.pop_lru();
            } else {
                break;
            }
        }
    }
}
