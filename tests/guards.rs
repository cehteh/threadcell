use threadcell::ThreadCell;

#[test]
fn guard() {
    static DISOWNED: ThreadCell<i32> = ThreadCell::new_disowned(234);

    std::thread::spawn(|| {
        let guard = DISOWNED.acquire_guard();
        assert_eq!(*guard, 234);
    })
    .join()
    .unwrap();
}

#[test]
#[should_panic]
fn guard_panic() {
    static DISOWNED: ThreadCell<i32> = ThreadCell::new_disowned(234);

    let guard = DISOWNED.acquire_guard();
    assert_eq!(*guard, 234);

    std::thread::spawn(|| {
        let _guard = DISOWNED.acquire_guard();
    })
    .join()
    .unwrap();
}

#[test]
fn guard_drop() {
    static DISOWNED: ThreadCell<i32> = ThreadCell::new_disowned(234);

    let guard = DISOWNED.acquire_guard();
    assert_eq!(*guard, 234);
    drop(guard);

    std::thread::spawn(|| {
        let guard = DISOWNED.acquire_guard();
        assert_eq!(*guard, 234);
    })
    .join()
    .unwrap();
}

#[test]
fn guard_mut() {
    static mut DISOWNED: ThreadCell<i32> = ThreadCell::new_disowned(234);

    let mut guard = unsafe { DISOWNED.acquire_guard_mut() };
    assert_eq!(*guard, 234);
    *guard = 345;
    drop(guard);

    std::thread::spawn(|| {
        let guard = unsafe { DISOWNED.acquire_guard() };
        assert_eq!(*guard, 345);
    })
    .join()
    .unwrap();
}

#[test]
fn try_acquire_guard() {
    let threadcell: ThreadCell<i32> = ThreadCell::new_disowned(0);

    let guard = threadcell.try_acquire_guard().expect("Some(Guard)");
    assert_eq!(*guard, i32::default());
}

#[test]
fn try_acquire_guard_mut() {
    let mut threadcell: ThreadCell<i32> = ThreadCell::new_disowned(0);

    *threadcell.try_acquire_guard_mut().expect("Some(Guard)") = 234;
    assert_eq!(*threadcell.acquire_get(), 234);
}

#[test]
#[should_panic]
fn two_guard_panic() {
    let threadcell: ThreadCell<i32> = ThreadCell::new_disowned(0);

    let _guard1 = threadcell.acquire_guard();
    let _guard2 = threadcell.acquire_guard();
}

#[test]
fn no_guard_after_acquire() {
    let threadcell: ThreadCell<i32> = ThreadCell::new_disowned(0);

    threadcell.try_acquire();
    assert!(threadcell.is_acquired());
    assert!(!threadcell.is_guarded());
    assert!(threadcell.try_acquire_guard().is_none());
}

#[test]
fn no_acquire_after_guard() {
    let threadcell: ThreadCell<i32> = ThreadCell::new_disowned(0);

    let _guard = threadcell.try_acquire_guard().expect("Some(Guard)");
    assert!(threadcell.is_guarded());
    assert!(!threadcell.try_acquire());
}

#[test]
#[should_panic]
fn cant_release_guarded() {
    let threadcell: ThreadCell<i32> = ThreadCell::new_disowned(0);

    let _guard = threadcell.try_acquire_guard().expect("Some(Guard)");
    assert!(threadcell.is_guarded());
    threadcell.release();
}
