// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests only: counts the bytes allocated on the current thread, so a test can check that a
//! stream goes through a few buffers instead of being held whole (other threads do not count).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static LIVE: Cell<i64> = const { Cell::new(0) };
    static PEAK: Cell<i64> = const { Cell::new(0) };
}

fn add(delta: i64) {
    let _ = LIVE.try_with(|live| {
        let now = live.get() + delta;
        live.set(now);
        let _ = PEAK.try_with(|peak| peak.set(peak.get().max(now)));
    });
}

struct Meter;

// SAFETY: every call goes to the system allocator unchanged; only the sizes are counted.
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            add(layout.size() as i64);
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(layout);
        if !p.is_null() {
            add(layout.size() as i64);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        add(-(layout.size() as i64));
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = System.realloc(ptr, layout, new_size);
        if !p.is_null() {
            add(new_size as i64 - layout.size() as i64);
        }
        p
    }
}

#[global_allocator]
static METER: Meter = Meter;

/// Starts a measurement on this thread; pass the result to [`peak_since`].
pub fn start() -> i64 {
    let now = LIVE.with(Cell::get);
    PEAK.with(|p| p.set(now));
    now
}

/// The most bytes this thread had allocated at once since [`start`], beyond what it had then.
pub fn peak_since(start: i64) -> u64 {
    PEAK.with(|p| (p.get() - start).max(0) as u64)
}
