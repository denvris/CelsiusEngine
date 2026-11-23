use mlua::{Function, IntoLua, Lua, Result, Table, Value};

#[derive(Clone, Copy, Debug)]
pub enum Hand {
    Left,
    Right,
}

impl Hand {
    fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// A joint with valid tracking-space position (meters) and orientation (x, y, z, w).
pub struct JointPose<'a> {
    pub name: &'a str,
    pub position: [f32; 3],
    pub orientation: [f32; 4],
}

pub struct VrCallbacks {
    hand_pose: Option<Function>,
    hand_gesture: Option<Function>,
    locomotion_step: Option<Function>,
}

impl VrCallbacks {
    pub fn from_module(module: &Table) -> Result<Self> {
        Ok(Self {
            hand_pose: module.raw_get("on_hand_pose")?,
            hand_gesture: module.raw_get("process_hand_gesture")?,
            locomotion_step: module.raw_get("on_locomotion_step")?,
        })
    }

    /// Dispatches valid joints by name. An empty slice means there is no usable pose for this hand.
    pub fn hand_pose(&self, hand: Hand, joints: &[JointPose<'_>]) -> Result<()> {
        if let Some(callback) = &self.hand_pose {
            callback.call::<()>((hand.as_str(), JointTable(joints)))?;
        }
        Ok(())
    }

    pub fn hand_gesture(&self, gesture: &str, hand: Hand) -> Result<()> {
        if let Some(callback) = &self.hand_gesture {
            callback.call::<()>((gesture, hand.as_str()))?;
        }
        Ok(())
    }

    pub fn locomotion_step(&self) -> Result<()> {
        if let Some(callback) = &self.locomotion_step {
            callback.call::<()>(())?;
        }
        Ok(())
    }
}

struct JointTable<'a>(&'a [JointPose<'a>]);

impl IntoLua for JointTable<'_> {
    fn into_lua(self, lua: &Lua) -> Result<Value> {
        let joints = lua.create_table()?;
        for joint in self.0 {
            let pose = lua.create_table()?;
            pose.raw_set("position", joint.position)?;
            pose.raw_set("orientation", joint.orientation)?;
            joints.raw_set(joint.name, pose)?;
        }
        Ok(Value::Table(joints))
    }
}
