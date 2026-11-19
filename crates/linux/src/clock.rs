use std::mem::MaybeUninit;
use std::ptr::null_mut;

pub fn local_clock() -> Option<(u32, u32)> {
    let now = unsafe { libc::time(null_mut()) };
    let mut local = MaybeUninit::<libc::tm>::uninit();
    let converted = unsafe { libc::localtime_r(&now, local.as_mut_ptr()) };
    if converted.is_null() {
        return None;
    }
    let local = unsafe { local.assume_init() };
    Some((
        u32::try_from(local.tm_hour).ok()?,
        u32::try_from(local.tm_min).ok()?,
    ))
}
