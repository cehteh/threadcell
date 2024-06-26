#![doc = include_str!("../README.md")]
#![warn(clippy::cargo_common_metadata)]
#![warn(clippy::doc_markdown)]
#![warn(clippy::missing_panics_doc)]
#![warn(clippy::must_use_candidate)]
#![warn(clippy::semicolon_if_nothing_returned)]
#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]
#![cfg_attr(feature = "nightly_thread_id_value", feature(thread_id_value))]

use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{cmp, fmt, mem};

/// A cell that can be owned by a single thread or none at all.
pub struct ThreadCell<T> {
    data: ManuallyDrop<T>,
    thread_id: AtomicU64,
}

// We use the highest bit of a thread id to indicate that we hold a guard
const GUARD_BIT: u64 = i64::MAX as u64 + 1;

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
            thread_id: AtomicU64::new(current_thread_id()),
        }
    }

    /// Takes the ownership of a cell.
    ///
    /// # Panics
    ///
    /// When the cell is already owned by this thread or it is owned by another thread.
    pub fn acquire(&self) {
        self.thread_id
            .compare_exchange(0, current_thread_id(), Ordering::Acquire, Ordering::Relaxed)
            .expect("Thread can not acquire ThreadCell");
    }

    /// Tries to take the ownership of a cell. Returns true when the ownership could be
    /// obtained or the cell was already owned by the current thread and false when the cell
    /// is owned by another thread.
    pub fn try_acquire(&self) -> bool {
        if self.is_acquired() {
            true
        } else {
            self.thread_id
                .compare_exchange(0, current_thread_id(), Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
        }
    }

    /// Tries to take the ownership of a cell. Returns true when the ownership could be
    /// obtained and false when the cell is already owned or owned by another thread.
    /// Note that this fails when the cell is already owned (unlike `try_acquire()`).
    pub fn try_acquire_once(&self) -> bool {
        self.thread_id
            .compare_exchange(0, current_thread_id(), Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// Takes the ownership of a cell and returns a reference to its value.
    ///
    /// # Panics
    ///
    /// When the cell is owned by another thread.
    pub fn acquire_get(&self) -> &T {
        if !self.is_owned() {
            self.acquire();
        }
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
        if !self.is_owned() {
            self.acquire();
        }
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
        self.thread_id
            .compare_exchange(
                0,
                current_thread_id() | GUARD_BIT,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .expect("Thread can not acquire ThreadCell");
        Guard(self)
    }

    /// Acquires a `ThreadCell` returning a `Option<Guard>` that releases it when becoming
    /// dropped.  Returns `None` when self is owned by another thread.
    #[inline]
    #[mutants::skip]
    pub fn try_acquire_guard(&self) -> Option<Guard<T>> {
        if self
            .thread_id
            .compare_exchange(
                0,
                current_thread_id() | GUARD_BIT,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            Some(Guard(self))
        } else {
            None
        }
    }

    /// Acquires a `ThreadCell` returning a `GuardMut` that releases it when becoming dropped.
    ///
    /// # Panics
    ///
    /// When the cell is owned by another thread.
    #[inline]
    pub fn acquire_guard_mut(&mut self) -> GuardMut<T> {
        self.thread_id
            .compare_exchange(
                0,
                current_thread_id() | GUARD_BIT,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .expect("Thread can not acquire ThreadCell");
        GuardMut(self)
    }

    /// Acquires a `ThreadCell` returning a `Option<GuardMut>` that releases it when becoming
    /// dropped.  Returns `None` when self is owned by another thread.
    #[inline]
    pub fn try_acquire_guard_mut(&mut self) -> Option<GuardMut<T>> {
        if self
            .thread_id
            .compare_exchange(
                0,
                current_thread_id() | GUARD_BIT,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            Some(GuardMut(self))
        } else {
            None
        }
    }

    /// Runs a closure on a `ThreadCell` with acquire/release.
    ///
    /// # Panics
    ///
    /// When the cell is already owned by the current thread or is owned by another thread.
    pub fn with<R, F: FnOnce(&T) -> R>(&self, f: F) -> R {
        f(&*self.acquire_guard())
    }

    /// Runs a closure on a mutable `ThreadCell` with acquire/release.
    ///
    /// # Panics
    ///
    /// When the cell is already owned by the current thread or is owned by another thread.
    pub fn with_mut<R, F: FnOnce(&mut T) -> R>(&mut self, f: F) -> R {
        f(&mut *self.acquire_guard_mut())
    }

    /// Tries to run a closure on a `ThreadCell` with acquire/release.  Returns `Some(Result)`
    /// when the cell could be acquired and None when it is owned by another thread.
    pub fn try_with<R, F: FnOnce(&T) -> R>(&self, f: F) -> Option<R> {
        Some(f(&*self.try_acquire_guard()?))
    }

    /// Tries to run a closure on a mutable `ThreadCell` with acquire/release.  Returns
    /// `Some(Result)` when the cell could be acquired and None when it is owned by another
    /// thread.
    pub fn try_with_mut<R, F: FnOnce(&mut T) -> R>(&mut self, f: F) -> Option<R> {
        Some(f(&mut *self.try_acquire_guard_mut()?))
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
    ///
    /// # Panics
    ///
    /// The `ThreadCell` has a `Guard` on it. `steal()` can only be used with acquire/release
    /// semantics.
    pub unsafe fn steal(&self) -> &Self {
        if !self.is_acquired() {
            assert!(
                self.thread_id.load(Ordering::Acquire) & GUARD_BIT == 0,
                "Can't steal guarded ThreadCell"
            );
            self.thread_id.store(current_thread_id(), Ordering::SeqCst);
        }

        self
    }

    /// Sets a `ThreadCell` which is owned by the current thread into the disowned state.
    ///
    /// # Safety
    ///
    /// The current thread must not use any references it has to the cell after releasing it.
    ///
    /// # Panics
    ///
    /// The current thread does not own the cell.
    pub unsafe fn release(&self) {
        self.thread_id
            .compare_exchange(current_thread_id(), 0, Ordering::Release, Ordering::Relaxed)
            .expect("Thread has no access to ThreadCell");
    }

    /// Unsafe as it doesn't check for ownership.
    #[mutants::skip]
    unsafe fn release_unchecked(&self) {
        debug_assert!(self.is_owned());
        self.thread_id.store(0, Ordering::Release);
    }

    /// Tries to set a `ThreadCell` which is owned by the current thread into the disowned
    /// state. Returns *true* on success and *false* when the current thread does not own the
    /// cell.
    pub fn try_release(&self) -> bool {
        self.thread_id
            .compare_exchange(current_thread_id(), 0, Ordering::Release, Ordering::Relaxed)
            .is_ok()
    }

    /// Returns true when the current thread owns this cell.
    #[inline(always)]
    pub fn is_owned(&self) -> bool {
        // This can be Relaxed because when we already own it (with Acquire), no other thread
        // can change the ownership.  When we do not own it this may return Zero or some other
        // thread id in a racy way, which is ok (to indicate disowned state) either way.
        self.thread_id.load(Ordering::Relaxed) & !GUARD_BIT == current_thread_id()
    }

    /// Returns true when this `ThreadCell` is not owned by any thread. As this can change at
    /// any time by another taking ownership of this `ThreadCell` the result of this function
    /// may be **inexact and racy**. Use this only when only a hint is required or access to the
    /// `ThreadCell` is synchronized by some other means.
    #[inline(always)]
    pub fn is_disowned(&self) -> bool {
        self.thread_id.load(Ordering::Acquire) == 0
    }

    /// Returns true when the current thread owns this cell by acquire.
    #[inline(always)]
    pub fn is_acquired(&self) -> bool {
        // This can be Relaxed because when we already own it (with Acquire), no other thread
        // can change the ownership.  When we do not own it this may return Zero or some other
        // thread id in a racy way, which is ok (to indicate disowned state) either way.
        self.thread_id.load(Ordering::Relaxed) == current_thread_id()
    }

    /// Returns true when the current thread holds a guard on this cell.
    #[inline(always)]
    pub fn is_guarded(&self) -> bool {
        // This can be Relaxed because when we already own it (with Acquire), no other thread
        // can change the ownership.  When we do not own it this may return Zero or some other
        // thread id in a racy way, which is ok (to indicate disowned state) either way.
        self.thread_id.load(Ordering::Relaxed) == current_thread_id() | GUARD_BIT
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
        debug_assert!(self.is_owned(), "Thread has no access to ThreadCell");
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
#[mutants::skip]
impl<T> Drop for ThreadCell<T> {
    // In debug builds we check first for ownership since dropping cells whose types do not
    // need dropping would still be a violation.
    #[cfg(debug_assertions)]
    fn drop(&mut self) {
        let owner = self.thread_id.load(Ordering::Acquire) & !GUARD_BIT;
        if owner == 0 || owner == current_thread_id() {
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
            let owner = self.thread_id.load(Ordering::Acquire) & !GUARD_BIT;
            if owner == 0 || owner == current_thread_id() {
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
#[mutants::skip]
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
#[mutants::skip]
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
#[mutants::skip]
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
#[mutants::skip]
impl<T: fmt::Display> fmt::Display for ThreadCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        fmt::Display::fmt(self.get(), f)
    }
}

#[allow(clippy::doc_markdown)]
#[mutants::skip]
/// Debug information of a `ThreadCell`.
/// Prints "\<ThreadCell\>" when the current thread does not own the cell.
impl<T: fmt::Debug> fmt::Debug for ThreadCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self.try_get() {
            Some(data) => f.debug_struct("ThreadCell").field("data", data).finish(),
            None => f.write_str("<ThreadCell>"),
        }
    }
}

#[cfg(not(feature = "nightly_thread_id_value"))]
use std::num::NonZeroU64;

/// A unique identifier for every thread.
#[cfg(not(feature = "nightly_thread_id_value"))]
struct ThreadId(NonZeroU64);

#[cfg(not(feature = "nightly_thread_id_value"))]
impl ThreadId {
    #[inline]
    #[must_use]
    #[mutants::skip]
    fn current() -> ThreadId {
        thread_local!(static THREAD_ID: NonZeroU64 = {
            static COUNTER: AtomicU64 = AtomicU64::new(1);
            {
                let id = NonZeroU64::new(COUNTER.fetch_add(1, Ordering::Relaxed)).unwrap();
                assert!(id.get() <= i64::MAX as u64, "more than i64::MAX threads");
                id
            }
        });
        THREAD_ID.with(|&x| ThreadId(x))
    }

    #[inline(always)]
    #[must_use]
    #[mutants::skip]
    fn as_u64(&self) -> NonZeroU64 {
        self.0
    }
}

#[test]
#[cfg(not(feature = "nightly_thread_id_value"))]
fn threadid() {
    let main = ThreadId::current().as_u64().get();
    let child = std::thread::spawn(|| ThreadId::current().as_u64().get())
        .join()
        .unwrap();

    // just info, actual values are unspecified
    println!("{main}, {child}");

    assert_ne!(main, 0);
    assert_ne!(main, child);
}

#[cfg(not(feature = "nightly_thread_id_value"))]
#[mutants::skip]
#[inline]
fn current_thread_id() -> u64 {
    ThreadId::current().as_u64().get()
}

#[cfg(feature = "nightly_thread_id_value")]
#[mutants::skip]
#[inline]
fn current_thread_id() -> u64 {
    std::thread::current().id().as_u64().get()
}

/// Guards that a referenced `ThreadCell` becomes properly released when its guard becomes
/// dropped. This covers releasing threadcells on panic.  Guards do not prevent the explicit
/// release of a `ThreadCell`. Deref a `Guard` referencing a released `ThreadCell` will panic!
#[repr(transparent)]
pub struct Guard<'a, T>(&'a ThreadCell<T>);

/// Releases the referenced `ThreadCell` when it is owned by the current thread.
impl<T> Drop for Guard<'_, T> {
    #[mutants::skip]
    fn drop(&mut self) {
        unsafe {
            // SAFETY: a guard is guaranteed to own the cell
            self.0.release_unchecked();
        }
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
/// it becomes dropped.  Guards do not prevent the explicit release of a `ThreadCell`. Deref a
/// `GuardMut` referencing a released `ThreadCell` will panic!
#[repr(transparent)]
pub struct GuardMut<'a, T>(&'a mut ThreadCell<T>);

/// Releases the referenced `ThreadCell` when it is owned by the current thread.
impl<T> Drop for GuardMut<'_, T> {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: a guard is guaranteed to own the cell
            self.0.release_unchecked();
        }
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
