#[aviutl2::plugin(GenericPlugin)]
struct Vi5Aux2 {}

impl aviutl2::generic::GenericPlugin for Vi5Aux2 {
    fn new(_info: aviutl2::AviUtl2Info) -> aviutl2::AnyResult<Self> {
        Ok(Self {})
    }

    fn register(&mut self, host_app_handle: &mut aviutl2::generic::HostAppHandle) {
        host_app_handle.set_plugin_information(&format!(
            "vi5.aux2 / https://github.com/sevenc-nanashi/vi5.aux2 / v{}",
            env!("CARGO_PKG_VERSION")
        ))
    }
}

aviutl2::register_generic_plugin!(Vi5Aux2);
