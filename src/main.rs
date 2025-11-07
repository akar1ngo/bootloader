#![no_main]
#![no_std]

extern crate alloc;

mod initrd;
mod kernel;
mod memory;
mod net;
mod rt;
mod types;

use log::info;
use uefi::allocator::Allocator;
use uefi::prelude::*;
use uefi::{cstr8, entry};

use crate::initrd::install_initrd_config_table;
use crate::kernel::load_and_start_kernel;
use crate::memory::alloc_pages_and_copy;
use crate::net::NetworkManager;
use crate::net::tftp::download_file;
use crate::rt::error_exit;
use crate::types::Result;

#[global_allocator]
static GLOBAL_ALLOCATOR: Allocator = Allocator;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    match run() {
        Ok(status) => status,
        Err(status) => error_exit(status),
    }
}

fn run() -> Result<Status> {
    let mut network = NetworkManager::new()?;
    network.initialize()?;

    let (ip_addr, server_ip) = network.get_network_config();
    info!("I have IP address: {ip_addr}");

    let kernel_data = download_file(network.base_code(), &server_ip, cstr8!("bzImage"), 32 << 20)?;
    let initrd_data = download_file(network.base_code(), &server_ip, cstr8!("initrd"), 1024 << 20)?;
    let initrd_base = alloc_pages_and_copy(&initrd_data)?;
    // SAFETY: initrd_base is valid pointer when function succeeds
    unsafe {
        install_initrd_config_table(initrd_base, initrd_data.len())?;
    }

    load_and_start_kernel(&kernel_data)?;

    Ok(Status::SUCCESS)
}
