use core::{
    fmt::{Debug, Formatter},
    marker::PhantomData,
    ops::{Deref, DerefMut, Index, IndexMut},
    slice::SliceIndex,
};
use std::collections::VecDeque;

use maitake_sync::blocking::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use serde::{Deserialize, Serialize};

use crate::{
    Handle,
    io::{FromEmbed, Read, Write},
    new_handle,
    storage::Storage,
};

#[cfg(not(feature = "alloc"))]
pub type VecDeque<T> = heapless::Deque<T, 2>;

pub struct LMap<K, V, S> {
    handle: [u8; 4],
    loaded: RwLock<VecDeque<(K, V)>>,
    lock: RwLock<()>,
    len: usize,
    buf_size: usize,
    storage: S,
}

impl<K, V, S> LMap<K, V, S> {
    pub fn new_with(handle: Handle, buf_size: usize, storage: S) -> Self {
        LMap {
            handle: handle.0.to_be_bytes(),
            loaded: RwLock::new(VecDeque::new()),
            lock: RwLock::new(()),
            len: 0,
            buf_size,
            storage,
        }
    }

    pub fn new(storage: S) -> Self {
        LMap::new_with(new_handle(), 2, storage)
    }
}

impl<K, V, S> LMap<K, V, S>
where
    K: Ord,
    S: Storage + Read + Write,
{
    pub fn len(&self) -> usize {
        self.len
    }

    fn get_key(&self, index: &K) -> Vec<u8>
    where
        K: Serialize,
    {
        let mut key = postcard::to_allocvec(index).expect("Failed to serialize key");
        key.extend_from_slice(&self.handle);
        key
    }

    /// NOTE: Do not hold these references for an extended period of time.
    /// This does NOT spin waiting for a write lock if it misses the cache, but
    /// then it will not cache what it found. I consider this an acceptable failure
    /// mode for the intended use case.
    pub fn get_elem<'de, 's: 'de>(&'s self, key: &K) -> Option<Ref<'s, K, V>>
    where
        K: Serialize + Clone,
        V: Deserialize<'de>,
        S: 'de,
    {
        let loaded = self.loaded.read();
        let mut found = None;
        for (i, (idx, _)) in loaded.iter().enumerate() {
            if idx == key {
                found = Some(i);
            }
        }
        if let Some(i) = found {
            let guard = self.lock.read();
            return loaded.get(i).map(|(k, v)| unsafe { Ref::new(guard, k, v) });
        }
        drop(loaded);

        let key_bytes = self.get_key(key);
        let reader = FromEmbed::new(self.storage.get::<S>(&key_bytes).ok()??);
        let buffer = self.storage.buffer(&key_bytes);
        let (elem, _buf) = postcard::from_eio::<V, _>((reader, buffer)).ok()?;
        if let Some(mut loaded) = self.loaded.try_write() {
            loaded.push_back((key.clone(), elem));
            if loaded.len() > self.buf_size {
                loaded.pop_front();
            }
        }
        let read_guard = self.loaded.read();
        let back = read_guard.back();
        let guard = self.lock.read();
        back.map(|(k, v)| unsafe { Ref::new(guard, k, v) })
    }

    pub fn get_elem_mut<'de, 's: 'de>(&'s mut self, index: usize) -> Option<RefMut<'s, K, V>>
    where
        K: Deserialize<'de>,
        V: Deserialize<'de>,
        S: 'de,
    {
        let loaded = self.loaded.get_mut();
        let mut found = None;
        for (i, (idx, _)) in loaded.iter().enumerate() {
            if *idx == index {
                found = Some(i);
            }
        }
        if let Some(i) = found {
            let guard = self.lock.write();
            loaded
                .get_mut(i)
                .map(|(_, e)| unsafe { RefMut::new(guard, e) })
        } else {
            self.get_stored_mut(index)
        }
    }

    fn get_stored_mut<'de, 's: 'de>(&'s mut self, index: usize) -> Option<RefMut<'s, K, V>>
    where
        K: Deserialize<'de>,
        V: Deserialize<'de>,
        S: 'de,
    {
        let key = self.get_key(index);
        let reader = FromEmbed::new(self.storage.get::<S>(&key).ok()??);
        let buffer = self.storage.buffer(&key);
        let (elem, _buf) = postcard::from_eio::<T, _>((reader, buffer)).ok()?;
        let loaded = self.loaded.get_mut();
        loaded.push_back((index, elem));
        if loaded.len() > self.buf_size {
            loaded.pop_front();
        }
        let guard = self.lock.write();
        let back = loaded.back_mut();
        back.map(|(_, e)| unsafe { RefMut::new(guard, e) })
    }

    // pub fn get<I>(&self, index: I) -> Option<&<I as SliceIndex<[T]>>::Output>
    // where
    //     I: SliceIndex<[T]>,
    // {
    //     unimplemented!()
    // }

    // pub fn get_mut<I>(&mut self, index: I) -> Option<&mut <I as SliceIndex<[T]>>::Output>
    // where
    //     I: SliceIndex<[T]>,
    // {
    //     unimplemented!()
    // }

    pub fn insert(&mut self, index: K, elem: V) {
        let key = self.get_key(index);
        let mut writer = FromEmbed::new(self.storage.writer(&key).expect("failed to get writer"));
        postcard::to_eio(&elem, &mut writer).expect("Failed to serialize element");
        self.len += 1;
        let loaded = self.loaded.get_mut();
        loaded.push_back((index, elem));
        self.storage.put::<_>(&key, writer.into_inner()).ok();
    }

    pub fn remove<'de, 's: 'de>(&'s mut self, index: &K) -> Option<V>
    where
        K: Deserialize<'de>,
        V: Deserialize<'de>,
        S: 'de,
    {
        if index >= self.len {
            return None;
        }
        let mut found = None;
        let mut loaded = self.loaded.write();
        for (i, (idx, _)) in loaded.iter().enumerate() {
            if *idx == index {
                found = Some(i);
            }
        }
        let key = self.get_key(index);
        if let Some(i) = found {
            self.storage.del(&key).ok();
            loaded.remove(i).map(|(_, e)| e)
        } else {
            let reader = FromEmbed::new(self.storage.get::<S>(&key).ok()??);
            let buffer = self.storage.buffer(&key);
            let (elem, _buf) = postcard::from_eio::<T, _>((reader, buffer)).ok()?;
            self.storage.del(&key).ok();
            Some(elem)
        }
    }
}

