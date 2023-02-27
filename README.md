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


## Guard

`threadcell::Guard` and `threadcell::GuardMut` are optional and handle proper acquire/release
for thradcells. There can be only one guard active per threadcell.

Guards implement `Deref` and `DerefMut` making accessing threadcells more ergonomic.

A side effect of being optional is that becomes possible to explicitly release a `ThreadCell`
while it still has a active guard. Dereferencing such a Guard will panic then. Thus care
should be taken than threadcells are either managed by guards or manually managed.


# Use Cases

 * Single threaded applications that need a static mutable global variable can use
   `ThreadCell<RefCell<T>>`.
 * A `static mut ThreadCell<T>` will needs unsafe code but is actually safe because
   `ThreadCell` guards against concurrent access.
 * Sharing data between threads where synchronizaton is done out of band with other
   syncronization primitives.
