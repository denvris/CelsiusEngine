use std::time::Duration;

use celsius_engine::graphics::Camera;
use celsius_engine::scripting::{LuaScene, ScriptRuntime};
use celsius_engine::vr::{
    Hand, HeadBobDetector, JointPose, PlayerTracking, Pose, TrackingFrame, VrCallbacks,
};
use glam::{Quat, Vec3};

fn jog_height(frame: u32, baseline: f32) -> f32 {
    baseline + 0.009 * (frame as f32 * 0.01 * std::f32::consts::TAU * 2.6).sin()
}

#[test]
fn cadence_detects_shallow_bobs_without_triggering_on_a_crouch() {
    let tick = Duration::from_millis(10);
    let mut detector = HeadBobDetector::new(1.7).unwrap();
    let crouch = (0..110).map(|i| 1.7 - 0.25 * (i as f32 / 110.0 * std::f32::consts::PI).sin());
    for height in crouch {
        assert!(!detector.update(Some(height), tick));
    }
    for i in 0..300 {
        assert!(!detector.update(Some(1.70 + 0.001 * (i as f32).sin()), tick));
    }
    let steps = (0..160)
        .filter(|&i| detector.update(Some(jog_height(i, 1.68)), tick))
        .count();
    assert!(steps >= 2, "Detected {steps} shallow steps");
    assert!(!detector.update(None, tick));
    for i in 0..49 {
        assert!(!detector.update(Some(jog_height(i, 1.68)), tick));
    }
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
    for i in 0..120 {
        frame.head = pose(Vec3::new(0.0, jog_height(i, 1.67), 0.0));
        game.tick(&frame, Duration::from_millis(10), false, false)?;
    }
    frame.hands[0].0[5] = pose(Vec3::ZERO);
    frame.hands[0].0[10] = pose(Vec3::X * 0.01);
    let (_, offset) = game.tick(&frame, Duration::from_millis(16), false, false)?;
    assert!(offset.z < 0.0);
    assert!(game.module().raw_get::<u32>("steps")? >= 1);
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
    assert_eq!(enabled.spheres.len(), 7);
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
    for i in 0..120 {
        frame.head = Some(Pose {
            position: Vec3::Y * jog_height(i, 1.69),
            orientation: yaw,
        });
        lab.tick(&frame, Duration::from_millis(10), false, false)?;
    }
    let (world, offset) = lab.tick(&frame, Duration::from_millis(10), false, false)?;
    assert_eq!(world.ray_avatar_boxes.len(), 7);

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

#[cfg(all(windows, target_pointer_width = "64"))]
#[test]
fn same_camera_reflects_a_moving_object_on_successive_frames()
-> Result<(), Box<dyn std::error::Error>> {
    use celsius_engine::graphics::{BoxMesh, Scene, SphereMesh, d3d11};

    let shader = "float3 shade_surface(Surface s) { float3 n = normalize(s.normal); return trace_radiance(s.world + n * 0.006, reflect(normalize(s.world - camera.xyz), n)); }";
    let mut renderer = d3d11::Renderer::new(d3d11::create_device(None)?, shader, None)?;
    let camera = Camera {
        position: Vec3::ZERO,
        orientation: Quat::IDENTITY,
        fov: [-0.6, 0.6, -0.6, 0.6],
    };
    let mut scene = Scene {
        ray_tracing: true,
        traced_boxes: 1,
        spheres: vec![SphereMesh {
            center: Vec3::new(0.0, 0.0, -3.0),
            radius: 1.0,
            color: Vec3::ONE,
            material: 10.0,
        }],
        boxes: vec![BoxMesh {
            center: Vec3::Z,
            size: Vec3::splat(1.0),
            orientation: Quat::from_rotation_y(0.6),
            color: Vec3::new(2.0, 0.0, 0.0),
            material: 2.0,
        }],
        ..Default::default()
    };
    let path = std::env::temp_dir().join(format!("celsius-ray-test-{}.png", std::process::id()));
    let mut capture = |scene: &Scene| -> Result<[u8; 4], Box<dyn std::error::Error>> {
        renderer.upload(scene)?;
        renderer.capture_png(65, 65, camera, 0.0, &path)?;
        let result = image::open(&path);
        std::fs::remove_file(&path)?;
        Ok(result?.into_rgba8().get_pixel(32, 32).0)
    };
    let reflected = capture(&scene)?;
    scene.boxes[0].center.x = 4.0;
    let moved = capture(&scene)?;
    assert!(
        reflected[0] > 200 && reflected[2] == 0,
        "Pixel was {reflected:?}"
    );
    assert!(moved[2] > 100, "Pixel stayed at {moved:?}");
    Ok(())
}