pub struct Ref<'a, K, V> {
    _guard: RwLockReadGuard<'a, ()>,
    k: *const K,
    v: *const V,
}

unsafe impl<'a, K: Ord + Sync, V: Sync> Send for Ref<'a, K, V> {}
unsafe impl<'a, K: Ord + Sync, V: Sync> Sync for Ref<'a, K, V> {}

impl<'a, K: Ord, V> Ref<'a, K, V> {
    pub(crate) unsafe fn new(guard: RwLockReadGuard<'a, ()>, k: *const K, v: *const V) -> Self {
        Self {
            _guard: guard,
            k,
            v,
        }
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }

    pub fn pair(&self) -> (&K, &V) {
        unsafe { (&*self.k, &*self.v) }
    }

    pub fn map<F, T>(self, f: F) -> MappedRef<'a, K, V, T>
    where
        F: FnOnce(&V) -> &T,
    {
        MappedRef {
            _guard: self._guard,
            k: self.k,
            v: f(unsafe { &*self.v }),
            _marker: PhantomData,
        }
    }

    pub fn try_map<F, T>(self, f: F) -> Result<MappedRef<'a, K, V, T>, Self>
    where
        F: FnOnce(&V) -> Option<&T>,
    {
        if let Some(v) = f(unsafe { &*self.v }) {
            Ok(MappedRef {
                _guard: self._guard,
                k: self.k,
                v,
                _marker: PhantomData,
            })
        } else {
            Err(self)
        }
    }
}

impl<'a, K: Ord + Debug, V: Debug> Debug for Ref<'a, K, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ref")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<'a, K: Ord, V> Deref for Ref<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

pub struct RefMut<'a, K, V> {
    guard: RwLockWriteGuard<'a, ()>,
    k: *const K,
    v: *mut V,
}

unsafe impl<'a, K: Ord + Sync, V: Sync> Send for RefMut<'a, K, V> {}
unsafe impl<'a, K: Ord + Sync, V: Sync> Sync for RefMut<'a, K, V> {}

impl<'a, K: Ord, V> RefMut<'a, K, V> {
    pub(crate) unsafe fn new(guard: RwLockWriteGuard<'a, ()>, k: *const K, v: *mut V) -> Self {
        Self { guard, k, v }
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }

    pub fn value_mut(&mut self) -> &mut V {
        self.pair_mut().1
    }

    pub fn pair(&self) -> (&K, &V) {
        unsafe { (&*self.k, &*self.v) }
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        unsafe { (&*self.k, &mut *self.v) }
    }

