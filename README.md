A cell whose value can only be accessed by a owning thread.  Much like a Mutex but without
blocking locks. Access to ThreadCells is passed cooperatively between threads.


# Semantics

## ThreadCell

A ThreadCell and references therof can always be send to other threads

 * A ThreadCell that is owned by a thread then only that thread can:
   * Access its value
   * Drop the cell.
   * Set the Cell into a disowned state.
 * On a ThreadCell that is disowned any thread can:
   * Take ownership of it
   * Drop it

Threads that do not own a ThreadCell and access its value will panic.  There are 'try_*'
variants in the API that will not panic but return a bool or Option instead.


## Api

There are two variants how Threadcells can be used. From 'v0.11' on these are mutually
exclusive.


### Acquire/Release

Offers manual control over a `ThreadCell` ownership. The disadvantage here is that when a
thread holding a `ThreadCell` will panic, this cell stays owned by the dead thread. One either
needs to discover these cases and then `steal()` the cell or use that only in cases where
panics are impossible or aborting the whole process. This API can be used to implement custom
guard types as well.


### Guard

`threadcell::Guard` and `threadcell::GuardMut` are handle proper acquire/release for
threadcells. There can be only one guard active per threadcell. As long a thread has a `Guard`
the threadcell is owned by that thread and will be released when the `Guard` becomes dropped.

Guards implement `Deref` and `DerefMut` making accessing threadcells more ergonomic.


# Use Cases

 * Single threaded applications that need a static mutable global variable can use
   `ThreadCell<RefCell<T>>`.
 * A `static mut ThreadCell<T>` will needs unsafe code but is actually safe because
   `ThreadCell` guards against concurrent access.
 * Sharing data between threads where synchronizaton is done out of band with other
   syncronization primitives.
