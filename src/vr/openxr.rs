mod ffi;

use std::{
    error::Error,
    ffi::{CStr, c_char},
    path::Path,
    ptr, thread,
    time::{Duration, Instant},
};

use glam::{Quat, Vec3};
use libloading::os::windows::Library;
use windows::{
    Win32::Graphics::{
        Direct3D11::ID3D11Device,
        Dxgi::Common::{
            DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        },
    },
    core::Interface,
};

use super::{BodyFrame, HandFrame, Pose, TrackingFrame};
use crate::graphics::{
    Camera, Scene,
    d3d11::{self, RenderTarget, Renderer},
};
use ffi::*;

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct OwnedHandle {
    raw: Handle,
    destroy: Destroy,
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.raw);
        }
    }
}

struct Runtime {
    // Destroy the instance before unloading the loader.
    instance: OwnedHandle,
    _loader: Library,
    api: Api,
    system: Handle,
    system_name: String,
    hand_api: Option<HandApi>,
    body_api: Option<BodyApi>,
    full_body: bool,
}

struct EyeTarget {
    // Drop D3D render-target views before destroying their OpenXR swapchain.
    target: RenderTarget,
    swapchain: OwnedHandle,
    image: SubImage,
}

pub struct Frame {
    pub tracking: TrackingFrame,
    pub elapsed: Duration,
    pub reference_changed: bool,
}

pub fn probe(loader: Option<&Path>) -> Result<()> {
    let runtime = Runtime::connect(loader)?;
    println!("OpenXR system: {}", runtime.system_name);
    println!("Articulated hand tracking: {}", runtime.hand_api.is_some());
    println!(
        "Body tracking: {}. Full body: {}.",
        runtime.body_api.is_some(),
        runtime.full_body
    );
    Ok(())
}

/// Drives the compositor independently of gameplay. The scene callback returns world geometry
/// and the world offset to apply to both eyes. It runs once per rendered stereo frame.
pub fn run(
    loader: Option<&Path>,
    surface: &str,
    hdri: Option<&Path>,
    render_scale: f32,
    scene: impl FnMut(Frame) -> Result<(Scene, Vec3)>,
) -> Result<()> {
    run_with_renderer(
        loader,
        render_scale,
        |device| Renderer::new(device, surface, hdri),
        scene,
    )
}

