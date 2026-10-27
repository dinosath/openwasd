//! Hardware smoke check: two independent Linux virtual gamepads, removed on exit.
#[cfg(target_os = "linux")]
fn main() -> std::io::Result<()> {
    use openwasd_output::{Gamepad, GamepadReport};
    use std::{thread, time::Duration};
    let mut first = Gamepad::create("OpenWASD test gamepad 1")?;
    let mut second = Gamepad::create("OpenWASD test gamepad 2")?;
    println!("Created two independent gamepads for 10 seconds");
    first.send(GamepadReport {
        buttons: 1,
        ..Default::default()
    })?;
    second.send(GamepadReport {
        buttons: 2,
        ..Default::default()
    })?;
    thread::sleep(Duration::from_millis(100));
    first.send(GamepadReport::default())?;
    second.send(GamepadReport::default())?;
    thread::sleep(Duration::from_secs(10));
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Linux uinput is required");
}
