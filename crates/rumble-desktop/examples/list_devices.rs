use cpal::traits::{DeviceTrait, HostTrait};
use rumble_client_traits::AudioBackend;

fn main() {
    let host = cpal::default_host();
    println!("host id: {:?}", host.id());
    println!(
        "cpal default input  description = {:?}",
        host.default_input_device()
            .and_then(|d| d.description().ok())
            .map(|d| d.name().to_string())
    );
    println!(
        "cpal default output description = {:?}",
        host.default_output_device()
            .and_then(|d| d.description().ok())
            .map(|d| d.name().to_string())
    );

    let backend = rumble_desktop::DesktopAudioBackend::new();
    println!("\n=== enumerated input devices ===");
    for d in backend.list_input_devices() {
        println!(
            "  id={:<40} name={:?} pipeline={:?} default={}",
            d.id, d.name, d.pipeline, d.is_default
        );
    }
    println!("\n=== enumerated output devices ===");
    for d in backend.list_output_devices() {
        println!(
            "  id={:<40} name={:?} pipeline={:?} default={}",
            d.id, d.name, d.pipeline, d.is_default
        );
    }
}
