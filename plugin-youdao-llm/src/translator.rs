use lib::utils::{format_messages, stream2normal};
#[cfg(test)]
use lib::utils::{test_translate, test_translate_stream};
use lib::{TranslateResult, TranslateStreamChunk, TranslateTask, Translator};
use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use hex::ToHex;
use language_tags::LanguageTag;
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fmt::{Display, Formatter};
use tokio::sync::mpsc::Sender;

#[derive(Debug, Serialize, Deserialize)]
pub enum YoudaoLLMLanguages {
    ///简体中文
    Chinese,
    ///英语
    English,
}

impl Display for YoudaoLLMLanguages {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            YoudaoLLMLanguages::Chinese => f.write_str("zh-CHS"),
            YoudaoLLMLanguages::English => f.write_str("en"),
        }
    }
}

impl TryFrom<LanguageTag> for YoudaoLLMLanguages {
    type Error = anyhow::Error;

    fn try_from(tag: LanguageTag) -> Result<Self, Self::Error> {
        let primary = tag.primary_language().to_ascii_lowercase();
        let primary = primary.as_str();
        match primary {
            "zh" => Ok(Self::Chinese),
            "en" => Ok(Self::English),
            _ => bail!("Unsupported language tag: {}", tag),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct YoudaoLLMTranslator {
    pub prompt: Option<String>,
    pub api_key: String,
    pub api_secret: String,
}

impl YoudaoLLMTranslator {
    fn build_request(&self, task: &TranslateTask) -> Result<Value> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let source_language = task
            .source_language
            .clone()
            .ok_or(anyhow!(""))
            .and_then(|tag| tag.try_into())
            .map(|lang: YoudaoLLMLanguages| lang.to_string())
            .unwrap_or("auto".to_string());

        let target_language = task
            .target_language
            .clone()
            .ok_or(anyhow!("缺少参数: target_language"))
            .and_then(|tag| tag.try_into())
            .map(|lang: YoudaoLLMLanguages| lang.to_string())?;

        let mut data = json!({
            "i": task.content.clone(),
            "from": source_language,
            "to": target_language,
            "streamType": "increment",
            "appKey": self.api_key.clone(),
            "salt": uuid.clone(),
            "signType": "v3",
            "curtime": now.to_string(),
        });

        if let Some(prompt) = &task.user_prompt {
            data["prompt"] = Value::String(format_messages(&prompt, &task)?);
        } else if let Some(prompt) = &self.prompt {
            data["prompt"] = Value::String(format_messages(&prompt, &task)?);
        }

        if let Some(extra) = task.extra.clone() {
            if let Some(handle) = extra["handleOption"].as_str() {
                data["handleOption"] = Value::String(handle.to_string());
            }

            if let Some(polish) = extra["polishOption"].as_str() {
                data["polishOption"] = Value::String(polish.to_string());
            }

            if let Some(expand) = extra["expandOption"].as_str() {
                data["expandOption"] = Value::String(expand.to_string());
            }
        }

        let sign = {
            let chars = task.content.chars().collect::<Vec<_>>();
            let input = if chars.len() > 20 {
                format!(
                    "{}{}{}",
                    String::from_iter(&chars[..10]),
                    chars.len(),
                    String::from_iter(&chars[chars.len() - 10..])
                )
            } else {
                String::from_iter(chars)
            };

            let text = format!(
                "{}{}{}{}{}",
                self.api_key, input, uuid, now, self.api_secret
            );

            let mut hasher = Sha256::new();
            hasher.update(text);
            let hash = hasher.finalize();

            hash.encode_hex()
        };

        data["sign"] = Value::String(sign);

        Ok(data)
    }
}

#[async_trait]
impl Translator for YoudaoLLMTranslator {
    type This = Self;

    async fn new(config: Value) -> Result<Self> {
        serde_json::from_value(config).map_err(|e| anyhow!(e))
    }

    fn get_supported_input_languages(&self) -> Result<Vec<String>> {
        Ok(vec![
            "zh".to_string(),
            "en".to_string(),
        ])
    }

    fn get_supported_output_languages(&self) -> Result<Vec<String>> {
        Ok(vec![
            "zh".to_string(),
            "en".to_string(),
        ])
    }

    fn is_supported_input_language(&self, lang: String) -> Result<bool> {
        let tag = LanguageTag::parse(lang.as_str())?;
        let li = vec![
            "zh".to_string(),
            "en".to_string(),
        ];
        Ok(li.contains(&tag.primary_language().to_string()))
    }

    fn is_supported_output_language(&self, lang: String) -> Result<bool> {
        let tag = LanguageTag::parse(lang.as_str())?;
        let li = vec![
            "zh".to_string(),
            "en".to_string(),
        ];
        Ok(li.contains(&tag.primary_language().to_string()))
    }

    async fn translate(&self, task: TranslateTask) -> Result<TranslateResult> {
        stream2normal(self, task).await
    }

    async fn translate_stream(
        &self,
        task: TranslateTask,
        sender: Sender<TranslateStreamChunk>,
    ) -> Result<()> {
        let client = Client::new();

        let body = self.build_request(&task)?;

        let builder = client
            .post("https://openapi.youdao.com/llm_trans")
            .form(&body);

        let mut es = EventSource::new(builder)?;

        while let Some(event) = es.next().await {
            match event {
                Ok(Event::Open) => sender.send(TranslateStreamChunk::Start).await?,
                Ok(Event::Message(message)) => {
                    let data: Value = serde_json::from_str(message.data.as_str())?;
                    sender
                        .send(TranslateStreamChunk::Delta(TranslateResult {
                            reasoning: None,
                            content: data["transIncre"].as_str().map(|s| s.to_string()),
                        }))
                        .await?
                }
                Err(err) => {
                    es.close();
                    if !matches!(err, reqwest_eventsource::Error::StreamEnded) {
                        bail!(err);
                    }
                }
            }
        }

        sender.send(TranslateStreamChunk::End).await?;

        Ok(())
    }
}

#[tokio::test]
async fn test_youdao_llm() -> Result<()> {
    let translator = YoudaoLLMTranslator {
        prompt: None,
        api_key: env!("YOUDAO_API_KEY").to_string(),
        api_secret: env!("YOUDAO_API_SECRET").to_string(),
    };

    test_translate(translator).await
}

#[tokio::test]
async fn test_youdao_llm_stream() -> Result<()> {
    let translator = YoudaoLLMTranslator {
        prompt: None,
        api_key: env!("YOUDAO_API_KEY").to_string(),
        api_secret: env!("YOUDAO_API_SECRET").to_string(),
    };

    test_translate_stream(translator).await
}
