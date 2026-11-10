//! Lua 5.4 scripts for event conditions and actions (Phase 3 slice 4; behaviour doc
//! `docs/models/lua-scripting.md`).
//!
//! Every run gets a fresh, sandboxed interpreter:
//! - **Libraries:** only `table`, `string`, `math` and `utf8`. No `io`, `os`, `package`,
//!   `debug` or `coroutine`. `load`, `loadfile`, `dofile`, `require`, `collectgarbage`,
//!   `print`, `warn`, `pcall`, `xpcall`, `next`, `string.dump`, `math.random` and
//!   `math.randomseed` are removed. Scripts are text only, never bytecode.
//! - **Limits:** 1 000 000 VM instructions, 16 MiB of memory, and 64 KiB of source.
//! - **Determinism:**
//!   - `pairs` visits keys in sorted order (numbers, then strings, then booleans).
//!   - `tostring` and `string.format` never print addresses.
//!   - The math functions are the Rust `libm` ones.
//!   - `math.random` is replaced by `sim.random()`, which draws from the `Events` stream.
//!   - Weak tables and finalizers are refused.
//!
//! A script sees the simulation only through the `sim` table. Action scripts may also ask for
//! actions, which the engine carries out after the script returns.

use crate::eval::EventWorld;
use crate::model::{Audience, EventAction};
use mlua::chunk::ChunkMode;
use mlua::{HookTriggers, Lua, LuaOptions, StdLib, Table, Value, VmState};
use sim_c2::Posture;
use std::cell::{Cell, RefCell};

/// Most VM instructions one run may execute.
pub const INSTRUCTION_LIMIT: u64 = 1_000_000;
/// Most memory one run may allocate.
pub const MEMORY_LIMIT_BYTES: usize = 16 * 1024 * 1024;
/// Longest script accepted.
pub const SCRIPT_MAX_BYTES: usize = 64 * 1024;
/// The instruction count is checked every this many instructions.
const HOOK_EVERY: u32 = 1_000;

/// Runs before every script, with the full base library still present. It installs the
/// deterministic replacements and then removes everything a script must not reach.
const PRELUDE: &str = r##"
local next, type, getmetatable, setmetatable0, rawget, rawequal, error, select =
  next, type, getmetatable, setmetatable, rawget, rawequal, error, select
local sort, unpack, tostring0, format0, find = table.sort, table.unpack, tostring, string.format, string.find
local rank = { number = 1, string = 2, boolean = 3 }
local opaque = { table = true, ["function"] = true, userdata = true, thread = true }

