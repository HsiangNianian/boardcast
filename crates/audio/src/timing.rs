use std::time::Instant;
#[cfg(any(target_os = "macos", test))]
use std::time::Duration;

/// ScreenCaptureKit and CPAL CoreAudio use the Mach host clock, even across processes.
pub(crate) fn host_time(nanos: u64) -> Option<Instant> {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct Timebase { numer: u32, denom: u32 }
        unsafe extern "C" {
            fn mach_absolute_time() -> u64;
            fn mach_timebase_info(info: *mut Timebase) -> i32;
        }
        let mut scale = Timebase { numer: 0, denom: 0 };
        // SAFETY: the kernel writes one initialized C timebase struct; neither call retains pointers.
        if unsafe { mach_timebase_info(&mut scale) } != 0 || scale.denom == 0 { return None; }
        let before = Instant::now();
        let ticks = unsafe { mach_absolute_time() };
        let after = Instant::now();
        let now = u128::from(ticks) * u128::from(scale.numer) / u128::from(scale.denom);
        correlate(nanos, u64::try_from(now).ok()?, before + (after - before) / 2)
    }
    #[cfg(not(target_os = "macos"))]
    { let _ = nanos; None }
}

#[cfg(any(target_os = "macos", test))]
fn correlate(nanos: u64, now: u64, at: Instant) -> Option<Instant> {
    if nanos <= now { at.checked_sub(Duration::from_nanos(now - nanos)) }
    else { at.checked_add(Duration::from_nanos(nanos - now)) }
}

pub(crate) fn microphone(info: &cpal::InputCallbackInfo) -> Option<Instant> {
    let stamp = info.timestamp();
    #[cfg(target_os = "macos")]
    { host_time(u64::try_from(stamp.capture.as_nanos()).ok()?) }
    #[cfg(not(target_os = "macos"))]
    { Instant::now().checked_sub(stamp.callback.checked_duration_since(stamp.capture)?) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_time_keeps_start_offsets_and_ignores_delivery_delay() {
        let now = Instant::now();
        let first = correlate(1_000_000_000, 5_000_000_000, now).unwrap();
        let second = correlate(3_000_000_000, 5_000_000_000, now).unwrap();
        assert_eq!(second - first, Duration::from_secs(2));
        assert_eq!(correlate(3_000_000_000, 8_000_000_000, now + Duration::from_secs(3)), Some(second));
        assert_eq!(correlate(6_000_000_000, 5_000_000_000, now), Some(now + Duration::from_secs(1)));
    }
}
