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

pub struct LVec<T, S> {
    handle: [u8; 4],
    loaded: RwLock<VecDeque<(usize, T)>>,
    lock: RwLock<()>,
    len: usize,
    buf_size: usize,
    storage: S,
}

impl<T, S> LVec<T, S> {
    pub fn new_with(handle: Handle, buf_size: usize, storage: S) -> Self {
        LVec {
            handle: handle.0.to_be_bytes(),
            loaded: RwLock::new(VecDeque::new()),
            lock: RwLock::new(()),
            len: 0,
            buf_size,
            storage,
        }
    }

    pub fn new(storage: S) -> Self {
        LVec::new_with(new_handle(), 2, storage)
    }
}

impl<T, S> LVec<T, S>
where
    S: Storage + Read + Write,
    T: Serialize,
{
    pub fn len(&self) -> usize {
        self.len
    }

    fn get_key(&self, index: usize) -> Vec<u8> {
        let mut key = self.handle.to_vec();
        key.extend_from_slice(&index.to_be_bytes());
        key
    }

    /// NOTE: Do not hold these references for an extended period of time.
    /// This does NOT spin waiting for a write lock if it misses the cache, but
    /// then it will not cache what it found. I consider this an acceptable failure
    /// mode for the intended use case.
    pub fn get_elem<'de, 's: 'de>(&'s self, index: usize) -> Option<Ref<'s, T>>
    where
        T: Deserialize<'de>,
        S: 'de,
    {
        let loaded = self.loaded.read();
        let mut found = None;
        for (i, (idx, _)) in loaded.iter().enumerate() {
            if *idx == index {
                found = Some(i);
            }
        }
        if let Some(i) = found {
            let guard = self.lock.read();
            return loaded.get(i).map(|(_, e)| unsafe { Ref::new(guard, e) });
        }
        drop(loaded);

        let key = self.get_key(index);
        let reader = FromEmbed::new(self.storage.get::<S>(&key).ok()??);
        let buffer = self.storage.buffer(&key);
        let (elem, _buf) = postcard::from_eio::<T, _>((reader, buffer)).ok()?;
        if let Some(mut loaded) = self.loaded.try_write() {
            loaded.push_back((index, elem));
            if loaded.len() > self.buf_size {
                loaded.pop_front();
            }
        }
        let read_guard = self.loaded.read();
        let back = read_guard.back();
        let guard = self.lock.read();
        back.map(|(_, e)| unsafe { Ref::new(guard, e) })
    }

    pub fn get_elem_mut<'de, 's: 'de>(&'s mut self, index: usize) -> Option<RefMut<'s, T>>
    where
        T: Deserialize<'de>,
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

    fn get_stored_mut<'de, 's: 'de>(&'s mut self, index: usize) -> Option<RefMut<'s, T>>
    where
        T: Deserialize<'de>,
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

    pub fn push(&mut self, elem: T) {
        let index = self.len;
        let key = self.get_key(index);
        let mut writer = FromEmbed::new(self.storage.writer(&key).expect("failed to get writer"));
        postcard::to_eio(&elem, &mut writer).expect("Failed to serialize element");
        self.storage.put::<_>(&key, writer.into_inner()).ok();
        self.len += 1;
        let loaded = self.loaded.get_mut();
        loaded.push_back((index, elem));
    }

    pub fn insert(&mut self, index: usize, elem: T) {
        let key = self.get_key(index);
        let mut writer = FromEmbed::new(self.storage.writer(&key).expect("failed to get writer"));
        postcard::to_eio(&elem, &mut writer).expect("Failed to serialize element");
        self.len += 1;
        let loaded = self.loaded.get_mut();
        loaded.push_back((index, elem));
        self.storage.put::<_>(&key, writer.into_inner()).ok();
    }

    pub fn remove<'de, 's: 'de>(&'s mut self, index: usize) -> Option<T>
    where
        T: Deserialize<'de>,
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
            self.len = self.len.saturating_sub(1);
            loaded.remove(i).map(|(_, e)| e)
        } else {
            let reader = FromEmbed::new(self.storage.get::<S>(&key).ok()??);
            let buffer = self.storage.buffer(&key);
            let (elem, _buf) = postcard::from_eio::<T, _>((reader, buffer)).ok()?;
            self.storage.del(&key).ok();
            self.len = self.len.saturating_sub(1);
            Some(elem)
        }
    }

    pub fn pop<'de, 's: 'de>(&'s mut self) -> Option<T>
    where
        T: Deserialize<'de>,
        S: 'de,
    {
        if self.len == 0 {
            return None;
        }
        let index = self.len - 1;
        self.remove(index)
    }

    pub fn index_ref<'de, 's: 'de, 'o: 'de>(&'s self, index: usize) -> &'o T
    where
        T: Deserialize<'de>,
        S: 'de,
    {
        // SAFETY
        //
        // Yes, I have read https://doc.rust-lang.org/nomicon/transmutes.html
        //
        // I promise this is necessary specifically to launder two lifetimes
        // which I know to be compatible but which the compiler gets confused by.
        // This explicitly specifies the output lifetime, it exceeds the deserialize lifetime
        // but is shorter than the 's self lifetime.
        unsafe {
            let get_ref = self.get_elem(index).expect("index out of bounds");
            let get_ref = get_ref.value();

            core::mem::transmute::<&T, &'o T>(get_ref)
        }
    }

    pub fn index_get_mut<'de, 'o: 'de, 's: 'o>(&'s mut self, index: usize) -> &'o mut T
    where
        T: Deserialize<'de>,
        S: 'de,
    {
        // SAFETY
        //
        // Yes, I have read https://doc.rust-lang.org/nomicon/transmutes.html
        //
        // I promise this is necessary specifically to launder two lifetimes
        // which I know to be compatible but which the compiler gets confused by.
        // This explicitly specifies the output lifetime, it exceeds the deserialize lifetime
        // but is shorter than the 's self lifetime.
        unsafe {
            let mut mut_ref = self.get_elem_mut(index).expect("index out of bounds");
            let mut_ref = mut_ref.value_mut();

            core::mem::transmute::<&mut T, &'o mut T>(mut_ref)
        }
    }
}