    pub fn map<F, T>(self, f: F) -> MappedRefMut<'a, K, V, T>
    where
        F: FnOnce(&mut V) -> &mut T,
    {
        MappedRefMut {
            _guard: self.guard,
            k: self.k,
            v: f(unsafe { &mut *self.v }),
            _marker: PhantomData,
        }
    }

    pub fn try_map<F, T>(self, f: F) -> Result<MappedRefMut<'a, K, V, T>, Self>
    where
        F: FnOnce(&mut V) -> Option<&mut T>,
    {
        let v = match f(unsafe { &mut *(self.v as *mut _) }) {
            Some(v) => v,
            None => return Err(self),
        };
        let guard = self.guard;
        let k = self.k;
        Ok(MappedRefMut {
            _guard: guard,
            k,
            v,
            _marker: PhantomData,
        })
    }
}

impl<'a, K: Ord + Debug, V: Debug> Debug for RefMut<'a, K, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefMut")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<'a, K: Ord, V> Deref for RefMut<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, K: Ord, V> DerefMut for RefMut<'a, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

pub struct MappedRef<'a, K, V, T> {
    _guard: RwLockReadGuard<'a, ()>,
    k: *const K,
    v: *const T,
    _marker: PhantomData<V>,
}

impl<'a, K: Ord, V, T> MappedRef<'a, K, V, T> {
    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &T {
        self.pair().1
    }

    pub fn pair(&self) -> (&K, &T) {
        unsafe { (&*self.k, &*self.v) }
    }

    pub fn map<F, T2>(self, f: F) -> MappedRef<'a, K, V, T2>
    where
        F: FnOnce(&T) -> &T2,
    {
        MappedRef {
            _guard: self._guard,
            k: self.k,
            v: f(unsafe { &*self.v }),
            _marker: PhantomData,
        }
    }

    pub fn try_map<F, T2>(self, f: F) -> Result<MappedRef<'a, K, V, T2>, Self>
    where
        F: FnOnce(&T) -> Option<&T2>,
    {
        let v = match f(unsafe { &*self.v }) {
            Some(v) => v,
            None => return Err(self),
        };
        let guard = self._guard;
        Ok(MappedRef {
            _guard: guard,
            k: self.k,
            v,
            _marker: PhantomData,
        })
    }
}

impl<'a, K: Ord + Debug, V, T: Debug> Debug for MappedRef<'a, K, V, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MappedRef")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<'a, K: Ord, V, T> Deref for MappedRef<'a, K, V, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<'a, K: Ord, V, T: core::fmt::Display> core::fmt::Display for MappedRef<'a, K, V, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self.value(), f)
    }
}

impl<'a, K: Ord, V, T: AsRef<TDeref>, TDeref: ?Sized> AsRef<TDeref> for MappedRef<'a, K, V, T> {
    fn as_ref(&self) -> &TDeref {
        self.value().as_ref()
    }
}

pub struct MappedRefMut<'a, K, V, T> {
    _guard: RwLockWriteGuard<'a, ()>,
    k: *const K,
    v: *mut T,
    _marker: PhantomData<V>,
}

impl<'a, K: Ord, V, T> MappedRefMut<'a, K, V, T> {
    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &T {
        self.pair().1
    }

    pub fn value_mut(&mut self) -> &mut T {
        self.pair_mut().1
    }

    pub fn pair(&self) -> (&K, &T) {
        unsafe { (&*self.k, &*self.v) }
    }

    pub fn pair_mut(&mut self) -> (&K, &mut T) {
        unsafe { (&*self.k, &mut *self.v) }
    }

    pub fn map<F, T2>(self, f: F) -> MappedRefMut<'a, K, V, T2>
    where
        F: FnOnce(&mut T) -> &mut T2,
    {
        MappedRefMut {
            _guard: self._guard,
            k: self.k,
            v: f(unsafe { &mut *self.v }),
            _marker: PhantomData,
        }
    }

    pub fn try_map<F, T2>(self, f: F) -> Result<MappedRefMut<'a, K, V, T2>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut T2>,
    {
        let v = match f(unsafe { &mut *(self.v as *mut _) }) {
            Some(v) => v,
            None => return Err(self),
        };
        let guard = self._guard;
        let k = self.k;
        Ok(MappedRefMut {
            _guard: guard,
            k,
            v,
            _marker: PhantomData,
        })
    }
}

impl<'a, K: Ord + Debug, V, T: Debug> Debug for MappedRefMut<'a, K, V, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MappedRefMut")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<'a, K: Ord, V, T> Deref for MappedRefMut<'a, K, V, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<'a, K: Ord, V, T> DerefMut for MappedRefMut<'a, K, V, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value_mut()
    }
}
