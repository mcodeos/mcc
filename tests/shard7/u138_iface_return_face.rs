//! U138 — an instance method may return its own interface port, and the
//! returned face is judged by the same engine the statement-level face uses.
//!
//! `return IF` inside a component method used to die at Pass1 with E4007: the
//! return face collapsed to one member-less lane while the peer's
//! statement-level `ha.IF` face expands memberwise, so the row counts never
//! matched and the engine never ran. The shape authority is the receiver's
//! own pins table — the collapsed lane expands to the port's declared
//! members, and the call then connects memberwise exactly like `ha.IF ->
//! hb.IF` (interface connect rule §1.2, R8: one engine, both emission sites).

use crate::common;
use mcc::{McIds, McURI};

const IFACE_RETURN_FIXTURE: &str = r#"
interface P2P(role)
{
    pins = [
        [1,2] = [A, B]
    ]
    role Tx { peer = Rx }
    role Rx { peer = Tx }
}

interface LYNX(role)
{
    pins = [
        [1,2] = [A, B]
    ]
    role Host { peer = Dev }
    role Dev { peer = Host }
}

component HUBT { pins = [ [1,2] = IF::P2P(Tx) ]
    func mk() {
        return IF
    }
}
component HUBR { pins = [ [1,2] = IF::P2P(Rx) ]
    func mk() {
        return IF
    }
}
component LDEV { pins = [ [1,2] = IF::LYNX(Dev) ] }
"#;

/// Codes this section judges: the shape family and the interface connect
/// rule. Build-info codes and the return-only-body advisory are other
/// checks' territory.
fn u138_codes(codes: &[u32]) -> Vec<u32> {
    codes
        .iter()
        .filter(|c| matches!(c, 4005 | 4007 | 4120..=4123))
        .copied()
        .collect()
}

/// Build `main` with the fixture and `body`; returns (judged codes, net
/// partition of the top module, normalized like the connect-rule locks).
fn build_return(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{IFACE_RETURN_FIXTURE}module main {{\n    HUBT ha\n    HUBR hb\n    LDEV la\n{body}\n}}\n"
    );
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, &src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !matches!(*c, 5641 | 5642 | 5643 | 5054))
        .collect();
    codes.sort_unstable();
    let mut partition: Vec<Vec<String>> = net_store
        .get("main")
        .map(|t| {
            t.iter()
                .map(|(_, pts)| {
                    let mut ps: Vec<String> = pts.iter().map(|p| p.path.clone()).collect();
                    ps.sort();
                    ps
                })
                .filter(|ps| !ps.is_empty())
                .collect()
        })
        .unwrap_or_default();
    partition.sort();
    (u138_codes(&codes), partition)
}

/// The U138 form: the method returns its own interface port and the call
/// wires it memberwise — the same two nets the statement-level face makes,
/// with no shape or connect-rule verdict.
#[test]
fn u138__method_return_interface_port_connects_memberwise() {
    let (codes, nets) = build_return("    hb.mk() -> ha.IF", "/mcc/u138-method-return.mc");
    let (baseline_codes, baseline_nets) =
        build_return("    hb.IF -> ha.IF", "/mcc/u138-baseline.mc");
    assert_eq!(
        nets, baseline_nets,
        "the returned interface face must land the same memberwise nets as the statement-level face"
    );
    assert_eq!(
        codes, baseline_codes,
        "the returned face is judged by the same rule and must stay as quiet as the baseline"
    );
}

/// Control: the statement-level face on the same fixture — quiet, two nets.
#[test]
fn u138__statement_level_face_stays_quiet() {
    let (codes, nets) = build_return("    hb.IF -> ha.IF", "/mcc/u138-stmt-control.mc");
    assert_eq!(codes, Vec::<u32>::new(), "control must raise no verdict");
    assert_eq!(
        nets,
        [
            vec!["ha.1".to_string(), "hb.1".to_string()],
            vec!["ha.2".to_string(), "hb.2".to_string()]
        ],
        "two interface ports connect memberwise; got {nets:?}"
    );
}

/// R8: the returned face goes through the engine's emission site, so the
/// connect rule still judges it — a cross-family peer raises E4120.
#[test]
fn u138__returned_face_is_still_judged_cross_family() {
    let (codes, _nets) = build_return("    hb.mk() -> la.IF", "/mcc/u138-cross-family.mc");
    assert_eq!(
        codes,
        vec![4120],
        "the method-returned P2P face against a LYNX peer is cross-family; got {codes:?}"
    );
}
