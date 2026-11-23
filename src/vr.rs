mod locomotion;
#[cfg(all(windows, target_pointer_width = "64"))]
pub mod openxr;
mod script_events;
mod tracking;

pub use locomotion::HeadBobDetector;
pub use script_events::{Hand, JointPose, VrCallbacks};
pub use tracking::{
    Avatar, BodyFrame, HAND_BONES, HAND_JOINTS, HandFrame, PlayerTracking, Pose, TrackingFrame,
};
