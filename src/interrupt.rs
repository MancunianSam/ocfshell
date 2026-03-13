use signal_hook::consts::SIGINT;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub type Interrupt = Arc<AtomicBool>;

pub fn install_sigint_flag() -> Interrupt {
    let flag: Interrupt = Arc::new(AtomicBool::new(false));
    let flag2 = flag.clone();

    signal_hook::flag::register(SIGINT, flag2).expect("failed to register SIGINT handler");

    flag
}

pub fn clear_interrupt(flag: &Interrupt) {
    flag.store(false, Ordering::SeqCst);
}

pub fn interrupted(flag: &Interrupt) -> bool {
    flag.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn flag_starts_false() {
        let flag = Arc::new(AtomicBool::new(false));
        assert!(!interrupted(&flag));
    }

    #[test]
    fn clear_interrupt_sets_false() {
        let flag = Arc::new(AtomicBool::new(true));
        clear_interrupt(&flag);
        assert!(!interrupted(&flag));
    }

    #[test]
    fn interrupted_reflects_flag() {
        let flag = Arc::new(AtomicBool::new(false));
        assert!(!interrupted(&flag));
        flag.store(true, Ordering::SeqCst);
        assert!(interrupted(&flag));
    }
}
