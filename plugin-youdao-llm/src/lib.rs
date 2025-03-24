pub mod translator;

use macros::build_ffi;
use crate::translator::YoudaoLLMTranslator;

build_ffi!("youdao_llm", YoudaoLLMTranslator);
