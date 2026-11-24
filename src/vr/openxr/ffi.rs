#![allow(non_snake_case)]

use std::{
    ffi::{CStr, c_char, c_void},
    ptr,
};
use windows::Win32::{Foundation::LUID, Graphics::Direct3D::D3D_FEATURE_LEVEL};

pub type Handle = u64;
pub type Time = i64;
pub type Code = i32;
type Flags = u64;

pub const PRIMARY_STEREO: i32 = 2;
pub const HMD: i32 = 1;
pub const VIEW_SPACE: i32 = 1;
pub const LOCAL_SPACE: i32 = 2;
pub const STAGE_SPACE: i32 = 3;
pub const OPAQUE: i32 = 1;
pub const COLOR_ATTACHMENT: Flags = 1;
pub const VALID_POSE: Flags = 3;
pub const VALID_POSITION: Flags = 2;
pub const TRACKED_POSE: Flags = 15;
pub const EVENT_UNAVAILABLE: Code = 4;
pub const PATH_UNSUPPORTED: Code = -22;
pub const POSE_INPUT: i32 = 4;
pub const SPACE_UNSUPPORTED: Code = -31;
pub const STATE_READY: i32 = 2;
pub const STATE_FOCUSED: i32 = 5;
pub const STATE_STOPPING: i32 = 6;
pub const STATE_LOSS_PENDING: i32 = 7;
pub const STATE_EXITING: i32 = 8;
pub const EVENT_INSTANCE_LOSS: i32 = 17;
pub const EVENT_SESSION_STATE: i32 = 18;
pub const EVENT_REFERENCE_CHANGE: i32 = 40;

pub const D3D11_EXTENSION: &CStr = c"XR_KHR_D3D11_enable";
pub const HAND_EXTENSION: &CStr = c"XR_EXT_hand_tracking";
pub const BODY_EXTENSION: &CStr = c"XR_FB_body_tracking";
pub const FULL_BODY_EXTENSION: &CStr = c"XR_META_body_tracking_full_body";
pub const FULL_BODY_JOINT_SET: i32 = 1000274000;

