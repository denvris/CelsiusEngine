use std::time::Duration;

use celsius_engine::{scripting::ScriptRuntime, timing::FrameClock};
use mlua::Function;

fn main() -> mlua::Result<()> {
    let scripts = ScriptRuntime::new()?;
    let progression = scripts.load_module("@progression.lua", include_str!("progression.lua"))?;
    let award_xp: Function = progression.get("award_xp")?;

    let step = Duration::from_secs_f64(1.0 / 90.0);
    let mut clock = FrameClock::new(step, 4).expect("valid frame clock settings");
    let mut total_xp = 80;
    let mut level = 1;
    for _ in 0..clock.advance(step * 2).simulation_steps {
        (total_xp, level) = award_xp.call((total_xp, 15))?;
    }

    println!("Progression module returned {total_xp} XP and level {level}");
    Ok(())
}
