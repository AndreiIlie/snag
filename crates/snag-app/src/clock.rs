use snag_core::filename::LocalTime;
use std::{
    io,
    mem::MaybeUninit,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Clock<S> {
    pub sleep: S,
    pub now: fn() -> SystemTime,
    pub local_time: fn(SystemTime) -> io::Result<LocalTime>,
}

impl<S> Clock<S> {
    pub fn system(sleep: S) -> Self {
        Self {
            sleep,
            now: SystemTime::now,
            local_time,
        }
    }
}

fn local_time(instant: SystemTime) -> io::Result<LocalTime> {
    let seconds = match instant.duration_since(UNIX_EPOCH) {
        Ok(delta) => i128::from(delta.as_secs()),
        Err(error) => {
            let delta = error.duration();
            -i128::from(delta.as_secs()) - i128::from(delta.subsec_nanos() != 0)
        }
    };
    let overflow = || io::Error::new(io::ErrorKind::InvalidData, "capture time out of range");
    let seconds: libc::time_t = seconds.try_into().map_err(|_| overflow())?;
    let mut out = MaybeUninit::uninit();
    // localtime_r writes a complete tm to our buffer on success.
    let tm = unsafe {
        if libc::localtime_r(&seconds, out.as_mut_ptr()).is_null() {
            return Err(io::Error::last_os_error());
        }
        out.assume_init()
    };
    Ok(LocalTime {
        year: tm.tm_year.checked_add(1900).ok_or_else(overflow)?,
        month: (tm.tm_mon + 1) as u32,
        day: tm.tm_mday as u32,
        hour: tm.tm_hour as u32,
        minute: tm.tm_min as u32,
        second: tm.tm_sec as u32,
    })
}