function pairs(t)
  if type(t) ~= "table" then error("pairs: table expected", 2) end
  local keys = {}
  for k in next, t do
    if not rank[type(k)] then error("pairs: keys must be numbers, strings or booleans", 2) end
    keys[#keys + 1] = k
  end
  sort(keys, function(a, b)
    local ra, rb = rank[type(a)], rank[type(b)]
    if ra ~= rb then return ra < rb end
    if ra == 3 then return b and not a end
    return a < b
  end)
  local i = 0
  return function()
    i = i + 1
    local k = keys[i]
    if k ~= nil then return k, t[k] end
  end, t, nil
end

local function plain(v)
  if opaque[type(v)] then
    local mt = getmetatable(v)
    if type(mt) == "table" and rawget(mt, "__tostring") then return tostring0(v) end
    return type(v)
  end
  return v
end

function tostring(v)
  local p = plain(v)
  if rawequal(p, v) then return tostring0(v) end
  return p
end

string.format = function(fmt, ...)
  if type(fmt) == "string" and find(fmt, "%p", 1, true) then
    error("string.format: %p is not allowed", 2)
  end
  local n = select("#", ...)
  local args = { ... }
  for i = 1, n do args[i] = plain(args[i]) end
  return format0(fmt, unpack(args, 1, n))
end

function setmetatable(t, mt)
  if type(mt) == "table" and (rawget(mt, "__gc") ~= nil or rawget(mt, "__mode") ~= nil) then
    error("setmetatable: __gc and __mode are not allowed", 2)
  end
  return setmetatable0(t, mt)
end

for _, name in ipairs({ "next", "load", "loadfile", "dofile", "require", "collectgarbage",
                        "print", "warn", "pcall", "xpcall" }) do
  _G[name] = nil
end
string.dump, math.random, math.randomseed = nil, nil, nil
"##;

fn err(e: &mlua::Error) -> String {
    let text = e.to_string();
    let first = text.lines().next().unwrap_or("script error");
    first.chars().take(300).collect()
}

/// A one-argument math function.
type Unary = fn(f64) -> f64;

/// A fresh sandboxed interpreter with the limits set and the prelude run.
fn sandbox() -> mlua::Result<Lua> {
    let lua = Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
        LuaOptions::default(),
    )?;
    lua.set_memory_limit(MEMORY_LIMIT_BYTES)?;
    let used = Cell::new(0u64);
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(HOOK_EVERY),
        move |_, _| {
            used.set(used.get() + u64::from(HOOK_EVERY));
            if used.get() > INSTRUCTION_LIMIT {
                Err(mlua::Error::runtime(format!(
                    "instruction limit ({INSTRUCTION_LIMIT}) exceeded"
                )))
            } else {
                Ok(VmState::Continue)
            }
        },
    )?;
    let math: Table = lua.globals().get("math")?;
    let unary: [(&str, Unary); 6] = [
        ("sin", libm::sin),
        ("cos", libm::cos),
        ("tan", libm::tan),
        ("asin", libm::asin),
        ("acos", libm::acos),
        ("exp", libm::exp),
    ];
    for (name, f) in unary {
        math.set(name, lua.create_function(move |_, x: f64| Ok(f(x)))?)?;
    }
    math.set(
        "atan",
        lua.create_function(|_, (y, x): (f64, Option<f64>)| Ok(libm::atan2(y, x.unwrap_or(1.0))))?,
    )?;
    math.set(
        "log",
        lua.create_function(|_, (x, base): (f64, Option<f64>)| {
            Ok(match base {
                None => libm::log(x),
                Some(2.0) => libm::log2(x),
                Some(10.0) => libm::log10(x),
                Some(b) => libm::log(x) / libm::log(b),
            })
        })?,
    )?;
    math.set(
        "pow",
        lua.create_function(|_, (x, y): (f64, f64)| Ok(libm::pow(x, y)))?,
    )?;
    lua.load(PRELUDE).set_name("prelude").exec()?;
    Ok(lua)
}

fn check_size(script: &str) -> Result<(), String> {
    if script.len() > SCRIPT_MAX_BYTES {
        Err(format!(
            "script is {} bytes; the limit is {SCRIPT_MAX_BYTES}",
            script.len()
        ))
    } else if script.trim().is_empty() {
        Err("script is empty".into())
    } else {
        Ok(())
    }
}

/// Compiles a script without running it (scenario validation).
pub fn check(script: &str) -> Result<(), String> {
    check_size(script)?;
    let lua = sandbox().map_err(|e| err(&e))?;
    lua.load(script)
        .set_mode(ChunkMode::Text)
        .set_name("script")
        .into_function()
        .map(|_| ())
        .map_err(|e| err(&e))
}

fn int<T: TryFrom<i64>>(v: &Value, what: &str) -> mlua::Result<T> {
    let n = match v {
        Value::Integer(i) => Some(*i),
        #[allow(clippy::cast_possible_truncation)]
        Value::Number(f) if f.fract() == 0.0 && f.is_finite() => Some(*f as i64),
        _ => None,
    };
    n.and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| mlua::Error::runtime(format!("{what} must be a whole number in range")))
}

fn audience(v: &Value) -> mlua::Result<Audience> {
    match v {
        Value::String(s) if s.to_str()?.as_ref() as &str == "all" => Ok(Audience::All),
        Value::String(s) if s.to_str()?.as_ref() as &str == "white_cell" => Ok(Audience::WhiteCell),
        other => Ok(Audience::Side {
            side: int(
                other,
                "message audience (\"all\", \"white_cell\" or a side)",
            )?,
        }),
    }
}

