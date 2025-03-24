pub mod translator;

use macros::build_ffi;
use crate::translator::QwenMtTranslator;

build_ffi!("qwen", QwenMtTranslator);
