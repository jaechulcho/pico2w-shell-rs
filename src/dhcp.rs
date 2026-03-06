#![allow(dead_code)]

use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{Ipv4Address, Stack};
use embassy_time::Duration;

#[embassy_executor::task]
pub async fn dhcp_server_task(stack: Stack<'static>, server_ip: Ipv4Address) {
    let mut rx_buffer = [0; 512];
    let mut tx_buffer = [0; 512];
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];

    loop {
        let mut socket = UdpSocket::new(
            stack,
            &mut rx_meta,
            &mut rx_buffer,
            &mut tx_meta,
            &mut tx_buffer,
        );

        if let Err(e) = socket.bind(67) {
            defmt::error!("DHCP bind error: {:?}", e);
            embassy_time::Timer::after(Duration::from_secs(1)).await;
            continue;
        }

        defmt::info!("DHCP Server listening on UDP:67");

        loop {
            let mut buf = [0; 512];
            match socket.recv_from(&mut buf).await {
                Ok((n, _endpoint)) => {
                    if n < 240 {
                        continue; // Packet too small for DHCP
                    }

                    let op = buf[0];
                    if op != 1 {
                        continue; // Not a BOOTREQUEST
                    }

                    // Extract Mac Address
                    let mut chaddr = [0u8; 16];
                    chaddr.copy_from_slice(&buf[28..44]);

                    // Extract Transaction ID
                    let mut xid = [0u8; 4];
                    xid.copy_from_slice(&buf[4..8]);

                    // Extract DHCP Message Type First
                    let mut is_discover = false;
                    let mut is_request = false;
                    let mut i = 240;
                    while i < n {
                        let opt = buf[i];
                        if opt == 255 {
                            break;
                        } else if opt == 0 {
                            i += 1;
                            continue;
                        }

                        let len = buf[i + 1] as usize;
                        if opt == 53 && len == 1 {
                            let msg_type = buf[i + 2];
                            if msg_type == 1 {
                                is_discover = true;
                            } else if msg_type == 3 {
                                is_request = true;
                            }
                        }
                        i += 2 + len;
                    }

                    if !is_discover && !is_request {
                        continue;
                    }

                    let client_ip = Ipv4Address::new(192, 168, 4, 2);

                    // Build DHCP Reply
                    let mut reply = [0u8; 300];
                    reply[0] = 2; // BOOTREPLY
                    reply[1] = 1; // HTYPE ethernet
                    reply[2] = 6; // HLEN 6 bytes
                    reply[3] = 0; // HOPS
                    reply[4..8].copy_from_slice(&xid); // XID
                    reply[8..10].copy_from_slice(&[0, 0]); // SECS
                    reply[10..12].copy_from_slice(&[0, 0]); // FLAGS
                    reply[12..16].copy_from_slice(&[0, 0, 0, 0]); // CIADDR
                    reply[16..20].copy_from_slice(&client_ip.octets()); // YIADDR
                    reply[20..24].copy_from_slice(&server_ip.octets()); // SIADDR (Next server)
                    reply[24..28].copy_from_slice(&[0, 0, 0, 0]); // GIADDR
                    reply[28..44].copy_from_slice(&chaddr); // CHADDR

                    // Magic Cookie
                    reply[236..240].copy_from_slice(&[99, 130, 83, 99]);

                    let mut r_idx = 240;

                    // Message Type Option
                    reply[r_idx] = 53;
                    reply[r_idx + 1] = 1;
                    reply[r_idx + 2] = if is_discover { 2 } else { 5 }; // OFFER or ACK
                    r_idx += 3;

                    // Server Identifier
                    reply[r_idx] = 54;
                    reply[r_idx + 1] = 4;
                    reply[r_idx + 2..r_idx + 6].copy_from_slice(&server_ip.octets());
                    r_idx += 6;

                    // IP Address Lease Time
                    reply[r_idx] = 51;
                    reply[r_idx + 1] = 4;
                    reply[r_idx + 2..r_idx + 6].copy_from_slice(&[0, 0, 14, 16]); // 3600 seconds
                    r_idx += 6;

                    // Subnet Mask
                    reply[r_idx] = 1;
                    reply[r_idx + 1] = 4;
                    reply[r_idx + 2..r_idx + 6].copy_from_slice(&[255, 255, 255, 0]);
                    r_idx += 6;

                    // Router
                    reply[r_idx] = 3;
                    reply[r_idx + 1] = 4;
                    reply[r_idx + 2..r_idx + 6].copy_from_slice(&server_ip.octets());
                    r_idx += 6;

                    // DNS Server
                    reply[r_idx] = 6;
                    reply[r_idx + 1] = 4;
                    reply[r_idx + 2..r_idx + 6].copy_from_slice(&server_ip.octets());
                    r_idx += 6;

                    // End Option
                    reply[r_idx] = 255;
                    r_idx += 1;

                    // Send back to client (Broadcast UDP 68)
                    let broadcast_endpoint = embassy_net::IpEndpoint::new(
                        embassy_net::IpAddress::Ipv4(Ipv4Address::new(255, 255, 255, 255)),
                        68,
                    );

                    if let Err(e) = socket.send_to(&reply[..r_idx], broadcast_endpoint).await {
                        defmt::warn!("DHCP send_to error: {:?}", e);
                    } else {
                        defmt::info!(
                            "DHCP Sent {} to {:?}",
                            if is_discover { "OFFER" } else { "ACK" },
                            client_ip
                        );
                    }
                }
                Err(e) => {
                    defmt::warn!("DHCP receive error: {:?}", e);
                    break;
                }
            }
        }
    }
}