impl<T, S> IndexMut<usize> for LVec<T, S>
where
    T: for<'de> Deserialize<'de> + Serialize,
    S: Storage + Read + Write,
{
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.index_get_mut(index)
    }
}

impl<T, S> Index<usize> for LVec<T, S>
where
    T: for<'de> Deserialize<'de> + Serialize,
    S: Storage + Read + Write,
{
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.index_ref(index)
    }
}

pub struct IntoIter<T, S: Storage> {
    storage: S,
    handle: [u8; 4],
    cursor: usize,
    rev_cursor: Option<usize>,
    len: usize,
    _marker: PhantomData<T>,
}

impl<T, S: Storage> IntoIter<T, S> {
    fn get_key(&self, index: usize) -> Vec<u8> {
        let mut key = self.handle.to_vec();
        key.extend_from_slice(&index.to_be_bytes());
        key
    }
}

fn get_stored<'de, T, S>(key: &[u8], storage: &'de S, buffer: &'de mut [u8]) -> Option<T>
where
    T: Deserialize<'de>,
    S: Storage + Read,
{
    let reader = FromEmbed::new(storage.get::<S>(&key).ok()??);
    let (elem, _buf) = postcard::from_eio::<T, _>((reader, buffer)).ok()?;
    Some(elem)
}

impl<T, S> IntoIterator for LVec<T, S>
where
    T: for<'de> Deserialize<'de>,
    S: Storage + Read,
{
    type Item = T;

    type IntoIter = IntoIter<T, S>;

    fn into_iter(self) -> Self::IntoIter {
        let storage = self.storage;
        let cursor = 0;
        let len = self.len;
        IntoIter {
            handle: self.handle,
            storage,
            cursor,
            rev_cursor: Some(len - 1),
            len,
            _marker: PhantomData,
        }
    }
}

impl<T, S> Iterator for IntoIter<T, S>
where
    T: for<'de> Deserialize<'de>,
    S: Storage + Read,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let key = self.get_key(self.cursor);
        self.cursor += 1;
        let elem = get_stored(&key, &self.storage, self.storage.buffer(&key));
        self.storage.del(&key).ok();
        elem
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<T, S> DoubleEndedIterator for IntoIter<T, S>
where
    T: for<'de> Deserialize<'de>,
    S: Storage + Read,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.rev_cursor.is_none() {
            None
        } else {
            let key = self.get_key(self.rev_cursor.unwrap());
            if self.rev_cursor.is_some_and(|c| c == 0) {
                self.rev_cursor = None;
            } else {
                self.rev_cursor = self.rev_cursor.map(|i| i - 1);
            }
            let elem = get_stored(&key, &self.storage, self.storage.buffer(&key));
            self.storage.del(&key).ok();
            elem
        }
    }
}

