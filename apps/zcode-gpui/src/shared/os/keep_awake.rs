//! Prevents system sleep during long-running agent turns.
//! Uses SetThreadExecutionState on Windows (ES_SYSTEM_REQUIRED | ES_AWAYMODE_REQUIRED).

#![allow(dead_code)]

#[cfg(windows)]
unsafe extern "system" {
    fn SetThreadExecutionState(es_flags: u32) -> u32;
}

pub struct KeepAwakeGuard {
    active: bool,
}

impl KeepAwakeGuard {
    pub fn acquire() -> Self {
        #[cfg(windows)]
        unsafe {
            const ES_CONTINUOUS: u32 = 0x8000_0000;
            const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
            const ES_AWAYMODE_REQUIRED: u32 = 0x0000_0040;
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_AWAYMODE_REQUIRED);
        }
        Self { active: true }
    }
}

impl Default for KeepAwakeGuard {
    fn default() -> Self {
        Self::acquire()
    }
}

impl Drop for KeepAwakeGuard {
    fn drop(&mut self) {
        if self.active {
            #[cfg(windows)]
            unsafe {
                const ES_CONTINUOUS: u32 = 0x8000_0000;
                SetThreadExecutionState(ES_CONTINUOUS);
            }
            self.active = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keep_awake_lifecycle() {
        let guard = KeepAwakeGuard::acquire();
        assert!(guard.active);
        drop(guard);
    }
}
