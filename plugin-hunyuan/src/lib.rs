pub mod translator;

#[cfg(feature = "dylib")]
pub mod lib {
    use macros::build_ffi;
    use crate::translator::HunyuanTranslator;

    build_ffi!("hunyuan", HunyuanTranslator);
}
