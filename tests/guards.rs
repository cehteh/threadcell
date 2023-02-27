use threadcell::{Guard, GuardMut, ThreadCell};

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

#[test]
fn try_acquire_guard() {
    let threadcell: ThreadCell<i32> = ThreadCell::default();

    let guard = threadcell.try_acquire_guard().expect("Some(Guard)");
    assert_eq!(*guard, i32::default());
}

#[test]
fn try_acquire_guard_mut() {
    let mut threadcell: ThreadCell<i32> = ThreadCell::default();

    *threadcell.try_acquire_guard_mut().expect("Some(Guard)") = 234;
    assert_eq!(*threadcell.acquire_get(), 234);
}

#[test]
#[should_panic]
fn two_guard_panic() {
    let threadcell: ThreadCell<i32> = ThreadCell::default();

    let _guard1 = threadcell.acquire_guard();
    let _guard2 = threadcell.acquire_guard();
}
