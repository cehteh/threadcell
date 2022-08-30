use threadcell::ThreadCell;

#[test]
fn access_owned() {
    let owned = ThreadCell::new_owned(123);
    assert_eq!(*owned.get(), 123);
}

#[test]
#[should_panic]
fn access_disowned() {
    let disowned = ThreadCell::new_disowned(234);
    let _fail = disowned.get();
}

#[test]
fn mutate_owned() {
    let mut owned = ThreadCell::new_owned(123);
    *owned.get_mut() = 234;
    assert_eq!(*owned.get(), 234);
}

use std::cell::RefCell;
static GLOBAL: ThreadCell<RefCell<u64>> = ThreadCell::new_disowned(RefCell::new(345));

#[test]
fn access_global() {
    GLOBAL.acquire();
    assert_eq!(*GLOBAL.get().borrow(), 345);
}
