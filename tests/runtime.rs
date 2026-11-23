use std::time::Duration;

use celsius_engine::{scripting::ScriptRuntime, timing::FrameClock};
use mlua::Function;

#[test]
fn script_exports_can_be_called_from_rust() -> mlua::Result<()> {
    let runtime = ScriptRuntime::new()?;
    let module = runtime.load_module(
        "@progression.lua",
        include_str!("../examples/progression.lua"),
    )?;
    let award_xp: Function = module.get("award_xp")?;
    assert_eq!(award_xp.call::<(u32, u32)>((80, 25))?, (105, 2));
    Ok(())
}

#[test]
fn bare_module_globals_stay_local() -> mlua::Result<()> {
    let runtime = ScriptRuntime::new()?;
    runtime.load_module("@first.lua", "counter = 1; return {}")?;
    let second = runtime.load_module("@second.lua", "return { counter = counter or 0 }")?;
    assert_eq!(second.get::<u32>("counter")?, 0);
    Ok(())
}

#[test]
fn frame_clock_carries_partial_steps_and_caps_catch_up() {
    let step = Duration::from_millis(10);
    let mut clock = FrameClock::new(step, 3).unwrap();
    let partial = clock.advance(Duration::from_millis(5));
    assert_eq!(partial.simulation_steps, 0);
    assert_eq!(partial.interpolation_alpha, 0.5);
    assert_eq!(clock.advance(Duration::from_millis(5)).simulation_steps, 1);
    let stalled = clock.advance(Duration::from_millis(80));
    assert_eq!(stalled.simulation_steps, 3);
    assert_eq!(stalled.dropped_time, Duration::from_millis(50));
    assert_eq!(clock.advance(Duration::ZERO).simulation_steps, 0);
}
