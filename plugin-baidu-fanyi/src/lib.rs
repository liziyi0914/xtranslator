pub mod translator;

#[cfg(feature = "dylib")]
pub mod lib {
    use macros::build_ffi;
    use crate::translator::BaiduFanyiTranslator;

    build_ffi!("baidu_fanyi", BaiduFanyiTranslator);
}
