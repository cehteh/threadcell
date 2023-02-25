#![doc = include_str!("../README.md")]
#![warn(clippy::cargo_common_metadata)]
#![warn(clippy::doc_markdown)]
#![warn(clippy::missing_panics_doc)]
#![warn(clippy::must_use_candidate)]
#![warn(clippy::semicolon_if_nothing_returned)]
#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

use std::mem::ManuallyDrop;
use std::num::NonZeroU64;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{cmp, fmt, mem};

/// A cell that can be owned by a single thread or none at all.
pub struct ThreadCell<T> {
    data: ManuallyDrop<T>,
    thread_id: AtomicU64,
}

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<T: Send> Send for ThreadCell<T> {}
unsafe impl<T: Send> Sync for ThreadCell<T> {}

impl<T> ThreadCell<T> {
    /// Creates a `ThreadCell` that is not owned by any thread. This is a const fn which
    /// allows static construction of `ThreadCells`.
    pub const fn new_disowned(data: T) -> Self {
        Self {
            data: ManuallyDrop::new(data),
            thread_id: AtomicU64::new(0),
        }
    }

    /// Creates a `ThreadCell` that is owned by the current thread.
    pub fn new_owned(data: T) -> Self {
        Self {
            data: ManuallyDrop::new(data),
            thread_id: AtomicU64::new(ThreadId::current().as_u64()),
        }
    }

