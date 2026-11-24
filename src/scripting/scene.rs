use std::time::Duration;

use glam::{Quat, Vec3};
use mlua::{Error, Function, Result, Table};

use crate::graphics::{BoxMesh, Scene, SphereMesh};
use crate::vr::{HAND_BONES, PlayerTracking, TrackingFrame, VrCallbacks};

use super::ScriptRuntime;

/// A Lua gameplay module supplies scene objects and receives the engine's VR events.
pub struct LuaScene {
    module: Table,
    callbacks: VrCallbacks,
    on_frame: Option<Function>,
    tracking: PlayerTracking,
}

impl LuaScene {
    pub fn load(runtime: &ScriptRuntime, name: &str, source: &str) -> Result<Self> {
        let module = runtime.load_module(name, source)?;
        let callbacks = VrCallbacks::from_module(&module)?;
        let on_frame = module.raw_get("on_frame")?;
        Ok(Self {
            module,
            callbacks,
            on_frame,
            tracking: PlayerTracking::default(),
        })
    }

    pub fn module(&self) -> &Table {
        &self.module
    }

    pub fn tick(
        &mut self,
        frame: &TrackingFrame,
        elapsed: Duration,
        reference_changed: bool,
        show_head: bool,
    ) -> Result<(Scene, Vec3)> {
        if reference_changed || elapsed > Duration::from_millis(100) {
            self.tracking.reset_reference();
        }
        let speed = number(&self.module, "step_speed", 1.6, 0.0, 4.0)?;
        let bounds = number(&self.module, "movement_limit", 9.5, 0.0, 100.0)?;
        self.tracking
            .tick(frame, elapsed, &self.callbacks, speed, bounds)?;
        let offset = self.tracking.offset;
        if let Some(callback) = &self.on_frame {
            callback.call::<()>((elapsed.as_secs_f32().min(0.1), offset.to_array()))?;
        }
        let objects: Table = self.module.raw_get("objects")?;
        let mut scene = Scene {
            cloud_density: number(&self.module, "cloud_density", 0.0, 0.0, 2.0)?,
            volumetric_density: number(&self.module, "volumetric_density", 0.0, 0.0, 0.5)?,
            ray_tracing: self
                .module
                .raw_get::<Option<bool>>("ray_tracing")?
                .unwrap_or(false),
            ..Default::default()
        };
        for object in objects.sequence_values::<Table>() {
            let object = object?;
            let center = vector(&object, "position")?;
            let color = vector(&object, "color")?;
            if color.min_element() < 0.0 {
                return Err(Error::runtime("Object color must be nonnegative"));
            }
            let material = number(&object, "material", 0.0, -100.0, 100.0)?;
            if object.raw_get::<Option<String>>("shape")?.as_deref() == Some("sphere") {
                scene.spheres.push(SphereMesh {
                    center,
                    radius: number(&object, "radius", 0.5, 0.001, 100.0)?,
                    color,
                    material,
                });
                continue;
            }
            let size = vector(&object, "size")?;
            if size.min_element() <= 0.0 {
                return Err(Error::runtime("Object size must be positive"));
            }
            let angle = number(&object, "yaw", 0.0, -10.0, 10.0)?;
            scene.boxes.push(BoxMesh {
                center,
                size,
                color,
                orientation: Quat::from_rotation_y(angle),
                material,
            });
        }
        scene.traced_boxes = scene.boxes.len();
        let hands_only = self
            .module
            .raw_get::<Option<bool>>("hands_only")?
            .unwrap_or(false);
        let trace_avatar = scene.ray_tracing
            && self
                .module
                .raw_get::<Option<bool>>("trace_avatar")?
                .unwrap_or(false);
        self.append_avatar(&mut scene, frame, show_head, hands_only, trace_avatar);
        Ok((scene, offset))
    }

