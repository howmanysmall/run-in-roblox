use std::io::Write;

use rbx_xml::EncodeError;
use rbx_dom_weak::{WeakDom, InstanceBuilder};

static PLUGIN_TEMPLATE: &'static str = include_str!("plugin_main_template.lua");

pub struct RunInRbxPlugin<'a> {
    pub port: u16,
    pub server_id: &'a str,
    pub lua_script: &'a str,
}

impl<'a> RunInRbxPlugin<'a> {
    pub fn write<W: Write>(&self, output: W) -> Result<(), EncodeError> {
        let dom = self.build_plugin();
        let root_id = dom.root_ref();
        rbx_xml::to_writer_default(output, &dom, &[root_id])
    }
}

impl<'a> RunInRbxPlugin<'a> {
    fn build_plugin(&self) -> WeakDom {
        let complete_source = PLUGIN_TEMPLATE
            .replace("{{PORT}}", &self.port.to_string())
            .replace("{{SERVER_ID}}", self.server_id);

        let plugin_script = InstanceBuilder::new("Script")
            .with_name("run-in-roblox-plugin")
            .with_property("Source", complete_source);

        let main_source = format!("return function()\n{}\nend", self.lua_script);
        let injected_main = InstanceBuilder::new("ModuleScript")
            .with_name("Main")
            .with_property("Source", main_source);

        let mut dom = WeakDom::new(plugin_script);
        let root_id = dom.root_ref();
    dom.insert(root_id, injected_main);
        dom
    }
}
