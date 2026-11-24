use std::{ffi::CStr, ptr};

use crate::vr::Pose;

use super::{OwnedHandle, Result, ffi::*, fixed_name, pose};

pub(super) struct GripInput {
    spaces: [OwnedHandle; 2],
    set: OwnedHandle,
}

impl GripInput {
    pub fn new(api: &Api, instance: Handle, session: Handle) -> Result<Self> {
        let paths = [
            path(api, instance, c"/user/hand/left")?,
            path(api, instance, c"/user/hand/right")?,
        ];
        let info = ActionSetInfo {
            name: fixed_name(c"grip_tracking"),
            localized_name: fixed_name(c"Grip tracking"),
            ..Default::default()
        };
        let mut raw = 0;
        check(
            unsafe { (api.xrCreateActionSet)(instance, &info, &mut raw) },
            "xrCreateActionSet",
        )?;
        let set = OwnedHandle {
            raw,
            destroy: api.xrDestroyActionSet,
        };
        let info = ActionInfo {
            name: fixed_name(c"controller_grip"),
            kind: POSE_INPUT,
            subaction_count: paths.len() as u32,
            subactions: paths.as_ptr(),
            localized_name: fixed_name(c"Controller grip"),
            ..Default::default()
        };
        let mut action = 0;
        check(
            unsafe { (api.xrCreateAction)(set.raw, &info, &mut action) },
            "xrCreateAction",
        )?;

        let bindings = [
            ActionBinding {
                action,
                binding: path(api, instance, c"/user/hand/left/input/grip/pose")?,
            },
            ActionBinding {
                action,
                binding: path(api, instance, c"/user/hand/right/input/grip/pose")?,
            },
        ];
        for profile in [
            c"/interaction_profiles/oculus/touch_controller",
            c"/interaction_profiles/khr/simple_controller",
        ] {
            let info = SuggestedBindings {
                profile: path(api, instance, profile)?,
                count: bindings.len() as u32,
                bindings: bindings.as_ptr(),
                ..Default::default()
            };
            let code = unsafe { (api.xrSuggestInteractionProfileBindings)(instance, &info) };
            if code != PATH_UNSUPPORTED {
                check(code, "xrSuggestInteractionProfileBindings")?;
            }
        }

        let space = |subaction| -> Result<_> {
            let info = ActionSpaceInfo {
                action,
                subaction,
                pose: RawPose::IDENTITY,
                ..Default::default()
            };
            let mut raw = 0;
            check(
                unsafe { (api.xrCreateActionSpace)(session, &info, &mut raw) },
                "xrCreateActionSpace",
            )?;
            Ok(OwnedHandle {
                raw,
                destroy: api.xrDestroySpace,
            })
        };
        let spaces = [space(paths[0])?, space(paths[1])?];
        let attach = AttachActionSets {
            count: 1,
            sets: ptr::from_ref(&set.raw),
            ..Default::default()
        };
        check(
            unsafe { (api.xrAttachSessionActionSets)(session, &attach) },
            "xrAttachSessionActionSets",
        )?;
        Ok(Self { spaces, set })
    }

    pub fn sample(
        &self,
        api: &Api,
        session: Handle,
        base: Handle,
        time: Time,
        shift: f32,
    ) -> Result<[Option<Pose>; 2]> {
        let active = ActiveActionSet {
            action_set: self.set.raw,
            subaction: 0,
        };
        let sync = SyncActions {
            count: 1,
            sets: ptr::from_ref(&active),
            ..Default::default()
        };
        let code = unsafe { (api.xrSyncActions)(session, &sync) };
        check(code, "xrSyncActions")?;
        let mut grips = [None; 2];
        if code == 0 {
            for (grip, space) in grips.iter_mut().zip(&self.spaces) {
                let mut located = SpaceLocation::default();
                check(
                    unsafe { (api.xrLocateSpace)(space.raw, base, time, &mut located) },
                    "xrLocateSpace (controller)",
                )?;
                if located.flags & VALID_POSE == VALID_POSE {
                    *grip = Some(pose(located.pose, shift));
                }
            }
        }
        Ok(grips)
    }
}

fn path(api: &Api, instance: Handle, name: &CStr) -> Result<Handle> {
    let mut raw = 0;
    check(
        unsafe { (api.xrStringToPath)(instance, name.as_ptr(), &mut raw) },
        "xrStringToPath",
    )?;
    Ok(raw)
}
