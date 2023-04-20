#![feature(thread_id_value)]

fn main() {
    let id: u64 = std::thread::current().id().as_u64().get();
    assert_ne!(id, 0);
}
