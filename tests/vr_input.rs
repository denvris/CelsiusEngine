use std::time::Duration;

use celsius_engine::graphics::Camera;
use celsius_engine::scripting::{LuaScene, ScriptRuntime};
use celsius_engine::vr::{
    Hand, HeadBobDetector, JointPose, PlayerTracking, Pose, TrackingFrame, VrCallbacks,
};
use glam::{Quat, Vec3};

#[test]
fn step_requires_a_fresh_down_and_up_cycle() {
    let mut detector = HeadBobDetector::new(1.7).unwrap();
    let tick = Duration::from_millis(140);

    assert!(!detector.update(Some(1.7), tick));
    assert!(!detector.update(Some(1.66), tick));
    assert!(!detector.update(Some(1.74), Duration::from_millis(10)));

    assert!(!detector.update(Some(1.7), tick));
    assert!(!detector.update(Some(1.66), tick));
    assert!(!detector.update(None, tick));
    assert!(!detector.update(Some(1.74), tick));

    assert!(!detector.update(Some(1.7), tick));
    assert!(!detector.update(Some(1.66), tick));
    assert!(!detector.update(Some(1.66), Duration::from_secs(1)));
    assert!(!detector.update(Some(1.74), tick));

    assert!(!detector.update(Some(1.7), tick));
    assert!(!detector.update(Some(1.66), tick));
    assert!(detector.update(Some(1.74), tick));
    assert!(!detector.update(Some(1.74), tick));
}

#[test]
fn lua_ability_requires_the_matching_hands_current_pose() -> mlua::Result<()> {
    let runtime = ScriptRuntime::new()?;
    let gameplay =
        runtime.load_module("@vr_input.lua", include_str!("../examples/vr_input.lua"))?;
    let callbacks = VrCallbacks::from_module(&gameplay)?;
    callbacks.hand_pose(
        Hand::Right,
        &[JointPose {
            name: "index_tip",
            position: [0.2, 1.3, -0.4],
            orientation: [0.0, 0.0, 0.0, 1.0],
        }],
    )?;
    callbacks.hand_gesture("pinch", Hand::Left)?;
    assert_eq!(gameplay.raw_get::<u32>("abilities_cast")?, 0);
    callbacks.hand_gesture("pinch", Hand::Right)?;
    callbacks.hand_pose(Hand::Right, &[])?;
    callbacks.hand_gesture("pinch", Hand::Right)?;
    callbacks.locomotion_step()?;
    assert_eq!(gameplay.raw_get::<u32>("abilities_cast")?, 1);
    assert_eq!(gameplay.raw_get::<u32>("steps")?, 1);
    Ok(())
}

#[test]
fn baseplate_moves_forward_and_sends_one_pinch_while_held() -> mlua::Result<()> {
    let scripts = ScriptRuntime::new()?;
    let mut game = LuaScene::load(
        &scripts,
        "@baseplate.lua",
        include_str!("../examples/baseplate.lua"),
    )?;
    let pose = |position| {
        Some(Pose {
            position,
            orientation: Quat::IDENTITY,
        })
    };
    let mut frame = TrackingFrame::default();
    for height in [1.67, 1.62, 1.62, 1.72] {
        frame.head = pose(Vec3::new(0.0, height, 0.0));
        game.tick(&frame, Duration::from_millis(50), false, false)?;
    }
    frame.hands[0].0[5] = pose(Vec3::ZERO);
    frame.hands[0].0[10] = pose(Vec3::X * 0.01);
    let (_, offset) = game.tick(&frame, Duration::from_millis(16), false, false)?;
    assert!(offset.z < 0.0);
    assert_eq!(game.module().raw_get::<u32>("steps")?, 1);
    assert_eq!(game.module().raw_get::<u32>("pinches")?, 1);

    frame.head = None;
    let (_, stopped) = game.tick(&frame, Duration::from_millis(16), false, false)?;
    assert_eq!(stopped, offset);
    assert_eq!(game.module().raw_get::<u32>("pinches")?, 1);
    frame.hands[0].0[10] = None;
    game.tick(&frame, Duration::from_millis(16), false, false)?;
    frame.hands[0].0[10] = pose(Vec3::X * 0.01);
    game.tick(&frame, Duration::from_millis(16), false, false)?;
    assert_eq!(game.module().raw_get::<u32>("pinches")?, 2);
    Ok(())
}