// Subset of the Khronos OpenXR 1.0 ABI for 64-bit Windows.
macro_rules! xr_struct {
    ($name:ident, $tag:expr, $($field:ident: $ty:ty),* $(,)?) => {
        #[repr(C)]
        #[derive(Clone, Copy)]
        pub struct $name {
            pub ty: i32,
            pub next: *mut c_void,
            $(pub $field: $ty,)*
        }
        impl Default for $name {
            fn default() -> Self {
                Self {
                    ty: $tag,
                    next: ptr::null_mut(),
                    // These C fields are exclusively numeric types, raw pointers or POD records.
                    $($field: unsafe { std::mem::zeroed() },)*
                }
            }
        }
    };
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AppInfo {
    pub name: [c_char; 128],
    pub version: u32,
    pub engine: [c_char; 128],
    pub engine_version: u32,
    pub api_version: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawPose {
    pub rotation: [f32; 4],
    pub position: [f32; 3],
}

impl RawPose {
    pub const IDENTITY: Self = Self {
        rotation: [0.0, 0.0, 0.0, 1.0],
        position: [0.0; 3],
    };
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SubImage {
    pub swapchain: Handle,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub array_index: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Joint {
    pub flags: Flags,
    pub pose: RawPose,
    pub radius: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BodyJoint {
    pub flags: Flags,
    pub pose: RawPose,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ActionBinding {
    pub action: Handle,
    pub binding: Handle,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ActiveActionSet {
    pub action_set: Handle,
    pub subaction: Handle,
}

xr_struct!(Extension, 2, name: [c_char; 128], version: u32);
xr_struct!(InstanceInfo, 3, flags: Flags, app: AppInfo, layer_count: u32, layers: *const *const c_char, extension_count: u32, extensions: *const *const c_char);
xr_struct!(SystemInfo, 4, form_factor: i32);
xr_struct!(SystemProperties, 5, system: Handle, vendor: u32, name: [c_char; 256], graphics: [u32; 3], tracking: [u32; 2]);
xr_struct!(ViewInfo, 6, config: i32, time: Time, space: Handle);
xr_struct!(View, 7, pose: RawPose, fov: [f32; 4]);
xr_struct!(SessionInfo, 8, flags: Flags, system: Handle);
xr_struct!(SwapchainInfo, 9, flags: Flags, usage: Flags, format: i64, samples: u32, width: u32, height: u32, faces: u32, array_size: u32, mip_count: u32);
xr_struct!(SessionBegin, 10, config: i32);
xr_struct!(ViewState, 11, flags: Flags);
xr_struct!(FrameEnd, 12, time: Time, blend: i32, count: u32, layers: *const *const Projection);
xr_struct!(Event, 16, payload: [u8; 4000]);
xr_struct!(SessionStateEvent, 18, session: Handle, state: i32, time: Time);
xr_struct!(ActionSetInfo, 28, name: [c_char; 64], localized_name: [c_char; 128], priority: u32);
xr_struct!(ActionInfo, 29, name: [c_char; 64], kind: i32, subaction_count: u32, subactions: *const Handle, localized_name: [c_char; 128]);
xr_struct!(FrameWait, 33,);
xr_struct!(Projection, 35, flags: Flags, space: Handle, view_count: u32, views: *const ProjectionView);
xr_struct!(SpaceInfo, 37, kind: i32, pose: RawPose);
xr_struct!(ActionSpaceInfo, 38, action: Handle, subaction: Handle, pose: RawPose);
xr_struct!(ViewConfig, 41, width: u32, max_width: u32, height: u32, max_height: u32, samples: u32, max_samples: u32);
xr_struct!(SpaceLocation, 42, flags: Flags, pose: RawPose);
xr_struct!(FrameState, 44, time: Time, period: i64, should_render: u32);
xr_struct!(FrameBegin, 46,);
xr_struct!(ProjectionView, 48, pose: RawPose, fov: [f32; 4], image: SubImage);
xr_struct!(SuggestedBindings, 51, profile: Handle, count: u32, bindings: *const ActionBinding);
xr_struct!(ImageAcquire, 55,);
xr_struct!(ImageWait, 56, timeout: i64);
xr_struct!(ImageRelease, 57,);
xr_struct!(AttachActionSets, 60, count: u32, sets: *const Handle);
xr_struct!(SyncActions, 61, count: u32, sets: *const ActiveActionSet);
xr_struct!(D3dBinding, 1000027000, device: *mut c_void);
xr_struct!(D3dImage, 1000027001, texture: *mut c_void);
xr_struct!(D3dRequirements, 1000027002, luid: LUID, feature_level: D3D_FEATURE_LEVEL);
xr_struct!(HandProperties, 1000051000, supported: u32);
xr_struct!(HandInfo, 1000051001, hand: i32, joint_set: i32);
xr_struct!(HandLocate, 1000051002, space: Handle, time: Time);
xr_struct!(HandLocations, 1000051003, active: u32, count: u32, joints: *mut Joint);
xr_struct!(BodyInfo, 1000076001, joint_set: i32);
xr_struct!(BodyLocate, 1000076002, space: Handle, time: Time);
xr_struct!(BodyProperties, 1000076004, supported: u32);
xr_struct!(BodyLocations, 1000076005, active: u32, confidence: f32, count: u32, joints: *mut BodyJoint, skeleton_changes: u32, time: Time);
xr_struct!(FullBodyProperties, 1000274000, supported: u32);

pub type GetProc = unsafe extern "system" fn(Handle, *const c_char, *mut *mut c_void) -> Code;
pub type Destroy = unsafe extern "system" fn(Handle) -> Code;

pub fn check(code: Code, operation: &str) -> super::Result<()> {
    if code < 0 {
        let description = match code {
            -51 => " (runtime unavailable)",
            -35 => " (HMD unavailable. Connect the headset and enter the active OpenXR runtime.)",
            -6 => " (initialization failed)",
            _ => "",
        };
        return Err(format!("{operation} returned OpenXR {code}{description}").into());
    }
    Ok(())
}

macro_rules! xr_functions {
    ($struct:ident {$($name:ident($($ty:ty),*)),+ $(,)?}) => {
        pub struct $struct {
            $(pub $name: unsafe extern "system" fn($($ty),*) -> Code,)+
        }
        impl $struct {
            pub fn load(get: GetProc, instance: Handle) -> super::Result<Self> {
                Ok(Self {
                    $($name: {
                        let name = concat!(stringify!($name), "\0");
                        let mut raw = ptr::null_mut();
                        check(unsafe { get(instance, name.as_ptr().cast(), &mut raw) }, stringify!($name))?;
                        if raw.is_null() {
                            return Err(concat!("OpenXR returned a null function: ", stringify!($name)).into());
                        }
                        // get returns the exact function paired with the C ABI signature above.
                        unsafe { std::mem::transmute::<*mut c_void, unsafe extern "system" fn($($ty),*) -> Code>(raw) }
                    },)+
                })
            }
        }
    };
}

xr_functions!(Global {
    xrEnumerateInstanceExtensionProperties(*const c_char, u32, *mut u32, *mut Extension),
    xrCreateInstance(*const InstanceInfo, *mut Handle),
});

xr_functions!(InstanceLifecycle {
    xrDestroyInstance(Handle),
});

xr_functions!(Api {
    xrGetSystem(Handle, *const SystemInfo, *mut Handle),
    xrGetSystemProperties(Handle, Handle, *mut SystemProperties),
    xrGetD3D11GraphicsRequirementsKHR(Handle, Handle, *mut D3dRequirements),
    xrEnumerateEnvironmentBlendModes(Handle, Handle, i32, u32, *mut u32, *mut i32),
    xrEnumerateViewConfigurationViews(Handle, Handle, i32, u32, *mut u32, *mut ViewConfig),
    xrCreateSession(Handle, *const SessionInfo, *mut Handle),
    xrDestroySession(Handle),
    xrBeginSession(Handle, *const SessionBegin),
    xrEndSession(Handle),
    xrPollEvent(Handle, *mut Event),
    xrCreateReferenceSpace(Handle, *const SpaceInfo, *mut Handle),
    xrDestroySpace(Handle),
    xrLocateSpace(Handle, Handle, Time, *mut SpaceLocation),
    xrStringToPath(Handle, *const c_char, *mut Handle),
    xrCreateActionSet(Handle, *const ActionSetInfo, *mut Handle),
    xrDestroyActionSet(Handle),
    xrCreateAction(Handle, *const ActionInfo, *mut Handle),
    xrSuggestInteractionProfileBindings(Handle, *const SuggestedBindings),
    xrCreateActionSpace(Handle, *const ActionSpaceInfo, *mut Handle),
    xrAttachSessionActionSets(Handle, *const AttachActionSets),
    xrSyncActions(Handle, *const SyncActions),
    xrEnumerateSwapchainFormats(Handle, u32, *mut u32, *mut i64),
    xrCreateSwapchain(Handle, *const SwapchainInfo, *mut Handle),
    xrDestroySwapchain(Handle),
    xrEnumerateSwapchainImages(Handle, u32, *mut u32, *mut D3dImage),
    xrAcquireSwapchainImage(Handle, *const ImageAcquire, *mut u32),
    xrWaitSwapchainImage(Handle, *const ImageWait),
    xrReleaseSwapchainImage(Handle, *const ImageRelease),
    xrWaitFrame(Handle, *const FrameWait, *mut FrameState),
    xrBeginFrame(Handle, *const FrameBegin),
    xrEndFrame(Handle, *const FrameEnd),
    xrLocateViews(Handle, *const ViewInfo, *mut ViewState, u32, *mut u32, *mut View),
});

xr_functions!(HandApi {
    xrCreateHandTrackerEXT(Handle, *const HandInfo, *mut Handle),
    xrDestroyHandTrackerEXT(Handle),
    xrLocateHandJointsEXT(Handle, *const HandLocate, *mut HandLocations),
});

xr_functions!(BodyApi {
    xrCreateBodyTrackerFB(Handle, *const BodyInfo, *mut Handle),
    xrDestroyBodyTrackerFB(Handle),
    xrLocateBodyJointsFB(Handle, *const BodyLocate, *mut BodyLocations),
});
