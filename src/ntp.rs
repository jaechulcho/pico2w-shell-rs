use crate::logger;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpAddress, Stack};
use embassy_rp::aon_timer::{DateTime, DayOfWeek};
use embassy_time::{Duration, Timer};

#[embassy_executor::task]
pub async fn ntp_sync_task(stack: Stack<'static>) {
    let mut rx_meta = [PacketMetadata::EMPTY; 1];
    let mut rx_payload = [0u8; 512];
    let mut tx_meta = [PacketMetadata::EMPTY; 1];
    let mut tx_payload = [0u8; 512];

    loop {
        // Wait for network to be up
        stack.wait_config_up().await;
        defmt::info!("NTP Task: Network is UP");

        let server_host = if let Some(config) = logger::read_ntp_conf().await {
            config.server
        } else {
            let mut s = heapless::String::<64>::new();
            s.push_str("pool.ntp.org").unwrap();
            s
        };

        defmt::info!("NTP syncing with: {}", server_host.as_str());

        // Resolve DNS
        match stack
            .dns_query(server_host.as_str(), embassy_net::dns::DnsQueryType::A)
            .await
        {
            Ok(addrs) => {
                defmt::info!("NTP Task: Resolved DNS");
                if !addrs.is_empty() {
                    let addr = addrs[0];
                    if let Ok(timestamp) = get_ntp_time(
                        stack,
                        addr,
                        &mut rx_meta[..],
                        &mut rx_payload[..],
                        &mut tx_meta[..],
                        &mut tx_payload[..],
                    )
                    .await
                    {
                        // NTP Epoch is 1900-01-01. Unix Epoch is 1970-01-01.
                        // Difference is 2,208,988,800 seconds.
                        if timestamp > 2208988800 {
                            let mut unix_time = timestamp - 2208988800;

                            // Apply Timezone Offset
                            let mut tz_suffix = "UTC";
                            if let Some(tz) = logger::read_tz_conf().await {
                                let offset_secs = tz.offset_minutes * 60;
                                if offset_secs >= 0 {
                                    unix_time += offset_secs as u32;
                                    tz_suffix = "Local";
                                } else {
                                    unix_time -= (-offset_secs) as u32;
                                    tz_suffix = "Local";
                                }
                            }

                            // Convert Unix timestamp to DateTime (Simplified, UTC/Local)
                            if let Some(dt) = unix_to_datetime(unix_time) {
                                let mut msg = heapless::String::<64>::new();
                                let _ = core::fmt::write(
                                    &mut msg,
                                    format_args!(
                                        "NTP Sync: {:04}-{:02}-{:02} {:02}:{:02}:{:02} {}",
                                        dt.year,
                                        dt.month,
                                        dt.day,
                                        dt.hour,
                                        dt.minute,
                                        dt.second,
                                        tz_suffix
                                    ),
                                );
                                defmt::info!("NTP Task: Applied sync");
                                let _ = logger::write_log(msg.as_str()).await;
                                let _ = logger::set_rtc_time(dt).await;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                defmt::warn!("NTP DNS query failed: {:?}", e);
            }
        }

        // Sync every 1 hour
        Timer::after(Duration::from_secs(3600)).await;
    }
}

async fn get_ntp_time(
    stack: Stack<'static>,
    server: IpAddress,
    rx_meta: &mut [PacketMetadata],
    rx_buf: &mut [u8],
    tx_meta: &mut [PacketMetadata],
    tx_buf: &mut [u8],
) -> Result<u32, ()> {
    let mut socket = UdpSocket::new(stack, rx_meta, rx_buf, tx_meta, tx_buf);
    socket.bind(0).map_err(|_| ())?;

    let mut ntp_packet = [0u8; 48];
    ntp_packet[0] = 0x1B; // LI=0, VN=3, Mode=3 (Client)

    if socket.send_to(&ntp_packet, (server, 123)).await.is_err() {
        defmt::warn!("NTP Task: UDP Send Failed");
        return Err(());
    }
    defmt::info!("NTP Task: Sent UDP Request");

    let mut response = [0u8; 48];
    match embassy_time::with_timeout(Duration::from_secs(5), socket.recv_from(&mut response)).await
    {
        Ok(Ok((n, _remote))) => {
            if n >= 48 {
                // Transmit Timestamp is at offset 40
                let seconds =
                    u32::from_be_bytes([response[40], response[41], response[42], response[43]]);
                Ok(seconds)
            } else {
                Err(())
            }
        }
        _ => Err(()),
    }
}

// Very simplified Unix to DateTime converter (UTC)
fn unix_to_datetime(t: u32) -> Option<DateTime> {
    let seconds = t % 60;
    let minutes = (t / 60) % 60;
    let hours = (t / 3600) % 24;
    let days_since_epoch = t / 86400;

    // 1970 was a common year.
    let mut year = 1970;
    let mut days_remaining = days_since_epoch;

    loop {
        let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if is_leap { 366 } else { 365 };
        if days_remaining < days_in_year {
            break;
        }
        days_remaining -= days_in_year;
        year += 1;
    }

    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = if is_leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for &days in month_days.iter() {
        if days_remaining < days as u32 {
            break;
        }
        days_remaining -= days as u32;
        month += 1;
    }

    let day = days_remaining + 1;
    let day_of_week = match (days_since_epoch + 4) % 7 {
        0 => DayOfWeek::Sunday,
        1 => DayOfWeek::Monday,
        2 => DayOfWeek::Tuesday,
        3 => DayOfWeek::Wednesday,
        4 => DayOfWeek::Thursday,
        5 => DayOfWeek::Friday,
        6 => DayOfWeek::Saturday,
        _ => unreachable!(),
    };

    Some(DateTime {
        year: year as u16,
        month: month as u8,
        day: day as u8,
        day_of_week,
        hour: hours as u8,
        minute: minutes as u8,
        second: seconds as u8,
    })
}
