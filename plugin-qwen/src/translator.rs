use anyhow::{anyhow, bail, Result};
use async_openai::config::OpenAIConfig;
use async_openai::types::{ChatCompletionRequestMessage, CreateChatCompletionRequestArgs};
use async_openai::Client;
use async_trait::async_trait;
use futures_util::StreamExt;
use language_tags::LanguageTag;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fmt::{Display, Formatter};
use tokio::sync::mpsc::Sender;
use lib::{TranslateResult, TranslateStreamChunk, TranslateTask, Translator};
#[cfg(test)]
use lib::utils::{test_translate, test_translate_stream};

#[derive(Debug, Serialize, Deserialize)]
pub enum QwenMtModel {
    #[serde(rename = "qwen-mt-plus")]
    QwenMtPlus,
    #[serde(rename = "qwen-mt-turbo")]
    QwenMtTurbo,
}

impl TryFrom<String> for QwenMtModel {
    type Error = anyhow::Error;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        match value.as_str() {
            "qwen-mt-plus" => Ok(QwenMtModel::QwenMtPlus),
            "qwen-mt-turbo" => Ok(QwenMtModel::QwenMtTurbo),
            _ => Err(anyhow!("Invalid model: {}", value)),
        }
    }
}

impl Display for QwenMtModel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            QwenMtModel::QwenMtPlus => f.write_str("qwen-mt-plus"),
            QwenMtModel::QwenMtTurbo => f.write_str("qwen-mt-turbo"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum QwenMtLanguages {
    ///中文
    Chinese,
    ///英语
    English,
    ///日语
    Japanese,
    ///韩语
    Korean,
    ///泰语
    Thai,
    ///法语
    French,
    ///德语
    German,
    ///西班牙语
    Spanish,
    ///阿拉伯语
    Arabic,
    ///印尼语
    Indonesian,
    ///越南语
    Vietnamese,
    ///巴西葡萄牙语
    Portuguese,
    ///意大利语
    Italian,
    ///荷兰语
    Dutch,
    ///俄语
    Russian,
    ///高棉语
    Khmer,
    ///宿务语
    Cebuano,
    ///菲律宾语
    Filipino,
    ///捷克语
    Czech,
    ///波兰语
    Polish,
    ///波斯语
    Persian,
    ///希伯来语
    Hebrew,
    ///土耳其语
    Turkish,
    ///印地语
    Hindi,
    ///孟加拉语
    Bengali,
    ///乌尔都语
    Urdu,
}

impl Display for QwenMtLanguages {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("{:?}", self).as_str())
    }
}

impl TryFrom<LanguageTag> for QwenMtLanguages {
    type Error = anyhow::Error;

