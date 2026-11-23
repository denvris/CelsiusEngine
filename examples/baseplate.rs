#[cfg(all(windows, target_pointer_width = "64"))]
mod windows_demo {
    use std::{env, ffi::OsString, fs, path::PathBuf, time::Duration};

    use celsius_engine::{
        graphics::{Camera, d3d11},
        scripting::{LuaScene, ScriptRuntime},
        vr::{
            HandFrame, Pose, TrackingFrame,
            openxr::{self, Result},
        },
    };
    use glam::{Mat4, Quat, Vec3};

    enum Mode {
        Headset,
        Probe,
        Preview(PathBuf),
    }

    pub fn run() -> Result<()> {
        let mut args = env::args_os().skip(1);
        let mut mode = Mode::Headset;
        let mut loader = env::var_os("CELSIUS_OPENXR_LOADER").map(PathBuf::from);
        let mut game = None;
        let mut shader = None;
        let mut hdr = None;
        let mut render_scale = 0.75;
        while let Some(arg) = args.next() {
            let mut value = || -> Result<OsString> {
                args.next()
                    .ok_or_else(|| format!("Missing value for {}", arg.to_string_lossy()).into())
            };
            match arg.to_str() {
                Some("--probe") => mode = Mode::Probe,
                Some("--preview") => mode = Mode::Preview(value()?.into()),
                Some("--loader") => loader = Some(value()?.into()),
                Some("--game") => game = Some(PathBuf::from(value()?)),
                Some("--shader") => shader = Some(PathBuf::from(value()?)),
                Some("--hdr") => hdr = Some(PathBuf::from(value()?)),
                Some("--scale") => render_scale = value()?.to_string_lossy().parse()?,
                Some("--help") => {
                    println!(
                        "baseplate [--probe | --preview output.png] [--loader path] [--game game.lua] [--shader surface.hlsl] [--hdr sky.hdr] [--scale 0.4..1.0]"
                    );
                    return Ok(());
                }
                _ => return Err(format!("Unknown option: {}", arg.to_string_lossy()).into()),
            }
        }

        if matches!(mode, Mode::Probe) {
            return openxr::probe(loader.as_deref());
        }
        let surface = shader.map(fs::read_to_string).transpose()?;
        let surface = surface.as_deref().unwrap_or(d3d11::DEFAULT_SURFACE);
        let scripts = ScriptRuntime::new()?;
        let source = game.as_ref().map(fs::read_to_string).transpose()?;
        let name = game.as_ref().map_or_else(
            || "@examples/baseplate.lua".to_owned(),
            |path| format!("@{}", path.display()),
        );
        let mut scene = LuaScene::load(
            &scripts,
            &name,
            source.as_deref().unwrap_or(include_str!("baseplate.lua")),
        )?;
        match mode {
            Mode::Probe => unreachable!(),
            Mode::Headset => openxr::run(
                loader.as_deref(),
                surface,
                hdr.as_deref(),
                render_scale,
                |frame| {
                    Ok(scene.tick(
                        &frame.tracking,
                        frame.elapsed,
                        frame.reference_changed,
                        false,
                    )?)
                },
            ),
            Mode::Preview(path) => {
                let mut tracking = TrackingFrame {
                    head: Some(Pose {
                        position: Vec3::new(0.0, 1.67, 0.0),
                        orientation: Quat::IDENTITY,
                    }),
                    hands: [synthetic_hand(-1.0), synthetic_hand(1.0)],
                    ..Default::default()
                };
                for (height, frames) in [(1.67, 1), (1.62, 8), (1.72, 1), (1.67, 18)] {
                    tracking.head.as_mut().unwrap().position.y = height;
                    for _ in 0..frames {
                        scene.tick(&tracking, Duration::from_millis(16), false, true)?;
                    }
                }
                let hand = &mut tracking.hands[1].0;
                hand[5].as_mut().unwrap().position = hand[10].unwrap().position + Vec3::X * 0.012;
                let (world, offset) =
                    scene.tick(&tracking, Duration::from_millis(16), false, true)?;
                let camera_position = Vec3::new(2.8, 2.08, -3.9);
                let target = Vec3::new(0.0, 1.1, -1.0);
                let (_, orientation, position) = Mat4::look_at_rh(camera_position, target, Vec3::Y)
                    .inverse()
                    .to_scale_rotation_translation();
                let camera = Camera {
                    position,
                    orientation,
                    fov: [-0.72, 0.72, -0.42, 0.42],
                };
                let device = d3d11::create_device(None)?;
                let mut renderer = d3d11::Renderer::new(device, surface, hdr.as_deref())?;
                renderer.upload(&world)?;
                renderer.capture_png(1152, 672, camera, world.cloud_density, &path)?;
                println!(
                    "Preview: {}. Simulated displacement: {:.2} m. Steps: {:?}. Pinches: {:?}.",
                    path.display(),
                    offset.length(),
                    scene.module().raw_get::<Option<u32>>("steps")?,
                    scene.module().raw_get::<Option<u32>>("pinches")?,
                );
                Ok(())
            }
        }
    }

    fn synthetic_hand(side: f32) -> HandFrame {
        let origin = Vec3::new(side * 0.39, 1.35 + side * 0.07, -0.46);
        let rotation = Quat::from_rotation_z(side * -0.43) * Quat::from_rotation_x(-0.42);
        let point = |position| {
            Some(Pose {
                position: origin + rotation * position,
                orientation: rotation,
            })
        };
        let mut joints = [None; 26];
        joints[0] = point(Vec3::new(0.0, 0.042, 0.0));
        joints[1] = point(Vec3::ZERO);
        for joint in 0..4 {
            let t = joint as f32 / 3.0;
            joints[joint + 2] = point(Vec3::new(
                -side * (0.028 + 0.031 * t),
                0.023 + 0.058 * t,
                -0.023 * t,
            ));
        }
        for finger in 0..4 {
            for joint in 0..5 {
                let t = joint as f32 / 4.0;
                let x = side * (-0.025 + 0.017 * finger as f32) * (1.0 + 0.65 * t);
                let length = [0.112, 0.128, 0.115, 0.088][finger];
                joints[6 + finger * 5 + joint] =
                    point(Vec3::new(x, 0.012 + length * t, -t * t * 0.025));
            }
        }
        HandFrame(joints)
    }
}

#[cfg(all(windows, target_pointer_width = "64"))]
fn main() -> celsius_engine::vr::openxr::Result<()> {
    windows_demo::run()
}

#[cfg(not(all(windows, target_pointer_width = "64")))]
fn main() -> std::process::ExitCode {
    eprintln!("This OpenXR demo requires 64-bit Windows and Direct3D 11.");
    std::process::ExitCode::FAILURE
}
