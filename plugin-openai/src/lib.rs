pub mod translator;

#[cfg(feature = "dylib")]
pub mod lib {
    use macros::build_ffi;
    use crate::translator::OpenAITranslator;

    build_ffi!("openai", OpenAITranslator);
}