    fn append_avatar(
        &self,
        scene: &mut Scene,
        tracking: &TrackingFrame,
        show_head: bool,
        hands_only: bool,
        trace_avatar: bool,
    ) {
        let slate = Vec3::new(0.08, 0.15, 0.24);
        let trim = Vec3::new(0.16, 0.24, 0.38);
        let skins = [Vec3::new(0.02, 0.68, 0.72), Vec3::new(0.98, 0.38, 0.11)];
        let offset = self.tracking.offset;

        for (i, (hand, color)) in tracking.hands.iter().zip(skins).enumerate() {
            let fallback;
            let hand = if !hand.is_visible()
                && let Some(grip) = tracking.controller_grips[i]
            {
                fallback = crate::vr::HandFrame::from_grip(grip, i == 0);
                &fallback
            } else {
                hand
            };
            for &(a, b) in &HAND_BONES {
                if let Some((a, b)) = hand.0[a].zip(hand.0[b]) {
                    let width = if b.position.distance(a.position) > 0.055 {
                        0.015
                    } else {
                        0.011
                    };
                    scene.boxes.extend(BoxMesh::segment(
                        a.position + offset,
                        b.position + offset,
                        width,
                        color,
                    ));
                }
            }
            if let Some(palm) = hand.0[0] {
                if trace_avatar {
                    scene.ray_avatar_boxes.push(scene.boxes.len());
                }
                scene.boxes.push(BoxMesh {
                    center: palm.position + offset,
                    orientation: palm.orientation,
                    size: Vec3::new(0.074, 0.031, 0.076),
                    color,
                    material: 0.0,
                });
            }
        }

        if hands_only {
            return;
        }
        let Some(avatar) = self.tracking.avatar(tracking, 0.0) else {
            return;
        };
        for (center, size, color) in [
            (
                (avatar.chest + avatar.hips) * 0.5,
                Vec3::new(0.33, 0.36, 0.17),
                slate,
            ),
            (avatar.hips, Vec3::new(0.27, 0.14, 0.15), trim),
        ] {
            scene.boxes.push(BoxMesh {
                center,
                size,
                orientation: avatar.orientation,
                color,
                material: 0.0,
            });
        }
        if show_head && let Some(head) = tracking.head {
            let neck = head.position + offset + avatar.orientation * Vec3::new(0.0, -0.17, 0.025);
            scene.boxes.extend(BoxMesh::segment(
                avatar.chest - Vec3::Y * 0.035,
                neck,
                0.085,
                slate,
            ));
            scene.boxes.push(BoxMesh {
                center: head.position + offset - Vec3::Y * 0.045,
                size: Vec3::new(0.2, 0.22, 0.2),
                orientation: head.orientation,
                color: trim,
                material: 0.0,
            });
        }
        for (arm, color) in avatar.arms.iter().zip(skins) {
            if let Some([shoulder, elbow, wrist]) = *arm {
                scene
                    .boxes
                    .extend(BoxMesh::segment(shoulder, elbow, 0.082, trim));
                scene
                    .boxes
                    .extend(BoxMesh::segment(elbow, wrist, 0.062, color * 0.58));
            }
        }
        let leg_start = scene.boxes.len();
        for [hip, knee, foot] in avatar.legs {
            scene
                .boxes
                .extend(BoxMesh::segment(hip, knee, 0.108, slate));
            scene
                .boxes
                .extend(BoxMesh::segment(knee, foot, 0.078, trim));
            scene.boxes.push(BoxMesh {
                center: foot + avatar.orientation * Vec3::new(0.0, -0.03, -0.045),
                size: Vec3::new(0.115, 0.08, 0.22),
                orientation: avatar.orientation,
                color: slate,
                material: 0.0,
            });
        }
        if trace_avatar {
            scene.ray_avatar_boxes.extend(leg_start..scene.boxes.len());
        }
    }
}

fn vector(table: &Table, name: &str) -> Result<Vec3> {
    let value = Vec3::from_array(table.raw_get::<[f32; 3]>(name)?);
    if !value.is_finite() {
        return Err(Error::runtime(format!(
            "{name} must contain finite numbers"
        )));
    }
    Ok(value)
}

fn number(table: &Table, name: &str, default: f32, min: f32, max: f32) -> Result<f32> {
    let value = table.raw_get::<Option<f32>>(name)?.unwrap_or(default);
    if !(min..=max).contains(&value) {
        return Err(Error::runtime(format!(
            "{name} must be between {min} and {max}"
        )));
    }
    Ok(value)
}