/// Allows an application to select the render pipeline while retaining the OpenXR frame loop.
pub fn run_with_renderer(
    loader: Option<&Path>,
    render_scale: f32,
    renderer: impl FnOnce(ID3D11Device) -> Result<Renderer>,
    mut scene: impl FnMut(Frame) -> Result<(Scene, Vec3)>,
) -> Result<()> {
    if !render_scale.is_finite() || !(0.4..=1.0).contains(&render_scale) {
        return Err("Render scale must be between 0.4 and 1.0".into());
    }
    let runtime = Runtime::connect(loader)?;
    eprintln!(
        "OpenXR: selected headset {}. Creating D3D11 session.",
        runtime.system_name
    );
    let api = &runtime.api;
    let instance = runtime.instance.raw;
    let system = runtime.system;
    let mut requirements = D3dRequirements::default();
    check(
        unsafe { (api.xrGetD3D11GraphicsRequirementsKHR)(instance, system, &mut requirements) },
        "xrGetD3D11GraphicsRequirementsKHR",
    )?;
    let device = d3d11::create_device(Some((requirements.luid, requirements.feature_level)))?;
    let session = runtime.session(&device)?;
    let mut renderer = renderer(device)?;
    eprintln!("OpenXR: creating stereo swapchains.");
    let view_space = reference_space(api, session.raw, VIEW_SPACE)?;
    let (world_space, local_floor) = match try_space(api, session.raw, STAGE_SPACE) {
        (SPACE_UNSUPPORTED, _) => (reference_space(api, session.raw, LOCAL_SPACE)?, true),
        (code, raw) => {
            check(code, "xrCreateReferenceSpace (stage)")?;
            (
                OwnedHandle {
                    raw,
                    destroy: api.xrDestroySpace,
                },
                false,
            )
        }
    };
    let mut floor_shift = if local_floor { None } else { Some(0.0) };

    let modes = enumerate(
        0,
        |capacity, count, data| unsafe {
            (api.xrEnumerateEnvironmentBlendModes)(
                instance,
                system,
                PRIMARY_STEREO,
                capacity,
                count,
                data,
            )
        },
        "xrEnumerateEnvironmentBlendModes",
    )?;
    if !modes.contains(&OPAQUE) {
        return Err("The runtime did not expose opaque stereo composition".into());
    }
    let configs = enumerate(
        ViewConfig::default(),
        |capacity, count, data| unsafe {
            (api.xrEnumerateViewConfigurationViews)(
                instance,
                system,
                PRIMARY_STEREO,
                capacity,
                count,
                data,
            )
        },
        "xrEnumerateViewConfigurationViews",
    )?;
    let [left, right]: [ViewConfig; 2] = configs
        .try_into()
        .map_err(|_| "Expected two OpenXR views")?;
    let formats = enumerate(
        0,
        |capacity, count, data| unsafe {
            (api.xrEnumerateSwapchainFormats)(session.raw, capacity, count, data)
        },
        "xrEnumerateSwapchainFormats",
    )?;
    let format = formats
        .into_iter()
        .find(|f| {
            [
                DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
                DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
            ]
            .iter()
            .any(|format| i64::from(format.0) == *f)
        })
        .ok_or("No sRGB color swapchain format supported")?;
    let targets = [
        EyeTarget::new(api, session.raw, &renderer, left, format, render_scale)?,
        EyeTarget::new(api, session.raw, &renderer, right, format, render_scale)?,
    ];
    let trackers = runtime
        .hand_api
        .as_ref()
        .map(|hand| -> Result<_> {
            Ok([
                create_hand(hand, session.raw, 1)?,
                create_hand(hand, session.raw, 2)?,
            ])
        })
        .transpose()?;
    let body_tracker = runtime.body_api.as_ref().and_then(|body| {
        match create_body(body, session.raw, runtime.full_body) {
            Ok(tracker) => Some(tracker),
            Err(error) => {
                eprintln!("Body tracker unavailable, using estimated pose: {error}");
                None
            }
        }
    });

    println!(
        "OpenXR system: {}. Articulated hands: {}. Body tracking: {}. Full body: {}.",
        runtime.system_name,
        trackers.is_some(),
        body_tracker.is_some(),
        body_tracker.is_some() && runtime.full_body,
    );
    let mut running = false;
    let mut focused = false;
    let mut reset = true;
    let mut previous_time = None;
    let mut ready_deadline = Some(Instant::now() + Duration::from_secs(45));
    eprintln!("OpenXR: waiting for the runtime to mark the session ready.");
    loop {
        loop {
            let mut event = Event::default();
            let code = unsafe { (api.xrPollEvent)(instance, &mut event) };
            if code == EVENT_UNAVAILABLE {
                break;
            }
            check(code, "xrPollEvent")?;
            match event.ty {
                EVENT_INSTANCE_LOSS => return Ok(()),
                EVENT_SESSION_STATE => {
                    // Event buffers share their C header and alignment with the tagged payload.
                    let event = unsafe { &*ptr::from_ref(&event).cast::<SessionStateEvent>() };
                    if event.session != session.raw {
                        continue;
                    }
                    focused = event.state == STATE_FOCUSED;
                    reset |= !focused;
                    match event.state {
                        STATE_READY => {
                            let begin = SessionBegin {
                                config: PRIMARY_STEREO,
                                ..Default::default()
                            };
                            check(
                                unsafe { (api.xrBeginSession)(session.raw, &begin) },
                                "xrBeginSession",
                            )?;
                            running = true;
                            ready_deadline = None;
                            eprintln!("OpenXR: stereo session started.");
                        }
                        STATE_STOPPING => {
                            check(unsafe { (api.xrEndSession)(session.raw) }, "xrEndSession")?;
                            running = false;
                            previous_time = None;
                        }
                        STATE_EXITING | STATE_LOSS_PENDING => return Ok(()),
                        _ => {}
                    }
                }
                EVENT_REFERENCE_CHANGE => {
                    reset = true;
                    if local_floor {
                        floor_shift = None;
                    }
                }
                _ => {}
            }
        }
        if !running {
            if ready_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err("OpenXR session did not become ready within 45 seconds. Enter the active PCVR runtime inside the headset and launch again.".into());
            }
            thread::sleep(Duration::from_millis(12));
            continue;
        }

        let mut frame = FrameState::default();
        check(
            unsafe { (api.xrWaitFrame)(session.raw, &FrameWait::default(), &mut frame) },
            "xrWaitFrame",
        )?;
        check(
            unsafe { (api.xrBeginFrame)(session.raw, &FrameBegin::default()) },
            "xrBeginFrame",
        )?;
        let elapsed = Duration::from_nanos(
            previous_time
                .map_or(frame.period, |previous| frame.time.saturating_sub(previous))
                .max(0) as u64,
        );
        previous_time = Some(frame.time);

        let work = (|| -> Result<Option<[ProjectionView; 2]>> {
            if frame.should_render == 0 {
                reset = true;
                return Ok(None);
            }
            let mut views = [View::default(); 2];
            let view_info = ViewInfo {
                config: PRIMARY_STEREO,
                time: frame.time,
                space: world_space.raw,
                ..Default::default()
            };
            let mut state = ViewState::default();
            let mut count = 0;
            check(
                unsafe {
                    (api.xrLocateViews)(
                        session.raw,
                        &view_info,
                        &mut state,
                        2,
                        &mut count,
                        views.as_mut_ptr(),
                    )
                },
                "xrLocateViews",
            )?;
            if count != 2 || state.flags & VALID_POSE != VALID_POSE {
                reset = true;
                return Ok(None);
            }
            let mut head = SpaceLocation::default();
            check(
                unsafe {
                    (api.xrLocateSpace)(view_space.raw, world_space.raw, frame.time, &mut head)
                },
                "xrLocateSpace",
            )?;
            let shift = *floor_shift.get_or_insert_with(|| 1.65 - views[0].pose.position[1]);
            let tracking_head = (focused && head.flags & TRACKED_POSE == TRACKED_POSE)
                .then(|| pose(head.pose, shift));
            let mut tracking = TrackingFrame {
                head: tracking_head,
                ..Default::default()
            };
            if focused
                && let Some((hand, trackers)) = runtime.hand_api.as_ref().zip(trackers.as_ref())
            {
                for (frame_hand, tracker) in tracking.hands.iter_mut().zip(trackers) {
                    *frame_hand =
                        sample_hand(hand, tracker.raw, world_space.raw, frame.time, shift)?;
                }
            }
            if focused
                && let Some((body, tracker)) = runtime.body_api.as_ref().zip(body_tracker.as_ref())
            {
                let (sample, body_hands) = sample_body(
                    body,
                    tracker.raw,
                    world_space.raw,
                    frame.time,
                    shift,
                    runtime.full_body,
                )?;
                tracking.body = sample;
                if trackers.is_none() {
                    tracking.hands = body_hands;
                }
            }
            let (world, player_offset) = scene(Frame {
                tracking,
                elapsed,
                reference_changed: reset,
            })?;
            reset = false;
            renderer.upload(&world)?;

            let mut projection = [ProjectionView::default(); 2];
            for ((projection, view), eye) in projection.iter_mut().zip(views).zip(&targets) {
                let eye_pose = pose(view.pose, shift);
                // XrFovf stores up before down. The renderer uses bottom before top.
                let [l, r, t, b] = view.fov.map(f32::tan);
                let camera = Camera {
                    position: eye_pose.position + player_offset,
                    orientation: eye_pose.orientation,
                    fov: [l, r, b, t],
                };

                eye.render(api, &renderer, camera, world.cloud_density)?;
                *projection = ProjectionView {
                    pose: view.pose,
                    fov: view.fov,
                    image: eye.image,
                    ..Default::default()
                };
            }
            Ok(Some(projection))
        })();

        let mut end = FrameEnd {
            time: frame.time,
            blend: OPAQUE,
            ..Default::default()
        };
        let end_result = if let Ok(Some(ref views)) = work {
            let layer = Projection {
                space: world_space.raw,
                view_count: 2,
                views: views.as_ptr(),
                ..Default::default()
            };
            let layers = [&raw const layer];
            end.count = 1;
            end.layers = layers.as_ptr();
            unsafe { (api.xrEndFrame)(session.raw, &end) }
        } else {
            unsafe { (api.xrEndFrame)(session.raw, &end) }
        };
        work?;
        check(end_result, "xrEndFrame")?;
    }
}

