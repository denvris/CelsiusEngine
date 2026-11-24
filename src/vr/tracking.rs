use std::time::Duration;

use glam::{Quat, Vec3};

use super::{Hand, HeadBobDetector, JointPose, VrCallbacks};

pub const HAND_JOINTS: [&str; 26] = [
    "palm",
    "wrist",
    "thumb_metacarpal",
    "thumb_proximal",
    "thumb_distal",
    "thumb_tip",
    "index_metacarpal",
    "index_proximal",
    "index_intermediate",
    "index_distal",
    "index_tip",
    "middle_metacarpal",
    "middle_proximal",
    "middle_intermediate",
    "middle_distal",
    "middle_tip",
    "ring_metacarpal",
    "ring_proximal",
    "ring_intermediate",
    "ring_distal",
    "ring_tip",
    "little_metacarpal",
    "little_proximal",
    "little_intermediate",
    "little_distal",
    "little_tip",
];
pub const HAND_BONES: [(usize, usize); 25] = [
    (1, 0),
    (1, 2),
    (2, 3),
    (3, 4),
    (4, 5),
    (1, 6),
    (6, 7),
    (7, 8),
    (8, 9),
    (9, 10),
    (1, 11),
    (11, 12),
    (12, 13),
    (13, 14),
    (14, 15),
    (1, 16),
    (16, 17),
    (17, 18),
    (18, 19),
    (19, 20),
    (1, 21),
    (21, 22),
    (22, 23),
    (23, 24),
    (24, 25),
];

#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub position: Vec3,
    pub orientation: Quat,
}

#[derive(Default)]
pub struct HandFrame(pub [Option<Pose>; 26]);

impl HandFrame {
    pub fn is_visible(&self) -> bool {
        self.0[0].is_some() || self.0[1].is_some()
    }

    /// A relaxed hand model anchored to a tracked controller grip. Render-only.
    pub fn from_grip(grip: Pose, left: bool) -> Self {
        let side = if left { -1.0 } else { 1.0 };
        let point = |x, y, z| {
            Some(Pose {
                position: grip.position + grip.orientation * Vec3::new(x, y, z),
                orientation: grip.orientation,
            })
        };
        let mut joints = [None; 26];
        joints[1] = point(0.0, 0.0, 0.045);
        joints[0] = point(0.0, 0.0, 0.0);
        for i in 0..4 {
            let t = i as f32 / 3.0;
            joints[2 + i] = point(
                -side * (0.022 + 0.021 * t),
                0.011 + 0.017 * t,
                0.017 - 0.048 * t,
            );
        }
        for (finger, length) in [0.074, 0.082, 0.075, 0.061].into_iter().enumerate() {
            let x = side * (-0.026 + 0.017 * finger as f32);
            for (j, t) in [0.0_f32, 0.33, 0.60, 0.83, 1.0].into_iter().enumerate() {
                joints[6 + finger * 5 + j] = point(x, -0.042 * t * t, 0.002 - length * t);
            }
        }
        Self(joints)
    }
}

#[derive(Default)]
pub struct BodyFrame {
    pub chest: Option<Vec3>,
    pub hips: Option<Vec3>,
    /// Left and right chains in shoulder, elbow, wrist order.
    pub arms: [[Option<Vec3>; 3]; 2],
    /// Left and right chains in hip, knee, ankle order.
    pub legs: [[Option<Vec3>; 3]; 2],
}

#[derive(Default)]
pub struct TrackingFrame {
    pub head: Option<Pose>,
    pub hands: [HandFrame; 2],
    pub controller_grips: [Option<Pose>; 2],
    pub body: BodyFrame,
}

pub struct Avatar {
    pub chest: Vec3,
    pub hips: Vec3,
    pub orientation: Quat,
    pub arms: [Option<[Vec3; 3]>; 2],
    pub legs: [[Vec3; 3]; 2],
}

#[derive(Default)]
pub struct PlayerTracking {
    pub offset: Vec3,
    speed: f32,
    stride_time: f32,
    stride_remaining: f32,
    heading: Vec3,
    gait: f32,
    detector: Option<HeadBobDetector>,
    pinching: [bool; 2],
}

impl PlayerTracking {
    pub fn reset_reference(&mut self) {
        self.detector = None;
        self.speed = 0.0;
        self.stride_remaining = 0.0;
        self.heading = Vec3::ZERO;
        self.pinching = [false; 2];
    }