    /// Takes the ownership of a cell. This is a no-op when the cell is already owned by the
    /// current thread.
    ///
    /// # Panics
    ///
    /// When the cell is owned by another thread.
    pub fn acquire(&self) {
        if !self.is_owned() {
            self.thread_id
                .compare_exchange(
                    0,
                    ThreadId::current().as_u64(),
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .expect("Thread has no access to ThreadCell");
        }
    }

    /// Tries to take the ownership of a cell. Returns true when the ownership could be
    /// obtained or the cell was already owned by the current thread and false when the cell
    /// is owned by another thread.
    pub fn try_acquire(&self) -> bool {
        if self.is_owned() {
            true
        } else {
            self.thread_id
                .compare_exchange(
                    0,
                    ThreadId::current().as_u64(),
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok()
        }
    }

    /// Takes the ownership of a cell and returns a reference to its value.
    ///
    /// # Panics
    ///
    /// When the cell is owned by another thread.
    pub fn acquire_get(&self) -> &T {
        self.acquire();
        // Safety: we have it
        unsafe { self.get_unchecked() }
    }

    /// Tries to take the ownership of a cell and returns a reference to its value.
    /// Will return 'None' when the cell is owned by another thread.
    pub fn try_acquire_get(&self) -> Option<&T> {
        if self.try_acquire() {
            // Safety: we have it
            Some(unsafe { self.get_unchecked() })
        } else {
            None
        }
    }

    /// Takes the ownership of a cell and returns a mutable reference to its value.
    ///
    /// # Panics
    ///
    /// When the cell is owned by another thread.
    pub fn acquire_get_mut(&mut self) -> &mut T {
        self.acquire();
        // Safety: we have it
        unsafe { self.get_mut_unchecked() }
    }

    /// Tries to take the ownership of a cell and returns a mutable reference to its value.
    /// Will return 'None' when the cell is owned by another thread.
    pub fn try_acquire_get_mut(&mut self) -> Option<&mut T> {
        if self.try_acquire() {
            // Safety: we have it
            Some(unsafe { self.get_mut_unchecked() })
        } else {
            None
        }
    }

    /// Acquires a `ThreadCell` returning a `Guard` that releases it when becoming dropped.
    ///
    /// # Panics
    ///
    /// When the cell is owned by another thread.
    #[inline]
    pub fn acquire_guard(&self) -> Guard<T> {
        Guard::acquire(self)
    }

    /// Acquires a `ThreadCell` returning a `Option<Guard>` that releases it when becoming
    /// dropped.  Returns `None` when self is owned by another thread.
    #[inline]
    pub fn try_acquire_guard(&self) -> Option<Guard<T>> {
        Guard::try_acquire(self)
    }

    /// Acquires a `ThreadCell` returning a `GuardMut` that releases it when becoming dropped.
    ///
    /// # Panics
    ///
    /// When the cell is owned by another thread.
    #[inline]
    pub fn acquire_guard_mut(&mut self) -> GuardMut<T> {
        GuardMut::acquire(self)
    }

    /// Acquires a `ThreadCell` returning a `Option<GuardMut>` that releases it when becoming
    /// dropped.  Returns `None` when self is owned by another thread.
    #[inline]
    pub fn try_acquire_guard_mut(&mut self) -> Option<GuardMut<T>> {
        GuardMut::try_acquire(self)
    }

    /// Takes the ownership of a cell unconditionally. This is a no-op when the cell is
    /// already owned by the current thread. Returns 'self' thus it can be chained with
    /// `.release()`.
    ///
    /// # Safety
    ///
    /// This method does not check if the cell is owned by another thread. The owning thread
    /// may operate on the content, thus a data race/UB will happen when the accessed value is
    /// not Sync. The previous owning thread may panic when it expects owning the cell. The
    /// only safe way to use this method is to recover a cell that is owned by a thread that
    /// finished without releasing it (e.g after a panic). Attention should be paid to the
    /// fact that the value protected by the `ThreadCell` might be in a undefined state.
    pub unsafe fn steal(&self) -> &Self {
        if !self.is_owned() {
            self.thread_id
                .store(ThreadId::current().as_u64(), Ordering::SeqCst);
        }
        self
    }

    /// Sets a `ThreadCell` which is owned by the current thread into the disowned state.
    ///
    /// # Panics
    ///
    /// The current thread does not own the cell.
    pub fn release(&self) {
        self.thread_id
            .compare_exchange(
                ThreadId::current().as_u64(),
                0,
                Ordering::Release,
                Ordering::Relaxed,
            )
            .expect("Thread has no access to ThreadCell");
    }

    /// Tries to set a `ThreadCell` which is owned by the current thread into the disowned
    /// state. Returns *true* on success and *false* when the current thread does not own the
    /// cell.
    pub fn try_release(&self) -> bool {
        self.thread_id
            .compare_exchange(
                ThreadId::current().as_u64(),
                0,
                Ordering::Release,
                Ordering::Relaxed,
            )
            .is_ok()
    }

    /// Returns true when the current thread owns this cell.
    #[inline(always)]
    pub fn is_owned(&self) -> bool {
        self.thread_id.load(Ordering::Relaxed) == ThreadId::current().as_u64()
    }

    #[inline]
    #[track_caller]
    fn assert_owned(&self) {
        assert!(self.is_owned(), "Thread has no access to ThreadCell");
    }

    /// Consumes a owned cell and returns its content.
    ///
    /// # Panics
    ///
    /// The current thread does not own the cell.
    #[inline]
    pub fn into_inner(mut self) -> T {
        self.assert_owned();
        unsafe { ManuallyDrop::take(&mut self.data) }
    }

    /// Gets an immutable reference to the cells content.
    ///
    /// # Panics
    ///
    /// The current thread does not own the cell.
    #[inline]
    pub fn get(&self) -> &T {
        self.assert_owned();
        &self.data
    }

    /// Gets a mutable reference to the cells content.
    ///
    /// # Panics
    ///
    /// The current thread does not own the cell.
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        self.assert_owned();
        &mut self.data
    }

    /// Tries to get an immutable reference to the cells content.
    /// Returns 'None' when the thread does not own the cell.
    #[inline]
    pub fn try_get(&self) -> Option<&T> {
        if self.is_owned() {
            Some(&self.data)
        } else {
            None
        }
    }

    /// Tries to get a mutable reference to the cells content.
    /// Returns 'None' when the thread does not own the cell.
    #[inline]
    pub fn try_get_mut(&mut self) -> Option<&mut T> {
        if self.is_owned() {
            Some(&mut self.data)
        } else {
            None
        }
    }

    /// Gets an immutable reference to the cells content without checking for ownership.
    ///
    /// # Safety
    ///
    /// This is always safe when the thread owns the cell, for example after a `acquire()`
    /// call.  When the current thread does not own the cell then it is only safe when T is a
    /// Sync type.
    // PLANNED: When specialization is available: 'fn is_sync<T>() -> bool' and debug_assert!(is_owned() || is_sync::<T>())
    #[inline]
    pub unsafe fn get_unchecked(&self) -> &T {
        &self.data
    }

    /// Gets an mutable reference to the cells content without checking for ownership.
    ///
    /// # Safety
    ///
    /// This is always safe when the thread owns the cell, for example after a `acquire()`
    /// call.  When the current thread does not own the cell then it is only safe when T is a
    /// Sync type.
    // PLANNED: When specialization is available: 'fn is_sync<T>() -> bool' and debug_assert!(is_owned() || is_sync::<T>())
    #[inline]
    pub unsafe fn get_mut_unchecked(&mut self) -> &mut T {
        &mut self.data
    }
}

/// Destroys a `ThreadCell`. The cell must be either owned by the current thread or disowned.
///
/// # Panics
///
/// Another thread owns the cell.
impl<T> Drop for ThreadCell<T> {
    // In debug builds we check first for ownership since dropping cells whose types do not
    // need dropping would be still a violation.
    #[cfg(debug_assertions)]
    fn drop(&mut self) {
        let owner = self.thread_id.load(Ordering::Relaxed);
        if owner == 0 || owner == ThreadId::current().as_u64() {
            if mem::needs_drop::<T>() {
                unsafe { ManuallyDrop::drop(&mut self.data) };
            }
        } else {
            panic!("Thread has no access to ThreadCell");
        }
    }