impl Runtime {
    fn connect(path: Option<&Path>) -> Result<Self> {
        use libloading::os::windows::{
            LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
        };
        let loader = if let Some(path) = path {
            if !path.is_absolute() {
                return Err("Provide an absolute path to openxr_loader.dll".into());
            }
            unsafe {
                Library::load_with_flags(
                    path,
                    LOAD_LIBRARY_SEARCH_DEFAULT_DIRS | LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
                )
            }
        } else {
            unsafe {
                Library::load_with_flags("openxr_loader.dll", LOAD_LIBRARY_SEARCH_DEFAULT_DIRS)
            }
        }
        .map_err(|e| {
            format!("Could not load OpenXR loader. Use --loader with its absolute path. {e}")
        })?;
        let get = unsafe { *loader.get::<GetProc>(b"xrGetInstanceProcAddr\0")? };
        let global = Global::load(get, 0)?;
        eprintln!("OpenXR: querying the active runtime's extensions.");
        let extensions = enumerate(
            Extension::default(),
            |capacity, count, data| unsafe {
                (global.xrEnumerateInstanceExtensionProperties)(ptr::null(), capacity, count, data)
            },
            "xrEnumerateInstanceExtensionProperties",
        )?;
        let supported = |name: &CStr| {
            extensions
                .iter()
                .any(|extension| unsafe { CStr::from_ptr(extension.name.as_ptr()) == name })
        };
        if !supported(D3D11_EXTENSION) {
            return Err("The active runtime lacks XR_KHR_D3D11_enable".into());
        }
        let hand_enabled = supported(HAND_EXTENSION);
        let body_enabled = supported(BODY_EXTENSION);
        let full_body_enabled = body_enabled && supported(FULL_BODY_EXTENSION);
        let mut enabled = vec![D3D11_EXTENSION.as_ptr()];
        if hand_enabled {
            enabled.push(HAND_EXTENSION.as_ptr());
        }
        if body_enabled {
            enabled.push(BODY_EXTENSION.as_ptr());
        }
        if full_body_enabled {
            enabled.push(FULL_BODY_EXTENSION.as_ptr());
        }
        let info = InstanceInfo {
            app: AppInfo {
                name: fixed_name(c"Celsius Baseplate"),
                version: 1,
                engine: fixed_name(c"Celsius Engine"),
                engine_version: 1,
                api_version: 1u64 << 48,
            },
            extension_count: enabled.len() as u32,
            extensions: enabled.as_ptr(),
            ..Default::default()
        };
        let mut raw = 0;
        eprintln!("OpenXR: creating the runtime instance.");
        check(
            unsafe { (global.xrCreateInstance)(&info, &mut raw) },
            "xrCreateInstance",
        )?;
        let lifecycle = InstanceLifecycle::load(get, raw)?;
        let instance = OwnedHandle {
            raw,
            destroy: lifecycle.xrDestroyInstance,
        };
        let api = Api::load(get, raw)?;
        let mut system = 0;
        let info = SystemInfo {
            form_factor: HMD,
            ..Default::default()
        };
        eprintln!("OpenXR: looking for a headset.");
        check(
            unsafe { (api.xrGetSystem)(raw, &info, &mut system) },
            "xrGetSystem",
        )?;
        let mut hand = HandProperties::default();
        let mut body = BodyProperties::default();
        let mut full_body_properties = FullBodyProperties::default();
        let mut properties = SystemProperties::default();
        if hand_enabled {
            properties.next = ptr::from_mut(&mut hand).cast();
        }
        if body_enabled {
            body.next = properties.next;
            properties.next = ptr::from_mut(&mut body).cast();
        }
        if full_body_enabled {
            full_body_properties.next = properties.next;
            properties.next = ptr::from_mut(&mut full_body_properties).cast();
        }
        check(
            unsafe { (api.xrGetSystemProperties)(raw, system, &mut properties) },
            "xrGetSystemProperties",
        )?;
        let hand_api = (hand_enabled && hand.supported != 0)
            .then(|| HandApi::load(get, raw))
            .transpose()?;
        let body_api = (body_enabled && body.supported != 0)
            .then(|| BodyApi::load(get, raw))
            .transpose()?;
        let full_body =
            body_api.is_some() && full_body_enabled && full_body_properties.supported != 0;
        let system_name = unsafe { CStr::from_ptr(properties.name.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        Ok(Self {
            instance,
            _loader: loader,
            api,
            system,
            system_name,
            hand_api,
            body_api,
            full_body,
        })
    }

    fn session(&self, device: &ID3D11Device) -> Result<OwnedHandle> {
        let mut binding = D3dBinding {
            device: device.as_raw(),
            ..Default::default()
        };
        let info = SessionInfo {
            system: self.system,
            next: ptr::from_mut(&mut binding).cast(),
            ..Default::default()
        };
        let mut raw = 0;
        check(
            unsafe { (self.api.xrCreateSession)(self.instance.raw, &info, &mut raw) },
            "xrCreateSession",
        )?;
        Ok(OwnedHandle {
            raw,
            destroy: self.api.xrDestroySession,
        })
    }
}

impl EyeTarget {
    fn new(
        api: &Api,
        session: Handle,
        renderer: &Renderer,
        config: ViewConfig,
        format: i64,
        scale: f32,
    ) -> Result<Self> {
        let width = ((config.width as f32 * scale).round() as u32).max(1);
        let height = ((config.height as f32 * scale).round() as u32).max(1);
        let info = SwapchainInfo {
            usage: COLOR_ATTACHMENT,
            format,
            samples: 1,
            width,
            height,
            faces: 1,
            array_size: 1,
            mip_count: 1,
            ..Default::default()
        };
        let mut raw = 0;
        check(
            unsafe { (api.xrCreateSwapchain)(session, &info, &mut raw) },
            "xrCreateSwapchain",
        )?;
        let swapchain = OwnedHandle {
            raw,
            destroy: api.xrDestroySwapchain,
        };
        let images = enumerate(
            D3dImage::default(),
            |capacity, count, data| unsafe {
                (api.xrEnumerateSwapchainImages)(raw, capacity, count, data)
            },
            "xrEnumerateSwapchainImages",
        )?;
        let textures = images
            .iter()
            .map(|image| unsafe { d3d11::borrow_image(image.texture) })
            .collect::<Option<Vec<_>>>()
            .ok_or("OpenXR returned a null D3D11 swapchain texture")?;
        let target = renderer.target(&textures, width, height, DXGI_FORMAT(format.try_into()?))?;
        let image = SubImage {
            swapchain: raw,
            width: width.try_into()?,
            height: height.try_into()?,
            ..Default::default()
        };
        Ok(Self {
            swapchain,
            target,
            image,
        })
    }

    fn render(&self, api: &Api, renderer: &Renderer, camera: Camera, clouds: f32) -> Result<()> {
        let mut index = 0;
        let raw = self.swapchain.raw;
        check(
            unsafe { (api.xrAcquireSwapchainImage)(raw, &ImageAcquire::default(), &mut index) },
            "xrAcquireSwapchainImage",
        )?;
        let wait = ImageWait {
            timeout: i64::MAX,
            ..Default::default()
        };
        check(
            unsafe { (api.xrWaitSwapchainImage)(raw, &wait) },
            "xrWaitSwapchainImage",
        )?;
        let draw = renderer.draw(&self.target, index as usize, camera, clouds);
        let release = unsafe { (api.xrReleaseSwapchainImage)(raw, &ImageRelease::default()) };
        draw?;
        check(release, "xrReleaseSwapchainImage")
    }
}

fn enumerate<T: Copy>(
    default: T,
    mut call: impl FnMut(u32, *mut u32, *mut T) -> Code,
    name: &str,
) -> Result<Vec<T>> {
    let mut count = 0;
    check(call(0, &mut count, ptr::null_mut()), name)?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut values = vec![default; count as usize];
    check(call(count, &mut count, values.as_mut_ptr()), name)?;
    values.truncate(count as usize);
    Ok(values)
}

fn fixed_name(name: &CStr) -> [c_char; 128] {
    let mut out = [0; 128];
    for (out, byte) in out.iter_mut().zip(name.to_bytes()) {
        *out = *byte as c_char;
    }
    out
}

fn try_space(api: &Api, session: Handle, kind: i32) -> (Code, Handle) {
    let info = SpaceInfo {
        kind,
        pose: RawPose::IDENTITY,
        ..Default::default()
    };
    let mut raw = 0;
    let code = unsafe { (api.xrCreateReferenceSpace)(session, &info, &mut raw) };
    (code, raw)
}

fn reference_space(api: &Api, session: Handle, kind: i32) -> Result<OwnedHandle> {
    let (code, raw) = try_space(api, session, kind);
    check(code, "xrCreateReferenceSpace")?;
    Ok(OwnedHandle {
        raw,
        destroy: api.xrDestroySpace,
    })
}

fn create_hand(api: &HandApi, session: Handle, hand: i32) -> Result<OwnedHandle> {
    let mut raw = 0;
    let info = HandInfo {
        hand,
        joint_set: 0,
        ..Default::default()
    };
    check(
        unsafe { (api.xrCreateHandTrackerEXT)(session, &info, &mut raw) },
        "xrCreateHandTrackerEXT",
    )?;
    Ok(OwnedHandle {
        raw,
        destroy: api.xrDestroyHandTrackerEXT,
    })
}

fn sample_hand(
    api: &HandApi,
    hand: Handle,
    space: Handle,
    time: Time,
    shift: f32,
) -> Result<HandFrame> {
    let mut joints = [Joint {
        flags: 0,
        pose: RawPose::IDENTITY,
        radius: 0.0,
    }; 26];
    let info = HandLocate {
        space,
        time,
        ..Default::default()
    };
    let mut out = HandLocations {
        count: joints.len() as u32,
        joints: joints.as_mut_ptr(),
        ..Default::default()
    };
    check(
        unsafe { (api.xrLocateHandJointsEXT)(hand, &info, &mut out) },
        "xrLocateHandJointsEXT",
    )?;
    Ok(if out.active != 0 {
        HandFrame(
            joints.map(|joint| {
                (joint.flags & VALID_POSE == VALID_POSE).then(|| pose(joint.pose, shift))
            }),
        )
    } else {
        HandFrame::default()
    })
}

fn pose(raw: RawPose, shift: f32) -> Pose {
    Pose {
        position: Vec3::from_array(raw.position) + Vec3::Y * shift,
        orientation: Quat::from_array(raw.rotation).normalize(),
    }
}

fn create_body(api: &BodyApi, session: Handle, full_body: bool) -> Result<OwnedHandle> {
    let mut raw = 0;
    let info = BodyInfo {
        joint_set: if full_body { FULL_BODY_JOINT_SET } else { 0 },
        ..Default::default()
    };
    check(
        unsafe { (api.xrCreateBodyTrackerFB)(session, &info, &mut raw) },
        "xrCreateBodyTrackerFB",
    )?;
    Ok(OwnedHandle {
        raw,
        destroy: api.xrDestroyBodyTrackerFB,
    })
}

fn sample_body(
    api: &BodyApi,
    body: Handle,
    space: Handle,
    time: Time,
    shift: f32,
    full_body: bool,
) -> Result<(BodyFrame, [HandFrame; 2])> {
    let mut joints = [BodyJoint {
        flags: 0,
        pose: RawPose::IDENTITY,
    }; 84];
    let info = BodyLocate {
        space,
        time,
        ..Default::default()
    };
    let mut out = BodyLocations {
        count: if full_body { 84 } else { 70 },
        joints: joints.as_mut_ptr(),
        ..Default::default()
    };
    check(
        unsafe { (api.xrLocateBodyJointsFB)(body, &info, &mut out) },
        "xrLocateBodyJointsFB",
    )?;
    if out.active == 0 || out.confidence < 0.45 {
        return Ok(Default::default());
    }
    let position = |i: usize| {
        let joint = &joints[i];
        (joint.flags & VALID_POSITION != 0)
            .then(|| Vec3::from_array(joint.pose.position) + Vec3::Y * shift)
    };
    // The 70 upper-body indices are identical for XR_FB_body_tracking and the META full-body set.
    let frame = BodyFrame {
        chest: position(5),
        hips: position(1),
        arms: [[10, 11, 19].map(position), [15, 16, 45].map(position)],
        legs: [[70, 71, 73].map(position), [77, 78, 80].map(position)],
    };
    let hands = [18, 44].map(|first| {
        HandFrame(std::array::from_fn(|i| {
            let joint = joints[first + i];
            (joint.flags & VALID_POSE == VALID_POSE).then(|| pose(joint.pose, shift))
        }))
    });
    Ok((frame, hands))
}
