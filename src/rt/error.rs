use core::time::Duration;

use uefi::boot;
use uefi::prelude::*;

pub(crate) fn error_exit(status: Status) -> Status {
    boot::stall(Duration::from_secs(10));
    status
}
