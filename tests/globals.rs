use std::cell::RefCell;
use threadcell::ThreadCell;
static GLOBAL: ThreadCell<RefCell<u64>> = ThreadCell::new_disowned(RefCell::new(345));

#[test]
fn access_global() {
    GLOBAL.acquire();
    assert_eq!(*GLOBAL.get().borrow(), 345);
    *GLOBAL.get().borrow_mut() = 456;
    assert_eq!(*GLOBAL.get().borrow(), 456);
}

static mut MUT_GLOBAL: ThreadCell<u64> = ThreadCell::new_disowned(345);

#[test]
fn access_mut_global() {
    unsafe {
        MUT_GLOBAL.acquire();
        assert_eq!(*MUT_GLOBAL.get(), 345);
        *MUT_GLOBAL.get_mut() = 456;
        assert_eq!(*MUT_GLOBAL.get(), 456);
    }
}

static MUT_GLOBAL2: ThreadCell<RefCell<u64>> = ThreadCell::new_disowned(RefCell::new(123));

#[test]
fn concurrent_mut_global() {
    let thread = std::thread::spawn(|| loop {
        if let Some(guard) = MUT_GLOBAL2.try_acquire_guard() {
            let cell = guard.borrow();
            if *cell == 345 {
                return;
            }
        }
    });

    loop {
        if let Some(guard) = MUT_GLOBAL2.try_acquire_guard() {
            let mut cell = guard.borrow_mut();
            if *cell == 345 {
                break;
            } else {
                *cell = 345;
            }
        }
    }

    thread.join().unwrap();
}
