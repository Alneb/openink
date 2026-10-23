//! Record a real stylus trace on the device (works with the bootloader locked):
//!   adb push pen-dump /data/local/tmp/ && adb shell /data/local/tmp/pen-dump > trace.txt
//! The `adb shell` user is in the `input` group. Output is a text trace that the decoder tests
//! replay: a `#` header with the probed device, then `time_us kind code value` per event.

use openink_pen::{device::PenDevice, PenDecoder};

fn main() {
    let found = PenDevice::find().unwrap_or_else(|e| fail(&format!("cannot list /dev/input: {e}")));
    for s in &found.skipped {
        eprintln!("[pen-dump] skipped {}: {}", s.path.display(), s.error);
    }
    let Some(mut dev) = found.pen else { fail("no device with ABS_X/Y/PRESSURE + a pen tool key (PEN/BRUSH/PENCIL/AIRBRUSH)") };
    println!("# openink-pen trace v1");
    println!("# device {} name {:?}", dev.path.display(), dev.name);
    println!("# x {} {} y {} {} pressure {} {} eraser {} tilt {}",
        dev.x.min, dev.x.max, dev.y.min, dev.y.max, dev.pressure.min, dev.pressure.max, dev.has_eraser, dev.has_tilt);
    eprintln!("[pen-dump] recording {} ({}); Ctrl-C to stop", dev.name, dev.path.display());
    let mut dec = PenDecoder::new(dev.pressure);
    loop {
        let ev = dev.next_event().unwrap_or_else(|e| fail(&format!("read: {e}")));
        println!("{} {} {} {}", ev.time_us, ev.kind, ev.code, ev.value);
        for pe in dev.decode(&mut dec, ev).into_iter().flatten() {
            eprintln!("[pen-dump] {:?} {:?} ({}, {}) p={:.3}", pe.action, pe.tool, pe.x, pe.y, pe.pressure);
        }
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("[pen-dump] FAILED: {msg}");
    std::process::exit(1)
}
