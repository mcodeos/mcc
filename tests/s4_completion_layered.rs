//! Manual verification of the S4 layered completion RPC (design §8.1):
//! position-derived scope, per-layer serialization, and member enumeration.
#![allow(dead_code)]

use mcc::rpc::protocol::RpcMethodRegistry;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

static S4_LOCK: Mutex<()> = Mutex::new(());

fn hbl_project_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME set");
    PathBuf::from(home).join("work/mo/mcd/projects/hbl")
}

fn sys_root() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME set");
    PathBuf::from(home).join("work/mo")
}

fn setup() -> (Arc<RpcMethodRegistry>, std::path::PathBuf) {
    let project_root = hbl_project_dir();
    let _cwd = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&project_root).expect("chdir project root");
    mcc::mcc_set_system_root(sys_root().as_path());
    mcc::mcc_set_project_root(&project_root);

    let server = mcc::rpc::handlers::register_all(mcc::rpc::RpcServerBuilder::new()).build();
    let registry = server.registry();
    let _ = registry.call("init", None);
    let _ = registry.call("lib.load", Some(serde_json::json!({ "name": "mcode" })));
    let entry = project_root.join("src/us513.mc");
    let _ = registry.call(
        "load_project",
        Some(serde_json::json!({ "entry": entry.to_string_lossy() })),
    );
    (registry, entry)
}

/// Compute the cursor byte positions the S4 assertions probe, from the source
/// text rather than hardcoded offsets. The mcd `us513.mc` fixture has grown
/// over time (crystal X6 config block), shifting line offsets — hardcoded
/// byte positions silently landed in the wrong scope.
///
/// Returns `(module_body, func_body)`:
/// - `module_body`: the byte offset of the blank line immediately above
///   `func i2c()` — inside `module US513`, outside any function.
/// - `func_body`: the byte offset just past the `{` of `func i2c()` — inside
///   the `US513.i2c` function body.
fn probe_positions(entry: &std::path::Path) -> (usize, usize) {
    let src = std::fs::read_to_string(entry).expect("read us513.mc");
    let func = src
        .find("func i2c()")
        .unwrap_or_else(|| panic!("func i2c not found in us513.mc"));
    // The blank line immediately above `func i2c()`: `rfind("\n\n")` returns
    // the byte offset of the `\n` that ends the line before the blank line —
    // i.e. the start of the blank line itself, inside `module US513`.
    let module_body = src[..func].rfind("\n\n").unwrap_or(0);
    let brace = src[func..].find('{').expect("func i2c open brace") + func;
    let func_body = brace + 1;
    (module_body, func_body)
}

fn completion(
    registry: &Arc<RpcMethodRegistry>,
    uri: &str,
    position: usize,
    prefix: Option<&str>,
) -> Value {
    let mut params = serde_json::json!({ "uri": uri, "position": position });
    if let Some(p) = prefix {
        params["prefix"] = serde_json::json!(p);
    }
    registry
        .call("completion", Some(params))
        .expect("completion rpc")
}

/// Recover from a panicked test so one failure does not poison the rest.
fn s4_lock() -> std::sync::MutexGuard<'static, ()> {
    S4_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Module body (blank line above `func i2c`, inside `module US513`): the
/// authoritative scope must be "US513" and P2 must surface the module port
/// I2C0.
#[test]
fn layered_module_body_scope() {
    let _lock = s4_lock();
    let (registry, entry) = setup();
    let uri = entry.to_string_lossy();
    let (module_body, _func_body) = probe_positions(&entry);
    let resp = completion(&registry, &uri, module_body, Some("I2"));
    eprintln!("MODULE-BODY response: {resp}");
    assert_eq!(resp["scope_path"], "US513", "module-body scope");
    let p2 = resp["layers"]["P2"].as_array().expect("P2 layer");
    assert!(
        p2.iter().any(|i| i["name"] == "I2C0"),
        "P2 must contain I2C0: {p2:?}"
    );
    // P4/P5 layers must be present in the response envelope too.
    assert!(resp["truncated_layers"].is_array());
}

/// Function body (just past the `{` of `func i2c`, inside module US513): the
/// authoritative scope must be "US513.i2c" and P2 must surface the instance
/// uC.
#[test]
fn layered_func_body_scope() {
    let _lock = s4_lock();
    let (registry, entry) = setup();
    let uri = entry.to_string_lossy();
    let (_module_body, func_body) = probe_positions(&entry);
    let resp = completion(&registry, &uri, func_body, None);
    eprintln!("FUNC-BODY response: {resp}");
    assert_eq!(resp["scope_path"], "US513.i2c", "func-body scope");
    let p2 = resp["layers"]["P2"].as_array().expect("P2 layer");
    assert!(
        p2.iter().any(|i| i["name"] == "uC"),
        "P2 must contain uC: {p2:?}"
    );
}

/// Member access `uC.` inside `func i2c`: the Member layer must enumerate the
/// component instance members (pins/insts/funcs) of the MCU class.
#[test]
fn member_enumeration_component() {
    let _lock = s4_lock();
    let (registry, entry) = setup();
    let uri = entry.to_string_lossy();
    let (_module_body, func_body) = probe_positions(&entry);
    let params = serde_json::json!({
        "uri": uri, "position": func_body, "member_root": "uC"
    });
    let resp = registry
        .call("completion", Some(params))
        .expect("completion rpc");
    eprintln!("MEMBER-uC response: {resp}");
    let members = resp["layers"]["Member"].as_array().expect("Member layer");
    assert!(!members.is_empty(), "Member layer must not be empty");
    // MCU instances expose funcs (power/i2c/...) and pins.
    assert!(
        members
            .iter()
            .any(|i| i["name"] == "power" && i["kind"] == "function"),
        "Member must contain func power: {members:?}"
    );
    assert!(
        members.iter().any(|i| i["kind"] == "pin"),
        "Member must contain pins: {members:?}"
    );
}

/// Member access `this.` inside `func i2c`: resolves to the enclosing module
/// US513 members (ports/labels/insts/funcs).
#[test]
fn member_enumeration_this() {
    let _lock = s4_lock();
    let (registry, entry) = setup();
    let uri = entry.to_string_lossy();
    let (_module_body, func_body) = probe_positions(&entry);
    let params = serde_json::json!({
        "uri": uri, "position": func_body, "member_root": "this"
    });
    let resp = registry
        .call("completion", Some(params))
        .expect("completion rpc");
    eprintln!("MEMBER-this response: {resp}");
    let members = resp["layers"]["Member"].as_array().expect("Member layer");
    assert!(!members.is_empty(), "Member layer must not be empty");
    assert!(
        members
            .iter()
            .any(|i| i["name"] == "I2C0" && i["kind"] == "port"),
        "this. must contain port I2C0: {members:?}"
    );
    assert!(
        members
            .iter()
            .any(|i| i["name"] == "i2c" && i["kind"] == "function"),
        "this. must contain func i2c: {members:?}"
    );
}