    fn try_from(tag: LanguageTag) -> Result<Self, Self::Error> {
        let primary = tag.primary_language().to_ascii_lowercase();
        let primary = primary.as_str();
        match primary {
            "zh" => Ok(Self::Chinese),
            "en" => Ok(Self::English),
            "ja" => Ok(Self::Japanese),
            "ko" => Ok(Self::Korean),
            "th" => Ok(Self::Thai),
            "fr" => Ok(Self::French),
            "de" => Ok(Self::German),
            "es" => Ok(Self::Spanish),
            "ar" => Ok(Self::Arabic),
            "id" => Ok(Self::Indonesian),
            "vi" => Ok(Self::Vietnamese),
            "pt" => {
                if let Some(region) = tag.region() {
                    if region.to_ascii_uppercase() == "BR" {
                        Ok(Self::Portuguese)
                    } else {
                        bail!("Unsupported language tag: {}", tag)
                    }
                } else {
                    bail!("Unsupported language tag: {}", tag)
                }
            }
            "it" => Ok(Self::Italian),
            "nl" => Ok(Self::Dutch),
            "ru" => Ok(Self::Russian),
            "km" => Ok(Self::Khmer),
            "ceb" => Ok(Self::Cebuano),
            "fil" => Ok(Self::Filipino),
            "cs" => Ok(Self::Czech),
            "pl" => Ok(Self::Polish),
            "fa" => Ok(Self::Persian),
            "he" => Ok(Self::Hebrew),
            "tr" => Ok(Self::Turkish),
            "hi" => Ok(Self::Hindi),
            "bn" => Ok(Self::Bengali),
            "ur" => Ok(Self::Urdu),
            _ => bail!("Unsupported language tag: {}", tag),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QwenMtTranslator {
    pub model: QwenMtModel,
    pub api_key: String,
}

impl QwenMtTranslator {
    fn build_request(&self, task: &TranslateTask, stream: bool) -> Result<Value> {
        let mut request_args = CreateChatCompletionRequestArgs::default();

        request_args.model(self.model.to_string()).messages(vec![
            ChatCompletionRequestMessage::User(task.content.clone().into()),
        ]);

        request_args.stream(stream);

        let mut request = serde_json::to_value(request_args.build()?)?;

        let source_language = task
            .source_language
            .clone()
            .ok_or(anyhow!(""))
            .and_then(|tag| tag.try_into())
            .map(|lang: QwenMtLanguages| lang.to_string())
            .unwrap_or("auto".to_string());

        let target_language = task
            .target_language
            .clone()
            .ok_or(anyhow!("缺少参数: target_language"))
            .and_then(|tag| tag.try_into())
            .map(|lang: QwenMtLanguages| lang.to_string())?;

        let mut options = Value::Object(Map::new());

        options["source_lang"] = Value::String(source_language);
        options["target_lang"] = Value::String(target_language);

        if let Some(field) = &task.field {
            options["domains"] = Value::String(field.clone());
        }

        if task.terms.len() > 0 {
            let list = task
                .terms
                .iter()
                .map(|i| {
                    json!({
                        "source": i.source,
                        "target": i.target,
                    })
                })
                .collect::<Vec<_>>();
            options["terms"] = Value::Array(list);
        }

        if task.references.len() > 0 {
            let list = task
                .references
                .iter()
                .map(|i| {
                    json!({
                        "source": i.source,
                        "target": i.target,
                    })
                })
                .collect::<Vec<_>>();
            options["tm_list"] = Value::Array(list);
        }

        request["translation_options"] = options;

        Ok(request)
    }
}

#[async_trait]
impl Translator for QwenMtTranslator {
    type This = Self;

    async fn new(config: Value) -> Result<Self> {
        serde_json::from_value(config).map_err(|e| anyhow!(e))
    }

    async fn translate(&self, task: TranslateTask) -> Result<TranslateResult> {
        let client = Client::with_config(
            OpenAIConfig::new()
                .with_api_base("https://dashscope.aliyuncs.com/compatible-mode/v1".to_string())
                .with_api_key(self.api_key.clone()),
        );

        let request = self.build_request(&task, false)?;

        let value: Value = client
            .chat()
            .create_byot(request)
            .await
            .map_err(|e| anyhow!(e))?;

        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string());

        Ok(TranslateResult {
            reasoning: None,
            content,
        })
    }

    async fn translate_stream(
        &self,
        task: TranslateTask,
        sender: Sender<TranslateStreamChunk>,
    ) -> Result<()> {
        let client = Client::with_config(
            OpenAIConfig::new()
                .with_api_base("https://dashscope.aliyuncs.com/compatible-mode/v1".to_string())
                .with_api_key(self.api_key.clone()),
        );

        let request = self.build_request(&task, true)?;

        let mut stream = client
            .chat()
            .create_stream_byot::<_, Value>(request)
            .await
            .map_err(|e| anyhow!(e))?;

        sender.send(TranslateStreamChunk::Start).await?;

        let mut cache = "".to_string();

        while let Some(result) = stream.next().await {
            if let Ok(chunk) = result {
                let content = chunk["choices"][0]["delta"]["content"]
                    .as_str()
                    .map(|s| s.to_string());

                sender
                    .send(TranslateStreamChunk::Delta(TranslateResult {
                        // content: content.map(|s| s[cache..].to_string()),
                        content: content
                            .clone()
                            .and_then(|s| s.strip_prefix(cache.as_str()).map(ToString::to_string)),
                        reasoning: None,
                    }))
                    .await?;

                cache = content.unwrap_or("".to_string());
            } else {
                bail!(result.unwrap_err())
            }
        }

        sender.send(TranslateStreamChunk::End).await?;

        Ok(())
    }
}

#[tokio::test]
async fn test_qwen() -> Result<()> {
    let translator = QwenMtTranslator {
        model: QwenMtModel::QwenMtTurbo,
        api_key: env!("QWEN_API_KEY").to_string(),
    };

    test_translate(translator).await
}

#[tokio::test]
async fn test_qwen_stream() -> Result<()> {
    let translator = QwenMtTranslator {
        model: QwenMtModel::QwenMtTurbo,
        api_key: env!("QWEN_API_KEY").to_string(),
    };

    test_translate_stream(translator).await
}