#[test]
fn asymmetric_eye_projection_maps_fov_to_clip_edges() {
    let view = Camera {
        position: Vec3::ZERO,
        orientation: Quat::IDENTITY,
        fov: [-0.8, 1.1, -0.5, 0.7],
    }
    .view_projection();
    for (x, y, expected_x, expected_y) in [(-0.8, -0.5, -1.0, -1.0), (1.1, 0.7, 1.0, 1.0)] {
        let projected = view.project_point3(Vec3::new(x, y, -1.0));
        assert!((projected.x - expected_x).abs() < 1e-5);
        assert!((projected.y - expected_y).abs() < 1e-5);
        assert!((0.0..1.0).contains(&projected.z));
    }
}

#[test]
fn avatar_uses_available_leg_poses_independently() {
    let mut frame = TrackingFrame {
        head: Some(Pose {
            position: Vec3::Y * 1.6,
            orientation: Quat::IDENTITY,
        }),
        ..Default::default()
    };
    frame.body.legs[0] = [
        Some(Vec3::new(-0.1, 1.0, 0.0)),
        Some(Vec3::new(-0.1, 0.7, -0.3)),
        Some(Vec3::new(-0.1, 0.5, -0.35)),
    ];
    let mut player = PlayerTracking::default();
    player.offset = Vec3::NEG_Z;
    let avatar = player.avatar(&frame, 0.0).unwrap();
    assert_eq!(
        avatar.legs[0][2],
        frame.body.legs[0][2].unwrap() + player.offset
    );
    assert!((avatar.legs[1][2].y - 0.07).abs() < 1e-5);
}

#[test]
fn reflection_lab_keeps_geometry_when_ray_tracing_is_disabled() -> mlua::Result<()> {
    let scripts = ScriptRuntime::new()?;
    let mut lab = LuaScene::load(
        &scripts,
        "@reflection_lab.lua",
        include_str!("../examples/reflection_lab.lua"),
    )?;
    let frame = TrackingFrame::default();
    let (enabled, _) = lab.tick(&frame, Duration::ZERO, false, false)?;
    assert!(enabled.ray_tracing);
    assert_eq!(enabled.spheres.len(), 6);
    assert_eq!(enabled.traced_boxes, enabled.boxes.len());
    lab.module().raw_set("ray_tracing", false)?;
    let (disabled, _) = lab.tick(&frame, Duration::ZERO, false, false)?;
    assert!(!disabled.ray_tracing);
    assert_eq!(disabled.spheres.len(), enabled.spheres.len());
    assert_eq!(disabled.boxes.len(), enabled.boxes.len());
    assert_eq!(disabled.cloud_density, enabled.cloud_density);
    Ok(())
}

#[test]
fn reflection_lab_walks_towards_gaze_and_renders_controller_hands() -> mlua::Result<()> {
    let script = ScriptRuntime::new()?;
    let mut lab = LuaScene::load(
        &script,
        "@reflection_lab.lua",
        include_str!("../examples/reflection_lab.lua"),
    )?;
    let grip = Pose {
        position: Vec3::new(-0.25, 1.4, -0.55),
        orientation: Quat::IDENTITY,
    };
    let yaw = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
    let mut frame = TrackingFrame {
        controller_grips: [Some(grip), None],
        ..Default::default()
    };
    for height in [1.69, 1.65, 1.65, 1.65] {
        frame.head = Some(Pose {
            position: Vec3::Y * height,
            orientation: yaw,
        });
        let (_, offset) = lab.tick(&frame, Duration::from_millis(40), false, false)?;
        assert_eq!(offset, Vec3::ZERO);
    }
    frame.head.as_mut().unwrap().position.y = 1.69;
    let (world, offset) = lab.tick(&frame, Duration::from_millis(40), false, false)?;
    assert!(offset.x > 0.0 && offset.z.abs() < 1e-5 && offset.y == 0.0);
    assert!(
        world.boxes[world.traced_boxes..]
            .iter()
            .any(|mesh| { mesh.center.distance(grip.position + offset) < 1e-5 })
    );
    frame.head.as_mut().unwrap().orientation = Quat::IDENTITY;
    let (_, turned) = lab.tick(&frame, Duration::from_millis(40), false, false)?;
    assert!((turned.x - offset.x).abs() < 1e-5 && turned.z < 0.0);
    frame.head = None;
    let (_, stopped) = lab.tick(&frame, Duration::from_millis(40), false, false)?;
    assert_eq!(turned, stopped);
    Ok(())
}