    // In release builds we can reverse the check to be slightly more efficient. The side
    // effect that dropping cells which one are not allowed to but don't need a destructor
    // either is safe and harmless anyway.
    #[cfg(not(debug_assertions))]
    fn drop(&mut self) {
        if mem::needs_drop::<T>() {
            let owner = self.thread_id.load(Ordering::Relaxed);
            if owner == 0 || owner == ThreadId::current().as_u64() {
                unsafe { ManuallyDrop::drop(&mut self.data) };
            } else {
                panic!("Thread has no access to ThreadCell");
            }
        }
    }
}

/// Creates a new owned `ThreadCell` from the given value.
impl<T> From<T> for ThreadCell<T> {
    #[inline]
    fn from(t: T) -> ThreadCell<T> {
        ThreadCell::new_owned(t)
    }
}

/// Clones a owned `ThreadCell`.
///
/// # Panics
///
/// Another thread owns the cell.
impl<T: Clone> Clone for ThreadCell<T> {
    #[inline]
    fn clone(&self) -> ThreadCell<T> {
        ThreadCell::new_owned(self.get().clone())
    }
}

/// Creates a new owned `ThreadCell` with the default constructed target value.
impl<T: Default> Default for ThreadCell<T> {
    #[inline]
    fn default() -> ThreadCell<T> {
        ThreadCell::new_owned(T::default())
    }
}

/// Check two `ThreadCells` for partial equality.
///
/// # Panics
///
/// Either cell is not owned by the current thread.
impl<T: PartialEq> PartialEq for ThreadCell<T> {
    #[inline]
    fn eq(&self, other: &ThreadCell<T>) -> bool {
        *self.get() == *other.get()
    }
}

impl<T: Eq> Eq for ThreadCell<T> {}

/// Comparison functions between `ThreadCells`.
///
/// # Panics
///
/// Either cell is not owned by the current thread.
impl<T: PartialOrd> PartialOrd for ThreadCell<T> {
    #[inline]
    fn partial_cmp(&self, other: &ThreadCell<T>) -> Option<cmp::Ordering> {
        self.get().partial_cmp(other.get())
    }

    #[inline]
    fn lt(&self, other: &ThreadCell<T>) -> bool {
        *self.get() < *other.get()
    }

    #[inline]
    fn le(&self, other: &ThreadCell<T>) -> bool {
        *self.get() <= *other.get()
    }

    #[inline]
    fn gt(&self, other: &ThreadCell<T>) -> bool {
        *self.get() > *other.get()
    }

    #[inline]
    fn ge(&self, other: &ThreadCell<T>) -> bool {
        *self.get() >= *other.get()
    }
}

/// Compare two `ThreadCells`.
///
/// # Panics
///
/// Either cell is not owned by the current thread.
impl<T: Ord> Ord for ThreadCell<T> {
    #[inline]
    fn cmp(&self, other: &ThreadCell<T>) -> cmp::Ordering {
        self.get().cmp(other.get())
    }
}

/// Formatted output of the value inside a `ThreadCell`.
///
/// # Panics
///
/// The cell is not owned by the current thread.
impl<T: fmt::Display> fmt::Display for ThreadCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        fmt::Display::fmt(self.get(), f)
    }
}

/// Debug information of a `ThreadCell`.
/// Prints "\<invalid thread\>" when the current thread does not own the cell.
impl<T: fmt::Debug> fmt::Debug for ThreadCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self.try_get() {
            Some(data) => f.debug_struct("ThreadCell").field("data", data).finish(),
            None => f.write_str("<invalid thread>"),
        }
    }
}

/// A unique identifier for every thread.
struct ThreadId(NonZeroU64);

// PLANNED: nightly impl (std::thread::ThreadId)
impl ThreadId {
    #[inline]
    #[must_use]
    fn current() -> ThreadId {
        thread_local!(static THREAD_ID: NonZeroU64 = {
            static COUNTER: AtomicU64 = AtomicU64::new(1);
            NonZeroU64::new(COUNTER.fetch_add(1, Ordering::SeqCst)).expect("more than u64::MAX threads")
        });
        THREAD_ID.with(|&x| ThreadId(x))
    }

    #[inline(always)]
    #[must_use]
    fn as_u64(&self) -> u64 {
        self.0.get()
    }
}

/// Guards that a referenced `ThreadCell` becomes properly released when its guard becomes
/// dropped. This should cover most panics that do not end in an `abort()` as well. There are
/// some cases where panics can escape this, for example when one registers custom panic
/// handlers. Guards do not prevent the explicit release of a `ThreadCell`. Deref a `Guard`
/// referencing a released `ThreadCell` will panic!
#[repr(transparent)]
pub struct Guard<'a, T>(&'a ThreadCell<T>);

