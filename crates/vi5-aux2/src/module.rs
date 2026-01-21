use aviutl2::{AnyResult, AviUtl2Info, module::ScriptModuleFunctions};

#[aviutl2::plugin(ScriptModule)]
pub struct InternalModule;

impl aviutl2::module::ScriptModule for InternalModule {
    fn new(_info: AviUtl2Info) -> AnyResult<Self> {
        Ok(Self)
    }

    fn plugin_info(&self) -> aviutl2::module::ScriptModuleTable {
        aviutl2::module::ScriptModuleTable {
            information: "vi5.aux2 Internal Module".to_string(),
            functions: Self::functions(),
        }
    }
}

#[aviutl2::module::functions]
impl InternalModule {
    fn serialize_string(&self, input: String) -> aviutl2::AnyResult<String> {
        Ok(input)
    }
}
