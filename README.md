A cell whose value can only be accessed by a owning thread.
Much like a Mutex but without blocking locks and guards.


# Semantics

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


# Use Cases

 * Single threaded applications that need a static mutable global variable can use
   `ThreadCell<RefCell<T>>`.
 * Sharing data between threads where synchronizaton is done out of band with other
   syncronization primitives.

