use mcc::{McIds, McURI};

#[test]
fn conditional_pin_alias_resolves_to_physical_pin() {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/dynamic-pin-access.mc".to_string();
    let source = r#"
component CONFIGURABLE(partno::STRING = "BASE")
{
    pins = [
        io 1 = BASE_IO
    ]

    if (partno == "WIDE")
    {
        pins += [
            io [2:3] = GPIO[8:9]
        ]
    }
}

module main
{
    CONFIGURABLE("WIDE") U_WIDE
    SIGNAL -> U_WIDE.GPIO8
}
"#;

    mcc::mcc_load_from_string(&uri, source);
    let instance = mcc::mcc_build(&McIds::from("main"), &uri).expect("build dynamic pin fixture");
    let diagnostics = mcc::mcc_diagnose_all();

    assert!(
        diagnostics.iter().all(|diagnostic| diagnostic.code != 1802),
        "dynamic pin alias was rejected: {:?}",
        diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, &diagnostic.msg))
            .collect::<Vec<_>>()
    );

    let paths: Vec<&str> = instance
        .connections
        .iter()
        .flat_map(|connection| connection.points.iter().map(|point| point.path.as_str()))
        .collect();
    assert!(paths.contains(&"U_WIDE.2"), "resolved paths: {paths:?}");
    assert!(
        !paths.contains(&"U_WIDE.GPIO8"),
        "resolved paths: {paths:?}"
    );

    let component = instance
        .components
        .iter()
        .find(|component| component.name == "U_WIDE")
        .expect("U_WIDE instance");
    assert_eq!(component.pin_name("2").as_deref(), Some("GPIO8"));
}
