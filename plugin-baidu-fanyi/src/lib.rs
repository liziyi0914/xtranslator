pub mod translator;

#[cfg(feature = "dylib")]
pub mod lib {
    use crate::translator::BaiduFanyiTranslator;
    use macros::build_ffi;

    build_ffi!("baidu_fanyi", BaiduFanyiTranslator);
}
