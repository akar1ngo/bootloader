use log::info;
use uefi::boot::{LoadImageSource, image_handle, load_image, open_protocol_exclusive, start_image};
use uefi::prelude::*;
use uefi::proto::loaded_image::LoadedImage;
use uefi::{CStr16, cstr16};

use crate::memory::alloc_pages_and_copy;
use crate::types::Result;

pub(crate) fn load_and_start_kernel(kernel_data: &[u8]) -> Result<()> {
    let kernel_base = alloc_pages_and_copy(kernel_data)?;
    let kernel_len = kernel_data.len();

    // SAFETY: we copied kernel_len bytes into kernel_base
    let buffer = unsafe { core::slice::from_raw_parts(kernel_base.as_ptr(), kernel_len) };

    let source = LoadImageSource::FromBuffer {
        buffer,
        file_path: None,
    };

    let kernel_handle = load_image(image_handle(), source).map_err(|e| {
        info!("Failed to load kernel image: {:?}", e);
        e.status()
    })?;

    setup_kernel_options(kernel_handle)?;

    info!("Starting kernel image");

    start_image(kernel_handle).map_err(|e| {
        info!("Failed to start image: {:?}", e);
        e.status()
    })?;

    Ok(())
}

fn setup_kernel_options(kernel_handle: Handle) -> Result<()> {
    let mut image = open_protocol_exclusive::<LoadedImage>(kernel_handle).map_err(|e| {
        info!("Failed to open LoadedImage protocol: {:?}", e);
        e.status()
    })?;

    info!("Setting kernel load options");

    // TODO: This works because the string will not get dropped. When we start allowing users to
    // specify their own options, we should probably take a reference annotated with lifetimes.
    static KERNEL_OPTS: &CStr16 = cstr16!(
        "init=/nix/store/pg9asbr6hx4515is7akx9ypygg28ama9-nixos-system-nixos-kexec-25.05.20251019.33c6dca/init loglevel=4 efi=debug"
    );

    // SAFETY: `KERNEL_OPTS` has static lifetime.
    unsafe {
        image.set_load_options(KERNEL_OPTS.as_bytes().as_ptr(), KERNEL_OPTS.num_bytes() as u32);
    }

    Ok(())
}
