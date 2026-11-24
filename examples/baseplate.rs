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
    const LAB_SHADER: &str = include_str!("reflection_lab.hlsl");
    const LAB_GAME: &str = include_str!("reflection_lab.lua");

    enum Mode {
        Headset,
        Probe,
        Preview(PathBuf),
        WalkPreview(PathBuf),
    }

    pub fn run() -> Result<()> {
        let mut args = env::args_os().skip(1);
        let mut mode = Mode::Headset;
        let mut loader = env::var_os("CELSIUS_OPENXR_LOADER").map(PathBuf::from);
        let mut game = None;
        let mut shader = None;
        let mut hdr = None;
        let mut render_scale = 0.75;
        let mut lab = false;
        let mut ray_tracing = None;
        let mut assets = PathBuf::from("assets/reflection-lab");
        let mut session_options = openxr::SessionOptions::default();
        while let Some(arg) = args.next() {
            let mut value = || -> Result<OsString> {
                args.next()
                    .ok_or_else(|| format!("Missing value for {}", arg.to_string_lossy()).into())
            };
            match arg.to_str() {
                Some("--probe") => mode = Mode::Probe,
                Some("--preview") => mode = Mode::Preview(value()?.into()),
                Some("--walk-preview") => {
                    mode = Mode::WalkPreview(value()?.into());
                    lab = true;
                }
                Some("--loader") => loader = Some(value()?.into()),
                Some("--game") => game = Some(PathBuf::from(value()?)),
                Some("--shader") => shader = Some(PathBuf::from(value()?)),
                Some("--hdr") => hdr = Some(PathBuf::from(value()?)),
                Some("--lab") => lab = true,
                Some("--no-mirror") => session_options.desktop_mirror = false,
                Some("--assets") => assets = value()?.into(),
                Some("--raytracing") => {
                    ray_tracing = Some(match value()?.to_str() {
                        Some("on") => true,
                        Some("off") => false,
                        _ => return Err("--raytracing expects on or off".into()),
                    });
                }
                Some("--scale") => render_scale = value()?.to_string_lossy().parse()?,
                Some("--help") => {
                    println!(
                        "baseplate [--probe | --preview output.png | --walk-preview output.png] [--lab] [--no-mirror] [--raytracing on|off] [--assets directory] [--loader path] [--game game.lua] [--shader surface.hlsl] [--hdr sky.hdr] [--scale 0.4..1.0]"
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
        let surface = surface.as_deref().unwrap_or(if lab {
            LAB_SHADER
        } else {
            d3d11::DEFAULT_SURFACE
        });
        if lab && hdr.is_none() {
            let default_sky = assets.join("kloppenheim_05_puresky_4k.hdr");
            if default_sky.try_exists()? {
                hdr = Some(default_sky);
            }
        }
        let renderer = |device| -> Result<d3d11::Renderer> {
            let mut renderer = d3d11::Renderer::new(device, surface, hdr.as_deref())?;
            if lab {
                let loaded = renderer.load_material_maps(&assets)?;
                println!(
                    "Reflection lab: {loaded}/4 material maps, HDRI: {}",
                    hdr.is_some()
                );
            }
            Ok(renderer)
        };
        let scripts = ScriptRuntime::new()?;
        let source = game.as_ref().map(fs::read_to_string).transpose()?;
        let name = game.as_ref().map_or_else(
            || {
                if lab {
                    "@examples/reflection_lab.lua"
                } else {
                    "@examples/baseplate.lua"
                }
                .to_owned()
            },
            |path| format!("@{}", path.display()),
        );
        let mut scene = LuaScene::load(
            &scripts,
            &name,
            source.as_deref().unwrap_or(if lab {
                LAB_GAME
            } else {
                include_str!("baseplate.lua")
            }),
        )?;
        if let Some(enabled) = ray_tracing {
            scene.module().raw_set("ray_tracing", enabled)?;
        }
        match mode {
            Mode::Probe => unreachable!(),
            Mode::Headset => openxr::run_session(
                loader.as_deref(),
                render_scale,
                session_options,
                renderer,
                |frame| {
                    Ok(scene.tick(
                        &frame.tracking,
                        frame.elapsed,
                        frame.reference_changed,
                        false,
                    )?)
                },
            ),
            Mode::WalkPreview(path) => {
                let mut head = Pose {
                    position: Vec3::Y * 1.69,
                    orientation: Quat::IDENTITY,
                };
                let mut tracking = TrackingFrame {
                    controller_grips: [-1.0, 1.0].map(|side| {
                        Some(Pose {
                            position: Vec3::new(side * 0.28, 1.47, -0.55),
                            orientation: Quat::from_rotation_y(-side * 0.22),
                        })
                    }),
                    ..Default::default()
                };
                let elapsed = Duration::from_millis(20);
                for height in [1.69, 1.65, 1.65, 1.65, 1.65, 1.65, 1.69]
                    .into_iter()
                    .cycle()
                    .take(28)
                {
                    head.position.y = height;
                    tracking.head = Some(head);
                    scene.tick(&tracking, elapsed, false, false)?;
                }
                let (world, offset) = scene.tick(&tracking, elapsed, false, false)?;
                let camera = Camera {
                    position: head.position + offset,
                    orientation: Quat::from_rotation_x(-0.10),
                    fov: [-0.88, 0.88, -0.50, 0.50],
                };
                let mut renderer = renderer(d3d11::create_device(None)?)?;
                renderer.upload(&world)?;
                renderer.capture_png(1440, 810, camera, world.cloud_density, &path)?;
                println!(
                    "Walking preview: {}. Simulated advance: {:.2} m.",
                    path.display(),
                    offset.length()
                );
                Ok(())
            }
            Mode::Preview(path) => {
                if lab {
                    let (world, _) =
                        scene.tick(&TrackingFrame::default(), Duration::ZERO, false, false)?;
                    let camera = preview_camera(
                        Vec3::new(0.0, 2.18, 1.15),
                        Vec3::new(0.0, 1.29, -4.45),
                        [-0.72, 0.72, -0.38, 0.38],
                    );
                    let mut renderer = renderer(d3d11::create_device(None)?)?;
                    renderer.upload(&world)?;
                    renderer.capture_png(1440, 810, camera, world.cloud_density, &path)?;
                    println!(
                        "Reflection lab preview: {}. Scene rays: {}.",
                        path.display(),
                        world.ray_tracing
                    );
                    return Ok(());
                }
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

    fn preview_camera(position: Vec3, target: Vec3, fov: [f32; 4]) -> Camera {
        let (_, orientation, position) = Mat4::look_at_rh(position, target, Vec3::Y)
            .inverse()
            .to_scale_rotation_translation();
        Camera {
            position,
            orientation,
            fov,
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
