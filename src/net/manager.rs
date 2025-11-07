use core::net::IpAddr;

use log::info;
use uefi::boot::{ScopedProtocol, SearchType, locate_handle_buffer, open_protocol_exclusive};
use uefi::prelude::*;
use uefi::proto::network::pxe::{BaseCode, DhcpV4Packet};

use crate::types::Result;

pub(crate) struct NetworkManager {
    bc: ScopedProtocol<BaseCode>,
}

impl NetworkManager {
    pub fn new() -> Result<Self> {
        let bc = find_pxebc_proto()?;
        Ok(NetworkManager { bc })
    }

    pub fn initialize(&mut self) -> Result<()> {
        start_pxe_if_needed(&mut self.bc)?;
        perform_dhcp(&mut self.bc)?;
        Ok(())
    }

    pub fn get_network_config(&self) -> (IpAddr, IpAddr) {
        get_network_config(&self.bc)
    }

    pub fn base_code(&mut self) -> &mut ScopedProtocol<BaseCode> {
        &mut self.bc
    }
}

fn find_pxebc_proto() -> Result<ScopedProtocol<BaseCode>> {
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

fn start_pxe_if_needed(bc: &mut ScopedProtocol<BaseCode>) -> Result<()> {
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

fn perform_dhcp(bc: &mut ScopedProtocol<BaseCode>) -> Result<()> {
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

fn get_network_config(bc: &ScopedProtocol<BaseCode>) -> (IpAddr, IpAddr) {
    let packet: &DhcpV4Packet = bc.mode().dhcp_ack().as_ref();
    let ip_addr = IpAddr::from(packet.bootp_yi_addr);
    let server_ip = IpAddr::from(packet.bootp_si_addr);
    (ip_addr, server_ip)
}
