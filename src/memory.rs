use core::ptr::NonNull;

use log::info;
use uefi::boot::{AllocateType, MemoryType, PAGE_SIZE, allocate_pages};

use crate::types::Result;

pub(crate) fn alloc_pages_and_copy(data: &[u8]) -> Result<NonNull<u8>> {
    let pages = data.len().div_ceil(PAGE_SIZE);
    let addr = allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages).map_err(|e| {
        info!("Failed to allocate pages: {:?}", e);
        e.status()
    })?;

    // SAFETY: regions do not overlap and we allocated enough space
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), addr.as_ptr(), data.len());
    }

    Ok(addr)
}
