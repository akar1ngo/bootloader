#![no_main]
#![no_std]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::net;
use core::ptr::NonNull;
use core::time::Duration;

use log::info;
use uefi::allocator::Allocator;
use uefi::boot::*;
use uefi::prelude::*;
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::network::pxe::{BaseCode, DhcpV4Packet};
use uefi::{CStr16, Guid, guid};

const LINUX_EFI_INITRD_MEDIA_GUID: Guid = guid!("5568e427-68fc-4f3d-ac74-ca555231cc68");

#[global_allocator]
static GLOBAL_ALLOCATOR: Allocator = Allocator;

type AppResult<T> = Result<T, Status>;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    info!("Hello world!");

    match run() {
        Ok(status) => status,
        Err(status) => error_exit(status),
    }
}

fn run() -> AppResult<Status> {
    let mut bc = find_pxebc_proto()?;
    start_pxe_if_needed(&mut bc)?;

    perform_dhcp(&mut bc)?;
    let (ip_addr, server_ip) = get_network_config(&bc);
    info!("I have IP address: {ip_addr}");

    let kernel_data = download_file(&mut bc, &server_ip, cstr8!("bzImage"), 32 << 20)?;
    let initrd_data = download_file(&mut bc, &server_ip, cstr8!("initrd"), 1024 << 20)?;

    let initrd_base = alloc_pages_and_copy(&initrd_data)?;
    // SAFETY: initrd_base is valid pointer when function succeeds
    unsafe {
        install_initrd_config_table(initrd_base, initrd_data.len())?;
    }

    load_and_start_kernel_from_pages(&kernel_data)?;

    Ok(Status::SUCCESS)
}

fn find_pxebc_proto() -> AppResult<ScopedProtocol<BaseCode>> {
    let handle_buffer = locate_handle_buffer(SearchType::from_proto::<BaseCode>()).map_err(|e| {
        match e.status() {
            Status::NOT_FOUND => info!("No PXE BC handles were found!"),
            _ => info!("Error locating PXE BC handles: {:?}", e),
        }
        e.status()
    })?;

    for &handle in handle_buffer.iter() {
        match open_protocol_exclusive::<BaseCode>(handle) {
            Ok(proto) => return Ok(proto),
            Err(e) => info!("Failed to open PXE Base Code protocol: {:?}", e),
        }
    }

    Err(Status::NOT_FOUND)
}

fn start_pxe_if_needed(bc: &mut ScopedProtocol<BaseCode>) -> AppResult<()> {
    info!("Opened PXE Base Code protocol");
    if !bc.mode().started() {
        // TODO: ipv6 support
        info!("Starting...");
        bc.start(false).map_err(|e| {
            info!("Failed to start PXE: {:?}", e);
            e.status()
        })?;
    }
    Ok(())
}

fn perform_dhcp(bc: &mut ScopedProtocol<BaseCode>) -> AppResult<()> {
    if bc.mode().dhcp_ack_received() {
        info!("DHCP already set up... skipping DHCP process");
        return Ok(());
    }
    info!("Trying DHCP...");
    bc.dhcp(false).map_err(|e| {
        info!("Failed DHCP: {:?}", e);
        e.status()
    })
}

fn get_network_config(bc: &ScopedProtocol<BaseCode>) -> (net::IpAddr, net::IpAddr) {
    let packet: &DhcpV4Packet = bc.mode().dhcp_ack().as_ref();
    let ip_addr = net::IpAddr::from(packet.bootp_yi_addr);
    let server_ip = net::IpAddr::from(packet.bootp_si_addr);
    (ip_addr, server_ip)
}

fn download_file(
    bc: &mut ScopedProtocol<BaseCode>,
    server_ip: &net::IpAddr,
    filename: &uefi::CStr8,
    max_size_bytes: u64,
) -> AppResult<Vec<u8>> {
    let size = bc.tftp_get_file_size(server_ip, filename).map_err(|_| {
        info!("File not found: {filename}");
        Status::NOT_FOUND
    })?;

    info!("{filename} size: {size}");

    if size > max_size_bytes {
        info!("File size too large for {filename}");
        return Err(Status::ABORTED);
    }

    let mut buf = vec![0u8; size as usize];
    bc.tftp_read_file(server_ip, filename, Some(&mut buf[..]))
        .map_err(|e| {
            info!("Failed to read {filename}: {e:?}");
            e.status()
        })?;

    Ok(buf)
}

fn alloc_pages_and_copy(data: &[u8]) -> AppResult<NonNull<u8>> {
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

#[repr(C)]
struct LinuxEfiInitrd {
    base: usize,
    size: usize,
}

unsafe fn install_initrd_config_table(base: NonNull<u8>, size: usize) -> AppResult<()> {
    // Allocate memory for linux_efi_initrd as LOADER_DATA, so that the memory remains valid until
    // the kernel consumes it. For details, see `efi_load_initrd_dev_path` in Linux source code.
    let info_addr = allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|e| {
        info!("Failed to allocate initrd info page: {:?}", e);
        e.status()
    })?;
    let info_ptr = info_addr.as_ptr() as *mut LinuxEfiInitrd;
    let info_dat = LinuxEfiInitrd {
        base: base.as_ptr() as usize,
        size,
    };

    // SAFETY: info_ptr points to freshly allocated memory
    unsafe {
        core::ptr::write(info_ptr, info_dat);
    }

    // SAFETY: info_ptr outlives this program and gets consumed by the kernel
    unsafe {
        install_configuration_table(&LINUX_EFI_INITRD_MEDIA_GUID, info_ptr as *const c_void).map_err(|e| {
            info!("Failed to install initrd config table: {:?}", e);
            e.status()
        })
    }
}

fn load_and_start_kernel_from_pages(kernel_data: &[u8]) -> AppResult<()> {
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

fn setup_kernel_options(kernel_handle: Handle) -> AppResult<()> {
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

fn error_exit(status: Status) -> Status {
    boot::stall(Duration::from_secs(10));
    status
}
