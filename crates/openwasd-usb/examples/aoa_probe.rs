#[cfg(target_os = "linux")]
fn main() {
    use openwasd_usb::{Inventory, bridge::Host, connected_devices};
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let Inventory::Available(devices) = connected_devices() else {
        panic!("USB inventory unavailable");
    };
    for device in &devices {
        println!(
            "{} {} {:04x}:{:04x}",
            device.id, device.name, device.vendor_id, device.product_id
        );
    }
    let mut host = Host::default();
    for id in std::env::args().skip(1) {
        let Some(device) = devices.iter().find(|device| device.id == id) else {
            panic!("USB device not found: {id}");
        };
        host.connect(&device.id, &device.name);
    }
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(120) {
        for device in &devices {
            if let Some(status) = host.status(&device.id) {
                println!("{} status: {status}", device.id);
                for event in host.events(&device.id) {
                    println!("{} event: {event}", device.id);
                }
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {}
