use alloc::vec;
use alloc::vec::Vec;
use core::net::IpAddr;

use log::info;
use uefi::boot::ScopedProtocol;
use uefi::prelude::*;
use uefi::proto::network::pxe::BaseCode;

use crate::types::Result;

pub(crate) fn download_file(
    bc: &mut ScopedProtocol<BaseCode>,
    server_ip: &IpAddr,
    filename: &uefi::CStr8,
    max_size_bytes: u64,
) -> Result<Vec<u8>> {
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
