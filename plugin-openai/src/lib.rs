pub mod translator;

use macros::build_ffi;
use crate::translator::OpenAITranslator;

build_ffi!("openai", OpenAITranslator);
