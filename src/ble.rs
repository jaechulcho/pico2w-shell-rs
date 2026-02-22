use defmt::*;
use embassy_futures::join::join;
use embassy_futures::select::select;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use trouble_host::prelude::*;

const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 2;

// Channels for bridging BLE and UART
pub static BLE_RX_CHANNEL: Channel<CriticalSectionRawMutex, heapless::Vec<u8, 64>, 128> =
    Channel::new();
pub static BLE_TX_CHANNEL: Channel<CriticalSectionRawMutex, heapless::Vec<u8, 64>, 128> =
    Channel::new();

#[gatt_server]
pub struct Server {
    pub nus: NordicUartService,
}

#[gatt_service(uuid = "6e400001-b5a3-f393-e0a9-e50e24dcca9e")]
pub struct NordicUartService {
    #[characteristic(
        uuid = "6e400002-b5a3-f393-e0a9-e50e24dcca9e",
        write,
        write_without_response
    )]
    pub rx: heapless::Vec<u8, 64>,

    #[characteristic(uuid = "6e400003-b5a3-f393-e0a9-e50e24dcca9e", notify)]
    pub tx: heapless::Vec<u8, 64>,
}

pub async fn run_ble<C>(controller: C, device_name: &'static str)
where
    C: Controller,
{
    defmt::info!("[run_ble] Started");

    // Fixed address for testing without RNG dependency
    let address = Address {
        kind: AddrKind::RANDOM,
        addr: BdAddr::new([0xff, 0x8f, 0x1a, 0x05, 0xe4, 0xff]),
    };
    info!("BLE Address configured (static random, no RNG)");

    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();

    let stack = trouble_host::new(controller, &mut resources).set_random_address(address);

    let Host {
        mut peripheral,
        runner,
        ..
    } = stack.build();

    info!("Starting advertising and GATT service");

    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: device_name,
        appearance: &appearance::power_device::GENERIC_POWER_DEVICE,
    }))
    .unwrap();

    let _ = join(ble_task(runner), async {
        loop {
            match advertise(device_name, &mut peripheral, &server).await {
                Ok(conn) => {
                    info!("BLE Client connected");
                    let a = gatt_events_task(&server, &conn);
                    let b = tx_notify_task(&server, &conn);
                    select(a, b).await;
                    info!("BLE Client disconnected");
                }
                Err(e) => {
                    error!("[adv] error: {:?}", defmt::Debug2Format(&e));
                }
            }
        }
    })
    .await;
}

async fn ble_task<C: Controller, P: PacketPool>(mut runner: Runner<'_, C, P>) {
    loop {
        if let Err(e) = runner.run().await {
            error!("[ble_task] error: {:?}", defmt::Debug2Format(&e));
            break; // or panic
        }
    }
}

async fn gatt_events_task<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) -> Result<(), Error> {
    let rx_char = &server.nus.rx;
    let sender = BLE_RX_CHANNEL.sender();

    let _reason = loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => break reason,
            GattConnectionEvent::Gatt { event } => {
                if let GattEvent::Write(event) = &event {
                    if event.handle() == rx_char.handle {
                        // Received data on RX characteristic
                        let data = event.data();
                        let mut vec = heapless::Vec::new();
                        let _ = vec.extend_from_slice(data); // Might truncate if > 64 bytes
                        let _ = sender.try_send(vec);
                    }
                }
                match event.accept() {
                    Ok(reply) => {
                        let _ = reply.send().await;
                    }
                    Err(e) => warn!(
                        "[gatt] error sending response: {:?}",
                        defmt::Debug2Format(&e)
                    ),
                };
            }
            _ => {}
        }
    };
    Ok(())
}

async fn tx_notify_task<P: PacketPool>(server: &Server<'_>, conn: &GattConnection<'_, '_, P>) {
    let tx_char = &server.nus.tx;
    let receiver = BLE_TX_CHANNEL.receiver();

    loop {
        let data = receiver.receive().await;
        if tx_char.notify(conn, &data).await.is_err() {
            info!("[tx_notify] error notifying connection");
            break;
        }
    }
}

async fn advertise<'values, 'server, C: Controller>(
    name: &'values str,
    peripheral: &mut Peripheral<'values, C, DefaultPacketPool>,
    server: &'server Server<'values>,
) -> Result<GattConnection<'values, 'server, DefaultPacketPool>, BleHostError<C::Error>> {
    let mut advertiser_data = [0; 31];
    let adv_len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::ServiceUuids128(&[[
                0x9E, 0xCA, 0xDC, 0x24, 0x0E, 0xE5, 0xA9, 0xE0, 0x93, 0xF3, 0xA3, 0xB5, 0x01, 0x00,
                0x40, 0x6E,
            ]]),
        ],
        &mut advertiser_data[..],
    )?;

    let mut scan_data = [0; 31];
    let scan_len = AdStructure::encode_slice(
        &[AdStructure::CompleteLocalName(name.as_bytes())],
        &mut scan_data[..],
    )?;

    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &advertiser_data[..adv_len],
                scan_data: &scan_data[..scan_len],
            },
        )
        .await?;
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    Ok(conn)
}
