pub mod translator;

#[cfg(feature = "dylib")]
pub mod lib {
    use macros::build_ffi;
    use crate::translator::YoudaoLLMTranslator;

    build_ffi!("youdao_llm", YoudaoLLMTranslator);
}
