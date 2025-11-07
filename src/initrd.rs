use core::ffi::c_void;
use core::ptr::NonNull;

use log::info;
use uefi::boot::{AllocateType, MemoryType, PAGE_SIZE, allocate_pages, install_configuration_table};
use uefi::{Guid, guid};

use crate::types::Result;

const LINUX_EFI_INITRD_MEDIA_GUID: Guid = guid!("5568e427-68fc-4f3d-ac74-ca555231cc68");

#[repr(C)]
struct LinuxEfiInitrd {
    base: usize,
    size: usize,
}

pub(crate) unsafe fn install_initrd_config_table(base: NonNull<u8>, size: usize) -> Result<()> {
    // Allocate memory for linux_efi_initrd as LOADER_DATA, so that the memory remains valid until
    // the kernel consumes it. For details, see `efi_load_initrd_dev_path` in Linux source code.
    let info_ptr = alloc_pages_for_info().inspect_err(|&e| {
        info!("Failed to allocate initrd info page: {:?}", e);
    })?;

    let info_dat = LinuxEfiInitrd {
        base: base.as_ptr() as usize,
        size,
    };

    // SAFETY: info_ptr points to freshly allocated memory
    unsafe {
        core::ptr::write(info_ptr.as_ptr(), info_dat);
    }

    // SAFETY: info_ptr outlives this program and gets consumed by the kernel
    unsafe {
        install_configuration_table(&LINUX_EFI_INITRD_MEDIA_GUID, info_ptr.as_ptr() as *const c_void).map_err(|e| {
            info!("Failed to install initrd config table: {:?}", e);
            e.status()
        })
    }
}

fn alloc_pages_for_info() -> Result<NonNull<LinuxEfiInitrd>> {
    const NUM_PAGES: usize = core::mem::size_of::<LinuxEfiInitrd>().div_ceil(PAGE_SIZE);

    let addr = allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, NUM_PAGES).map_err(|e| {
        info!("Failed to allocate info page: {:?}", e);
        e.status()
    })?;

    // SAFETY: guaranteed non-null if function succeeds
    let ptr = unsafe { NonNull::new_unchecked(addr.as_ptr() as *mut LinuxEfiInitrd) };

    Ok(ptr)
}
