use threadcell::ThreadCell;

#[test]
fn smoke() {
    let _owned = ThreadCell::new_owned(123);
    let _disowned = ThreadCell::new_disowned(234);
}

#[test]
fn access_owned() {
    let owned = ThreadCell::new_owned(123);
    assert_eq!(*owned.get(), 123);
}

#[test]
fn is_acquired() {
    let owned = ThreadCell::new_disowned(123);
    assert!(!owned.is_acquired());
    owned.acquire();
    assert!(owned.is_acquired());
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

#[test]
fn release() {
    let threadcell = ThreadCell::new_owned(());
    threadcell.release();
    assert!(!threadcell.is_owned());
}

#[test]
fn try_with() {
    let threadcell = ThreadCell::new_disowned(234);
    threadcell
        .try_with(|v| assert_eq!(*v, 234))
        .expect("Acquired");
}

#[test]
fn try_with_mut() {
    let mut threadcell = ThreadCell::new_disowned(234);
    threadcell.try_with_mut(|v| *v = 345).expect("Acquired");
    threadcell
        .try_with(|v| assert_eq!(*v, 345))
        .expect("Acquired");
}

#[test]
fn try_get() {
    let threadcell = ThreadCell::new_owned(234);
    assert_eq!(*threadcell.try_get().expect("Acquired"), 234);
}

#[test]
fn try_acquire_get() {
    let threadcell = ThreadCell::new_disowned(234);
    assert_eq!(*threadcell.try_acquire_get().expect("Acquired"), 234);
}

#[test]
fn try_get_mut() {
    let mut threadcell = ThreadCell::new_owned(234);
    *threadcell.try_get_mut().expect("Acquired") = 345;
    assert_eq!(*threadcell.get(), 345);
}

#[test]
fn try_acquire_get_mut() {
    let mut threadcell = ThreadCell::new_disowned(234);
    *threadcell.try_acquire_get_mut().expect("Acquired") = 345;
    assert_eq!(*threadcell.get(), 345);
}

#[test]
fn try_release() {
    let threadcell = ThreadCell::new_owned(234);
    assert!(threadcell.try_release());
    assert!(!threadcell.try_release());
}
