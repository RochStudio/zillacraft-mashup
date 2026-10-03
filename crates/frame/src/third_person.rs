//! The view F5 cycles, as in Minecraft: first person, then a camera behind the player, then one
//! in front looking back at them. It is IW4's `cg_thirdPerson`, a client setting the camera,
//! the body and the view weapon all read, so it lives in one place for all of them.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThirdPerson {
    Off,
    Behind,
    InFront,
}

static MODE: AtomicU8 = AtomicU8::new(0);

/// The view as it is now.
pub fn third_person() -> ThirdPerson {
    match MODE.load(Ordering::Relaxed) {
        1 => ThirdPerson::Behind,
        2 => ThirdPerson::InFront,
        _ => ThirdPerson::Off,
    }
}

/// F5: on to the next view.
pub fn cycle_third_person() -> ThirdPerson {
    let next = (MODE.load(Ordering::Relaxed) + 1) % 3;
    MODE.store(next, Ordering::Relaxed);
    third_person()
}
