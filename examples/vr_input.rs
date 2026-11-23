use std::error::Error;
use std::time::Duration;

use celsius_engine::scripting::ScriptRuntime;
use celsius_engine::vr::{Hand, HeadBobDetector, JointPose, VrCallbacks};

fn main() -> Result<(), Box<dyn Error>> {
    let scripts = ScriptRuntime::new()?;
    let source = std::fs::read_to_string("examples/vr_input.lua")?;
    let gameplay = scripts.load_module("@examples/vr_input.lua", &source)?;
    let callbacks = VrCallbacks::from_module(&gameplay)?;

    let joints = [JointPose {
        name: "index_tip",
        position: [0.2, 1.3, -0.4],
        orientation: [0.0, 0.0, 0.0, 1.0],
    }];
    callbacks.hand_pose(Hand::Right, &joints)?;
    callbacks.hand_gesture("pinch", Hand::Right)?;

    let mut detector = HeadBobDetector::new(1.7).expect("finite calibration height");
    for (height, elapsed_ms) in [(1.7, 0), (1.66, 120), (1.74, 160)] {
        if detector.update(Some(height), Duration::from_millis(elapsed_ms)) {
            callbacks.locomotion_step()?;
        }
    }

    println!(
        "Simulated input triggered {} ability and {} locomotion step",
        gameplay.raw_get::<u32>("abilities_cast")?,
        gameplay.raw_get::<u32>("steps")?,
    );
    Ok(())
}
