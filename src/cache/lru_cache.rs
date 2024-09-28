use std::cmp::{Eq, PartialEq};
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::mem;
use std::ops::Drop;
use std::ptr;

struct KeyRef<K> {
    k: *const K,
}

struct LruEntry<K, V> {
    next: *mut LruEntry<K, V>,
    prev: *mut LruEntry<K, V>,
    key: K,
    value: V,
}

/// An LRU Cache.
pub struct LruCache<K, V> {
    map: HashMap<KeyRef<K>, Box<LruEntry<K, V>>>,
    max_size: usize,
    head: *mut LruEntry<K, V>,
}

unsafe impl<K: Send, V: Send> Send for LruCache<K, V> {}
unsafe impl<K: Sync, V: Sync> Sync for LruCache<K, V> {}

impl<K: Hash> Hash for KeyRef<K> {
    fn hash<S: Hasher>(&self, state: &mut S) {
        unsafe { (*self.k).hash(state) }
    }
}

impl<K: PartialEq> PartialEq for KeyRef<K> {
    fn eq(&self, other: &KeyRef<K>) -> bool {
        unsafe { (*self.k).eq(&*other.k) }
    }
}

impl<K: Eq> Eq for KeyRef<K> {}

impl<K, V> LruEntry<K, V> {
    fn new(k: K, v: V) -> LruEntry<K, V> {
        LruEntry {
            key: k,
            value: v,
            next: ptr::null_mut(),
            prev: ptr::null_mut(),
        }
    }
}

impl<K: Hash + Eq, V> LruCache<K, V> {
    /// Create an LRU Cache that holds at most `capacity` items.
    ///
    /// # Example
    ///
    /// ```
    /// use queen_io::cache::lru_cache::LruCache;
    /// let mut cache: LruCache<i32, &str> = LruCache::new(10);
    /// ```
    pub fn new(capacity: usize) -> LruCache<K, V> {
        let head = mem::MaybeUninit::<LruEntry<K, V>>::uninit();
        let cache = LruCache {
            map: HashMap::new(),
            max_size: capacity,
            head: unsafe {
                mem::transmute::<std::boxed::Box<LruEntry<K, V>>, *mut LruEntry<K, V>>(Box::new(
                    head.assume_init(),
                ))
            },
        };
        unsafe {
            (*cache.head).next = cache.head;
            (*cache.head).prev = cache.head;
        }
        cache
    }

    /// Put a key-value pair into cache.
    ///
    /// # Example
    ///
    /// ```
    /// use queen_io::cache::lru_cache::LruCache;
    /// let mut cache = LruCache::new(2);
    ///
    /// cache.put(1, "a");
    /// cache.put(2, "b");
    /// assert_eq!(cache.get(&1), Some(&"a"));
    /// assert_eq!(cache.get(&2), Some(&"b"));
    /// ```
    pub fn put(&mut self, k: K, v: V) {
        let (node_ptr, node_opt) = match self.map.get_mut(&KeyRef { k: &k }) {
            Some(node) => {
                node.value = v;
                let node_ptr: *mut LruEntry<K, V> = &mut **node;
                (node_ptr, None)
            }
            None => {
                let mut node = Box::new(LruEntry::new(k, v));
                let node_ptr: *mut LruEntry<K, V> = &mut *node;
                (node_ptr, Some(node))
            }
        };
        match node_opt {
            None => {
                // Existing node, just update LRU position
                self.detach(node_ptr);
                self.attach(node_ptr);
            }
            Some(node) => {
                let keyref = unsafe { &(*node_ptr).key };

                //self.map.swap(KeyRef{k: keyref}, node);
                self.map.insert(KeyRef { k: keyref }, node);
                self.attach(node_ptr);
                if self.len() > self.capacity() {
                    self.remove_lru();
                }
            }
        }
    }