impl<T, S> Drop for IntoIter<T, S>
where
    S: Storage,
{
    fn drop(&mut self) {
        let rev_cursor = self.rev_cursor.unwrap_or_default();
        for idx in self.cursor..=rev_cursor {
            let key = self.get_key(idx);
            self.storage.del(&key).ok();
        }
    }
}

pub struct Iter<'a, T, S> {
    storage: &'a S,
    handle: [u8; 4],
    cursor: usize,
    rev_cursor: Option<usize>,
    len: usize,
    _marker: PhantomData<&'a T>,
}

impl<'a, T, S> Iter<'a, T, S> {
    pub fn new(storage: &'a S, len: usize, handle: [u8; 4]) -> Self {
        let (cursor, rev_cursor) = (0, Some(len - 1));
        Self {
            handle,
            storage,
            len,
            cursor,
            rev_cursor,
            _marker: PhantomData,
        }
    }
}

impl<T, S: Storage> Iter<'_, T, S> {
    fn get_key(&self, index: usize) -> Vec<u8> {
        let mut key = self.handle.to_vec();
        key.extend_from_slice(&index.to_be_bytes());
        key
    }
}

impl<'a, T, S> Iterator for Iter<'a, T, S>
where
    T: Deserialize<'a>,
    S: Storage + Read,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.len {
            None
        } else {
            let key = self.get_key(self.cursor);
            self.cursor += 1;
            let elem = get_stored(&key, self.storage, self.storage.buffer(&key));
            elem
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'a, T, S> DoubleEndedIterator for Iter<'a, T, S>
where
    T: Deserialize<'a>,
    S: Storage + Read,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.rev_cursor.is_none() {
            None
        } else {
            let key = self.get_key(self.rev_cursor.unwrap());
            if self.rev_cursor.is_some_and(|c| c == 0) {
                self.rev_cursor = None;
            } else {
                self.rev_cursor = self.rev_cursor.map(|i| i - 1);
            }
            let elem = get_stored(&key, self.storage, self.storage.buffer(&key));
            elem
        }
    }
}

pub struct Ref<'a, V> {
    _guard: RwLockReadGuard<'a, ()>,
    v: *const V,
}

unsafe impl<'a, V: Sync> Send for Ref<'a, V> {}
unsafe impl<'a, V: Sync> Sync for Ref<'a, V> {}

impl<'a, V> Ref<'a, V> {
    pub(crate) unsafe fn new(guard: RwLockReadGuard<'a, ()>, v: *const V) -> Self {
        Self { _guard: guard, v }
    }

    pub fn value(&self) -> &V {
        unsafe { &*self.v }
    }
}

impl<'a, V: Debug> Debug for Ref<'a, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ref").field("v", &self.v).finish()
    }
}

impl<'a, V> Deref for Ref<'a, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, V> AsRef<V> for Ref<'a, V> {
    fn as_ref(&self) -> &V {
        self.value()
    }
}

pub struct RefMut<'a, V> {
    #[allow(dead_code)]
    guard: RwLockWriteGuard<'a, ()>,
    v: *mut V,
}

unsafe impl<'a, V: Sync> Send for RefMut<'a, V> {}
unsafe impl<'a, V: Sync> Sync for RefMut<'a, V> {}

impl<'a, V> RefMut<'a, V> {
    pub(crate) unsafe fn new(guard: RwLockWriteGuard<'a, ()>, v: *mut V) -> Self {
        Self { guard, v }
    }

    pub fn value(&self) -> &V {
        unsafe { &*self.v }
    }

    pub fn value_mut(&mut self) -> &mut V {
        unsafe { &mut *self.v }
    }
}

impl<'a, V: Debug> Debug for RefMut<'a, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefMut").field("v", &self.v).finish()
    }
}

impl<'a, V> Deref for RefMut<'a, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<'a, V> DerefMut for RefMut<'a, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

impl<'a, V> AsRef<V> for RefMut<'a, V> {
    fn as_ref(&self) -> &V {
        self.value()
    }
}
