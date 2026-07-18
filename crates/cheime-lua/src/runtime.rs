use cheime_extension::{
    Extension, ExtensionCandidate, ExtensionContext, ExtensionError, ExtensionOutput, Filter,
    Processor, Segment, Segmentor, Translator,
};
use cheime_model::{Candidate, CandidateId, Key, KeyEvent};
use mlua::{Function, Lua, Table, Value};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Capability {
    Clock,
    ConfigRead,
    ClipboardRead,
    ClipboardWrite,
    Network(String),
    Filesystem(String),
    ProcessSpawn,
    UserDictionaryWrite,
    SurroundingTextRead,
    AppIdentityRead,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum LuaError {
    #[error("Lua error: {0}")]
    Runtime(String),
    #[error("permission denied: {0:?}")]
    PermissionDenied(Capability),
    #[error("script not loaded: {0}")]
    NotLoaded(String),
}

impl From<mlua::Error> for LuaError {
    fn from(e: mlua::Error) -> Self {
        LuaError::Runtime(e.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct ScriptEntry {
    pub capabilities: HashSet<Capability>,
}

pub struct LuaRuntime {
    lua: Arc<Mutex<Lua>>,
    scripts: Vec<(String, ScriptEntry)>,
}

fn value_to_table(v: Value) -> Result<Table, ExtensionError> {
    match v {
        Value::Table(t) => Ok(t),
        other => Err(ExtensionError::Runtime(format!(
            "expected table, got {:?}",
            other.type_name()
        ))),
    }
}

impl LuaRuntime {
    pub fn new() -> Result<Self, LuaError> {
        Ok(Self {
            lua: Arc::new(Mutex::new(Lua::new())),
            scripts: Vec::new(),
        })
    }

    pub fn load_script(
        &mut self,
        name: &str,
        source: &str,
        capabilities: &[Capability],
    ) -> Result<(), LuaError> {
        let lua = self.lua.lock().unwrap();
        lua.load(source).set_name(name).exec()?;
        drop(lua);
        self.scripts.push((
            name.to_owned(),
            ScriptEntry {
                capabilities: capabilities.iter().cloned().collect(),
            },
        ));
        Ok(())
    }

    #[allow(dead_code)]
    pub fn check_capability(&self, name: &str, required: &Capability) -> Result<(), LuaError> {
        for (n, entry) in &self.scripts {
            if n == name {
                if entry.capabilities.contains(required) {
                    return Ok(());
                }
                return Err(LuaError::PermissionDenied(required.clone()));
            }
        }
        Err(LuaError::NotLoaded(name.to_owned()))
    }

    #[allow(dead_code)]
    pub fn has_cap(&self, name: &str, cap: &Capability) -> bool {
        self.scripts
            .iter()
            .any(|(n, entry)| n == name && entry.capabilities.contains(cap))
    }

    #[allow(dead_code)]
    fn inner(&self) -> std::sync::MutexGuard<'_, Lua> {
        self.lua.lock().unwrap()
    }

    fn call_fn(
        lua: &Lua,
        module_name: &str,
        fn_name: &str,
        args: Table,
    ) -> Result<Value, LuaError> {
        let globals = lua.globals();
        let module: Table = globals
            .get(module_name)
            .map_err(|_| LuaError::NotLoaded(module_name.to_owned()))?;
        let func: Function = module
            .get(fn_name)
            .map_err(|_| LuaError::Runtime(format!("{module_name}.{fn_name} not found")))?;
        Ok(func.call::<Value>(args)?)
    }

    fn build_env(
        lua: &Lua,
        name: &str,
        cap_check: &dyn Fn(&str, &Capability) -> bool,
        ctx: &ExtensionContext,
    ) -> Result<Table, LuaError> {
        let env = lua.create_table()?;
        env.set("composition", ctx.composition)?;
        env.set("schema_id", ctx.schema_id)?;

        let caps = lua.create_table()?;
        caps.set("clock", cap_check(name, &Capability::Clock))?;
        caps.set("config_read", cap_check(name, &Capability::ConfigRead))?;
        env.set("capabilities", caps)?;

        Ok(env)
    }
}

// ---- Extension implementations ----

pub fn make_lua_extension(
    lua: Arc<Mutex<Lua>>,
    scripts: Arc<Vec<(String, ScriptEntry)>>,
    name: &str,
) -> Box<dyn Extension> {
    Box::new(LuaExtension {
        lua,
        scripts,
        name: name.to_owned(),
    })
}

struct LuaExtension {
    lua: Arc<Mutex<Lua>>,
    scripts: Arc<Vec<(String, ScriptEntry)>>,
    name: String,
}

impl LuaExtension {
    fn find_caps(&self) -> Option<&HashSet<Capability>> {
        self.scripts
            .iter()
            .find(|(n, _)| n == &self.name)
            .map(|(_, e)| &e.capabilities)
    }

    fn has_cap(&self, cap: &Capability) -> bool {
        self.find_caps()
            .map(|caps| caps.contains(cap))
            .unwrap_or(false)
    }
}

impl Extension for LuaExtension {
    fn name(&self) -> &str {
        &self.name
    }

    fn processor(&self) -> Option<&dyn Processor> {
        Some(self)
    }

    fn translator(&self) -> Option<&dyn Translator> {
        Some(self)
    }

    fn filter(&self) -> Option<&dyn Filter> {
        Some(self)
    }

    fn segmentor(&self) -> Option<&dyn Segmentor> {
        Some(self)
    }
}

impl Processor for LuaExtension {
    fn process(
        &self,
        ctx: &ExtensionContext,
        key: &KeyEvent,
    ) -> Result<ExtensionOutput, ExtensionError> {
        let lua = self.lua.lock().unwrap();
        let env = LuaRuntime::build_env(&lua, &self.name, &|_, c| self.has_cap(c), ctx)
            .map_err(|e| ExtensionError::Runtime(e.to_string()))?;

        let key_str = match key.key {
            Key::Character(c) => c.to_string(),
            Key::Backspace => "BackSpace".into(),
            Key::Escape => "Escape".into(),
            Key::Enter => "Return".into(),
            Key::Space => "space".into(),
        };
        env.set("key", key_str.as_str())
            .map_err(|e| ExtensionError::Runtime(e.to_string()))?;

        let raw = LuaRuntime::call_fn(&lua, &self.name, "process", env)
            .map_err(|e| ExtensionError::Runtime(e.to_string()))?;
        let result = value_to_table(raw)?;

        let handled: bool = result.get("handled").unwrap_or(false);
        let composition: String = result.get("composition").unwrap_or_default();

        Ok(ExtensionOutput::Processor {
            handled,
            composition,
        })
    }
}

impl Translator for LuaExtension {
    fn translate(&self, ctx: &ExtensionContext) -> Result<ExtensionOutput, ExtensionError> {
        let lua = self.lua.lock().unwrap();
        let env = LuaRuntime::build_env(&lua, &self.name, &|_, c| self.has_cap(c), ctx)
            .map_err(|e| ExtensionError::Runtime(e.to_string()))?;

        let raw = LuaRuntime::call_fn(&lua, &self.name, "translate", env)
            .map_err(|e| ExtensionError::Runtime(e.to_string()))?;
        let result = value_to_table(raw)?;

        let candidates_array: Vec<Table> = result.get("candidates").unwrap_or_default();
        let candidates: Vec<ExtensionCandidate> = candidates_array
            .into_iter()
            .map(|t| ExtensionCandidate {
                text: t.get("text").unwrap_or_default(),
                code: t.get("code").unwrap_or_default(),
                weight: t.get("weight").ok(),
                annotation: t.get("annotation").ok(),
            })
            .collect();

        Ok(ExtensionOutput::Translator { candidates })
    }
}

impl Filter for LuaExtension {
    fn filter(
        &self,
        ctx: &ExtensionContext,
        candidates_in: &[Candidate],
    ) -> Result<ExtensionOutput, ExtensionError> {
        let lua = self.lua.lock().unwrap();
        let env = LuaRuntime::build_env(&lua, &self.name, &|_, c| self.has_cap(c), ctx)
            .map_err(|e| ExtensionError::Runtime(e.to_string()))?;

        let lua_cands = lua
            .create_table()
            .map_err(|e| ExtensionError::Runtime(e.to_string()))?;
        for (i, c) in candidates_in.iter().enumerate() {
            let t = lua
                .create_table()
                .map_err(|e| ExtensionError::Runtime(e.to_string()))?;
            t.set("text", c.text.as_str())
                .map_err(|e| ExtensionError::Runtime(e.to_string()))?;
            t.set("id", c.id.get())
                .map_err(|e| ExtensionError::Runtime(e.to_string()))?;
            lua_cands
                .set(i + 1, t)
                .map_err(|e| ExtensionError::Runtime(e.to_string()))?;
        }
        env.set("candidates", lua_cands)
            .map_err(|e| ExtensionError::Runtime(e.to_string()))?;

        let raw = LuaRuntime::call_fn(&lua, &self.name, "filter", env)
            .map_err(|e| ExtensionError::Runtime(e.to_string()))?;
        let result = value_to_table(raw)?;

        let filtered: Vec<Table> = result.get("candidates").unwrap_or_default();
        let candidates: Vec<Candidate> = filtered
            .into_iter()
            .map(|t| Candidate {
                id: CandidateId::new(t.get("id").unwrap_or(0)),
                text: t.get("text").unwrap_or_default(),
                annotation: t.get("annotation").ok(),
                source: format!("lua:{}", self.name),
            })
            .collect();

        Ok(ExtensionOutput::Filter { candidates })
    }
}

impl Segmentor for LuaExtension {
    fn segment(&self, ctx: &ExtensionContext) -> Result<ExtensionOutput, ExtensionError> {
        let lua = self.lua.lock().unwrap();
        let env = LuaRuntime::build_env(&lua, &self.name, &|_, c| self.has_cap(c), ctx)
            .map_err(|e| ExtensionError::Runtime(e.to_string()))?;

        let raw = LuaRuntime::call_fn(&lua, &self.name, "segment", env)
            .map_err(|e| ExtensionError::Runtime(e.to_string()))?;
        let result = value_to_table(raw)?;

        let segments_array: Vec<Table> = result.get("segments").unwrap_or_default();
        let segments: Vec<Segment> = segments_array
            .into_iter()
            .map(|t| Segment {
                start: t.get("start").unwrap_or(0),
                end: t.get("end").unwrap_or(0),
                tag: t.get("tag").unwrap_or_default(),
            })
            .collect();

        Ok(ExtensionOutput::Segmentor { segments })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::{Revision, SessionEpoch};

    #[test]
    fn loads_simple_lua_script() {
        let mut rt = LuaRuntime::new().unwrap();
        rt.load_script("test_mod", "test_mod = {}", &[]).unwrap();
        assert_eq!(rt.scripts.len(), 1);
    }

    #[test]
    fn capability_check_denies_unknown() {
        let mut rt = LuaRuntime::new().unwrap();
        rt.load_script("test", "test = {}", &[Capability::Clock])
            .unwrap();
        assert!(rt.has_cap("test", &Capability::Clock));
        assert!(!rt.has_cap("test", &Capability::Network("hw".into())));
    }

    #[test]
    fn missing_script_returns_error() {
        let rt = LuaRuntime::new().unwrap();
        assert_eq!(
            rt.check_capability("missing", &Capability::Clock),
            Err(LuaError::NotLoaded("missing".into()))
        );
    }

    #[test]
    fn lua_processor_returns_handled_result() {
        let mut rt = LuaRuntime::new().unwrap();
        let lua_src = r#"
            test_proc = {}
            function test_proc.process(env)
                return { handled = true, composition = env.composition .. env.key }
            end
        "#;
        rt.load_script("test_proc", lua_src, &[Capability::Clock])
            .unwrap();

        let scripts_snapshot: Arc<Vec<(String, ScriptEntry)>> = Arc::new(rt.scripts.clone());
        let ctx = ExtensionContext {
            session_epoch: SessionEpoch::new(1),
            revision: Revision::new(1),
            composition: "n",
            schema_id: "dp",
        };
        let ext = make_lua_extension(rt.lua.clone(), scripts_snapshot, "test_proc");
        let proc = ext.processor().unwrap();
        let output = proc
            .process(
                &ctx,
                &KeyEvent {
                    key: Key::Character('i'),
                    state: Default::default(),
                },
            )
            .unwrap();
        assert!(matches!(
            output,
            ExtensionOutput::Processor {
                handled: true,
                composition,
            } if composition == "ni"
        ));
    }

    #[test]
    fn lua_translator_returns_candidates() {
        let mut rt = LuaRuntime::new().unwrap();
        let lua_src = r#"
            test_trans = {}
            function test_trans.translate(env)
                return { candidates = {
                    { text = "你好", code = "ni hao", weight = 100 },
                    { text = "你", code = "ni", weight = 50 },
                } }
            end
        "#;
        rt.load_script("test_trans", lua_src, &[]).unwrap();

        let scripts_snapshot: Arc<Vec<(String, ScriptEntry)>> = Arc::new(rt.scripts.clone());
        let ctx = ExtensionContext {
            session_epoch: SessionEpoch::new(1),
            revision: Revision::new(2),
            composition: "ni",
            schema_id: "dp",
        };
        let ext = make_lua_extension(rt.lua.clone(), scripts_snapshot, "test_trans");
        let trans = ext.translator().unwrap();
        let output = trans.translate(&ctx).unwrap();
        assert!(matches!(
            &output,
            ExtensionOutput::Translator { candidates } if candidates.len() == 2
        ));
    }
}