    /// Return a value corresponding to the key in the cache.
    ///
    /// # Example
    ///
    /// ```
    /// use queen_io::cache::lru_cache::LruCache;
    /// let mut cache = LruCache::new(2);
    ///
    /// cache.put(1, "a");
    /// cache.put(2, "b");
    /// cache.put(2, "c");
    /// cache.put(3, "d");
    ///
    /// assert_eq!(cache.get(&1), None);
    /// assert_eq!(cache.get(&2), Some(&"c"));
    /// ```
    pub fn get<'a>(&'a mut self, k: &K) -> Option<&'a V> {
        let (value, node_ptr_opt) = match self.map.get_mut(&KeyRef { k }) {
            None => (None, None),
            Some(node) => {
                let node_ptr: *mut LruEntry<K, V> = &mut **node;
                (Some(unsafe { &(*node_ptr).value }), Some(node_ptr))
            }
        };
        match node_ptr_opt {
            None => (),
            Some(node_ptr) => {
                self.detach(node_ptr);
                self.attach(node_ptr);
            }
        }
        value
    }

    /// Remove and return a value corresponding to the key from the cache.
    ///
    /// # Example
    ///
    /// ```
    /// use queen_io::cache::lru_cache::LruCache;
    /// let mut cache = LruCache::new(2);
    ///
    /// cache.put(2, "a");
    ///
    /// assert_eq!(cache.pop(&1), None);
    /// assert_eq!(cache.pop(&2), Some("a"));
    /// assert_eq!(cache.pop(&2), None);
    /// assert_eq!(cache.len(), 0);
    /// ```
    pub fn pop(&mut self, k: &K) -> Option<V> {
        self.map
            .remove(&KeyRef { k })
            .map(|lru_entry| lru_entry.value)
    }

    /// Return the maximum number of key-value pairs the cache can hold.
    ///
    /// # Example
    ///
    /// ```
    /// use queen_io::cache::lru_cache::LruCache;
    /// let mut cache: LruCache<i32, &str> = LruCache::new(2);
    /// assert_eq!(cache.capacity(), 2);
    /// ```
    pub fn capacity(&self) -> usize {
        self.max_size
    }

    /// Change the number of key-value pairs the cache can hold. Remove
    /// least-recently-used key-value pairs if necessary.
    ///
    /// # Example
    ///
    /// ```
    /// use queen_io::cache::lru_cache::LruCache;
    /// let mut cache = LruCache::new(2);
    ///
    /// cache.put(1, "a");
    /// cache.put(2, "b");
    /// cache.put(3, "c");
    ///
    /// assert_eq!(cache.get(&1), None);
    /// assert_eq!(cache.get(&2), Some(&"b"));
    /// assert_eq!(cache.get(&3), Some(&"c"));
    ///
    /// cache.change_capacity(3);
    /// cache.put(1, "a");
    /// cache.put(2, "b");
    ///
    /// assert_eq!(cache.get(&1), Some(&"a"));
    /// assert_eq!(cache.get(&2), Some(&"b"));
    /// assert_eq!(cache.get(&3), Some(&"c"));
    ///
    /// cache.change_capacity(1);
    ///
    /// assert_eq!(cache.get(&1), None);
    /// assert_eq!(cache.get(&2), None);
    /// assert_eq!(cache.get(&3), Some(&"c"));
    /// ```
    pub fn change_capacity(&mut self, capacity: usize) {
        for _ in capacity..self.len() {
            self.remove_lru();
        }
        self.max_size = capacity;
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.len() == 0
    }

    #[inline]
    fn remove_lru(&mut self) {
        if !self.is_empty() {
            let lru = unsafe { (*self.head).prev };
            self.detach(lru);
            self.map.remove(&KeyRef {
                k: unsafe { &(*lru).key },
            });
        }
    }

    #[inline]
    fn detach(&mut self, node: *mut LruEntry<K, V>) {
        unsafe {
            (*(*node).prev).next = (*node).next;
            (*(*node).next).prev = (*node).prev;
        }
    }

    #[inline]
    fn attach(&mut self, node: *mut LruEntry<K, V>) {
        unsafe {
            (*node).next = (*self.head).next;
            (*node).prev = self.head;
            (*self.head).next = node;
            (*(*node).next).prev = node;
        }
    }
}

impl<A: fmt::Debug + Hash + Eq, B: fmt::Debug> fmt::Debug for LruCache<A, B> {
    /// Return a string that lists the key-value pairs from most-recently
    /// used to least-recently used.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{")?;
        let mut cur = self.head;
        for i in 0..self.len() {
            if i > 0 {
                write!(f, ", ")?
            }
            unsafe {
                cur = (*cur).next;
                write!(f, "{:?}", (*cur).key)?;
            }
            write!(f, ": ")?;
            unsafe {
                write!(f, "{:?}", (*cur).value)?;
            }
        }
        write!(f, r"}}")
    }
}

impl<A: fmt::Display + Hash + Eq, B: fmt::Display> fmt::Display for LruCache<A, B> {
    /// Return a string that lists the key-value pairs from most-recently
    /// used to least-recently used.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{")?;
        let mut cur = self.head;
        for i in 0..self.len() {
            if i > 0 {
                write!(f, ", ")?
            }
            unsafe {
                cur = (*cur).next;
                write!(f, "{}", (*cur).key)?;
            }
            write!(f, ": ")?;
            unsafe {
                write!(f, "{}", (*cur).value)?;
            }
        }
        write!(f, r"}}")
    }
}

impl<K, V> Drop for LruCache<K, V> {
    fn drop(&mut self) {
        unsafe {
            // Prevent compiler from trying to drop the un-initialized field in the sigil node.
            let node: Box<LruEntry<K, V>> = mem::transmute(self.head);
            let LruEntry {
                key: k, value: v, ..
            } = *node;
            mem::forget(k);
            mem::forget(v);
        }
    }
}
