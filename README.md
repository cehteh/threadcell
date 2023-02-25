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

`threadcell::Guard` is optional and (only) handle proper cleanup when it becomes
dropped. Unlike MutexGuards they are explicitly constructed.

# Use Cases

 * Single threaded applications that need a static mutable global variable can use
   `ThreadCell<RefCell<T>>`.
 * A `static mut ThreadCell<T>` will need some unsafe code but is actually safe because
   `ThreadCell` guards against concurrent access.
 * Sharing data between threads where synchronizaton is done out of band with other
   syncronization primitives.
