use mlua::chunk::ChunkMode;
use mlua::{Lua, LuaOptions, Result, StdLib, Table, Value};

mod scene;
pub use scene::LuaScene;

/// Hosts gameplay scripts authored by the game developer.
pub struct ScriptRuntime {
    lua: Lua,
}

impl ScriptRuntime {
    pub fn new() -> Result<Self> {
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
            LuaOptions::default(),
        )?;
        for name in ["dofile", "loadfile"] {
            lua.globals().raw_set(name, Value::Nil)?;
        }
        Ok(Self { lua })
    }

    /// Evaluates a source module and returns its exported table.
    pub fn load_module(&self, name: &str, source: &str) -> Result<Table> {
        let environment = self.lua.create_table()?;
        let metatable = self.lua.create_table()?;
        metatable.raw_set("__index", self.lua.globals())?;
        environment.set_metatable(Some(metatable))?;
        environment.raw_set("_G", environment.clone())?;

        self.lua
            .load(source)
            .set_name(name)
            .set_mode(ChunkMode::Text)
            .set_environment(environment)
            .eval()
    }
}
