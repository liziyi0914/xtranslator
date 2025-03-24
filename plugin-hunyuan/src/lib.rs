pub mod translator;

use macros::build_ffi;
use crate::translator::HunyuanTranslator;

build_ffi!("hunyuan", HunyuanTranslator);