fn posture(s: &str) -> mlua::Result<Posture> {
    Ok(match s {
        "friendly" => Posture::Friendly,
        "neutral" => Posture::Neutral,
        "unfriendly" => Posture::Unfriendly,
        "hostile" => Posture::Hostile,
        _ => return Err(mlua::Error::runtime(format!("unknown posture {s:?}"))),
    })
}

/// Runs `script` with the `sim` table installed. Returns its result and the actions it asked
/// for (always none when `allow_actions` is false: asking is an error).
fn run<R: mlua::FromLuaMulti>(
    script: &str,
    world: &mut dyn EventWorld,
    allow_actions: bool,
) -> Result<(R, Vec<EventAction>), String> {
    check_size(script)?;
    let lua = sandbox().map_err(|e| err(&e))?;
    let world = RefCell::new(world);
    let actions: RefCell<Vec<EventAction>> = RefCell::new(Vec::new());
    let ask = |name: &'static str, a: EventAction| -> mlua::Result<()> {
        if allow_actions {
            actions.borrow_mut().push(a);
            Ok(())
        } else {
            Err(mlua::Error::runtime(format!(
                "sim.{name} is only available in an action script"
            )))
        }
    };
    let result = lua.scope(|scope| {
        let sim = lua.create_table()?;
        sim.set(
            "time",
            scope.create_function(|_, ()| Ok(world.borrow().now_s()))?,
        )?;
        sim.set(
            "fired",
            scope.create_function(|_, id: String| Ok(world.borrow().fired(&id)))?,
        )?;
        sim.set(
            "unit_alive",
            scope.create_function(|_, id: Value| {
                Ok(world.borrow().unit_alive(int(&id, "unit id")?))
            })?,
        )?;
        sim.set(
            "units_alive",
            scope.create_function(|_, side: Value| {
                Ok(world.borrow().units_alive(int(&side, "side")?))
            })?,
        )?;
        sim.set(
            "unit",
            scope.create_function(|lua, id: Value| {
                let Some(u) = world.borrow().unit(int(&id, "unit id")?) else {
                    return Ok(Value::Nil);
                };
                let t = lua.create_table()?;
                t.set("id", u.id)?;
                t.set("side", u.side)?;
                t.set("lat", u.position.lat_deg)?;
                t.set("lon", u.position.lon_deg)?;
                Ok(Value::Table(t))
            })?,
        )?;
        sim.set(
            "random",
            scope.create_function(|_, ()| Ok(world.borrow_mut().draw()))?,
        )?;
        sim.set(
            "message",
            scope.create_function(|_, (to, text): (Value, String)| {
                ask(
                    "message",
                    EventAction::Message {
                        to: audience(&to)?,
                        text,
                    },
                )
            })?,
        )?;
        sim.set(
            "inject",
            scope.create_function(|_, inject: String| {
                ask("inject", EventAction::SendInject { inject })
            })?,
        )?;
        sim.set(
            "enable_event",
            scope.create_function(|_, (event, enabled): (String, bool)| {
                ask("enable_event", EventAction::EnableEvent { event, enabled })
            })?,
        )?;
        sim.set(
            "set_posture",
            scope.create_function(|_, (side, toward, p): (Value, Value, String)| {
                ask(
                    "set_posture",
                    EventAction::SetPosture {
                        side: int(&side, "side")?,
                        toward: int(&toward, "side")?,
                        posture: posture(&p)?,
                    },
                )
            })?,
        )?;
        sim.set(
            "set_speed",
            scope.create_function(|_, (unit, speed_mps): (Value, f64)| {
                ask(
                    "set_speed",
                    EventAction::SetSpeed {
                        unit: int(&unit, "unit id")?,
                        speed_mps,
                    },
                )
            })?,
        )?;
        sim.set(
            "spawn_reserve",
            scope.create_function(|_, unit: Value| {
                ask(
                    "spawn_reserve",
                    EventAction::SpawnReserve {
                        unit: int(&unit, "unit id")?,
                    },
                )
            })?,
        )?;
        lua.globals().set("sim", sim)?;
        lua.load(script)
            .set_mode(ChunkMode::Text)
            .set_name("script")
            .eval::<R>()
    });
    let r = result.map_err(|e| err(&e))?;
    Ok((r, actions.into_inner()))
}

