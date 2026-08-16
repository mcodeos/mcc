use mcc::{McIds, McURI};

fn main() {
    let ifs = r#"
interface DC(volt)
{
    pins = [
        1 = VOUT, "DC power positive"
        2 = GND, "DC power ground"
    ]
}
"#;
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let a: McURI = "/mcc/resolve/c3.defs.mc".to_string();
    mcc::mcc_load_from_string(&a, ifs);
    let b: McURI = "/mcc/resolve/net1.basic.mc".to_string();
    mcc::mcc_load_from_string(&b, r#"
module main
{
    in PWR_[VDD2, GND2]::DC(5V)
}
"#);
    println!("=== F12 dump of B ===");
    let dump = mcc::dump_symbols_f12_text(&b).unwrap();
    for line in dump.lines().filter(|l| l.contains("DC")) {
        println!("{line}");
    }
    println!("=== get_def from B ===");
    println!("{:?}", mcc::get_def(&McIds::from("DC"), &b));
}
