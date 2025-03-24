#[cfg(test)]
use lib::utils::{test_translate, test_translate_stream};
use lib::{TranslateResult, TranslateStreamChunk, TranslateTask, Translator};
use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use language_tags::LanguageTag;
use md5::Md5;
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;
use std::fmt::{Display, Formatter};
use tokio::sync::mpsc::Sender;
use lib::utils::normal2stream;

#[derive(Debug, Serialize, Deserialize)]
pub enum BaiduFanyiLanguages {
    /// 简体中文
    Chinese,
    /// 英语
    English,
    /// 粤语
    Yue,
    /// 文言文
    Wyw,
    /// 日语
    Japanese,
    /// 韩语
    Korean,
    /// 法语
    French,
    /// 西班牙语
    Spanish,
    /// 泰语
    Thai,
    /// 阿拉伯语
    Arabic,
    /// 俄语
    Russian,
    /// 葡萄牙语
    Portuguese,
    /// 德语
    German,
    /// 意大利语
    Italian,
    /// 希腊语
    Greek,
    /// 荷兰语
    Dutch,
    /// 波兰语
    Polish,
    /// 保加利亚语
    Bulgarian,
    /// 爱沙尼亚语
    Estonian,
    /// 丹麦语
    Danish,
    /// 芬兰语
    Finnish,
    /// 捷克语
    Czech,
    /// 罗马尼亚语
    Romanian,
    /// 斯洛文尼亚语
    Slovenian,
    /// 瑞典语
    Swedish,
    /// 匈牙利语
    Hungarian,
    /// 繁体中文
    TraditionalChinese,
    /// 越南语
    Vietnamese,
}

impl Display for BaiduFanyiLanguages {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let id = match self {
            BaiduFanyiLanguages::Chinese => "zh",
            BaiduFanyiLanguages::English => "en",
            BaiduFanyiLanguages::Yue => "yue",
            BaiduFanyiLanguages::Wyw => "wyw",
            BaiduFanyiLanguages::Japanese => "jp",
            BaiduFanyiLanguages::Korean => "kor",
            BaiduFanyiLanguages::French => "fra",
            BaiduFanyiLanguages::Spanish => "spa",
            BaiduFanyiLanguages::Thai => "th",
            BaiduFanyiLanguages::Arabic => "ara",
            BaiduFanyiLanguages::Russian => "ru",
            BaiduFanyiLanguages::Portuguese => "pt",
            BaiduFanyiLanguages::German => "de",
            BaiduFanyiLanguages::Italian => "it",
            BaiduFanyiLanguages::Greek => "el",
            BaiduFanyiLanguages::Dutch => "nl",
            BaiduFanyiLanguages::Polish => "pl",
            BaiduFanyiLanguages::Bulgarian => "bul",
            BaiduFanyiLanguages::Estonian => "est",
            BaiduFanyiLanguages::Danish => "dan",
            BaiduFanyiLanguages::Finnish => "fin",
            BaiduFanyiLanguages::Czech => "cs",
            BaiduFanyiLanguages::Romanian => "rom",
            BaiduFanyiLanguages::Slovenian => "slo",
            BaiduFanyiLanguages::Swedish => "swe",
            BaiduFanyiLanguages::Hungarian => "hu",
            BaiduFanyiLanguages::TraditionalChinese => "cht",
            BaiduFanyiLanguages::Vietnamese => "vie",
        };
        write!(f, "{}", id)
    }
}

impl TryFrom<LanguageTag> for BaiduFanyiLanguages {
    type Error = anyhow::Error;

