use core::sync::atomic::AtomicU8;

/// Log levels: 0: Error, 1: Warn, 2: Info, 3: Debug, 4: Trace
pub static LOG_LEVEL: AtomicU8 = AtomicU8::new(2); // Default to Info

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        if $crate::log_filter::LOG_LEVEL.load(core::sync::atomic::Ordering::Relaxed) >= 0 {
            defmt::error!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        if $crate::log_filter::LOG_LEVEL.load(core::sync::atomic::Ordering::Relaxed) >= 1 {
            defmt::warn!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        if $crate::log_filter::LOG_LEVEL.load(core::sync::atomic::Ordering::Relaxed) >= 2 {
            defmt::info!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        if $crate::log_filter::LOG_LEVEL.load(core::sync::atomic::Ordering::Relaxed) >= 3 {
            defmt::debug!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        if $crate::log_filter::LOG_LEVEL.load(core::sync::atomic::Ordering::Relaxed) >= 4 {
            defmt::trace!($($arg)*);
        }
    };
}
