//! Cache for some often-used objects.
use std::borrow::Borrow;
use std::hash::Hash;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;

use crate::io::Mp4File;
use crate::mp4box::MP4;

/// A cached version of [`Mp4File::open`](crate::io::Mp4File::open) and
/// [`MP4::read`](crate::MP4::read).
pub fn open_mp4(path: impl Into<String>, mmap_all: bool) -> io::Result<Arc<MP4>> {
    static MP4_FILES: Lazy<LruCache<String, Arc<MP4>>> = Lazy::new(|| LruCache::new(Duration::new(60, 0)));
    let path = path.into();
    let mp4 = match MP4_FILES.get(&path) {
        Some(mp4) => mp4,
        None => {
            let mut reader = Mp4File::open(&path, mmap_all)?;
            let mut mp4 = MP4::read(&mut reader)?;
            // TODO?: we probably should only do this for fragmented mp4,
            // not for pseudo-streaming mp4.
            for track in mp4.movie_mut().tracks_mut().iter_mut() {
                track.initial_empty_edit_to_dwell();
            }
            let mp4 = Arc::new(mp4);
            MP4_FILES.put(path, mp4.clone());
            mp4
        },
    };
    MP4_FILES.expire();
    Ok(mp4)
}

struct LruCacheEntry<T> {
    item:      T,
    last_used: Instant,
}

pub(crate) struct LruCache<K, V> {
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

    pub fn remove<Q: ?Sized>(&self, item_key: &Q) -> Option<V>
    where
        lru::KeyRef<K>: Borrow<Q>,
        Q: Hash + Eq,
    {
        let mut cache = self.cache.lock().unwrap();
        cache.pop(item_key).map(|e| e.item)
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