impl<'a, T> Guard<'a, T> {
    /// Acquires the supplied `ThreadCell` and creates a `Guard` referring to it.
    ///
    /// # Panics
    ///
    /// When the cell is owned by another thread.
    pub fn acquire(tc: &'a ThreadCell<T>) -> Self {
        tc.acquire();
        Self(tc)
    }

    /// Tries to acquire the supplied `ThreadCell` and creates a `Guard` referring to it. Will
    /// return `None` when the acquisition failed.
    pub fn try_acquire(tc: &'a ThreadCell<T>) -> Option<Self> {
        if tc.try_acquire() {
            Some(Self(tc))
        } else {
            None
        }
    }
}

/// Releases the referenced `ThreadCell` when it is owned by the current thread.
impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        self.0.try_release();
    }
}

/// One can deref a `Guard` as long the `ThreadCell` is owned by the current thread this
/// should be the case as long the guarded `ThreadCell` got not explicitly released or stolen.
///
/// # Panics
///
/// When the underlying `ThreadCell` is not owned by the current thread.
impl<T> Deref for Guard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

/// Mutable Guard that ensures that a referenced `ThreadCell` becomes properly released when
/// it becomes dropped. This should cover most panics that do not end in an `abort()` as
/// well. There are some cases where panics can escape this, for example when one registers
/// custom panic handlers. Guards do not prevent the explicit release of a `ThreadCell`. Deref
/// a `GuardMut` referencing a released `ThreadCell` will panic!
#[repr(transparent)]
pub struct GuardMut<'a, T>(&'a mut ThreadCell<T>);

impl<'a, T> GuardMut<'a, T> {
    /// Acquires the supplied `ThreadCell` and creates a `GuardMut` referring to it.
    ///
    /// # Panics
    ///
    /// When the cell is owned by another thread.
    pub fn acquire(tc: &'a mut ThreadCell<T>) -> Self {
        tc.acquire();
        Self(tc)
    }

    /// Tries to acquire the supplied `ThreadCell` and creates a `GuardMut` referring to it. Will
    /// return `None` when the acquisition failed.
    pub fn try_acquire(tc: &'a mut ThreadCell<T>) -> Option<Self> {
        if tc.try_acquire() {
            Some(Self(tc))
        } else {
            None
        }
    }
}

/// Releases the referenced `ThreadCell` when it is owned by the current thread.
impl<T> Drop for GuardMut<'_, T> {
    fn drop(&mut self) {
        self.0.try_release();
    }
}

/// One can deref a `GuardMut` as long the `ThreadCell` is owned by the current thread this
/// should be the case as long the guarded `ThreadCell` got not explicitly released or stolen.
///
/// # Panics
///
/// When the underlying `ThreadCell` is not owned by the current thread.
impl<T> Deref for GuardMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

impl<T> DerefMut for GuardMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.get_mut()
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn smoke() {
        let _owned = ThreadCell::new_owned(123);
        let _disowned = ThreadCell::new_disowned(234);
    }

    #[test]
    fn threadid() {
        let main = ThreadId::current().as_u64();
        let child = std::thread::spawn(|| ThreadId::current().as_u64())
            .join()
            .unwrap();

        // just info, actual values are unspecified
        println!("{main}, {child}");

        assert_ne!(main, child);
    }

    #[test]
    fn guard() {
        static DISOWNED: ThreadCell<i32> = ThreadCell::new_disowned(234);

        std::thread::spawn(|| {
            let guard = Guard::acquire(&DISOWNED);
            assert_eq!(*guard, 234);
        })
        .join()
        .unwrap();
    }

    #[test]
    #[should_panic]
    fn guard_panic() {
        static DISOWNED: ThreadCell<i32> = ThreadCell::new_disowned(234);

        let guard = Guard::acquire(&DISOWNED);
        assert_eq!(*guard, 234);

        std::thread::spawn(|| {
            let _guard = Guard::acquire(&DISOWNED);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn guard_drop() {
        static DISOWNED: ThreadCell<i32> = ThreadCell::new_disowned(234);

        let guard = Guard::acquire(&DISOWNED);
        assert_eq!(*guard, 234);
        drop(guard);

        std::thread::spawn(|| {
            let guard = Guard::acquire(&DISOWNED);
            assert_eq!(*guard, 234);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn guard_mut() {
        static mut DISOWNED: ThreadCell<i32> = ThreadCell::new_disowned(234);

        let mut guard = unsafe { GuardMut::acquire(&mut DISOWNED) };
        assert_eq!(*guard, 234);
        *guard = 345;
        drop(guard);

        std::thread::spawn(|| {
            let guard = unsafe { Guard::acquire(&DISOWNED) };
            assert_eq!(*guard, 345);
        })
        .join()
        .unwrap();
    }
}