/// Runs a condition script. It must return `true` or `false`, and may not ask for actions.
pub fn condition(script: &str, world: &mut dyn EventWorld) -> Result<bool, String> {
    let (v, _) = run::<Value>(script, world, false)?;
    match v {
        Value::Boolean(b) => Ok(b),
        _ => Err("a condition script must return true or false".into()),
    }
}

/// Runs an action script and returns the actions it asked for, in order.
pub fn action(script: &str, world: &mut dyn EventWorld) -> Result<Vec<EventAction>, String> {
    run::<Value>(script, world, true).map(|(_, a)| a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::UnitFact;
    use sim_geo::GeoPoint;

    /// Two units: 1 (side 1) and 2 (side 2); event "a" fired twice; draws count up from 0.1.
    struct World {
        draws: u32,
    }

    impl EventWorld for World {
        fn now_s(&self) -> f64 {
            90.0
        }
        fn fired(&self, event: &str) -> u32 {
            u32::from(event == "a") * 2
        }
        fn unit_alive(&self, unit: u64) -> bool {
            unit == 1 || unit == 2
        }
        fn units_alive(&self, _side: u16) -> u32 {
            1
        }
        fn draw(&mut self) -> f64 {
            self.draws += 1;
            0.1 * f64::from(self.draws)
        }
        fn unit(&self, unit: u64) -> Option<UnitFact> {
            self.unit_alive(unit).then(|| UnitFact {
                id: unit,
                side: u16::try_from(unit).unwrap(),
                position: GeoPoint::new(-39.5, 146.25),
            })
        }
    }

    fn world() -> World {
        World { draws: 0 }
    }

    fn cond(script: &str) -> Result<bool, String> {
        condition(script, &mut world())
    }

    #[test]
    fn conditions_read_the_simulation() {
        assert_eq!(cond("return sim.time() == 90"), Ok(true));
        assert_eq!(
            cond("return sim.fired('a') == 2 and sim.fired('b') == 0"),
            Ok(true)
        );
        assert_eq!(
            cond("return sim.unit_alive(1) and not sim.unit_alive(3)"),
            Ok(true)
        );
        assert_eq!(
            cond("local u = sim.unit(2) return u.side == 2 and u.lat < -39 and sim.unit(9) == nil"),
            Ok(true)
        );
        assert_eq!(cond("return sim.units_alive(1) >= 1"), Ok(true));
        assert!(cond("return 1").unwrap_err().contains("true or false"));
        assert!(cond("return sim.unit_alive(1.5)")
            .unwrap_err()
            .contains("whole number"));
        // A condition may not ask for actions.
        assert!(cond("sim.message('all', 'x') return true")
            .unwrap_err()
            .contains("only available in an action script"));
    }

    #[test]
    fn actions_are_collected_in_order() {
        let script = r"
            sim.message('all', 'to everyone')
            sim.message(2, 'to red')
            sim.message('white_cell', 'umpires only')
            sim.inject('i1')
            sim.enable_event('b', false)
            sim.set_posture(1, 2, 'hostile')
            sim.set_speed(1, 7.5)
            sim.spawn_reserve(11)
        ";
        let got = action(script, &mut world()).unwrap();
        assert_eq!(
            got,
            vec![
                EventAction::Message {
                    to: Audience::All,
                    text: "to everyone".into()
                },
                EventAction::Message {
                    to: Audience::Side { side: 2 },
                    text: "to red".into()
                },
                EventAction::Message {
                    to: Audience::WhiteCell,
                    text: "umpires only".into()
                },
                EventAction::SendInject {
                    inject: "i1".into()
                },
                EventAction::EnableEvent {
                    event: "b".into(),
                    enabled: false
                },
                EventAction::SetPosture {
                    side: 1,
                    toward: 2,
                    posture: Posture::Hostile
                },
                EventAction::SetSpeed {
                    unit: 1,
                    speed_mps: 7.5
                },
                EventAction::SpawnReserve { unit: 11 },
            ]
        );
        assert!(action("sim.set_posture(1, 2, 'angry')", &mut world())
            .unwrap_err()
            .contains("unknown posture"));
    }

    #[test]
    fn random_draws_come_from_the_simulation_stream() {
        let mut w = world();
        assert_eq!(
            condition("return sim.random() < 0.15 and sim.random() > 0.15", &mut w),
            Ok(true)
        );
        assert_eq!(w.draws, 2);
    }

    #[test]
    fn the_sandbox_has_no_way_out() {
        for name in [
            "io",
            "os",
            "debug",
            "package",
            "coroutine",
            "require",
            "load",
            "loadfile",
            "dofile",
            "collectgarbage",
            "print",
            "warn",
            "pcall",
            "xpcall",
            "next",
            "string.dump",
            "math.random",
            "math.randomseed",
        ] {
            assert_eq!(cond(&format!("return {name} == nil")), Ok(true), "{name}");
        }
        // Reaching the string library through a string's metatable finds the same cut-down table.
        assert_eq!(cond("return ('').dump == nil"), Ok(true));
        // Precompiled bytecode is refused, both at load and at validation.
        assert!(check("\x1bLua").is_err());
        assert!(cond("\x1bLuaT").is_err());
        // Globals do not survive from one run to the next.
        assert_eq!(cond("leak = 1 return true"), Ok(true));
        assert_eq!(cond("return leak == nil"), Ok(true));
    }

    #[test]
    fn runaway_scripts_hit_the_instruction_and_memory_limits() {
        let e = cond("while true do end").unwrap_err();
        assert!(e.contains("instruction limit"), "{e}");
        let e = cond("local t = {} for i = 1, 1e9 do t[i] = ('x'):rep(1000) .. i end").unwrap_err();
        assert!(
            e.contains("memory") || e.contains("instruction limit"),
            "{e}"
        );
        let e = cond("local s = ('x'):rep(64 * 1024 * 1024) return true").unwrap_err();
        assert!(e.contains("memory"), "{e}");
        let e = cond("local function f() return f() + 1 end return f()").unwrap_err();
        // Deep recursion stops at the stack limit or, first, the memory limit.
        assert!(e.contains("stack overflow") || e.contains("memory"), "{e}");
        let big = format!("return true --{}", "x".repeat(SCRIPT_MAX_BYTES));
        assert!(check(&big).unwrap_err().contains("limit"));
    }

    #[test]
    fn deterministic_replacements() {
        // pairs walks keys sorted: numbers, then strings, then booleans.
        let s = r"
            local t = { zulu = 1, alpha = 2, [3] = 'c', [1] = 'a', [true] = 'y', mike = 3 }
            local out = {}
            for k in pairs(t) do out[#out + 1] = tostring(k) end
            return table.concat(out, ',') == '1,3,alpha,mike,zulu,true'
        ";
        assert_eq!(cond(s), Ok(true));
        // Tables and functions print as their type name, never an address.
        assert_eq!(
            cond("return tostring({}) == 'table' and string.format('%s', print or tostring) == 'function'"),
            Ok(true)
        );
        assert!(cond("return string.format('%p', {}) ~= nil").is_err());
        // Finalizers and weak tables are refused.
        assert!(cond("setmetatable({}, { __gc = function() end }) return true").is_err());
        assert!(cond("setmetatable({}, { __mode = 'k' }) return true").is_err());
        // The math functions are libm's.
        assert_eq!(
            cond(&format!(
                "return math.sin(0.5) == {:?} and math.pow(2, 0.5) == {:?}",
                libm::sin(0.5),
                libm::pow(2.0, 0.5)
            )),
            Ok(true)
        );
        assert_eq!(
            cond("return math.log(8, 2) == 3 and math.atan(1, 1) * 4 == math.pi"),
            Ok(true)
        );
    }

    #[test]
    fn syntax_errors_are_found_at_validation() {
        assert!(check("return true").is_ok());
        assert!(check("return (").unwrap_err().contains("script"));
        assert!(check("   ").is_err());
    }
}
