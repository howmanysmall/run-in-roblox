use std::io::Cursor;

use run_in_roblox::{plugin::RunInRbxPlugin, message_receiver::RobloxMessage};

#[test]
fn plugin_contains_server_id_and_port() {
    let plugin = RunInRbxPlugin {
        port: 12345,
        server_id: "run-in-roblox-abc123",
        lua_script: "print('hi')",
    };

    let mut buf = Cursor::new(Vec::new());
    // write should succeed (we adapted plugin to new API)
    let res = plugin.write(&mut buf);
    assert!(res.is_ok());

    let data = String::from_utf8(buf.into_inner()).expect("utf8");
    assert!(data.contains("12345"));
    assert!(data.contains("run-in-roblox-abc123"));
}

#[test]
fn deserialize_messages() {
    let json = r#"[{"type":"Output","level":"Error","body":"Boom"}]"#;
    let msgs: Vec<RobloxMessage> = serde_json::from_str(json).expect("deserialize");
    assert_eq!(msgs.len(), 1);
}