    /// Offset stays in world space. Lua receives the original tracking-space joints.
    pub fn tick(
        &mut self,
        frame: &TrackingFrame,
        elapsed: Duration,
        callbacks: &VrCallbacks,
        speed: f32,
        limit: f32,
    ) -> mlua::Result<()> {
        if let Some(head) = frame.head {
            let forward = (head.orientation * Vec3::NEG_Z).with_y(0.0);
            if forward.length_squared() > 0.05 {
                self.heading = forward.normalize();
            }
            if self.detector.is_none() {
                self.detector = HeadBobDetector::new(head.position.y);
            }
            if let Some(detector) = &mut self.detector
                && detector.update(Some(head.position.y), elapsed)
                && let Some(interval) = detector.step_interval_seconds()
            {
                callbacks.locomotion_step()?;
                self.stride_time = interval;
                self.stride_remaining = (interval * 1.16).clamp(0.25, 0.75);
            }
        } else {
            self.detector = None;
            self.speed = 0.0;
            self.stride_remaining = 0.0;
            self.heading = Vec3::ZERO;
        }

        let dt = elapsed.as_secs_f32().min(0.05);
        let cruise = if self.stride_remaining > 0.0 {
            speed * (0.42 / self.stride_time).clamp(0.65, 1.35)
        } else {
            0.0
        };
        self.stride_remaining = (self.stride_remaining - dt).max(0.0);
        self.speed += (cruise - self.speed) * (1.0 - (-12.0 * dt).exp());
        let distance = self.speed * dt;
        self.offset =
            (self.offset + self.heading * distance).clamp(Vec3::splat(-limit), Vec3::splat(limit));
        self.gait += distance * 8.0;

        for (i, hand) in [Hand::Left, Hand::Right].into_iter().enumerate() {
            let joints = &frame.hands[i].0;
            let poses: Vec<_> = joints
                .iter()
                .zip(HAND_JOINTS)
                .filter_map(|(pose, name)| {
                    let pose = pose.as_ref()?;
                    Some(JointPose {
                        name,
                        position: pose.position.to_array(),
                        orientation: pose.orientation.to_array(),
                    })
                })
                .collect();
            callbacks.hand_pose(hand, &poses)?;
            let distance = joints[5]
                .zip(joints[10])
                .map(|(thumb, index)| thumb.position.distance(index.position));
            let was_pinching = self.pinching[i];
            self.pinching[i] =
                distance.is_some_and(|d| d < if was_pinching { 0.032 } else { 0.022 });
            if !was_pinching && self.pinching[i] {
                callbacks.hand_gesture("pinch", hand)?;
            }
        }
        Ok(())
    }

    pub fn avatar(&self, frame: &TrackingFrame, floor: f32) -> Option<Avatar> {
        let head = frame.head?.position;
        let body = &frame.body;
        let body_heading = body.arms[0][0]
            .zip(body.arms[1][0])
            .and_then(|(left, right)| Vec3::Y.cross(right - left).try_normalize());
        let heading = body_heading
            .or_else(|| self.heading.try_normalize())
            .unwrap_or(Vec3::NEG_Z);
        let right = heading.cross(Vec3::Y);
        let orientation = Quat::from_rotation_y((-heading.x).atan2(-heading.z));
        let chest = body.chest.unwrap_or(head - Vec3::Y * 0.27 - heading * 0.10);
        let hips = body
            .hips
            .unwrap_or_else(|| (chest - Vec3::Y * 0.40).with_y((chest.y - 0.40).max(floor + 0.33)));
        let orientation = match (body.chest, body.hips) {
            (Some(chest), Some(hips)) => (chest - hips).try_normalize().map_or(orientation, |up| {
                Quat::from_rotation_arc(Vec3::Y, up) * orientation
            }),
            _ => orientation,
        };
        let arms = [0, 1].map(|i| {
            let side = i as f32 * 2.0 - 1.0;
            let [shoulder, elbow, wrist] = body.arms[i];
            let wrist = frame.hands[i].0[1].map(|pose| pose.position).or(wrist)?;
            let shoulder = shoulder.unwrap_or(chest + right * side * 0.18);
            let elbow = elbow
                .unwrap_or_else(|| bend(shoulder, wrist, right * side * 0.6 - Vec3::Y, 0.32, 0.29));
            Some([shoulder, elbow, wrist].map(|p| p + self.offset))
        });
        let legs = [0, 1].map(|i| {
            if let [Some(hip), Some(knee), Some(ankle)] = body.legs[i] {
                return [hip, knee, ankle].map(|p| p + self.offset);
            }
            let side = i as f32 * 2.0 - 1.0;
            let hip = hips + right * side * 0.11;
            let stride = self.gait.sin() * side * self.speed.min(1.0);
            let foot = (hip + heading * (0.04 + 0.18 * stride))
                .with_y(floor + 0.07 + stride.max(0.0) * 0.09);
            let knee = bend(hip, foot, heading, 0.46, 0.45);
            [hip, knee, foot].map(|p| p + self.offset)
        });

        Some(Avatar {
            chest: chest + self.offset,
            hips: hips + self.offset,
            orientation,
            arms,
            legs,
        })
    }
}

fn bend(start: Vec3, end: Vec3, pole: Vec3, upper: f32, lower: f32) -> Vec3 {
    let delta = end - start;
    let axis = delta.try_normalize().unwrap_or(Vec3::Y);
    let length = delta
        .length()
        .clamp((upper - lower).abs() + 0.001, upper + lower);
    let along = (length * length + upper * upper - lower * lower) / (2.0 * length);
    let lateral = (pole - axis * axis.dot(pole))
        .try_normalize()
        .unwrap_or_else(|| axis.any_orthonormal_vector());
    start + axis * along + lateral * (upper * upper - along * along).max(0.0).sqrt()
}