    fn try_from(tag: LanguageTag) -> Result<Self, Self::Error> {
        let primary = tag.primary_language();

        // 特殊处理中文变体
        if primary == "zh" {
            return if tag.script() == Some("Hant") || tag.region().map_or(false, |r| ["TW", "HK", "MO"].contains(&r)) {
                Ok(Self::TraditionalChinese)
            } else {
                Ok(Self::Chinese)
            };
        }

        // 处理其他语言映射
        match primary {
            "en" => Ok(Self::English),
            "yue" => Ok(Self::Yue),
            "lzh" => Ok(Self::Wyw),
            "ja" => Ok(Self::Japanese),
            "ko" => Ok(Self::Korean),
            "fr" => Ok(Self::French),
            "es" => Ok(Self::Spanish),
            "th" => Ok(Self::Thai),
            "ar" => Ok(Self::Arabic),
            "ru" => Ok(Self::Russian),
            "pt" => Ok(Self::Portuguese),
            "de" => Ok(Self::German),
            "it" => Ok(Self::Italian),
            "el" => Ok(Self::Greek),
            "nl" => Ok(Self::Dutch),
            "pl" => Ok(Self::Polish),
            "bg" => Ok(Self::Bulgarian),
            "et" => Ok(Self::Estonian),
            "da" => Ok(Self::Danish),
            "fi" => Ok(Self::Finnish),
            "cs" => Ok(Self::Czech),
            "ro" => Ok(Self::Romanian),
            "sl" => Ok(Self::Slovenian),
            "sv" => Ok(Self::Swedish),
            "hu" => Ok(Self::Hungarian),
            "vi" => Ok(Self::Vietnamese),
            _ => bail!("Unsupported BCP47 language"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BaiduFanyiTranslator {
    pub app_id: String,
    pub secret: String,
}

impl BaiduFanyiTranslator {
    fn build_request(&self, task: &TranslateTask) -> Result<Value> {
        let uuid = uuid::Uuid::new_v4().to_string();

        let s = format!("{}{}{}{}", self.app_id, task.content, uuid, self.secret);

        let mut md5 = Md5::new();
        md5.update(s);
        let hash = hex::encode(md5.finalize());

        let source_language = task
            .source_language
            .clone()
            .ok_or(anyhow!(""))
            .and_then(|tag| tag.try_into())
            .map(|lang: BaiduFanyiLanguages| lang.to_string())
            .unwrap_or("auto".to_string());

        let target_language = task
            .target_language
            .clone()
            .ok_or(anyhow!("缺少参数: target_language"))
            .and_then(|tag| tag.try_into())
            .map(|lang: BaiduFanyiLanguages| lang.to_string())?;

        let data = json!({
            "q": task.content,
            "from": source_language,
            "to": target_language,
            "appid": self.app_id,
            "salt": uuid,
            "sign": hash,
        });

        Ok(data)
    }
}

#[async_trait]
impl Translator for BaiduFanyiTranslator {
    type This = Self;

    async fn new(config: Value) -> Result<Self> {
        serde_json::from_value(config).map_err(|e| anyhow!(e))
    }

    async fn translate(&self, task: TranslateTask) -> Result<TranslateResult> {
        let body = self.build_request(&task)?;

        let client = Client::new();
        let resp = client
            .request(Method::POST, "https://fanyi-api.baidu.com/api/trans/vip/translate")
            .form(&body)
            .send().await?;
        let json = resp.json::<Value>().await?;

        if json["error_code"].as_str().map(|n| n!="52000" ).unwrap_or(false) {
            bail!("Request API error: {}, {:?}", json["error_code"].as_str().unwrap(), json["error_msg"].as_str())
        }

        Ok(TranslateResult {
            reasoning: None,
            content: json["trans_result"][0]["dst"].as_str().map(|s| s.to_string()),
        })
    }

    async fn translate_stream(
        &self,
        task: TranslateTask,
        sender: Sender<TranslateStreamChunk>,
    ) -> Result<()> {
        normal2stream(self, task, sender).await
    }
}

#[tokio::test]
async fn test_baidu_fanyi() -> Result<()> {
    let translator = BaiduFanyiTranslator {
        app_id: env!("BAIDU_FANYI_APP_ID").to_string(),
        secret: env!("BAIDU_FANYI_SECRET").to_string(),
    };

    test_translate(translator).await
}

#[tokio::test]
async fn test_baidu_fanyi_stream() -> Result<()> {
    let translator = BaiduFanyiTranslator {
        app_id: env!("BAIDU_FANYI_APP_ID").to_string(),
        secret: env!("BAIDU_FANYI_SECRET").to_string(),
    };

    test_translate_stream(translator).await
}
